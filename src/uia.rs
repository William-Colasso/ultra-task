//! UI Automation: the "Elemento (UIA)" block and condition, and the element under each
//! recorded click.
//!
//! The pure parts (matching a recorded identification against a tree, picking the
//! clickable element of a chain, turning recorded clicks into UIA blocks) are kept apart
//! from the COM calls. COM calls only run on dedicated MTA worker threads, never on the UI
//! thread (UIA calls into our own egui window would deadlock it) nor on the hook thread
//! (a slow hook stalls the whole system's input), and every call has a timeout because
//! `Invoke` may block inside the target app.
use crate::model::glob_match;
use crate::win::{Hwnd, hw};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

// ---------- script data ----------

/// One element's identification, without the point (used for ancestors).
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UiaId {
    pub automation_id: String,
    pub name: String,
    pub control_type: String,
    pub class_name: String,
}

/// How a UIA block or condition finds its element inside the target window.
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UiaElem {
    /// Exact match when set; then `name` only breaks ties.
    pub automation_id: String,
    /// Glob (`*`, `?`), case-insensitive. Required to match when `automation_id` is empty.
    pub name: String,
    /// UIA ControlType without the prefix: "Button", "Edit", "CheckBox"…
    pub control_type: String,
    pub class_name: String,
    /// Nearest first, up to (not including) the target window. Only breaks ties.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ancestors: Vec<UiaId>,
    /// Point clicked when the element was captured (client-area pixels): breaks ties
    /// between equal candidates and is where the fallback click goes.
    pub x: i32,
    pub y: i32,
}

impl UiaElem {
    fn is_blank(&self) -> bool {
        [&self.automation_id, &self.name, &self.control_type, &self.class_name].iter().all(|s| s.trim().is_empty())
    }
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub enum UiaAction {
    #[default]
    Invoke,
    Toggle,
    /// Text with `{expr}` interpolation.
    SetValue(String),
    Select,
    Expand,
    Collapse,
    /// Variable that receives the ValuePattern value.
    ReadValue(String),
    /// Variable that receives the element's Name.
    ReadName(String),
}

impl UiaAction {
    pub fn label(&self) -> &'static str {
        match self {
            UiaAction::Invoke => "acionar",
            UiaAction::Toggle => "alternar",
            UiaAction::SetValue(_) => "definir valor",
            UiaAction::Select => "selecionar",
            UiaAction::Expand => "expandir",
            UiaAction::Collapse => "recolher",
            UiaAction::ReadValue(_) => "ler valor para",
            UiaAction::ReadName(_) => "ler nome para",
        }
    }

    /// Every action, for the editor's combo box.
    pub fn all() -> [UiaAction; 8] {
        let s = String::new;
        [UiaAction::Invoke, UiaAction::Toggle, UiaAction::SetValue(s()), UiaAction::Select, UiaAction::Expand, UiaAction::Collapse, UiaAction::ReadValue("valor".into()), UiaAction::ReadName("nome".into())]
    }
}

/// What the "elemento" condition checks.
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub enum UiaTest {
    #[default]
    Exists,
    NameContains(String),
    ValueContains(String),
    Checked,
}

impl UiaTest {
    pub fn label(&self) -> &'static str {
        match self {
            UiaTest::Exists => "existe",
            UiaTest::NameContains(_) => "nome contém",
            UiaTest::ValueContains(_) => "valor contém",
            UiaTest::Checked => "está marcado",
        }
    }

    pub fn all() -> [UiaTest; 4] {
        [UiaTest::Exists, UiaTest::NameContains(String::new()), UiaTest::ValueContains(String::new()), UiaTest::Checked]
    }
}

/// Result of capturing the element at a point (📍 or a recorded click).
#[derive(Clone, Debug, PartialEq)]
pub struct Captured {
    pub elem: UiaElem,
    pub action: Option<UiaAction>,
}

pub const DEFAULT_TIMEOUT_MS: &str = "5000";

pub fn default_timeout() -> String {
    DEFAULT_TIMEOUT_MS.into()
}

/// UIA ControlType names, indexed by `id - 50000`.
pub const CONTROL_TYPES: &[&str] = &[
    "Button", "Calendar", "CheckBox", "ComboBox", "Edit", "Hyperlink", "Image", "ListItem", "List", "Menu", "MenuBar", "MenuItem",
    "ProgressBar", "RadioButton", "ScrollBar", "Slider", "Spinner", "StatusBar", "Tab", "TabItem", "Text", "ToolBar", "ToolTip", "Tree",
    "TreeItem", "Custom", "Group", "Thumb", "DataGrid", "DataItem", "Document", "SplitButton", "Window", "Pane", "Header", "HeaderItem",
    "Table", "TitleBar", "Separator", "SemanticZoom", "AppBar",
];

