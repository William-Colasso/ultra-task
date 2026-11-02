//! Campo de texto com a cola de comandos: lista de variáveis e funções junto ao campo em foco.
//! A lógica (o que mostrar, o que inserir) fica em `catalog`; aqui só o desenho e o teclado.

use crate::catalog::{self, FieldKind, SugKind};
use eframe::egui::{self, Color32, Id, Key, Modifiers, RichText, Ui};
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;

/// Estado da cola entre quadros.
#[derive(Clone, Default)]
struct ColaState {
    sel: usize,
    /// Prefixo do quadro anterior: quando muda, a seleção volta ao topo.
    prefix: String,
    /// O ponteiro estava sobre a cola: ela continua aberta durante o clique, mesmo que o campo perca o foco.
    hovered: bool,
}

fn cursor_of(ctx: &egui::Context, id: Id) -> Option<usize> {
    TextEditState::load(ctx, id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0)
}

fn set_cursor(ctx: &egui::Context, id: Id, at: usize) {
    let mut st = TextEditState::load(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(at))));
    st.store(ctx, id);
}

/// Campo de uma linha com a cola. `red` pinta o texto de vermelho (expressão inválida).
/// Devolve a resposta do campo e `true` se o usuário pediu o Manual pela cola.
pub fn field(ui: &mut Ui, id: Id, text: &mut String, width: f32, kind: FieldKind, vars: &[String], hint: &str, red: bool) -> (egui::Response, bool) {
    let ctx = ui.ctx().clone();
    let sid = id.with("cola");
    let mut st: ColaState = ctx.data(|d| d.get_temp(sid)).unwrap_or_default();
    let focused = ctx.memory(|m| m.has_focus(id));

    // Teclado antes do campo, para que ⬆ ⬇ Enter não cheguem ao TextEdit.
    if focused {
        if let Some(q) = cursor_of(&ctx, id).and_then(|c| catalog::query(text, c, kind)) {
            let sugs = catalog::suggestions(&q, vars);
            if !sugs.is_empty() {
                st.sel = st.sel.min(sugs.len() - 1);
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                    st.sel = (st.sel + 1) % sugs.len();
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                    st.sel = (st.sel + sugs.len() - 1) % sugs.len();
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                    let (t, at) = catalog::apply(text, q.word_start, q.cursor, &sugs[st.sel].insert);
                    *text = t;
                    set_cursor(&ctx, id, at);
                }
            }
        }
    }

    let mut te = egui::TextEdit::singleline(text).id(id).desired_width(width).hint_text(hint);
    if red {
        te = te.text_color(Color32::from_rgb(230, 60, 60));
    }
    let out = te.show(ui);
    let resp = out.response.response.clone();
    let focused = resp.has_focus();
    let cursor = out.cursor_range.map(|r| r.primary.index.0).or_else(|| cursor_of(&ctx, id));

    let mut ask_manual = false;
    let q = cursor.and_then(|c| catalog::query(text, c, kind));
    let show = (focused || st.hovered) && q.is_some();
    st.hovered = false;
    if let (true, Some(q)) = (show, q) {
        let sugs = catalog::suggestions(&q, vars);
        let err = if q.expr.trim().is_empty() { None } else { q.error() };
        if q.prefix != st.prefix {
            st.sel = 0;
            st.prefix = q.prefix.clone();
        }
        if !sugs.is_empty() || err.is_some() {
            let mut pick = None;
            let area = egui::Area::new(id.with("cola_area")).order(egui::Order::Foreground).fixed_pos(resp.rect.left_bottom()).movable(false).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(460.0);
                    ui.horizontal(|ui| {
                        match &err {
                            Some(e) => ui.colored_label(Color32::from_rgb(230, 60, 60), format!("⚠ {e}")),
                            None => ui.weak("⬆ ⬇ escolhem, Enter insere"),
                        };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("?").on_hover_text("Manual: expressões").clicked() {
                                ask_manual = true;
                            }
                        });
                    });
                    if sugs.is_empty() {
                        return;
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for (i, s) in sugs.iter().enumerate() {
                            let mut label = RichText::new(&s.label).monospace();
                            if s.kind == SugKind::Var {
                                label = label.strong();
                            }
                            let r = ui.selectable_label(i == st.sel, label);
                            if i == st.sel && focused {
                                r.scroll_to_me(None);
                            }
                            if r.on_hover_text(&s.desc).clicked() {
                                pick = Some(i);
                            }
                            ui.weak(&s.desc);
                        }
                    });
                });
            });
            st.hovered = area.response.contains_pointer();
            if let Some(i) = pick {
                let (t, at) = catalog::apply(text, q.word_start, q.cursor, &sugs[i].insert);
                *text = t;
                set_cursor(&ctx, id, at);
                ctx.memory_mut(|m| m.request_focus(id));
                ctx.request_repaint();
                st.hovered = false;
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(sid, st));
    (resp, ask_manual)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    /// Roda o campo num egui sem janela: foca, manda os eventos de cada quadro e devolve o texto.
    fn drive(kind: FieldKind, frames: Vec<Vec<egui::Event>>) -> String {
        let ctx = egui::Context::default();
        let id = Id::unique("campo");
        let mut text = String::new();
        let vars = vec!["contador".to_string()];
        let frame = |events: Vec<egui::Event>, text: &mut String| {
            let raw = egui::RawInput { events, ..Default::default() };
            ctx.run_ui(raw, |ui| {
                field(ui, id, text, 200.0, kind, &vars, "", false);
            })
            .textures_delta
            .clear();
        };
        frame(vec![], &mut text);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(vec![], &mut text);
        for evs in frames {
            frame(evs, &mut text);
        }
        text
    }

    fn typed(s: &str) -> Vec<egui::Event> {
        vec![egui::Event::Text(s.into())]
    }

    #[test]
    fn enter_inserts_suggestion() {
        assert_eq!(drive(FieldKind::Expr, vec![typed("x + ra"), vec![key(Key::Enter)]]), "x + rand(");
        // ⬇ escolhe a segunda sugestão: "con" -> contador, contem(
        assert_eq!(drive(FieldKind::Expr, vec![typed("con"), vec![key(Key::ArrowDown)], vec![key(Key::Enter)]]), "contem(");
        // depois de inserir, o cursor fica após o "(": dá para continuar digitando
        assert_eq!(drive(FieldKind::Expr, vec![typed("ra"), vec![key(Key::Enter)], typed("1, 2)")]), "rand(1, 2)");
    }

    #[test]
    fn text_fields_only_inside_braces() {
        assert_eq!(drive(FieldKind::Text, vec![typed("n = {ra"), vec![key(Key::Enter)]]), "n = {rand(");
        assert_eq!(drive(FieldKind::Text, vec![typed("ra"), vec![key(Key::Enter)]]), "ra", "fora das chaves a cola não age");
    }
}
