//! Block editor widgets: colored cards with a drag handle, drop slots inside containers
//! and the drop indicator. The tree changes themselves live in `tree.rs`.

use crate::model::Block;
use crate::runner;
use crate::tree::{self, Category, Pos, Slot};
use eframe::egui::{self, Color32, Id, LayerId, Order, Pos2, Rangef, Rect, RichText, Sense, Stroke, Ui};
use std::sync::Arc;

/// Drag-and-drop payload of the block editor.
#[derive(Clone, Debug)]
pub enum Drag {
    /// An existing block, at its position when the drag started.
    Move(Pos),
    /// A new block from the palette.
    New(Block),
}

enum Op {
    Move(Pos, Pos),
    Insert(Pos, Block),
    Wrap(Pos, Block),
    Up(Pos),
    Down(Pos),
    Dup(Pos),
    Del(Pos),
    Explode(Pos),
}

/// Card colors per category: (fill, accent), readable in both themes.
pub fn colors(c: Category, dark: bool) -> (Color32, Color32) {
    let rgb = Color32::from_rgb;
    match (c, dark) {
        (Category::Action, false) => (rgb(0xE3, 0xEE, 0xFC), rgb(0x2F, 0x6F, 0xC9)),
        (Category::Action, true) => (rgb(0x1E, 0x2C, 0x40), rgb(0x5B, 0x9B, 0xEA)),
        (Category::Wait, false) => (rgb(0xFD, 0xF1, 0xDC), rgb(0xB3, 0x77, 0x08)),
        (Category::Wait, true) => (rgb(0x3A, 0x2F, 0x17), rgb(0xE0, 0xA6, 0x3A)),
        (Category::Control, false) => (rgb(0xEF, 0xE6, 0xFB), rgb(0x7E, 0x4A, 0xC9)),
        (Category::Control, true) => (rgb(0x2E, 0x24, 0x40), rgb(0xA7, 0x7F, 0xE6)),
        (Category::Vars, false) => (rgb(0xE3, 0xF5, 0xE6), rgb(0x2E, 0x8A, 0x43)),
        (Category::Vars, true) => (rgb(0x1D, 0x33, 0x24), rgb(0x5C, 0xC0, 0x74)),
        (Category::Debug, false) => (rgb(0xFB, 0xE6, 0xEA), rgb(0xB8, 0x3A, 0x52)),
        (Category::Debug, true) => (rgb(0x3A, 0x1F, 0x25), rgb(0xE0, 0x6B, 0x80)),
    }
}

/// Where the dragged block would land, and where to draw the indicator.
struct Target {
    to: Pos,
    y: f32,
    x: Rangef,
    ok: bool,
}

struct State<'a> {
    drag: Option<Arc<Drag>>,
    pointer: Option<Pos2>,
    target: Option<Target>,
    op: Option<Op>,
    dark: bool,
    fields: &'a mut dyn FnMut(&mut Ui, &mut Block, Id) -> bool,
    out: Output,
}

/// What the editor saw this frame (used by the tests to aim the pointer).
#[derive(Default, Debug)]
pub struct Output {
    /// Drag handle of every card.
    pub handles: Vec<(Pos, Rect)>,
    /// Every list (top level and container slots), by path.
    pub lists: Vec<(Vec<(usize, Slot)>, Rect)>,
    /// Current drop spot and whether it is allowed.
    pub target: Option<(Pos, bool)>,
    /// Collapse/expand button of every container card.
    pub toggles: Vec<(Pos, Rect)>,
    /// Every card.
    pub cards: Vec<(Pos, Rect)>,
    /// Items of the open context menu.
    pub menu: Vec<(String, Rect)>,
}

/// The block tree of one script. `fields` draws a block's own fields and returns true
/// when its "Explodir" button was clicked.
pub fn tree_ui(ui: &mut Ui, blocks: &mut Vec<Block>, id: Id, fields: &mut dyn FnMut(&mut Ui, &mut Block, Id) -> bool) -> Output {
    let ctx = ui.ctx().clone();
    let mut st = State {
        drag: egui::DragAndDrop::payload::<Drag>(&ctx),
        pointer: ui.input(|i| i.pointer.latest_pos()),
        target: None,
        op: None,
        dark: ui.visuals().dark_mode,
        fields,
        out: Output::default(),
    };
    list_ui(ui, blocks, &[], id, &mut st);

    if let Some(drag) = st.drag.clone() {
        paint_drag(ui, blocks, &drag, &st, id);
        if ui.input(|i| i.pointer.any_released()) {
            if let Some(t) = st.target.take().filter(|t| t.ok) {
                st.op = Some(match &*drag {
                    Drag::Move(from) => Op::Move(from.clone(), t.to),
                    Drag::New(b) => Op::Insert(t.to, b.clone()),
                });
            }
            egui::DragAndDrop::clear_payload(&ctx);
        }
    }
    if let Some(op) = st.op.take() {
        apply(blocks, op);
    }
    st.out
}