pub fn control_type_name(id: i32) -> String {
    usize::try_from(id - 50000).ok().and_then(|i| CONTROL_TYPES.get(i)).map_or_else(|| format!("ControlType {id}"), |s| s.to_string())
}

/// Short human description for logs and the editor: `Button "OK" #btnOk`.
pub fn describe(e: &UiaElem) -> String {
    let mut s = if e.control_type.is_empty() { "elemento".to_string() } else { e.control_type.clone() };
    if !e.name.is_empty() {
        s += &format!(" \"{}\"", e.name);
    }
    if !e.automation_id.is_empty() {
        s += &format!(" #{}", e.automation_id);
    }
    if e.name.is_empty() && e.automation_id.is_empty() && !e.class_name.is_empty() {
        s += &format!(" .{}", e.class_name);
    }
    s
}

/// Validation problems as (field sub-path, message); an empty sub-path means the element itself.
pub fn elem_errors(e: &UiaElem) -> Vec<(String, String)> {
    let mut out = vec![];
    if e.is_blank() {
        out.push((String::new(), "identificação vazia: preencha automation_id, name, control_type ou class_name (ou capture com o botão de captura)".into()));
    }
    let known = |ct: &str| ct.is_empty() || CONTROL_TYPES.iter().any(|k| k.eq_ignore_ascii_case(ct));
    if !known(&e.control_type) {
        out.push(("control_type".into(), format!("\"{}\" não é um ControlType da UIA (ex.: Button, Edit, CheckBox)", e.control_type)));
    }
    for (i, a) in e.ancestors.iter().enumerate() {
        if !known(&a.control_type) {
            out.push((format!("ancestors[{i}].control_type"), format!("\"{}\" não é um ControlType da UIA", a.control_type)));
        }
    }
    out
}

// ---------- choosing the element (pure) ----------

/// One element of the target window's tree, flattened in document order.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: UiaId,
    pub parent: Option<usize>,
    /// Screen pixels: left, top, right, bottom.
    pub rect: [i32; 4],
}

/// Hard requirements: `automation_id` and `control_type` when recorded; `name` when there
/// is no `automation_id`; `class_name` when it is all we have besides the type.
pub fn matches(want: &UiaElem, n: &UiaId) -> bool {
    if want.is_blank() {
        return false;
    }
    let (aid, name) = (!want.automation_id.is_empty(), !want.name.is_empty());
    let ct_ok = want.control_type.is_empty() || want.control_type.eq_ignore_ascii_case(&n.control_type);
    let aid_ok = !aid || want.automation_id == n.automation_id;
    let name_ok = aid || !name || glob_match(&want.name, &n.name);
    let class_ok = aid || name || want.class_name.is_empty() || want.class_name == n.class_name;
    ct_ok && aid_ok && name_ok && class_ok
}

fn same_id(a: &UiaId, b: &UiaId) -> bool {
    a.control_type.eq_ignore_ascii_case(&b.control_type) && a.automation_id == b.automation_id && a.name == b.name
}

/// Among the matching nodes, the best by: name/class/ancestor agreement, then containing
/// the captured point, then closeness to it, then document order.
pub fn choose(nodes: &[Node], want: &UiaElem, pt: Option<(i32, i32)>) -> Option<usize> {
    let key = |i: usize| {
        let n = &nodes[i];
        let mut score = 0u32;
        if !want.name.is_empty() && glob_match(&want.name, &n.id.name) {
            score += 4;
        }
        if !want.class_name.is_empty() && want.class_name == n.id.class_name {
            score += 2;
        }
        let mut p = n.parent;
        for a in &want.ancestors {
            let Some(pi) = p else { break };
            if same_id(a, &nodes[pi].id) {
                score += 1;
            }
            p = nodes[pi].parent;
        }
        let [l, t, r, b] = n.rect;
        let (inside, dist) = match pt {
            Some((x, y)) => {
                let (dx, dy) = ((l as i64 + r as i64) / 2 - x as i64, (t as i64 + b as i64) / 2 - y as i64);
                (x >= l && x < r && y >= t && y < b, dx * dx + dy * dy)
            }
            None => (false, 0),
        };
        (score, inside, -dist)
    };
    let mut best: Option<(usize, (u32, bool, i64))> = None;
    for i in (0..nodes.len()).filter(|&i| matches(want, &nodes[i].id)) {
        let k = key(i);
        if best.as_ref().is_none_or(|b| k > b.1) {
            best = Some((i, k));
        }
    }
    best.map(|b| b.0)
}

