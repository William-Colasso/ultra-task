//! Pure operations on the block tree (move, insert, wrap…), used by the block editor.
//! Nothing here touches egui, so every operation is unit-tested.

use crate::model::Block;

/// A child list of a container block.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Slot {
    /// "então" of Se.
    Then,
    /// "senão" of Se.
    Else,
    /// Body of Repetir / Enquanto.
    Body,
}

impl Slot {
    pub fn label(self) -> &'static str {
        match self {
            Slot::Then => "então:",
            Slot::Else => "senão:",
            Slot::Body => "faça:",
        }
    }
}

/// Where a block sits, or where one will be inserted: the chain of (index, slot) from
/// the top-level list down to the list, plus the index inside that list
/// (`index == len` means "at the end").
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Pos {
    pub list: Vec<(usize, Slot)>,
    pub index: usize,
}

impl Pos {
    pub fn new(list: &[(usize, Slot)], index: usize) -> Self {
        Pos { list: list.to_vec(), index }
    }

    /// The path of the child list `slot` of the block at `self`.
    pub fn child(&self, slot: Slot) -> Vec<(usize, Slot)> {
        let mut l = self.list.clone();
        l.push((self.index, slot));
        l
    }
}

/// The child lists a block has, in display order.
pub fn slots(b: &Block) -> &'static [Slot] {
    match b {
        Block::If { .. } => &[Slot::Then, Slot::Else],
        Block::Repeat { .. } | Block::While { .. } => &[Slot::Body],
        _ => &[],
    }
}

pub fn slot(b: &Block, s: Slot) -> Option<&Vec<Block>> {
    match (b, s) {
        (Block::If { then, .. }, Slot::Then) => Some(then),
        (Block::If { else_, .. }, Slot::Else) => Some(else_),
        (Block::Repeat { body, .. } | Block::While { body, .. }, Slot::Body) => Some(body),
        _ => None,
    }
}

pub fn slot_mut(b: &mut Block, s: Slot) -> Option<&mut Vec<Block>> {
    match (b, s) {
        (Block::If { then, .. }, Slot::Then) => Some(then),
        (Block::If { else_, .. }, Slot::Else) => Some(else_),
        (Block::Repeat { body, .. } | Block::While { body, .. }, Slot::Body) => Some(body),
        _ => None,
    }
}

fn list<'a>(root: &'a [Block], path: &[(usize, Slot)]) -> Option<&'a [Block]> {
    let mut cur = root;
    for &(i, s) in path {
        cur = slot(cur.get(i)?, s)?;
    }
    Some(cur)
}

fn list_mut<'a>(root: &'a mut Vec<Block>, path: &[(usize, Slot)]) -> Option<&'a mut Vec<Block>> {
    let mut cur = root;
    for &(i, s) in path {
        cur = slot_mut(cur.get_mut(i)?, s)?;
    }
    Some(cur)
}

pub fn get<'a>(root: &'a [Block], pos: &Pos) -> Option<&'a Block> {
    list(root, &pos.list)?.get(pos.index)
}

/// True when the list `target` is the block at `block` itself or lies inside it.
pub fn is_inside(target: &[(usize, Slot)], block: &Pos) -> bool {
    let n = block.list.len();
    target.len() > n && target[..n] == block.list[..] && target[n].0 == block.index
}

/// Can the block at `from` be dropped at `to`? A block can't go inside itself.
pub fn can_drop(from: &Pos, to: &Pos) -> bool {
    !is_inside(&to.list, from)
}

/// Inserts `b` at `to`. Returns false (and changes nothing) if `to` doesn't exist.
pub fn insert(root: &mut Vec<Block>, to: &Pos, b: Block) -> bool {
    match list_mut(root, &to.list) {
        Some(l) if to.index <= l.len() => {
            l.insert(to.index, b);
            true
        }
        _ => false,
    }
}

pub fn remove(root: &mut Vec<Block>, pos: &Pos) -> Option<Block> {
    let l = list_mut(root, &pos.list)?;
    (pos.index < l.len()).then(|| l.remove(pos.index))
}

/// Replaces the block at `pos` with `new` (used by "Explodir").
pub fn splice(root: &mut Vec<Block>, pos: &Pos, new: Vec<Block>) -> bool {
    match list_mut(root, &pos.list) {
        Some(l) if pos.index < l.len() => {
            l.splice(pos.index..=pos.index, new);
            true
        }
        _ => false,
    }
}