fn apply(blocks: &mut Vec<Block>, op: Op) {
    match op {
        Op::Move(from, to) => {
            tree::move_block(blocks, &from, &to);
        }
        Op::Insert(to, b) => {
            tree::insert(blocks, &to, b);
        }
        Op::Wrap(p, container) => {
            tree::wrap(blocks, &p, container);
        }
        Op::Up(p) if p.index > 0 => {
            tree::move_block(blocks, &p, &Pos::new(&p.list, p.index - 1));
        }
        Op::Down(p) => {
            tree::move_block(blocks, &p, &Pos::new(&p.list, p.index + 2));
        }
        Op::Dup(p) => {
            tree::duplicate(blocks, &p);
        }
        Op::Del(p) => {
            tree::remove(blocks, &p);
        }
        Op::Explode(p) => {
            if let Some(Block::Recording { events, .. }) = tree::get(blocks, &p) {
                let new = runner::explode(events);
                tree::splice(blocks, &p, new);
            }
        }
        Op::Up(_) => {}
    }
}

fn slot_salt(s: Slot) -> &'static str {
    match s {
        Slot::Then => "then",
        Slot::Else => "else",
        Slot::Body => "body",
    }
}

fn list_ui(ui: &mut Ui, blocks: &mut Vec<Block>, path: &[(usize, Slot)], id: Id, st: &mut State) {
    let mut rects = Vec::with_capacity(blocks.len());
    let resp = ui
        .vertical(|ui| {
            if blocks.is_empty() {
                let weak = ui.visuals().weak_text_color();
                egui::Frame::new().stroke(Stroke::new(1.0, weak)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                    ui.weak(if path.is_empty() { "Sem blocos: arraste um da paleta para cá (ou clique nele), ou grave uma macro" } else { "solte blocos aqui" });
                });
            }
            for i in 0..blocks.len() {
                let pos = Pos::new(path, i);
                let r = card_ui(ui, &mut blocks[i], &pos, id.with(i), st);
                st.out.cards.push((pos, r));
                rects.push(r);
            }
        })
        .response;
    st.out.lists.push((path.to_vec(), resp.rect));

    // Inner lists finish first, so the innermost list under the pointer wins.
    let (Some(drag), Some(p), None) = (&st.drag, st.pointer, &st.target) else { return };
    let gap = ui.spacing().item_spacing.y;
    let area = if path.is_empty() { ui.clip_rect() } else { resp.rect.expand2(egui::vec2(0.0, gap)) };
    if !area.contains(p) || !ui.clip_rect().contains(p) {
        return;
    }
    let index = rects.iter().position(|r| p.y < r.center().y).unwrap_or(rects.len());
    let y = match (index, rects.last()) {
        (_, None) => resp.rect.top() + gap,
        (0, _) => rects[0].top() - gap / 2.0,
        (i, _) => rects[i - 1].bottom() + gap / 2.0,
    };
    let to = Pos::new(path, index);
    let ok = match &**drag {
        Drag::Move(from) => tree::can_drop(from, &to),
        Drag::New(_) => true,
    };
    st.out.target = Some((to.clone(), ok));
    st.target = Some(Target { to, y, x: resp.rect.x_range(), ok });
}