/// Control patterns an element supports.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Patterns {
    pub invoke: bool,
    pub toggle: bool,
    pub select: bool,
    pub expand: bool,
    pub value: bool,
}

/// One element of the chain from the point (first) up to the target window (excluded).
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub id: UiaId,
    pub pats: Patterns,
}

/// `ElementFromPoint` returns the deepest element, often the text inside a button. Use it
/// if it does anything itself; otherwise the nearest actionable ancestor (up to 3 up).
pub fn pick_from_chain(chain: &[Link]) -> usize {
    let acts = |p: &Patterns| p.invoke || p.toggle || p.select;
    match chain.first() {
        Some(l) if acts(&l.pats) || l.pats.value || l.pats.expand => 0,
        _ => chain.iter().take(4).position(|l| acts(&l.pats)).unwrap_or(0),
    }
}

/// The action a click on this element most likely meant, if it can be done without input.
pub fn suggest_action(control_type: &str, p: Patterns) -> Option<UiaAction> {
    match control_type {
        "CheckBox" if p.toggle => Some(UiaAction::Toggle),
        "RadioButton" | "ListItem" | "TabItem" | "TreeItem" | "DataItem" if p.select => Some(UiaAction::Select),
        _ if p.invoke => Some(UiaAction::Invoke),
        _ if p.toggle => Some(UiaAction::Toggle),
        _ if p.select => Some(UiaAction::Select),
        _ => None,
    }
}

/// Identification + suggested action for a click at client point (x, y).
pub fn captured(chain: &[Link], x: i32, y: i32) -> Option<Captured> {
    let i = pick_from_chain(chain);
    let l = chain.get(i)?;
    let ancestors = chain[i + 1..].iter().take(4).map(|a| a.id.clone()).collect();
    let UiaId { automation_id, name, control_type, class_name } = l.id.clone();
    let elem = UiaElem { automation_id, name, control_type, class_name, ancestors, x, y };
    Some(Captured { action: suggest_action(&elem.control_type, l.pats), elem })
}

// ---------- interpreter interface ----------

/// What the interpreter asks of the element (see `runner::Io::uia`).
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Find,
    Invoke,
    Toggle,
    SetValue(String),
    Select,
    Expand,
    Collapse,
    Value,
    Name,
    Checked,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub elem: UiaElem,
    pub op: Op,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    NotFound,
    Done,
    Text(String),
    Bool(bool),
    /// The app did not answer within the call timeout (Invoke may block, e.g. on a modal).
    TimedOut,
}

/// Fake tree for tests: `choose` + the op applied to an in-memory element.
#[cfg(test)]
pub struct FakeTree {
    pub nodes: Vec<Node>,
    pub values: Vec<String>,
    pub checked: Vec<bool>,
    pub log: Vec<(usize, Op)>,
}

#[cfg(test)]
impl FakeTree {
    pub fn new(nodes: Vec<Node>) -> Self {
        let n = nodes.len();
        FakeTree { nodes, values: vec![String::new(); n], checked: vec![false; n], log: vec![] }
    }

    pub fn query(&mut self, q: &Query, pt: Option<(i32, i32)>) -> Reply {
        let Some(i) = choose(&self.nodes, &q.elem, pt) else { return Reply::NotFound };
        self.log.push((i, q.op.clone()));
        match &q.op {
            Op::SetValue(v) => self.values[i] = v.clone(),
            Op::Toggle => self.checked[i] = !self.checked[i],
            Op::Value => return Reply::Text(self.values[i].clone()),
            Op::Name => return Reply::Text(self.nodes[i].id.name.clone()),
            Op::Checked => return Reply::Bool(self.checked[i]),
            _ => {}
        }
        Reply::Done
    }
}

// ---------- worker thread with timeouts ----------

#[derive(Debug, PartialEq)]
pub enum CallErr {
    Timeout,
    Failed(String),
}

type Job<C> = Box<dyn FnOnce(Result<&mut C, &str>) + Send>;

