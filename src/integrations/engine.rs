//! Execução das integrações de um script: dispara, entrega em threads e devolve as respostas
//! para as variáveis. Quem chama é o interpretador, sempre na thread do fluxo; as entregas
//! nunca tocam nas variáveis diretamente.
use super::{Integration, Outcome, When, result_vars};
use evalexpr::{ContextWithMutableVariables, HashMapContext, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

struct Slot {
    /// Número do último disparo; respostas de disparos anteriores são descartadas.
    seq: u64,
    pending: bool,
    /// Próximo disparo do gatilho "a cada N s" (relógio do fluxo, em ms).
    next_ms: u64,
}

struct Done {
    idx: usize,
    seq: u64,
    out: Outcome,
}

pub struct Engine {
    list: Vec<Integration>,
    slots: Vec<Slot>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
}

fn set(vars: &mut HashMapContext, name: &str, v: Value) {
    let _ = vars.set_value(name.into(), v);
}

fn every_ms(s: f64) -> u64 {
    ((s * 1000.0) as u64).max(1)
}

impl Engine {
    /// Cria as variáveis de resultado (`r = ""`, `r_ok = false`, `r_status = 0`, `r_pendente = false`).
    pub fn new(list: &[Integration], vars: &mut HashMapContext, now: u64) -> Self {
        let (tx, rx) = channel();
        let slots = list
            .iter()
            .map(|it| Slot { seq: 0, pending: false, next_ms: if let When::Every { s } = it.when { now + every_ms(s) } else { u64::MAX } })
            .collect();
        for r in list.iter().filter_map(|it| it.result.as_deref()) {
            let [v, ok, status, pend] = result_vars(r);
            set(vars, &v, Value::String(String::new()));
            set(vars, &ok, Value::Boolean(false));
            set(vars, &status, Value::Int(0));
            set(vars, &pend, Value::Boolean(false));
        }
        Engine { list: list.to_vec(), slots, tx, rx }
    }

    /// Dispara a integração `idx` com os valores atuais. `r_pendente` vira verdadeiro já aqui.
    fn fire(&mut self, idx: usize, vars: &mut HashMapContext, log: &mut dyn FnMut(String)) {
        let it = &self.list[idx];
        let job = it.payload.render(vars).and_then(|b| it.dest.get().prepare(b, vars));
        let slot = &mut self.slots[idx];
        slot.seq += 1;
        let seq = slot.seq;
        match job {
            Ok(job) => {
                slot.pending = true;
                if let Some(r) = &it.result {
                    set(vars, &result_vars(r)[3], Value::Boolean(true));
                }
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let out = job();
                    let _ = tx.send(Done { idx, seq, out }); // o fluxo já acabou: a resposta é descartada
                });
            }
            Err(e) => {
                self.apply(Done { idx, seq, out: Outcome::failed(e) }, vars, log);
            }
        }
    }

    /// Grava uma resposta nas variáveis. Devolve `true` se alguma variável mudou.
    fn apply(&mut self, d: Done, vars: &mut HashMapContext, log: &mut dyn FnMut(String)) -> bool {
        let slot = &mut self.slots[d.idx];
        if d.seq != slot.seq {
            return false;
        }
        slot.pending = false;
        let it = &self.list[d.idx];
        if let Some(e) = &d.out.error {
            log(format!("integração '{}': {e}", it.name));
        } else if !d.out.ok {
            let body: String = d.out.body.chars().take(160).collect();
            log(format!("integração '{}': falhou (status {}) {}", it.name, d.out.status, body.trim()));
        }
        let Some(r) = &it.result else { return false };
        let [v, ok, status, pend] = result_vars(r);
        set(vars, &v, Value::String(d.out.body));
        set(vars, &ok, Value::Boolean(d.out.ok));
        set(vars, &status, Value::Int(d.out.status));
        set(vars, &pend, Value::Boolean(false));
        true
    }

    /// Um bloco definiu `name`: dispara as integrações "ao definir" que o escutam.
    pub fn on_set(&mut self, name: &str, vars: &mut HashMapContext, log: &mut dyn FnMut(String)) {
        for idx in 0..self.list.len() {
            if matches!(&self.list[idx].when, When::OnSet(names) if names.iter().any(|n| n == name)) {
                self.fire(idx, vars, log);
            }
        }
    }

    /// Aplica as respostas que chegaram e dispara os gatilhos "a cada N s" vencidos.
    /// Devolve `true` se alguma variável mudou.
    pub fn pump(&mut self, vars: &mut HashMapContext, now: u64, log: &mut dyn FnMut(String)) -> bool {
        let mut changed = false;
        while let Ok(d) = self.rx.try_recv() {
            changed |= self.apply(d, vars, log);
        }
        for idx in 0..self.list.len() {
            if let When::Every { s } = self.list[idx].when
                && now >= self.slots[idx].next_ms
            {
                self.slots[idx].next_ms = now + every_ms(s);
                if !self.slots[idx].pending {
                    self.fire(idx, vars, log);
                    changed = true;
                }
            }
        }
        changed
    }

    /// Fim da execução: dispara os gatilhos "ao terminar" e espera por eles, no máximo pelo
    /// maior timeout (+1 s de folga). `stop` interrompe a espera e descarta as respostas.
    pub fn finish(&mut self, vars: &mut HashMapContext, log: &mut dyn FnMut(String), stop: &AtomicBool) -> bool {
        let ends: Vec<usize> = (0..self.list.len()).filter(|&i| self.list[i].when == When::OnEnd).collect();
        let mut changed = !ends.is_empty();
        for &i in &ends {
            self.fire(i, vars, log);
        }
        let limit = Duration::from_millis(ends.iter().map(|&i| self.list[i].dest.get().timeout_ms()).max().unwrap_or(0) + 1000);
        let t0 = Instant::now();
        while ends.iter().any(|&i| self.slots[i].pending) && t0.elapsed() < limit {
            if stop.load(Ordering::Relaxed) {
                return changed;
            }
            if let Ok(d) = self.rx.recv_timeout(Duration::from_millis(50)) {
                changed |= self.apply(d, vars, log);
            }
        }
        while let Ok(d) = self.rx.try_recv() {
            changed |= self.apply(d, vars, log);
        }
        for &i in ends.iter().filter(|&&i| self.slots[i].pending) {
            log(format!("integração '{}': sem resposta dentro do timeout (ao terminar)", self.list[i].name));
        }
        changed
    }
}