fn card_ui(ui: &mut Ui, b: &mut Block, pos: &Pos, bid: Id, st: &mut State) -> Rect {
    let cat = tree::category(b);
    let (fill, accent) = colors(cat, st.dark);
    let dragged = matches!(st.drag.as_deref(), Some(Drag::Move(p)) if p == pos);
    let is_container = !tree::slots(b).is_empty();
    // View-only state: kept in egui memory, never in the script.
    let collapsed_id = bid.with("collapsed");
    let mut collapsed = is_container && ui.data(|d| d.get_temp::<bool>(collapsed_id)).unwrap_or(false);
    ui.push_id(pos.index, |ui| {
        if dragged {
            ui.set_opacity(0.4);
        }
        // The card's background senses right clicks; the widgets inside it still get theirs first.
        let card = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
            egui::Frame::group(ui.style()).fill(fill).stroke(Stroke::new(1.0, accent)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let h = ui
                        .add(egui::Label::new(RichText::new("☰").strong().color(accent)).selectable(false).sense(Sense::drag()))
                        .on_hover_text(format!("{}: arraste para mover; botão direito no cartão para mais opções", cat.label()));
                    h.dnd_set_drag_payload(Drag::Move(pos.clone()));
                    st.out.handles.push((pos.clone(), h.rect));
                    if is_container {
                        let t = ui.small_button(if collapsed { "⏵" } else { "⏷" }).on_hover_text(if collapsed { "Expandir" } else { "Recolher" });
                        st.out.toggles.push((pos.clone(), t.rect));
                        if t.clicked() {
                            collapsed = !collapsed;
                            ui.data_mut(|d| d.insert_temp(collapsed_id, collapsed));
                        }
                    }
                    if ui.small_button("⬆").on_hover_text("Subir").clicked() {
                        st.op = Some(Op::Up(pos.clone()));
                    }
                    if ui.small_button("⬇").on_hover_text("Descer").clicked() {
                        st.op = Some(Op::Down(pos.clone()));
                    }
                    if ui.small_button("📋").on_hover_text("Duplicar").clicked() {
                        st.op = Some(Op::Dup(pos.clone()));
                    }
                    if ui.small_button("🗑").on_hover_text("Excluir").clicked() {
                        st.op = Some(Op::Del(pos.clone()));
                    }
                    ui.add(egui::Label::new(RichText::new(b.title()).strong().color(accent)).selectable(false));
                    if (st.fields)(ui, b, bid) {
                        st.op = Some(Op::Explode(pos.clone()));
                    }
                    if collapsed {
                        let n = tree::count_inside(b);
                        ui.weak(if n == 1 { "(1 bloco dentro)".to_string() } else { format!("({n} blocos dentro)") });
                    }
                });
                if collapsed {
                    return;
                }
                for &s in tree::slots(b) {
                    let child = pos.child(s);
                    ui.indent(slot_salt(s), |ui| {
                        ui.weak(s.label());
                        if let Some(l) = tree::slot_mut(b, s) {
                            list_ui(ui, l, &child, bid.with(slot_salt(s)), st);
                        }
                    });
                }
            });
        });
        card.response.context_menu(|ui| {
            for c in Block::templates().into_iter().filter(|c| !tree::slots(c).is_empty()) {
                let label = format!("Envolver em {}", c.title());
                let r = ui.button(&label);
                st.out.menu.push((label, r.rect));
                if r.clicked() {
                    st.op = Some(Op::Wrap(pos.clone(), c));
                }
            }
            ui.separator();
            if ui.button("📋 Duplicar").clicked() {
                st.op = Some(Op::Dup(pos.clone()));
            }
            if ui.button("🗑 Excluir").clicked() {
                st.op = Some(Op::Del(pos.clone()));
            }
        });
        card.response.rect
    })
    .inner
}

/// Every block type, grouped by category. Dragging an item drops a new block into the
/// tree; clicking it appends one at the end. Returns each item's title and rect.
pub fn palette_ui(ui: &mut Ui, blocks: &mut Vec<Block>) -> Vec<(&'static str, Rect)> {
    let dark = ui.visuals().dark_mode;
    let width = ui.available_width();
    let templates = Block::templates();
    let mut items = Vec::with_capacity(templates.len());
    for cat in Category::ALL {
        let (fill, accent) = colors(cat, dark);
        ui.label(RichText::new(cat.label()).strong().color(accent));
        for t in templates.iter().filter(|t| tree::category(t) == cat) {
            let r = ui
                .add(egui::Button::new(RichText::new(t.title()).color(accent)).fill(fill).stroke(Stroke::new(1.0, accent)).min_size(egui::vec2(width, 0.0)).sense(Sense::click_and_drag()))
                .on_hover_text("Arraste para a árvore ou clique para pôr no fim");
            r.dnd_set_drag_payload(Drag::New(t.clone()));
            if r.clicked() {
                blocks.push(t.clone());
            }
            items.push((t.title(), r.rect));
        }
        ui.add_space(6.0);
    }
    items
}