/// A thread that owns a `C` (the COM objects, which must stay on the thread that made
/// them) and runs closures on it. A call that times out abandons the thread: it exits
/// when its stuck call returns, and the next call starts a fresh one.
pub struct Worker<C: 'static> {
    init: Arc<dyn Fn() -> Result<C, String> + Send + Sync>,
    tx: Option<Sender<Job<C>>>,
}

impl<C: 'static> Worker<C> {
    pub fn new(init: impl Fn() -> Result<C, String> + Send + Sync + 'static) -> Self {
        Worker { init: Arc::new(init), tx: None }
    }

    fn sender(&mut self) -> Sender<Job<C>> {
        if let Some(t) = &self.tx {
            return t.clone();
        }
        let (tx, rx) = channel::<Job<C>>();
        let init = self.init.clone();
        let _ = std::thread::Builder::new().name("ultratask-uia".into()).spawn(move || {
            let mut ctx = init();
            for job in rx {
                job(ctx.as_mut().map_err(|e| e.as_str()));
            }
        });
        self.tx = Some(tx.clone());
        tx
    }

    pub fn call<T: Send + 'static>(&mut self, timeout: Duration, f: impl FnOnce(&mut C) -> Result<T, String> + Send + 'static) -> Result<T, CallErr> {
        let (rtx, rrx) = channel();
        let job: Job<C> = Box::new(move |c| {
            let _ = rtx.send(c.map_err(str::to_string).and_then(f));
        });
        let gone = || CallErr::Failed("a thread da UI Automation parou".into());
        if self.sender().send(job).is_err() {
            self.tx = None;
            return Err(gone());
        }
        match rrx.recv_timeout(timeout) {
            Ok(r) => r.map_err(CallErr::Failed),
            Err(RecvTimeoutError::Timeout) => {
                self.tx = None;
                Err(CallErr::Timeout)
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.tx = None;
                Err(gone())
            }
        }
    }
}

// ---------- real UI Automation (COM) ----------

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Accessibility::*;
use windows::core::{BSTR, Interface};

/// Per-call limit. Invoke should return at once, but some apps block until a dialog closes.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(3);
/// Lookups stop flattening the tree past this many elements (huge web pages).
const MAX_NODES: usize = 20_000;

struct Com;

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// The COM objects of one worker thread. Field order = drop order: COM goes last.
pub struct Ctx {
    auto: IUIAutomation,
    walker: IUIAutomationTreeWalker,
    _com: Com,
}

impl Ctx {
    pub fn new() -> Result<Ctx, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| format!("COM: {e}"))?;
            let com = Com;
            let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|e| format!("UI Automation indisponível: {e}"))?;
            let walker = auto.ControlViewWalker().map_err(es)?;
            Ok(Ctx { auto, walker, _com: com })
        }
    }
}

pub fn session() -> Worker<Ctx> {
    Worker::new(Ctx::new)
}

pub fn call_err(e: CallErr) -> String {
    match e {
        CallErr::Timeout => "a UI Automation não respondeu a tempo".into(),
        CallErr::Failed(e) => e,
    }
}

fn es(e: windows::core::Error) -> String {
    format!("UI Automation: {}", e.message())
}

fn client_to_screen(h: Hwnd, x: i32, y: i32) -> (i32, i32) {
    let mut p = POINT { x, y };
    unsafe {
        let _ = ClientToScreen(hw(h), &mut p);
    }
    (p.x, p.y)
}

fn current_id(el: &IUIAutomationElement) -> UiaId {
    unsafe {
        UiaId {
            automation_id: el.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default(),
            name: el.CurrentName().map(|s| s.to_string()).unwrap_or_default(),
            control_type: el.CurrentControlType().map(|c| control_type_name(c.0)).unwrap_or_default(),
            class_name: el.CurrentClassName().map(|s| s.to_string()).unwrap_or_default(),
        }
    }
}

fn cached_node(el: &IUIAutomationElement, parent: Option<usize>) -> Node {
    unsafe {
        let r = el.CachedBoundingRectangle().unwrap_or_default();
        Node {
            id: UiaId {
                automation_id: el.CachedAutomationId().map(|s| s.to_string()).unwrap_or_default(),
                name: el.CachedName().map(|s| s.to_string()).unwrap_or_default(),
                control_type: el.CachedControlType().map(|c| control_type_name(c.0)).unwrap_or_default(),
                class_name: el.CachedClassName().map(|s| s.to_string()).unwrap_or_default(),
            },
            parent,
            rect: [r.left, r.top, r.right, r.bottom],
        }
    }
}