/// Inserts a copy of the block at `pos` right after it.
pub fn duplicate(root: &mut Vec<Block>, pos: &Pos) -> bool {
    let Some(b) = get(root, pos).cloned() else { return false };
    insert(root, &Pos::new(&pos.list, pos.index + 1), b)
}

/// Moves the block at `from` so that it lands where the drop indicator at `to` showed,
/// `to` being read in the tree *before* the move. Returns whether the tree changed:
/// dropping a block inside itself, at its own place, or at a missing spot changes nothing.
pub fn move_block(root: &mut Vec<Block>, from: &Pos, to: &Pos) -> bool {
    if !can_drop(from, to) || get(root, from).is_none() {
        return false;
    }
    match list(root, &to.list) {
        Some(l) if to.index <= l.len() => {}
        _ => return false,
    }
    if to.list == from.list && (to.index == from.index || to.index == from.index + 1) {
        return false;
    }
    let b = remove(root, from).expect("checked above");
    // Removing `from` shifts the later siblings, and anything under them, up by one.
    let mut to = to.clone();
    let n = from.list.len();
    if to.list == from.list && to.index > from.index {
        to.index -= 1;
    } else if to.list.len() > n && to.list[..n] == from.list[..] && to.list[n].0 > from.index {
        to.list[n].0 -= 1;
    }
    insert(root, &to, b)
}

/// Puts the block at `pos` inside `container` (at the start of its first child list),
/// and the container where the block was.
pub fn wrap(root: &mut Vec<Block>, pos: &Pos, mut container: Block) -> bool {
    let Some(&first) = slots(&container).first() else { return false };
    let Some(l) = list_mut(root, &pos.list) else { return false };
    if pos.index >= l.len() {
        return false;
    }
    let inner = std::mem::replace(&mut l[pos.index], Block::Stop);
    slot_mut(&mut container, first).expect("slots() lists it").insert(0, inner);
    l[pos.index] = container;
    true
}

/// How many blocks there are inside `b`, at any depth.
pub fn count_inside(b: &Block) -> usize {
    slots(b).iter().filter_map(|&s| slot(b, s)).flatten().map(|c| 1 + count_inside(c)).sum()
}

/// Editor grouping (palette sections and card colors).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Category {
    Action,
    Wait,
    Control,
    Vars,
    Debug,
}

impl Category {
    pub const ALL: [Category; 5] = [Category::Action, Category::Wait, Category::Control, Category::Vars, Category::Debug];

    pub fn label(self) -> &'static str {
        match self {
            Category::Action => "Ação",
            Category::Wait => "Espera",
            Category::Control => "Controle",
            Category::Vars => "Variáveis",
            Category::Debug => "Depuração",
        }
    }
}