/// Drop indicator, and the dragged block's name next to the pointer.
fn paint_drag(ui: &Ui, blocks: &[Block], drag: &Drag, st: &State, id: Id) {
    let Some(p) = st.pointer else { return };
    let painter = ui.ctx().layer_painter(LayerId::new(Order::Foreground, id.with("dnd")));
    let red = Color32::from_rgb(220, 50, 50);
    if let Some(t) = &st.target {
        let color = if t.ok { ui.visuals().selection.stroke.color } else { red };
        painter.hline(t.x, t.y, Stroke::new(3.0, color));
        painter.circle_filled(Pos2::new(t.x.min, t.y), 4.0, color);
    }
    let title = match drag {
        Drag::Move(from) => tree::get(blocks, from).map_or("bloco", |b| b.title()),
        Drag::New(b) => b.title(),
    };
    let text = match &st.target {
        Some(t) if !t.ok => format!("{title}: não pode soltar dentro dele mesmo"),
        _ => title.to_string(),
    };
    let font = egui::TextStyle::Body.resolve(ui.style());
    let at = p + egui::vec2(14.0, 10.0);
    let galley = painter.layout_no_wrap(text, font, ui.visuals().strong_text_color());
    let rect = Rect::from_min_size(at, galley.size()).expand(4.0);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, if st.target.as_ref().is_some_and(|t| !t.ok) { red } else { ui.visuals().weak_text_color() }), egui::StrokeKind::Outside);
    painter.galley(at, galley, Color32::PLACEHOLDER);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Condition;
    use egui::{Event, PointerButton, RawInput, vec2};

    fn log(s: &str) -> Block {
        Block::Log { text: s.into() }
    }

    /// Drives the real editor in a headless egui context with synthetic mouse events.
    struct Harness {
        ctx: egui::Context,
        blocks: Vec<Block>,
        out: Output,
        palette: Vec<(&'static str, Rect)>,
        frames: f64,
    }

    impl Harness {
        fn new(blocks: Vec<Block>) -> Self {
            let mut h = Harness { ctx: egui::Context::default(), blocks, out: Output::default(), palette: vec![], frames: 0.0 };
            h.frame(vec![]);
            h.frame(vec![]);
            h
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.frames += 1.0;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 2000.0))),
                time: Some(self.frames / 60.0),
                events,
                ..Default::default()
            };
            let (ctx, blocks, out, palette) = (&self.ctx, &mut self.blocks, &mut self.out, &mut self.palette);
            ctx.run_ui(input, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(160.0);
                        *palette = palette_ui(ui, blocks);
                    });
                    ui.vertical(|ui| {
                        *out = tree_ui(ui, blocks, Id::unique("t"), &mut |ui, b, _| {
                            ui.label(b.summary());
                            false
                        });
                    });
                });
            })
            .textures_delta
            .clear();
        }

        fn handle(&self, pos: &Pos) -> Pos2 {
            self.out.handles.iter().find(|(p, _)| p == pos).expect("handle").1.center()
        }

        fn list(&self, path: &[(usize, Slot)]) -> Rect {
            self.out.lists.iter().find(|(p, _)| p == path).expect("list").1
        }

        /// A point on the upper half of a card (its handle row).
        fn card_top(&self, pos: &Pos) -> Pos2 {
            self.handle(pos) - vec2(0.0, 4.0)
        }

        fn palette_item(&self, title: &str) -> Pos2 {
            self.palette.iter().find(|(t, _)| *t == title).expect("palette item").1.center()
        }

        fn click(&mut self, at: Pos2) {
            let btn = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![btn(true)]);
            self.frame(vec![btn(false)]);
            self.frame(vec![]);
        }

        /// Press on `from`'s handle, drag to `to`, and return the target seen just before releasing.
        fn drag(&mut self, from: &Pos, to: Pos2) -> Option<(Pos, bool)> {
            let start = self.handle(from);
            self.drag_from(start, to)
        }

        fn drag_from(&mut self, start: Pos2, to: Pos2) -> Option<(Pos, bool)> {
            let btn = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
            self.frame(vec![Event::PointerMoved(start)]);
            self.frame(vec![btn(start, true)]);
            self.frame(vec![Event::PointerMoved(start + vec2(0.0, 12.0))]);
            self.frame(vec![Event::PointerMoved(to)]);
            self.frame(vec![Event::PointerMoved(to)]);
            let seen = self.out.target.clone();
            self.frame(vec![btn(to, false)]);
            self.frame(vec![Event::PointerMoved(to + vec2(1.0, 0.0))]);
            seen
        }
    }

    fn titles(l: &[Block]) -> Vec<String> {
        l.iter()
            .map(|b| match b {
                Block::Log { text } => text.clone(),
                o => o.title().into(),
            })
            .collect()
    }

    #[test]
    fn drag_within_same_list_lands_at_indicator() {
        let mut h = Harness::new(vec![log("a"), log("b"), log("c"), log("d")]);
        // a dropped on the upper half of d: indicator between c and d.
        let to = h.card_top(&Pos::new(&[], 3));
        assert_eq!(h.drag(&Pos::new(&[], 0), to), Some((Pos::new(&[], 3), true)));
        assert_eq!(titles(&h.blocks), ["b", "c", "a", "d"]);
        // d dropped on the upper half of b (now first): goes to the top.
        let to = h.card_top(&Pos::new(&[], 0));
        assert_eq!(h.drag(&Pos::new(&[], 3), to), Some((Pos::new(&[], 0), true)));
        assert_eq!(titles(&h.blocks), ["d", "b", "c", "a"]);
    }

    #[test]
    fn drag_into_empty_slots_and_back_out() {
        let rep = Block::Repeat { times: "2".into(), body: vec![] };
        let iff = Block::If { cond: Condition::default(), then: vec![], else_: vec![] };
        let mut h = Harness::new(vec![log("a"), rep, iff, log("b")]);
        let body = [(1, Slot::Body)];
        let to = h.list(&body).center();
        assert_eq!(h.drag(&Pos::new(&[], 0), to), Some((Pos::new(&body, 0), true)));
        assert_eq!(titles(&h.blocks), ["Repetir", "Se", "b"]);
        assert_eq!(titles(tree::slot(&h.blocks[0], Slot::Body).unwrap()), ["a"]);

        let then = [(1, Slot::Then)];
        let to = h.list(&then).center();
        assert_eq!(h.drag(&Pos::new(&[], 2), to), Some((Pos::new(&then, 0), true)));
        let body = [(0, Slot::Body)];
        let els = [(1, Slot::Else)];
        let to = h.list(&els).center();
        assert_eq!(h.drag(&Pos::new(&body, 0), to), Some((Pos::new(&els, 0), true)));
        assert_eq!(titles(tree::slot(&h.blocks[1], Slot::Then).unwrap()), ["b"]);
        assert_eq!(titles(tree::slot(&h.blocks[1], Slot::Else).unwrap()), ["a"]);
        assert!(tree::slot(&h.blocks[0], Slot::Body).unwrap().is_empty());

        // Back to the top level, before Repetir.
        let to = h.card_top(&Pos::new(&[], 0));
        assert_eq!(h.drag(&Pos::new(&els, 0), to), Some((Pos::new(&[], 0), true)));
        assert_eq!(titles(&h.blocks), ["a", "Repetir", "Se"]);
    }

    #[test]
    fn drag_container_into_itself_is_refused() {
        let inner = Block::If { cond: Condition::default(), then: vec![], else_: vec![] };
        let blocks = vec![log("a"), Block::Repeat { times: "2".into(), body: vec![inner] }];
        let mut h = Harness::new(blocks.clone());
        let then = [(1, Slot::Body), (0, Slot::Then)];
        let to = h.list(&then).center();
        assert_eq!(h.drag(&Pos::new(&[], 1), to), Some((Pos::new(&then, 0), false)));
        assert_eq!(h.blocks, blocks);
        // Into its own body, below the inner Se.
        let to = h.list(&[(1, Slot::Body)]).left_bottom() + vec2(30.0, -1.0);
        assert_eq!(h.drag(&Pos::new(&[], 1), to), Some((Pos::new(&[(1, Slot::Body)], 1), false)));
        assert_eq!(h.blocks, blocks);
    }

    #[test]
    fn card_actions_work_on_nested_blocks() {
        let rec = Block::Recording { events: vec![], speed: 1.0 };
        let mut t = vec![Block::Repeat { times: "2".into(), body: vec![log("a"), rec] }];
        let body = [(0, Slot::Body)];
        apply(&mut t, Op::Dup(Pos::new(&body, 0)));
        apply(&mut t, Op::Explode(Pos::new(&body, 2)));
        assert_eq!(titles(tree::slot(&t[0], Slot::Body).unwrap()), ["a", "a"]);
        apply(&mut t, Op::Del(Pos::new(&body, 1)));
        apply(&mut t, Op::Down(Pos::new(&body, 0)));
        assert_eq!(titles(tree::slot(&t[0], Slot::Body).unwrap()), ["a"]);
    }

    #[test]
    fn palette_lists_every_block_type_by_category() {
        let h = Harness::new(vec![]);
        let all = Block::templates();
        assert_eq!(h.palette.len(), all.len());
        for t in &all {
            assert!(h.palette.iter().any(|(p, _)| *p == t.title()), "{}", t.title());
        }
        // Items come grouped: the category order never goes back.
        let cat_of = |title: &str| all.iter().find(|t| t.title() == title).map(tree::category).unwrap();
        let order: Vec<usize> = h.palette.iter().map(|(t, _)| Category::ALL.iter().position(|c| *c == cat_of(t)).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "{order:?}");
    }

    #[test]
    fn palette_click_appends_and_drag_inserts_into_nested_empty_slot() {
        let iff = Block::If { cond: Condition::default(), then: vec![Block::Repeat { times: "2".into(), body: vec![] }], else_: vec![] };
        let mut h = Harness::new(vec![iff]);
        let at = h.palette_item("Esperar");
        h.click(at);
        assert_eq!(titles(&h.blocks), ["Se", "Esperar"]);

        let body = [(0, Slot::Then), (0, Slot::Body)];
        let (from, to) = (h.palette_item("Mostrar no log"), h.list(&body).center());
        assert_eq!(h.drag_from(from, to), Some((Pos::new(&body, 0), true)));
        let rep = &tree::slot(&h.blocks[0], Slot::Then).unwrap()[0];
        assert_eq!(tree::slot(rep, Slot::Body).unwrap().len(), 1);
        assert!(matches!(tree::slot(rep, Slot::Body).unwrap()[0], Block::Log { .. }));
        assert_eq!(titles(&h.blocks), ["Se", "Esperar"]);
    }

    #[test]
    fn collapse_only_changes_the_view() {
        let blocks = vec![Block::Repeat { times: "2".into(), body: vec![log("a"), Block::While { cond: Condition::default(), body: vec![log("b")] }] }];
        let json = serde_json::to_string(&blocks).unwrap();
        let mut h = Harness::new(blocks);
        let rep = Pos::new(&[], 0);
        let toggle = |h: &Harness| h.out.toggles.iter().find(|(p, _)| *p == rep).unwrap().1.center();
        assert!(h.out.lists.iter().any(|(p, _)| p == &[(0, Slot::Body)]));
        h.click(toggle(&h));
        // Collapsed: the body isn't drawn (so it isn't a drop slot either), the tree is the same.
        assert!(!h.out.lists.iter().any(|(p, _)| p == &[(0, Slot::Body)]));
        assert_eq!(serde_json::to_string(&h.blocks).unwrap(), json);
        h.click(toggle(&h));
        assert!(h.out.lists.iter().any(|(p, _)| p == &[(0, Slot::Body)]));
        assert_eq!(serde_json::to_string(&h.blocks).unwrap(), json);
    }

    #[test]
    fn wrap_op_on_nested_block() {
        let mut t = vec![Block::Repeat { times: "2".into(), body: vec![log("a"), log("b")] }];
        let iff = Block::If { cond: Condition::default(), then: vec![], else_: vec![] };
        apply(&mut t, Op::Wrap(Pos::new(&[(0, Slot::Body)], 1), iff));
        let body = tree::slot(&t[0], Slot::Body).unwrap();
        assert_eq!(titles(body), ["a", "Se"]);
        assert_eq!(titles(tree::slot(&body[1], Slot::Then).unwrap()), ["b"]);
    }

    #[test]
    fn context_menu_wraps_in_place() {
        let mut h = Harness::new(vec![log("a"), log("b"), log("c")]);
        let card = h.out.cards.iter().find(|(p, _)| *p == Pos::new(&[], 1)).unwrap().1;
        // Right click on the card's background (right end of the card, past its widgets).
        let at = card.right_center() - vec2(6.0, 0.0);
        let rbtn = |pressed| Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Default::default() };
        h.frame(vec![Event::PointerMoved(at)]);
        h.frame(vec![rbtn(true)]);
        h.frame(vec![rbtn(false)]);
        h.frame(vec![]);
        let labels: Vec<&str> = h.out.menu.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["Envolver em Se", "Envolver em Repetir", "Envolver em Enquanto"]);
        let item = h.out.menu[1].1.center();
        h.click(item);
        assert_eq!(titles(&h.blocks), ["a", "Repetir", "c"]);
        assert_eq!(titles(tree::slot(&h.blocks[1], Slot::Body).unwrap()), ["b"]);
    }
}