fn has<T: Interface>(el: &IUIAutomationElement, id: UIA_PATTERN_ID) -> bool {
    unsafe { el.GetCurrentPatternAs::<T>(id).is_ok() }
}

fn patterns(el: &IUIAutomationElement) -> Patterns {
    Patterns {
        invoke: has::<IUIAutomationInvokePattern>(el, UIA_InvokePatternId),
        toggle: has::<IUIAutomationTogglePattern>(el, UIA_TogglePatternId),
        select: has::<IUIAutomationSelectionItemPattern>(el, UIA_SelectionItemPatternId),
        expand: has::<IUIAutomationExpandCollapsePattern>(el, UIA_ExpandCollapsePatternId),
        value: has::<IUIAutomationValuePattern>(el, UIA_ValuePatternId),
    }
}

fn pattern<T: Interface>(el: &IUIAutomationElement, id: UIA_PATTERN_ID, name: &str) -> Result<T, String> {
    unsafe { el.GetCurrentPatternAs::<T>(id) }.map_err(|_| format!("o elemento não tem o padrão {name} da UIA"))
}

/// Finds the element in the window (one cross-process round trip for the whole tree)
/// and applies `q.op` to it.
pub fn query(c: &mut Ctx, hwnd: Hwnd, q: &Query) -> Result<Reply, String> {
    unsafe {
        let root = c.auto.ElementFromHandle(hw(hwnd)).map_err(es)?;
        let req = c.auto.CreateCacheRequest().map_err(es)?;
        for p in [UIA_AutomationIdPropertyId, UIA_NamePropertyId, UIA_ControlTypePropertyId, UIA_ClassNamePropertyId, UIA_BoundingRectanglePropertyId] {
            req.AddProperty(p).map_err(es)?;
        }
        req.SetTreeScope(TreeScope_Subtree).map_err(es)?;
        let tree = root.BuildUpdatedCache(&req).map_err(es)?;
        let (mut nodes, mut els) = (vec![], vec![]);
        let mut stack = vec![(tree, None)];
        while let Some((el, parent)) = stack.pop() {
            if nodes.len() >= MAX_NODES {
                break;
            }
            nodes.push(cached_node(&el, parent));
            let me = nodes.len() - 1;
            if let Ok(kids) = el.GetCachedChildren() {
                let n = kids.Length().unwrap_or(0);
                for i in (0..n).rev() {
                    if let Ok(k) = kids.GetElement(i) {
                        stack.push((k, Some(me)));
                    }
                }
            }
            els.push(el);
        }
        let Some(i) = choose(&nodes, &q.elem, Some(client_to_screen(hwnd, q.elem.x, q.elem.y))) else { return Ok(Reply::NotFound) };
        let el = &els[i];
        let done = |r: windows::core::Result<()>| r.map(|_| Reply::Done).map_err(es);
        match &q.op {
            Op::Find => Ok(Reply::Done),
            Op::Invoke => done(pattern::<IUIAutomationInvokePattern>(el, UIA_InvokePatternId, "Invoke")?.Invoke()),
            Op::Toggle => done(pattern::<IUIAutomationTogglePattern>(el, UIA_TogglePatternId, "Toggle")?.Toggle()),
            Op::SetValue(v) => done(pattern::<IUIAutomationValuePattern>(el, UIA_ValuePatternId, "Value")?.SetValue(&BSTR::from(v.as_str()))),
            Op::Select => done(pattern::<IUIAutomationSelectionItemPattern>(el, UIA_SelectionItemPatternId, "SelectionItem")?.Select()),
            Op::Expand => done(pattern::<IUIAutomationExpandCollapsePattern>(el, UIA_ExpandCollapsePatternId, "ExpandCollapse")?.Expand()),
            Op::Collapse => done(pattern::<IUIAutomationExpandCollapsePattern>(el, UIA_ExpandCollapsePatternId, "ExpandCollapse")?.Collapse()),
            Op::Value => Ok(Reply::Text(pattern::<IUIAutomationValuePattern>(el, UIA_ValuePatternId, "Value")?.CurrentValue().map_err(es)?.to_string())),
            Op::Name => Ok(Reply::Text(el.CurrentName().map_err(es)?.to_string())),
            Op::Checked => Ok(Reply::Bool(pattern::<IUIAutomationTogglePattern>(el, UIA_TogglePatternId, "Toggle")?.CurrentToggleState().map_err(es)? == ToggleState_On)),
        }
    }
}

