//! "Elemento (UIA)" block and condition in the interpreter. The element lookup and the
//! pattern call go through `Io::uia`; waiting for the element, the fallback click and
//! the log messages live here, testable with a fake tree and a fake clock.
use super::*;
use crate::uia::{Op, Query, Reply, UiaAction, UiaElem, UiaTest, describe};

/// Pause between lookups while waiting for the element to appear.
const POLL_MS: u64 = 200;

impl Interp<'_> {
    pub(super) fn run_uia(&mut self, elem: &UiaElem, action: &UiaAction, timeout_ms: &str, fallback_click: bool) -> R {
        let op = match action {
            UiaAction::Invoke => Op::Invoke,
            UiaAction::Toggle => Op::Toggle,
            UiaAction::SetValue(t) => Op::SetValue(self.interp_text(t)?),
            UiaAction::Select => Op::Select,
            UiaAction::Expand => Op::Expand,
            UiaAction::Collapse => Op::Collapse,
            UiaAction::ReadValue(_) => Op::Value,
            UiaAction::ReadName(_) => Op::Name,
        };
        let read = matches!(action, UiaAction::ReadValue(_) | UiaAction::ReadName(_));
        let q = Query { elem: elem.clone(), op };
        let timeout = self.int(timeout_ms)?.max(0) as u64;
        let end = self.io.now_ms() + timeout;
        loop {
            match self.io.uia(&q)? {
                Reply::NotFound if self.io.now_ms() >= end => break,
                Reply::NotFound => self.sleep(POLL_MS)?,
                Reply::Text(s) => {
                    if let UiaAction::ReadValue(v) | UiaAction::ReadName(v) = action {
                        self.set_var(v, Value::String(s))?;
                        self.var_set(v);
                        self.snapshot_vars();
                    }
                    return Ok(());
                }
                // The call was delivered; a blocking Invoke (modal dialog…) is not a failure.
                Reply::TimedOut if !read => {
                    (self.log)(format!("aviso: {} não respondeu a tempo à ação '{}'; seguindo", describe(elem), action.label()));
                    return Ok(());
                }
                Reply::TimedOut => return Err(Halt::Error(format!("{} não respondeu a tempo", describe(elem)))),
                Reply::Done | Reply::Bool(_) => return Ok(()),
            }
        }
        if !fallback_click {
            return Err(Halt::Error(format!("elemento {} não encontrado em {timeout} ms", describe(elem))));
        }
        (self.log)(format!("aviso: elemento {} não encontrado; clicando no ponto capturado ({}, {})", describe(elem), elem.x, elem.y));
        self.run_block(&Block::Click { x: elem.x.to_string(), y: elem.y.to_string(), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 })
    }

    /// A single lookup (WaitUntil/While already poll). Failures read as false, with a warning.
    pub(super) fn cond_uia(&mut self, elem: &UiaElem, test: &UiaTest) -> R<bool> {
        let op = match test {
            UiaTest::Exists => Op::Find,
            UiaTest::NameContains(_) => Op::Name,
            UiaTest::ValueContains(_) => Op::Value,
            UiaTest::Checked => Op::Checked,
        };
        let r = match self.io.uia(&Query { elem: elem.clone(), op }) {
            Ok(r) => r,
            Err(Halt::Error(e)) => {
                self.warn(format!("condição elemento {}: {e}", describe(elem)));
                return Ok(false);
            }
            Err(h) => return Err(h),
        };
        Ok(match (test, r) {
            (_, Reply::NotFound) => false,
            (_, Reply::TimedOut) => {
                self.warn(format!("condição elemento {}: não respondeu a tempo", describe(elem)));
                false
            }
            (UiaTest::Exists, _) => true,
            (UiaTest::NameContains(t) | UiaTest::ValueContains(t), Reply::Text(s)) => s.to_lowercase().contains(&t.to_lowercase()),
            (UiaTest::Checked, Reply::Bool(b)) => b,
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uia::{FakeTree, Node, UiaId};

    /// Fake target window: a UIA tree whose "Salvar" button only shows up at `appears_at` ms.
    struct UiaIo {
        t: u64,
        tree: FakeTree,
        appears_at: u64,
        sent: Vec<Ev>,
        hang: bool,
    }

    impl Io for UiaIo {
        fn send(&mut self, ev: &Ev) -> R {
            self.sent.push(*ev);
            Ok(())
        }
        fn type_text(&mut self, _: &str) -> R {
            Ok(())
        }
        fn pixel(&mut self, _: i32, _: i32) -> R<[u8; 3]> {
            Ok([0; 3])
        }
        fn ocr(&mut self, _: Region) -> R<String> {
            Ok(String::new())
        }
        fn window_exists(&mut self) -> bool {
            true
        }
        fn window_foreground(&mut self) -> bool {
            false
        }
        fn window_title(&mut self) -> String {
            String::new()
        }
        fn now_ms(&mut self) -> u64 {
            self.t
        }
        fn sleep(&mut self, ms: u64) {
            self.t += ms;
        }
        fn uia(&mut self, q: &Query) -> R<Reply> {
            if self.hang {
                return Ok(Reply::TimedOut);
            }
            if self.t < self.appears_at && q.elem.automation_id == "btnSalvar" {
                return Ok(Reply::NotFound);
            }
            Ok(self.tree.query(q, None))
        }
    }

    fn n(ct: &str, name: &str, aid: &str, parent: Option<usize>) -> Node {
        Node { id: UiaId { automation_id: aid.into(), name: name.into(), control_type: ct.into(), class_name: String::new() }, parent, rect: [0, 0, 10, 10] }
    }

    fn fake_nodes() -> Vec<Node> {
        vec![
            n("Window", "Pedido", "", None),
            n("CheckBox", "Urgente", "chkUrgente", Some(0)),
            n("Edit", "Nome", "txtNome", Some(0)),
            n("Button", "Salvar", "btnSalvar", Some(0)),
        ]
    }

    fn el(ct: &str, aid: &str) -> UiaElem {
        UiaElem { control_type: ct.into(), automation_id: aid.into(), x: 7, y: 8, ..Default::default() }
    }

    fn uia(elem: UiaElem, action: UiaAction, timeout: &str, fallback: bool) -> Block {
        Block::Uia { elem, action, timeout_ms: timeout.into(), fallback_click: fallback }
    }

    fn run(blocks: &[Block], io: &mut UiaIo) -> (R, Vec<String>) {
        let stop = AtomicBool::new(false);
        let dbg = Debug::default();
        let mut logs = vec![];
        let r = {
            let mut log = |m: String| logs.push(m);
            Interp::new(io, &stop, &[], 1.0, &mut log, &dbg).run_blocks(blocks)
        };
        (r, logs)
    }

    fn io(appears_at: u64) -> UiaIo {
        UiaIo { t: 0, tree: FakeTree::new(fake_nodes()), appears_at, sent: vec![], hang: false }
    }

    #[test]
    fn actions_reads_and_conditions() {
        let mut io = io(0);
        let blocks = vec![
            Block::SetVar { name: "n".into(), expr: "41 + 1".into() },
            uia(el("Edit", "txtNome"), UiaAction::SetValue("Pedido {n}".into()), "1000", false),
            uia(el("CheckBox", "chkUrgente"), UiaAction::Toggle, "1000", false),
            uia(el("Edit", "txtNome"), UiaAction::ReadValue("lido".into()), "1000", false),
            uia(el("Button", "btnSalvar"), UiaAction::ReadName("nome".into()), "1000", false),
            uia(el("Button", "btnSalvar"), UiaAction::Invoke, "1000", false),
            Block::If {
                cond: Condition { kind: CondKind::Uia { elem: el("CheckBox", "chkUrgente"), test: UiaTest::Checked }, negate: false },
                then: vec![Block::Log { text: "marcado {lido} {nome}".into() }],
                else_: vec![Block::Stop],
            },
            Block::If {
                cond: Condition { kind: CondKind::Uia { elem: el("Edit", "txtNome"), test: UiaTest::ValueContains("pedido 42".into()) }, negate: false },
                then: vec![],
                else_: vec![Block::Stop],
            },
            Block::If {
                cond: Condition { kind: CondKind::Uia { elem: el("Button", "nada"), test: UiaTest::Exists }, negate: true },
                then: vec![],
                else_: vec![Block::Stop],
            },
        ];
        let (r, logs) = run(&blocks, &mut io);
        assert_eq!(r, Ok(()), "{logs:?}");
        assert!(logs.contains(&"🐞 marcado Pedido 42 Salvar".to_string()), "{logs:?}");
        assert!(io.sent.is_empty(), "UIA must not send any input");
        assert_eq!(io.tree.log.iter().filter(|l| l.1 == Op::Invoke).count(), 1);
    }

    #[test]
    fn waits_for_the_element_then_errors_or_falls_back() {
        // Shows up after 600 ms: found within a 2 s timeout, polling every 200 ms.
        let mut late = io(600);
        let (r, _) = run(&[uia(el("Button", "btnSalvar"), UiaAction::Invoke, "2000", false)], &mut late);
        assert_eq!(r, Ok(()));
        assert_eq!(late.t, 600);

        // Never shows up: a clear error after the timeout.
        let mut never = io(u64::MAX);
        let (r, _) = run(&[uia(el("Button", "btnSalvar"), UiaAction::Invoke, "1000", false)], &mut never);
        assert_eq!(r, Err(Halt::Error("elemento Button #btnSalvar não encontrado em 1000 ms".into())));
        assert!(never.t >= 1000 && never.sent.is_empty());

        // With the fallback: a warning and a click on the captured point.
        let mut fb = io(u64::MAX);
        let (r, logs) = run(&[uia(el("Button", "btnSalvar"), UiaAction::Invoke, "500", true)], &mut fb);
        assert_eq!(r, Ok(()));
        assert!(logs.iter().any(|l| l.contains("não encontrado; clicando no ponto capturado (7, 8)")), "{logs:?}");
        assert_eq!(fb.sent, [Ev::Move(7, 8), Ev::Button(Btn::Left, true, 7, 8), Ev::Button(Btn::Left, false, 7, 8)]);
    }

    #[test]
    fn hung_app_and_missing_uia() {
        // Invoke that blocks (modal dialog): warn and go on; a read that blocks is an error.
        let mut hung = io(0);
        hung.hang = true;
        let (r, logs) = run(&[uia(el("Button", "btnSalvar"), UiaAction::Invoke, "1000", false)], &mut hung);
        assert_eq!(r, Ok(()));
        assert!(logs.iter().any(|l| l.contains("não respondeu a tempo")), "{logs:?}");
        let (r, _) = run(&[uia(el("Edit", "txtNome"), UiaAction::ReadValue("v".into()), "1000", false)], &mut hung);
        assert!(matches!(r, Err(Halt::Error(e)) if e.contains("não respondeu")));

        // An Io without UIA (the trait default): the block errors, the condition is false.
        struct NoUia;
        impl Io for NoUia {
            fn send(&mut self, _: &Ev) -> R {
                Ok(())
            }
            fn type_text(&mut self, _: &str) -> R {
                Ok(())
            }
            fn pixel(&mut self, _: i32, _: i32) -> R<[u8; 3]> {
                Ok([0; 3])
            }
            fn ocr(&mut self, _: Region) -> R<String> {
                Ok(String::new())
            }
            fn window_exists(&mut self) -> bool {
                true
            }
            fn window_foreground(&mut self) -> bool {
                false
            }
            fn window_title(&mut self) -> String {
                String::new()
            }
            fn now_ms(&mut self) -> u64 {
                0
            }
            fn sleep(&mut self, _: u64) {}
        }
        let stop = AtomicBool::new(false);
        let dbg = Debug::default();
        let mut logs = vec![];
        let mut log = |m: String| logs.push(m);
        let mut io = NoUia;
        let mut it = Interp::new(&mut io, &stop, &[], 1.0, &mut log, &dbg);
        assert!(matches!(it.run_blocks(&[uia(el("Button", "x"), UiaAction::Invoke, "0", false)]), Err(Halt::Error(_))));
        let cond = Condition { kind: CondKind::Uia { elem: el("Button", "x"), test: UiaTest::Exists }, negate: false };
        assert_eq!(it.run_blocks(&[Block::If { cond, then: vec![Block::Stop], else_: vec![] }]), Ok(()));
        drop(it);
        assert!(logs.iter().any(|l| l.contains("indisponível")), "{logs:?}");
    }
}
