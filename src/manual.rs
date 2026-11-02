//! Janela "Manual": Expressões, Blocos, Condições, Teclas e Atalhos.
//! O conteúdo sai de `entries` (puro, testado); `window` só desenha.

use crate::catalog::{FUNCTIONS, OPERATORS, TEXT_SYNTAX};
use crate::model::{Block, Settings, all_keys, key_name};
use eframe::egui::{self, RichText};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Section {
    #[default]
    Expressoes,
    Blocos,
    Condicoes,
    Teclas,
    Atalhos,
}

impl Section {
    pub const ALL: [Section; 5] = [Section::Expressoes, Section::Blocos, Section::Condicoes, Section::Teclas, Section::Atalhos];

    pub fn title(self) -> &'static str {
        match self {
            Section::Expressoes => "Expressões",
            Section::Blocos => "Blocos",
            Section::Condicoes => "Condições",
            Section::Teclas => "Teclas",
            Section::Atalhos => "Atalhos",
        }
    }
}

/// Estado da janela: aberta ou não, e a seção mostrada.
#[derive(Default)]
pub struct Manual {
    pub open: bool,
    pub section: Section,
}

impl Manual {
    /// Abre direto numa seção (o "?" junto aos campos usa Expressões).
    pub fn ask(&mut self, s: Section) {
        self.open = true;
        self.section = s;
    }
}

/// Uma linha do manual: termo (em fonte fixa), explicação e, se houver, um exemplo.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub term: String,
    pub desc: String,
    pub example: Option<String>,
}

fn e(term: impl Into<String>, desc: impl Into<String>, example: Option<&str>) -> Entry {
    Entry { term: term.into(), desc: desc.into(), example: example.map(String::from) }
}

/// Texto curto de cada bloco. Variantes novas sem texto aqui aparecem só com o título
/// (e o teste `sections_have_content` acusa).
#[allow(unreachable_patterns)]
fn block_help(b: &Block) -> &'static str {
    match b {
        Block::Click { .. } => "clica em (x, y) da área cliente; ± px sorteia um desvio. Para arrastar: pressionar, Mover, soltar",
        Block::Move { .. } => "move o mouse para (x, y)",
        Block::Key { .. } => "aperta, pressiona ou solta uma tecla. Para Ctrl+S: Ctrl pressionar, S apertar, Ctrl soltar",
        Block::Type { .. } => "digita o texto; {expressão} é trocado pelo valor",
        Block::Scroll { .. } => "rola a roda em (x, y); 120 = um passo, negativo = para baixo",
        Block::Wait { .. } => "espera os milissegundos da expressão; ± ms sorteia um desvio",
        Block::WaitUntil { .. } => "confere a condição a cada 100 ms até ela valer ou o tempo acabar",
        Block::If { .. } => "roda \"então\" se a condição valer, senão \"senão\"",
        Block::Repeat { .. } => "repete o corpo N vezes (N é expressão)",
        Block::While { .. } => "repete o corpo enquanto a condição valer",
        Block::SetVar { .. } => "guarda o valor da expressão numa variável",
        Block::OcrToVar { .. } => "lê o texto de uma região da janela (OCR) e guarda numa variável",
        Block::Uia { .. } => "age num controle pela UI Automation, sem mouse nem teclado: acionar, alternar, definir valor ({expressão}), selecionar, expandir, recolher ou ler o valor/nome para uma variável. Capture o elemento com 📍; espera ele aparecer até o timeout e, sem achar, para com erro ou clica no ponto capturado",
        Block::Call { .. } => "roda outro script, com a mesma janela e as mesmas variáveis",
        Block::Log { .. } => "escreve no log do app; {expressão} é trocado pelo valor",
        Block::Breakpoint => "pausa aqui; veja as variáveis no painel e clique ▶ para continuar",
        Block::Stop => "encerra o script",
        _ => "",
    }
}