/// The element at screen point (sx, sy) inside window `root`; (cx, cy) is the same point
/// in client coordinates, stored as the fallback click.
pub fn capture(c: &mut Ctx, root: Hwnd, sx: i32, sy: i32, cx: i32, cy: i32) -> Result<Captured, String> {
    unsafe {
        let top = c.auto.ElementFromHandle(hw(root)).map_err(es)?;
        let mut el = c.auto.ElementFromPoint(POINT { x: sx, y: sy }).map_err(es)?;
        if let Ok(n) = c.walker.NormalizeElement(&el) {
            el = n;
        }
        let mut chain = vec![];
        let mut inside = false;
        for depth in 0..32 {
            if c.auto.CompareElements(&el, &top).is_ok_and(|b| b.as_bool()) {
                inside = true;
                break;
            }
            // Patterns cost 5 calls each; only the first few links can be picked.
            let pats = if depth < 4 { patterns(&el) } else { Patterns::default() };
            chain.push(Link { id: current_id(&el), pats });
            match c.walker.GetParentElement(&el) {
                Ok(p) => el = p,
                Err(_) => break,
            }
        }
        if !inside {
            return Err("o ponto não está dentro da janela-alvo".into());
        }
        captured(&chain, cx, cy).ok_or_else(|| "nenhum elemento UIA nesse ponto (a janela não expõe a árvore de acessibilidade?)".into())
    }
}