pub fn category(b: &Block) -> Category {
    match b {
        Block::Wait { .. } | Block::WaitUntil { .. } => Category::Wait,
        Block::If { .. } | Block::Repeat { .. } | Block::While { .. } | Block::Call { .. } | Block::Stop => Category::Control,
        Block::SetVar { .. } | Block::OcrToVar { .. } => Category::Vars,
        Block::Log { .. } | Block::Breakpoint => Category::Debug,
        // Recording, Move, Click, Key, Type, Scroll, and blocks added later default to "ação".
        _ => Category::Action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Condition;

    fn log(s: &str) -> Block {
        Block::Log { text: s.into() }
    }
    fn rep(body: Vec<Block>) -> Block {
        Block::Repeat { times: "2".into(), body }
    }
    fn iff(then: Vec<Block>, else_: Vec<Block>) -> Block {
        Block::If { cond: Condition::default(), then, else_ }
    }
    fn top(i: usize) -> Pos {
        Pos::new(&[], i)
    }
    fn names(l: &[Block]) -> Vec<String> {
        l.iter()
            .map(|b| match b {
                Block::Log { text } => text.clone(),
                other => other.title().into(),
            })
            .collect()
    }

    #[test]
    fn move_forward_in_same_list() {
        let mut t = vec![log("a"), log("b"), log("c"), log("d")];
        // Indicator between c and d (index 3): a lands right before d.
        assert!(move_block(&mut t, &top(0), &top(3)));
        assert_eq!(names(&t), ["b", "c", "a", "d"]);
        // To the end.
        assert!(move_block(&mut t, &top(0), &top(4)));
        assert_eq!(names(&t), ["c", "a", "d", "b"]);
    }

    #[test]
    fn move_backward_in_same_list() {
        let mut t = vec![log("a"), log("b"), log("c"), log("d")];
        assert!(move_block(&mut t, &top(3), &top(1)));
        assert_eq!(names(&t), ["a", "d", "b", "c"]);
        assert!(move_block(&mut t, &top(2), &top(0)));
        assert_eq!(names(&t), ["b", "a", "d", "c"]);
    }

    #[test]
    fn move_to_own_place_is_noop() {
        let mut t = vec![log("a"), log("b"), log("c")];
        let before = t.clone();
        assert!(!move_block(&mut t, &top(1), &top(1)));
        assert!(!move_block(&mut t, &top(1), &top(2)));
        assert_eq!(t, before);
    }

    #[test]
    fn move_into_empty_child_slots() {
        let mut t = vec![log("a"), rep(vec![]), iff(vec![], vec![]), log("b")];
        // a -> body of Repetir (Repetir is at index 1 before the move).
        assert!(move_block(&mut t, &top(0), &Pos::new(&[(1, Slot::Body)], 0)));
        assert_eq!(names(&t), ["Repetir", "Se", "b"]);
        assert_eq!(names(slot(&t[0], Slot::Body).unwrap()), ["a"]);
        // b -> "então" of Se.
        assert!(move_block(&mut t, &top(2), &Pos::new(&[(1, Slot::Then)], 0)));
        assert_eq!(names(slot(&t[1], Slot::Then).unwrap()), ["b"]);
        // a (inside Repetir) -> "senão" of Se.
        assert!(move_block(&mut t, &Pos::new(&[(0, Slot::Body)], 0), &Pos::new(&[(1, Slot::Else)], 0)));
        assert!(slot(&t[0], Slot::Body).unwrap().is_empty());
        assert_eq!(names(slot(&t[1], Slot::Else).unwrap()), ["a"]);
    }

    #[test]
    fn move_into_slot_of_later_sibling_after_shift() {
        // Moving a block that sits *before* the container: the container's index shifts.
        let mut t = vec![log("x"), log("a"), rep(vec![log("in")])];
        assert!(move_block(&mut t, &top(1), &Pos::new(&[(2, Slot::Body)], 1)));
        assert_eq!(names(&t), ["x", "Repetir"]);
        assert_eq!(names(slot(&t[1], Slot::Body).unwrap()), ["in", "a"]);
    }

    #[test]
    fn move_out_to_parent() {
        let mut t = vec![log("a"), iff(vec![log("t1"), rep(vec![log("deep")])], vec![]), log("b")];
        // deep (Se.então[1].corpo[0]) -> top level, between Se and b.
        let deep = Pos::new(&[(1, Slot::Then), (1, Slot::Body)], 0);
        assert!(move_block(&mut t, &deep, &top(2)));
        assert_eq!(names(&t), ["a", "Se", "deep", "b"]);
        // t1 -> the very top.
        assert!(move_block(&mut t, &Pos::new(&[(1, Slot::Then)], 0), &top(0)));
        assert_eq!(names(&t), ["t1", "a", "Se", "deep", "b"]);
        assert_eq!(names(slot(&t[2], Slot::Then).unwrap()), ["Repetir"]);
    }

    #[test]
    fn refuses_drop_into_itself_or_descendant() {
        let mut t = vec![log("a"), rep(vec![iff(vec![log("x")], vec![])]), log("b")];
        let before = t.clone();
        let outer = top(1);
        for to in [
            Pos::new(&[(1, Slot::Body)], 0),
            Pos::new(&[(1, Slot::Body)], 1),
            Pos::new(&[(1, Slot::Body), (0, Slot::Then)], 1),
            Pos::new(&[(1, Slot::Body), (0, Slot::Else)], 0),
        ] {
            assert!(!can_drop(&outer, &to), "{to:?}");
            assert!(!move_block(&mut t, &outer, &to), "{to:?}");
        }
        // The inner Se can't go into its own "senão" either, but can go to the parent list.
        let inner = Pos::new(&[(1, Slot::Body)], 0);
        assert!(!move_block(&mut t, &inner, &Pos::new(&[(1, Slot::Body), (0, Slot::Else)], 0)));
        assert_eq!(t, before);
        assert!(can_drop(&inner, &top(0)));
        // A sibling's slot with the same index prefix is fine.
        assert!(can_drop(&top(0), &Pos::new(&[(1, Slot::Body)], 0)));
    }

    #[test]
    fn refuses_missing_spots() {
        let mut t = vec![log("a"), log("b")];
        let before = t.clone();
        assert!(!move_block(&mut t, &top(5), &top(0)));
        assert!(!move_block(&mut t, &top(0), &top(9)));
        assert!(!move_block(&mut t, &top(0), &Pos::new(&[(1, Slot::Body)], 0)));
        assert!(!insert(&mut t, &Pos::new(&[(0, Slot::Then)], 0), log("z")));
        assert_eq!(t, before);
    }

    #[test]
    fn insert_into_nested_empty_slot() {
        let mut t = vec![iff(vec![rep(vec![])], vec![])];
        assert!(insert(&mut t, &Pos::new(&[(0, Slot::Then), (0, Slot::Body)], 0), log("novo")));
        let r = &slot(&t[0], Slot::Then).unwrap()[0];
        assert_eq!(names(slot(r, Slot::Body).unwrap()), ["novo"]);
    }

    #[test]
    fn duplicate_remove_splice_nested() {
        let mut t = vec![rep(vec![log("a"), log("b")])];
        let a = Pos::new(&[(0, Slot::Body)], 0);
        assert!(duplicate(&mut t, &a));
        assert_eq!(names(slot(&t[0], Slot::Body).unwrap()), ["a", "a", "b"]);
        assert_eq!(remove(&mut t, &Pos::new(&[(0, Slot::Body)], 2)), Some(log("b")));
        assert!(splice(&mut t, &a, vec![log("x"), log("y")]));
        assert_eq!(names(slot(&t[0], Slot::Body).unwrap()), ["x", "y", "a"]);
    }

    #[test]
    fn wrap_keeps_position() {
        let mut t = vec![log("a"), rep(vec![log("in1"), log("in2")]), log("c")];
        // Wrap a nested block in Se: it goes to "então", and Se takes its place.
        let in2 = Pos::new(&[(1, Slot::Body)], 1);
        assert!(wrap(&mut t, &in2, iff(vec![], vec![])));
        let body = slot(&t[1], Slot::Body).unwrap();
        assert_eq!(names(body), ["in1", "Se"]);
        assert_eq!(names(slot(&body[1], Slot::Then).unwrap()), ["in2"]);
        assert!(slot(&body[1], Slot::Else).unwrap().is_empty());
        // Wrap a top-level block in Enquanto: it goes to the body.
        let w = Block::While { cond: Condition::default(), body: vec![] };
        assert!(wrap(&mut t, &top(2), w));
        assert_eq!(names(&t), ["a", "Repetir", "Enquanto"]);
        assert_eq!(names(slot(&t[2], Slot::Body).unwrap()), ["c"]);
        // Not a container / missing block: nothing changes.
        let before = t.clone();
        assert!(!wrap(&mut t, &top(0), log("x")));
        assert!(!wrap(&mut t, &top(9), rep(vec![])));
        assert_eq!(t, before);
    }

    #[test]
    fn counts_blocks_inside() {
        let b = iff(vec![log("a"), rep(vec![log("b"), log("c")])], vec![log("d")]);
        assert_eq!(count_inside(&b), 5);
        assert_eq!(count_inside(&rep(vec![])), 0);
        assert_eq!(count_inside(&log("x")), 0);
    }

    #[test]
    fn categories() {
        for t in Block::templates() {
            let c = category(&t);
            let want = match t.title() {
                "Clique" | "Mover" | "Tecla" | "Digitar" | "Rolar" | "Reproduzir gravação" => Category::Action,
                "Esperar" | "Esperar até" => Category::Wait,
                "Se" | "Repetir" | "Enquanto" | "Chamar script" | "Parar" => Category::Control,
                "Definir variável" | "Ler texto (OCR)" => Category::Vars,
                _ => Category::Debug,
            };
            assert_eq!(c, want, "{}", t.title());
        }
    }

    #[test]
    fn nesting_survives_json() {
        let mut t = vec![log("a"), rep(vec![])];
        assert!(move_block(&mut t, &top(0), &Pos::new(&[(1, Slot::Body)], 0)));
        let s = crate::model::Script { blocks: t, ..Default::default() };
        let json = serde_json::to_string(&s).unwrap();
        let back: crate::model::Script = serde_json::from_str(&json).unwrap();
        assert_eq!(back.blocks.len(), 1);
        assert_eq!(names(slot(&back.blocks[0], Slot::Body).unwrap()), ["a"]);
    }
}