pub fn entries(s: Section, settings: &Settings) -> Vec<Entry> {
    match s {
        Section::Expressoes => {
            let mut v = vec![e("Onde", "campos x/y, tempos, Repetir, Definir variável e condição \"expressão\"; nos textos de Digitar e Mostrar no log, dentro de {…}", None)];
            v.extend(TEXT_SYNTAX.iter().map(|(t, d)| e(*t, *d, None)));
            v.push(e("Variáveis", "globais na execução, inclusive dentro de Chamar script; só existem depois do primeiro Definir variável ou Ler texto", None));
            v.extend(OPERATORS.iter().map(|o| e(o.op, o.desc, Some(o.example))));
            v.extend(FUNCTIONS.iter().map(|f| e(f.sig, if f.ours { format!("{} (UltraTask)", f.desc) } else { f.desc.into() }, Some(f.example))));
            v.push(e("Cola", "ao editar uma expressão, a lista mostra variáveis e funções; ⬆ ⬇ escolhem, Enter ou clique insere", None));
            v
        }
        Section::Blocos => Block::templates().iter().map(|b| e(b.title(), block_help(b), None)).collect(),
        Section::Condicoes => vec![
            e("não", "inverte o resultado da condição", None),
            e("pixel tem a cor", "a cor em (x, y) bate com a escolhida, com tolerância por canal; janela minimizada dá falso", None),
            e("texto (OCR) contém", "o texto lido na região contém o trecho, sem diferenciar maiúsculas", None),
            e("janela existe", "a janela-alvo está aberta", None),
            e("janela está em foco", "a janela-alvo é a janela ativa", None),
            e("título casa com", "o título bate com o padrão; * = qualquer trecho, ? = um caractere", Some("*Nível 3*")),
            e("expressão", "vale se a expressão der verdadeiro", Some("contador < 10 && contem(texto, \"OK\")")),
            e("elemento (UIA)", "o elemento capturado com 📍 existe, o nome ou o valor contém o texto (sem diferenciar maiúsculas), ou a caixa está marcada; uma busca só, sem esperar (use em Esperar até para esperar)", None),
        ],
        Section::Teclas => all_keys().into_iter().map(|vk| e(key_name(vk), format!("vk {vk}"), None)).collect(),
        Section::Atalhos => vec![
            e(settings.record.label(), "gravar / parar gravação (global)", None),
            e(settings.play.label(), "tocar / parar o script selecionado (global)", None),
            e(settings.panic.label(), "parar tudo (global)", None),
            e("Ctrl+S", "salvar o script", None),
            e("⬆ ⬇  Enter", "na cola: escolher e inserir a sugestão", None),
            e("Esc", "sair do campo e fechar a cola", None),
        ],
    }
}

pub fn window(ctx: &egui::Context, m: &mut Manual, settings: &Settings) {
    if !m.open {
        return;
    }
    let mut open = true;
    egui::Window::new("📖 Manual").open(&mut open).default_size([620.0, 480.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for s in Section::ALL {
                ui.selectable_value(&mut m.section, s, s.title());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("manual").num_columns(2).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                for en in entries(m.section, settings) {
                    ui.label(RichText::new(&en.term).monospace().strong());
                    ui.vertical(|ui| {
                        ui.label(&en.desc);
                        if let Some(x) = &en.example {
                            ui.label(RichText::new(format!("ex.: {x}")).monospace().weak());
                        }
                    });
                    ui.end_row();
                }
            });
        });
    });
    m.open &= open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_have_content() {
        let st = Settings::default();
        for s in Section::ALL {
            assert!(!entries(s, &st).is_empty(), "{s:?} vazia");
        }
        let ex = entries(Section::Expressoes, &st);
        for f in FUNCTIONS {
            assert!(ex.iter().any(|e| e.term == f.sig), "Manual sem {}", f.name);
        }
        assert!(entries(Section::Blocos, &st).iter().all(|e| !e.desc.is_empty()), "bloco sem texto no Manual");
    }

    #[test]
    fn ask_opens_on_section() {
        let mut m = Manual::default();
        m.ask(Section::Teclas);
        assert!(m.open);
        assert_eq!(m.section, Section::Teclas);
    }
}