/// 📍 in the editor: captures on a worker thread, delivers through the receiver and wakes the UI.
pub fn capture_async(root: Hwnd, sx: i32, sy: i32, cx: i32, cy: i32, wake: Arc<dyn Fn() + Send + Sync>) -> Receiver<Result<Captured, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = session().call(Duration::from_secs(5), move |c| capture(c, root, sx, sy, cx, cy)).map_err(call_err);
        let _ = tx.send(r);
        wake();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn id(ct: &str, name: &str, aid: &str) -> UiaId {
        UiaId { automation_id: aid.into(), name: name.into(), control_type: ct.into(), class_name: String::new() }
    }
    fn node(ct: &str, name: &str, aid: &str, parent: Option<usize>, rect: [i32; 4]) -> Node {
        Node { id: id(ct, name, aid), parent, rect }
    }
    fn want(ct: &str, name: &str, aid: &str) -> UiaElem {
        UiaElem { automation_id: aid.into(), name: name.into(), control_type: ct.into(), ..Default::default() }
    }

    /// Window with two panels, each with an "OK" button; a checkbox; an edit.
    pub fn tree() -> Vec<Node> {
        vec![
            node("Window", "Pedido", "", None, [0, 0, 400, 300]),                // 0
            node("Pane", "Cliente", "pnlCliente", Some(0), [0, 0, 200, 300]),    // 1
            node("Button", "OK", "", Some(1), [10, 250, 90, 280]),               // 2
            node("Pane", "Entrega", "pnlEntrega", Some(0), [200, 0, 400, 300]),  // 3
            node("Button", "OK", "", Some(3), [210, 250, 290, 280]),             // 4
            node("CheckBox", "Urgente", "chkUrgente", Some(0), [10, 10, 100, 30]), // 5
            node("Edit", "Nome", "txtNome", Some(1), [10, 50, 190, 70]),         // 6
            node("Button", "Salvar pedido 123", "btnSalvar", Some(0), [300, 10, 390, 40]), // 7
        ]
    }

    #[test]
    fn choose_by_identification() {
        let t = tree();
        // AutomationId wins even when the name changed.
        assert_eq!(choose(&t, &want("Button", "Gravar", "btnSalvar"), None), Some(7));
        // Wrong control type never matches.
        assert_eq!(choose(&t, &want("Edit", "", "btnSalvar"), None), None);
        // Name glob, case-insensitive, when there is no AutomationId.
        assert_eq!(choose(&t, &want("Button", "salvar pedido *", ""), None), Some(7));
        assert_eq!(choose(&t, &want("Button", "Cancelar", ""), None), None);
        // Blank identification matches nothing.
        assert_eq!(choose(&t, &UiaElem::default(), None), None);
        // Two "OK" buttons: ancestors decide…
        let mut ok = want("Button", "OK", "");
        ok.ancestors = vec![id("Pane", "Entrega", "pnlEntrega")];
        assert_eq!(choose(&t, &ok, None), Some(4));
        // …and without ancestors, the captured point does; with neither, document order.
        ok.ancestors.clear();
        assert_eq!(choose(&t, &ok, Some((250, 260))), Some(4));
        assert_eq!(choose(&t, &ok, Some((260, 10))), Some(4)); // closer to the second
        assert_eq!(choose(&t, &ok, None), Some(2));
        // Ancestors beat the point.
        ok.ancestors = vec![id("Pane", "Cliente", "pnlCliente")];
        assert_eq!(choose(&t, &ok, Some((250, 260))), Some(2));
        // Only a class name: required to match.
        let mut t2 = t.clone();
        t2[6].id.class_name = "WindowsForms10.EDIT".into();
        let by_class = UiaElem { class_name: "WindowsForms10.EDIT".into(), ..Default::default() };
        assert_eq!(choose(&t2, &by_class, None), Some(6));
        assert_eq!(choose(&t, &by_class, None), None);
    }

    #[test]
    fn chain_and_suggestion() {
        let p = |invoke, toggle, select| Patterns { invoke, toggle, select, ..Default::default() };
        // WPF: the click lands on the TextBlock inside the button.
        let chain = vec![
            Link { id: id("Text", "Salvar", ""), pats: p(false, false, false) },
            Link { id: id("Button", "Salvar", "btnSalvar"), pats: p(true, false, false) },
            Link { id: id("Pane", "", "painel"), pats: p(false, false, false) },
        ];
        let c = captured(&chain, 5, 6).unwrap();
        assert_eq!((c.elem.automation_id.as_str(), c.elem.x, c.elem.y), ("btnSalvar", 5, 6));
        assert_eq!(c.elem.ancestors, vec![id("Pane", "", "painel")]);
        assert_eq!(c.action, Some(UiaAction::Invoke));
        // An edit is used as is, with no action (stays a click).
        let edit = vec![Link { id: id("Edit", "", "txt"), pats: Patterns { value: true, ..Default::default() } }, chain[1].clone()];
        let c = captured(&edit, 0, 0).unwrap();
        assert_eq!((c.elem.control_type.as_str(), c.action), ("Edit", None));
        // Nothing actionable nearby: the deepest element, no action.
        assert_eq!(pick_from_chain(&[chain[0].clone(), chain[2].clone()]), 0);
        assert_eq!(captured(&[], 0, 0), None);
        assert_eq!(suggest_action("CheckBox", p(true, true, false)), Some(UiaAction::Toggle));
        assert_eq!(suggest_action("ListItem", p(true, false, true)), Some(UiaAction::Select));
        assert_eq!(suggest_action("Button", p(true, false, false)), Some(UiaAction::Invoke));
        assert_eq!(suggest_action("ComboBox", Patterns { expand: true, ..Default::default() }), None);
        assert_eq!(control_type_name(50000), "Button");
        assert_eq!(control_type_name(50040), "AppBar");
        assert_eq!(control_type_name(1), "ControlType 1");
    }

    #[test]
    fn validation_and_description() {
        assert_eq!(elem_errors(&UiaElem::default()).len(), 1);
        assert!(elem_errors(&want("button", "", "x")).is_empty()); // case-insensitive
        let mut bad = want("Botao", "", "");
        bad.ancestors = vec![id("Painel", "", "")];
        let fields: Vec<String> = elem_errors(&bad).into_iter().map(|e| e.0).collect();
        assert_eq!(fields, ["control_type", "ancestors[0].control_type"]);
        assert_eq!(describe(&want("Button", "OK", "btnOk")), "Button \"OK\" #btnOk");
        assert_eq!(describe(&UiaElem { class_name: "Edit".into(), ..Default::default() }), "elemento .Edit");
    }

    #[test]
    fn serde_defaults_and_roundtrip() {
        let e: UiaElem = serde_json::from_str(r#"{"name":"OK"}"#).unwrap();
        assert_eq!(e, UiaElem { name: "OK".into(), ..Default::default() });
        assert!(serde_json::from_str::<UiaElem>(r#"{"nome":"OK"}"#).is_err());
        let e = want("Button", "OK", "");
        let s = serde_json::to_string(&e).unwrap();
        assert!(!s.contains("ancestors"), "{s}");
        assert_eq!(serde_json::from_str::<UiaElem>(&s).unwrap(), e);
        let a: UiaAction = serde_json::from_str(r#"{"SetValue":"oi {n}"}"#).unwrap();
        assert_eq!(a, UiaAction::SetValue("oi {n}".into()));
    }

    #[test]
    fn fake_tree_ops() {
        let mut t = FakeTree::new(tree());
        let q = |e: UiaElem, op| Query { elem: e, op };
        assert_eq!(t.query(&q(want("CheckBox", "", "chkUrgente"), Op::Toggle), None), Reply::Done);
        assert_eq!(t.query(&q(want("CheckBox", "", "chkUrgente"), Op::Checked), None), Reply::Bool(true));
        assert_eq!(t.query(&q(want("Edit", "", "txtNome"), Op::SetValue("Ana".into())), None), Reply::Done);
        assert_eq!(t.query(&q(want("Edit", "", "txtNome"), Op::Value), None), Reply::Text("Ana".into()));
        assert_eq!(t.query(&q(want("Edit", "", "nada"), Op::Value), None), Reply::NotFound);
    }

    #[test]
    fn worker_times_out_and_recovers() {
        let mut w: Worker<u32> = Worker::new(|| Ok(7));
        assert_eq!(w.call(Duration::from_secs(5), |c| Ok(*c + 1)), Ok(8));
        let t0 = Instant::now();
        let r = w.call(Duration::from_millis(100), |_| {
            std::thread::sleep(Duration::from_millis(1500)); // a hung Invoke
            Ok(0)
        });
        assert_eq!(r, Err(CallErr::Timeout));
        assert!(t0.elapsed() < Duration::from_millis(1000));
        // The stuck thread is abandoned; the next call gets a fresh one.
        assert_eq!(w.call(Duration::from_secs(5), |c| Ok(*c)), Ok(7));
        assert_eq!(w.call(Duration::from_secs(5), |_| Err::<u32, _>("x".to_string())), Err(CallErr::Failed("x".into())));
        let mut bad: Worker<u32> = Worker::new(|| Err("sem COM".into()));
        assert_eq!(bad.call(Duration::from_secs(5), |c| Ok(*c)), Err(CallErr::Failed("sem COM".into())));
    }

    /// Real UI Automation on a window of our own: a checkbox toggled and an edit filled,
    /// both read back through UIA, with no input at all.
    #[test]
    #[ignore = "UI Automation de verdade (não funciona no Wine); rode no Windows com cargo test -- --ignored"]
    fn real_uia_on_own_window() {
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::core::w;
        let (tx, rx) = channel();
        std::thread::spawn(move || unsafe {
            let inst = Some(HINSTANCE(GetModuleHandleW(None).unwrap().0));
            let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("ultratask-uia"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 140, 140, 420, 200, None, None, inst, None).unwrap();
            let chk = WINDOW_STYLE(BS_AUTOCHECKBOX as u32) | WS_CHILD | WS_VISIBLE;
            CreateWindowExW(WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Urgente"), chk, 10, 10, 120, 24, Some(top), None, inst, None).unwrap();
            CreateWindowExW(WINDOW_EX_STYLE(0), w!("EDIT"), w!(""), WS_CHILD | WS_VISIBLE | WS_BORDER, 10, 50, 300, 24, Some(top), None, inst, None).unwrap();
            tx.send(top.0 as isize).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let top = rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let mut s = session();
        let mut ask = |elem: UiaElem, op: Op| s.call(CALL_TIMEOUT, move |c| query(c, top, &Query { elem, op })).unwrap();
        let chk = want("CheckBox", "Urgente", "");
        assert_eq!(ask(chk.clone(), Op::Checked), Reply::Bool(false));
        assert_eq!(ask(chk.clone(), Op::Toggle), Reply::Done);
        assert_eq!(ask(chk, Op::Checked), Reply::Bool(true));
        let edit = UiaElem { control_type: "Edit".into(), x: 20, y: 60, ..Default::default() };
        assert_eq!(ask(edit.clone(), Op::SetValue("UltraTask 42".into())), Reply::Done);
        assert_eq!(ask(edit, Op::Value), Reply::Text("UltraTask 42".into()));
        assert_eq!(ask(want("Button", "Não existe", ""), Op::Find), Reply::NotFound);
        // Capture at the checkbox's center.
        let (sx, sy) = client_to_screen(top, 60, 22);
        let cap = s.call(CALL_TIMEOUT, move |c| capture(c, top, sx, sy, 60, 22)).unwrap();
        assert_eq!((cap.elem.control_type.as_str(), cap.elem.name.as_str(), cap.action), ("CheckBox", "Urgente", Some(UiaAction::Toggle)));
    }
}
