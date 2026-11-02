use crate::editor;
use crate::hooks::{self, Cmd, Hooks, Out};
use crate::model::{self, *};
use crate::runner::{self, RunHandle};
use crate::win::{self, Hwnd, WinInfo};
use crate::{TrayMsg, make_tray};
use eframe::egui::{self, Color32, ComboBox, DragValue, Id, Ui, ViewportCommand};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Blocks,
    Config,
}

#[derive(Clone, Copy, PartialEq)]
enum Purpose {
    Play,
    Record,
}

/// What the next click on a window will be used for.
enum Pick {
    Target,
    Point(Id),
    Region(Id, Option<(i32, i32)>),
}

#[derive(Clone, Copy)]
enum PickData {
    Point(i32, i32, [u8; 3]),
    Region(Region),
}

pub struct App {
    scripts: Vec<Script>,
    sel: Option<usize>,
    dirty: bool,
    settings: Settings,
    hooks: Hooks,
    hook_rx: Receiver<Out>,
    log_tx: Sender<String>,
    log_rx: Receiver<String>,
    log: Vec<String>,
    runs: Vec<RunHandle>,
    tab: Tab,
    recording: Option<(usize, Hwnd)>,
    pick: Option<Pick>,
    pick_req: Option<Pick>,
    pick_result: Option<(Id, PickData)>,
    chooser: Option<(usize, Purpose, Vec<WinInfo>)>,
    rename: Option<String>,
    confirm_delete: bool,
    show_settings: bool,
    elevated_target: bool,
    wake: Arc<dyn Fn() + Send + Sync>,
    tray_rx: Receiver<TrayMsg>,
    _tray: Option<tray_icon::TrayIcon>,
    quitting: bool,
    last_poll: Instant,
    /// OnStartup scripts already fired this session.
    startup_done: HashSet<String>,
    /// OnWindowOpen: windows already seen, per "script|exe|title" so editing the rule re-seeds.
    seen: HashMap<String, HashSet<Hwnd>>,
    /// `--tray`: eframe always shows the window after its first frame, so re-hide for a few frames.
    hide_frames: u8,
}

fn theme_pref(t: Theme) -> egui::ThemePreference {
    match t {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    }
}

impl App {
    /// `show_event`: named event another UltraTask instance signals to bring us up.
    pub fn new(cc: &eframe::CreationContext<'_>, show_event: Option<isize>, start_hidden: bool) -> Self {
        let ctx = cc.egui_ctx.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let w2 = wake.clone();
        let (hooks, hook_rx) = Hooks::start(move || w2());
        let (log_tx, log_rx) = channel();
        let (tray_tx, tray_rx) = channel();
        let tray = make_tray(tray_tx.clone(), wake.clone());
        // Heartbeat: wakes `logic` every 2 s even while hidden in the tray (trigger polling),
        // and shows the window when a second instance is launched.
        let w3 = wake.clone();
        std::thread::spawn(move || {
            loop {
                match show_event {
                    Some(ev) if win::wait_show(ev, 2000) => {
                        let _ = tray_tx.send(TrayMsg::Show);
                    }
                    Some(_) => {}
                    None => std::thread::sleep(Duration::from_secs(2)),
                }
                w3();
            }
        });
        let settings = model::load_settings();
        cc.egui_ctx.set_theme(theme_pref(settings.theme));
        let (scripts, errs) = model::load_scripts();
        let mut app = App {
            sel: (!scripts.is_empty()).then_some(0),
            scripts,
            dirty: false,
            settings,
            hooks,
            hook_rx,
            log_tx,
            log_rx,
            log: vec![],
            runs: vec![],
            tab: Tab::Blocks,
            recording: None,
            pick: None,
            pick_req: None,
            pick_result: None,
            chooser: None,
            rename: None,
            confirm_delete: false,
            show_settings: false,
            elevated_target: false,
            wake,
            tray_rx,
            _tray: tray,
            quitting: false,
            last_poll: Instant::now(),
            startup_done: HashSet::new(),
            seen: HashMap::new(),
            hide_frames: if start_hidden { 3 } else { 0 },
        };
        for e in errs {
            app.say(format!("Erro ao ler script {e}"));
        }
        if app.settings.start_with_windows {
            // Re-write so the entry follows the exe if it was moved.
            if let Err(e) = win::set_autostart(true) {
                app.say(e);
            }
        }
        app.register_hotkeys();
        app
    }

    fn reload(&mut self) {
        self.save_dirty();
        let name = self.sel.map(|i| self.scripts[i].name.clone());
        let (scripts, errs) = model::load_scripts();
        self.scripts = scripts;
        self.sel = name.and_then(|n| self.scripts.iter().position(|s| s.name == n));
        for e in errs {
            self.say(format!("Erro ao ler script {e}"));
        }
        self.say(format!("Recarregado: {} scripts", self.scripts.len()));
        self.register_hotkeys();
    }

    /// Fires OnStartup / OnWindowOpen triggers. Runs every ~2 s (heartbeat), also while hidden.
    fn poll_triggers(&mut self) {
        if self.last_poll.elapsed() < Duration::from_millis(1500) {
            return;
        }
        self.last_poll = Instant::now();
        if self.scripts.iter().all(|s| s.trigger == Trigger::Manual) {
            return;
        }
        let wins = win::list_windows();
        let mut fire = vec![];
        for (i, s) in self.scripts.iter().enumerate() {
            let Some(rule) = &s.target else { continue };
            let matches: Vec<&WinInfo> = wins.iter().filter(|w| rule.matches(&w.exe, &w.title)).collect();
            match s.trigger {
                Trigger::Manual => {}
                Trigger::OnStartup { delay_s } => {
                    if let Some(w) = matches.first()
                        && self.startup_done.insert(s.name.clone())
                    {
                        fire.push((i, (*w).clone(), delay_s));
                    }
                }
                Trigger::OnWindowOpen { delay_s } => {
                    let key = format!("{}|{}|{}", s.name, rule.exe, rule.title);
                    let now: HashSet<Hwnd> = matches.iter().map(|w| w.hwnd).collect();
                    // First sight seeds the set: windows already open don't count as "opened".
                    let seen = self.seen.entry(key).or_insert_with(|| now.clone());
                    for w in &matches {
                        if !seen.contains(&w.hwnd) {
                            fire.push((i, (*w).clone(), delay_s));
                        }
                    }
                    *seen = now;
                }
            }
        }
        for (i, w, delay_s) in fire {
            self.say(format!("Gatilho: '{}' em '{}'", self.scripts[i].name, w.title));
            self.play_on(i, w, (delay_s.max(0.0) * 1000.0) as u64);
        }
    }

    fn say(&mut self, m: impl Into<String>) {
        self.log.push(m.into());
        if self.log.len() > 500 {
            self.log.drain(..100);
        }
    }

    fn register_hotkeys(&self) {
        let s = &self.settings;
        let mut list = vec![(hooks::RECORD_HOTKEY, s.record), (hooks::PLAY_HOTKEY, s.play), (hooks::PANIC_HOTKEY, s.panic)];
        for (i, sc) in self.scripts.iter().enumerate() {
            if let Some(hk) = sc.hotkey {
                list.push((hooks::SCRIPT_HOTKEY + i as i32, hk));
            }
        }
        self.hooks.send(Cmd::Hotkeys(list));
    }

    fn save_dirty(&mut self) {
        if let (true, Some(i)) = (self.dirty, self.sel) {
            if let Err(e) = model::save_script(&self.scripts[i]) {
                self.say(format!("Erro ao salvar '{}': {e}", self.scripts[i].name));
                return;
            }
            self.dirty = false;
        }
    }

    fn select(&mut self, i: usize) {
        self.save_dirty();
        self.sel = Some(i);
        self.confirm_delete = false;
        self.rename = None;
    }

    /// Re-sorts by name and keeps `name` selected (hotkey ids are index based → re-register).
    fn resort(&mut self, name: &str) {
        self.scripts.sort_by_key(|s| s.name.to_lowercase());
        self.sel = self.scripts.iter().position(|s| s.name == name);
        self.register_hotkeys();
    }

    fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.scripts.iter().any(|s| s.name.eq_ignore_ascii_case(n));
        if !taken(base) {
            return base.into();
        }
        (2..).map(|k| format!("{base} ({k})")).find(|n| !taken(n)).unwrap()
    }

    fn add_script(&mut self, s: Script) {
        self.save_dirty();
        let name = s.name.clone();
        if let Err(e) = model::save_script(&s) {
            self.say(format!("Erro ao salvar: {e}"));
        }
        self.scripts.push(s);
        self.resort(&name);
    }

    fn running(&self, name: &str) -> bool {
        self.runs.iter().any(|r| r.script == name && !r.done.load(Ordering::Relaxed))
    }

    fn show(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn panic(&mut self) {
        for r in &self.runs {
            r.stop.store(true, Ordering::SeqCst);
        }
        self.say("Parar tudo: todas as execuções foram interrompidas");
    }

    fn toggle_play(&mut self, i: usize, ctx: &egui::Context) {
        let Some(name) = self.scripts.get(i).map(|s| s.name.clone()) else { return };
        if self.running(&name) {
            for r in self.runs.iter().filter(|r| r.script == name) {
                r.stop.store(true, Ordering::SeqCst);
            }
        } else {
            self.start_with_target(i, Purpose::Play, ctx);
        }
    }

    fn toggle_record(&mut self, ctx: &egui::Context) {
        if self.recording.is_some() {
            self.hooks.send(Cmd::Record(None));
        } else if let Some(i) = self.sel {
            self.start_with_target(i, Purpose::Record, ctx);
        } else {
            self.say("Selecione um script para gravar");
        }
    }

    fn start_with_target(&mut self, i: usize, purpose: Purpose, ctx: &egui::Context) {
        let Some(rule) = self.scripts[i].target.clone() else {
            self.say(format!("'{}': defina a janela-alvo na aba Config", self.scripts[i].name));
            self.show(ctx);
            return;
        };
        let mut m = win::find_matches(&rule);
        match m.len() {
            0 => self.say(format!("Nenhuma janela aberta casa com {} / \"{}\"", rule.exe, rule.title)),
            1 => self.launch(i, purpose, m.remove(0), ctx),
            _ => {
                self.chooser = Some((i, purpose, m));
                self.show(ctx);
            }
        }
    }

    fn launch(&mut self, i: usize, purpose: Purpose, w: WinInfo, ctx: &egui::Context) {
        if !win::self_elevated() && win::is_elevated_pid(w.pid) {
            self.elevated_target = true;
            self.say(format!("'{}' roda como administrador: o Windows bloqueia input vindo do UltraTask", w.title));
            self.show(ctx);
            return;
        }
        match purpose {
            Purpose::Play => self.play_on(i, w, 0),
            Purpose::Record => {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                win::bring_to_front(w.hwnd);
                self.hooks.send(Cmd::Record(Some(w.hwnd)));
                self.recording = Some((i, w.hwnd));
                self.say(format!("Gravando em '{}'… {} para parar", w.title, self.settings.record.label()));
            }
        }
    }

    fn play_on(&mut self, i: usize, w: WinInfo, delay_ms: u64) {
        if !win::self_elevated() && win::is_elevated_pid(w.pid) {
            self.elevated_target = true;
            self.say(format!("'{}' roda como administrador: o Windows bloqueia input vindo do UltraTask", w.title));
            return;
        }
        self.save_dirty();
        let s = self.scripts[i].clone();
        if s.blocks.is_empty() {
            self.say(format!("'{}' não tem blocos", s.name));
            return;
        }
        let h = runner::spawn(s, self.scripts.clone(), w, &self.settings, delay_ms, self.log_tx.clone(), self.wake.clone());
        self.runs.push(h);
        // Keep finished runs around for inspecting their variables, but not forever.
        while self.runs.len() > 30 {
            match self.runs.iter().position(|r| r.done.load(Ordering::Relaxed)) {
                Some(k) => {
                    self.runs.remove(k);
                }
                None => break,
            }
        }
    }

    fn finish_record(&mut self, events: Vec<Event>, ctx: &egui::Context) {
        let Some((i, h)) = self.recording.take() else { return };
        self.show(ctx);
        if events.is_empty() || i >= self.scripts.len() {
            self.say("Gravação vazia (nada aconteceu na janela-alvo)");
            return;
        }
        let secs = events.last().map_or(0, |e| e.t) as f32 / 1000.0;
        self.say(format!("Gravação: {} eventos, {secs:.1}s", events.len()));
        if self.sel != Some(i) {
            self.select(i);
        }
        let s = &mut self.scripts[i];
        s.client_size = win::client_size(h);
        s.blocks.push(Block::Recording { events, speed: 1.0 });
        self.dirty = true;
        self.save_dirty();
    }

    fn start_pick(&mut self, ctx: &egui::Context, p: Pick) {
        if !matches!(p, Pick::Target) && self.sel_target().is_none() {
            self.say("Defina a janela-alvo (e deixe-a aberta) antes de capturar pontos");
            return;
        }
        self.pick = Some(p);
        self.hooks.send(Cmd::Pick);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        self.say(match self.pick {
            Some(Pick::Region(..)) => "Clique no canto SUPERIOR ESQUERDO da região",
            Some(Pick::Target) => "Clique na janela-alvo",
            _ => "Clique no ponto da janela-alvo",
        });
    }

    fn sel_target(&self) -> Option<WinInfo> {
        let rule = self.scripts.get(self.sel?)?.target.as_ref()?;
        win::find_matches(rule).into_iter().next()
    }

    fn on_pick(&mut self, x: i32, y: i32, root: Hwnd, ctx: &egui::Context) {
        let Some(p) = self.pick.take() else { return };
        if let Pick::Target = p {
            let w = win::window_info(root);
            if let Some(i) = self.sel {
                self.say(format!("Janela-alvo: {} ({})", w.title, w.exe));
                self.scripts[i].target = Some(TargetRule { exe: w.exe, title: w.title });
                self.dirty = true;
            }
            self.show(ctx);
            return;
        }
        let Some(t) = self.sel_target() else {
            self.show(ctx);
            return;
        };
        if root != t.hwnd {
            self.say("Aviso: o clique foi fora da janela-alvo");
        }
        let (cx, cy) = win::screen_to_client(t.hwnd, x, y);
        match p {
            Pick::Point(id) => {
                let mode = self.sel.map(|i| self.scripts[i].mode).unwrap_or_default();
                let color = win::capture_client(t.hwnd, mode).ok().and_then(|img| img.pixel(cx, cy)).unwrap_or_default();
                self.pick_result = Some((id, PickData::Point(cx, cy, color)));
            }
            Pick::Region(id, None) => {
                self.pick = Some(Pick::Region(id, Some((cx, cy))));
                self.hooks.send(Cmd::Pick);
                self.say("Agora clique no canto INFERIOR DIREITO");
                return;
            }
            Pick::Region(id, Some((x0, y0))) => {
                let r = Region { x: x0.min(cx), y: y0.min(cy), w: (cx - x0).abs(), h: (cy - y0).abs() };
                self.pick_result = Some((id, PickData::Region(r)));
            }
            Pick::Target => unreachable!(),
        }
        self.show(ctx);
    }

    fn rename_selected(&mut self, new: String) {
        let Some(i) = self.sel else { return };
        let new = new.trim().to_string();
        let old = self.scripts[i].name.clone();
        if new.is_empty() || new == old {
            return;
        }
        if self.scripts.iter().any(|s| s.name.eq_ignore_ascii_case(&new) && s.name != old) {
            self.say(format!("Já existe um script chamado '{new}'"));
            return;
        }
        let _ = model::delete_script(&old);
        self.scripts[i].name = new.clone();
        self.dirty = true;
        // Keep "Chamar script" blocks pointing at the renamed script.
        for (k, s) in self.scripts.iter_mut().enumerate() {
            if rename_calls(&mut s.blocks, &old, &new) && k != i {
                let _ = model::save_script(s);
            }
        }
        self.save_dirty();
        self.resort(&new);
    }

    fn delete_selected(&mut self) {
        let Some(i) = self.sel else { return };
        let s = self.scripts.remove(i);
        if let Err(e) = model::delete_script(&s.name) {
            self.say(format!("Erro ao excluir arquivo: {e}"));
        }
        self.dirty = false;
        self.sel = None;
        self.confirm_delete = false;
        self.register_hotkeys();
    }

    // ---------- panels ----------

    fn toolbar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let rec = self.recording.is_some();
            let rec_txt = if rec { "⏹ Parar gravação" } else { "⏺ Gravar" };
            if ui.add_enabled(self.sel.is_some() || rec, egui::Button::new(rec_txt)).on_hover_text(self.settings.record.label()).clicked() {
                self.toggle_record(ctx);
            }
            let sel_running = self.sel.is_some_and(|i| self.running(&self.scripts[i].name));
            if ui
                .add_enabled(self.sel.is_some(), egui::Button::new(if sel_running { "⏹ Parar" } else { "▶ Tocar" }))
                .on_hover_text(self.settings.play.label())
                .clicked()
            {
                self.toggle_play(self.sel.unwrap(), ctx);
            }
            if ui.add_enabled(!self.runs.is_empty(), egui::Button::new("⛔ Parar tudo")).on_hover_text(self.settings.panic.label()).clicked() {
                self.panic();
            }
            if ui.add_enabled(self.dirty, egui::Button::new("💾 Salvar")).on_hover_text("Ctrl+S").clicked() {
                self.save_dirty();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⚙ Ajustes").clicked() {
                    self.show_settings = !self.show_settings;
                }
                let dark = ui.ctx().theme() == egui::Theme::Dark;
                if ui.button(if dark { "☀" } else { "🌙" }).on_hover_text("Alternar tema claro/escuro").clicked() {
                    self.settings.theme = if dark { Theme::Light } else { Theme::Dark };
                    ui.ctx().set_theme(theme_pref(self.settings.theme));
                    if let Err(e) = model::save_settings(&self.settings) {
                        self.say(format!("Erro ao salvar ajustes: {e}"));
                    }
                }
                if rec {
                    ui.colored_label(Color32::from_rgb(230, 60, 60), "⏺ Gravando…");
                }
                if self.pick.is_some() {
                    ui.colored_label(Color32::from_rgb(230, 160, 30), "🎯 Aguardando clique…");
                }
            });
        });
        if self.elevated_target {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::from_rgb(230, 160, 30), "⚠ A janela-alvo roda como administrador.");
                if ui.button("Reiniciar UltraTask como administrador").clicked() {
                    self.save_dirty();
                    win::relaunch_as_admin();
                }
                if ui.small_button("✖").clicked() {
                    self.elevated_target = false;
                }
            });
        }
    }

    fn library(&mut self, ui: &mut Ui) {
        ui.heading("Scripts");
        ui.horizontal_wrapped(|ui| {
            if ui.button("🔄").on_hover_text("Recarregar scripts da pasta (ex.: criados por um agente de IA)").clicked() {
                self.reload();
            }
            if ui.button("➕ Novo").clicked() {
                let name = self.unique_name("Novo script");
                self.add_script(Script { name, ..Default::default() });
            }
            if let Some(i) = self.sel {
                if ui.button("📋 Duplicar").clicked() {
                    let mut s = self.scripts[i].clone();
                    s.name = self.unique_name(&format!("{} (cópia)", s.name));
                    s.hotkey = None;
                    self.add_script(s);
                }
                if ui.button("✏ Renomear").clicked() {
                    self.rename = Some(self.scripts[i].name.clone());
                }
                if ui.button("🗑 Excluir").clicked() {
                    self.confirm_delete = true;
                }
            }
        });
        if let Some(buf) = &mut self.rename {
            let mut done = None;
            ui.horizontal(|ui| {
                let r = ui.text_edit_singleline(buf);
                r.request_focus();
                if ui.button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    done = Some(true);
                }
                if ui.button("Cancelar").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
            match done {
                Some(true) => {
                    let n = self.rename.take().unwrap();
                    self.rename_selected(n);
                }
                Some(false) => self.rename = None,
                None => {}
            }
        }
        if self.confirm_delete {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::from_rgb(230, 60, 60), "Excluir de vez?");
                if ui.button("Sim").clicked() {
                    self.delete_selected();
                }
                if ui.button("Não").clicked() {
                    self.confirm_delete = false;
                }
            });
        }
        ui.separator();
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, s) in self.scripts.iter().enumerate() {
                let mut label = if self.running(&s.name) { format!("▶ {}", s.name) } else { s.name.clone() };
                if let Some(hk) = s.hotkey {
                    label += &format!("   [{}]", hk.label());
                }
                if ui.selectable_label(self.sel == Some(i), label).clicked() {
                    clicked = Some(i);
                }
            }
        });
        if let Some(i) = clicked {
            self.select(i);
        }
    }

    fn bottom(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong("Execuções");
            if self.runs.iter().any(|r| r.done.load(Ordering::Relaxed)) && ui.small_button("Limpar terminadas").clicked() {
                self.runs.retain(|r| !r.done.load(Ordering::Relaxed));
            }
        });
        if self.runs.is_empty() {
            ui.weak("nenhuma");
        }
        let mut dismiss = None;
        egui::ScrollArea::vertical().id_salt("runs").max_height(140.0).show(ui, |ui| {
            for (k, r) in self.runs.iter().enumerate() {
                let done = r.done.load(Ordering::Relaxed);
                let paused = r.dbg.paused.load(Ordering::Relaxed);
                ui.horizontal(|ui| {
                    if done {
                        if ui.small_button("✖").on_hover_text("Remover da lista").clicked() {
                            dismiss = Some(k);
                        }
                    } else {
                        if ui.small_button(if paused { "▶" } else { "⏸" }).on_hover_text(if paused { "Continuar" } else { "Pausar (debug)" }).clicked() {
                            r.dbg.paused.store(!paused, Ordering::SeqCst);
                        }
                        if ui.small_button("⏹").on_hover_text("Parar").clicked() {
                            r.stop.store(true, Ordering::SeqCst);
                        }
                    }
                    let status = if paused && !done { "⏸ pausado".to_string() } else { r.status.lock().unwrap().clone() };
                    ui.label(format!("{}  em  \"{}\"  —  {status}", r.script, r.window));
                    if !done {
                        ui.weak(format!("· {}", r.dbg.current.lock().unwrap()));
                    }
                });
                let vars = r.dbg.vars.lock().unwrap();
                if !vars.is_empty() {
                    ui.indent(k, |ui| {
                        egui::CollapsingHeader::new(format!("variáveis ({})", vars.len())).id_salt(("vars", k)).default_open(paused).show(ui, |ui| {
                            egui::Grid::new(("vg", k)).striped(true).show(ui, |ui| {
                                for (name, v) in vars.iter() {
                                    ui.monospace(name);
                                    ui.monospace(v);
                                    ui.end_row();
                                }
                            });
                        });
                    });
                }
            }
        });
        if let Some(k) = dismiss {
            self.runs.remove(k);
        }
        ui.separator();
        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            for l in &self.log {
                ui.monospace(l);
            }
        });
    }

    fn center(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(i) = self.sel else {
            ui.centered_and_justified(|ui| ui.label("Crie ou selecione um script à esquerda"));
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(&self.scripts[i].name);
            ui.add_space(16.0);
            ui.selectable_value(&mut self.tab, Tab::Blocks, "Blocos");
            ui.selectable_value(&mut self.tab, Tab::Config, "Config");
        });
        ui.separator();
        // ponytail: change detection by comparing against a clone; cheap since egui only repaints on input.
        let before = self.scripts[i].clone();
        let names: Vec<String> = self.scripts.iter().map(|s| s.name.clone()).collect();
        match self.tab {
            Tab::Blocks => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut ec = EditCtx { names: &names, pick_req: &mut self.pick_req, pick_result: &mut self.pick_result };
                    blocks_ui(ui, &mut self.scripts[i].blocks, Id::unique("blocks"), &mut ec);
                });
            }
            Tab::Config => egui::ScrollArea::vertical().show(ui, |ui| self.config_ui(ui, i)).inner,
        }
        if self.scripts[i] != before {
            self.dirty = true;
            if self.scripts[i].hotkey != before.hotkey {
                self.register_hotkeys();
            }
        }
        if let Some(p) = self.pick_req.take() {
            self.start_pick(ctx, p);
        }
    }

    fn config_ui(&mut self, ui: &mut Ui, i: usize) {
        let s = &mut self.scripts[i];
        let mut pick_target = false;
        egui::Grid::new("cfg").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label("Janela-alvo");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui.button("🎯 Clicar na janela").clicked() {
                        pick_target = true;
                    }
                    ComboBox::from_id_salt("winlist").selected_text("Escolher da lista").width(260.0).show_ui(ui, |ui| {
                        for w in win::list_windows() {
                            if ui.selectable_label(false, w.label()).clicked() {
                                s.target = Some(TargetRule { exe: w.exe, title: w.title });
                            }
                        }
                    });
                    if s.target.is_some() && ui.button("Limpar").clicked() {
                        s.target = None;
                    }
                });
                match &mut s.target {
                    Some(rule) => {
                        ui.horizontal(|ui| {
                            ui.label("Processo");
                            ui.text_edit_singleline(&mut rule.exe);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Título   ");
                            ui.add(egui::TextEdit::singleline(&mut rule.title).desired_width(320.0));
                        });
                        ui.weak("No título, * e ? são curingas (ex.: *Bloco de Notas)");
                        match win::find_matches(rule).len() {
                            0 => ui.colored_label(Color32::from_rgb(230, 160, 30), "⚠ nenhuma janela aberta casa com a regra"),
                            1 => ui.colored_label(Color32::from_rgb(60, 180, 90), "✔ casa com 1 janela"),
                            n => ui.label(format!("casa com {n} janelas — vai perguntar qual ao tocar")),
                        };
                    }
                    None => {
                        ui.weak("nenhuma definida");
                    }
                }
            });
            ui.end_row();

            ui.label("Modo");
            ui.vertical(|ui| {
                ui.radio_value(&mut s.mode, Mode::Background, "Segundo plano — não usa seu mouse/teclado (apps desktop)");
                ui.radio_value(&mut s.mode, Mode::Focus, "Foco — traz a janela para frente e usa o mouse/teclado reais (jogos)");
                match s.mode {
                    Mode::Background => {
                        ui.checkbox(&mut s.simulate_focus, "Simular foco (para apps/jogos que ignoram input quando inativos)");
                        ui.weak("Atalhos com Ctrl/Alt/Shift podem falhar em segundo plano; use o modo Foco nesses casos.");
                    }
                    Mode::Focus => {
                        ui.weak("Roda um script em modo Foco por vez. Se você mexer no mouse/teclado, ele pausa e retoma sozinho.");
                        ui.colored_label(Color32::from_rgb(230, 160, 30), "⚠ Jogos online com anti-cheat podem detectar input simulado e banir a conta.");
                    }
                }
            });
            ui.end_row();

            ui.label("Velocidade das gravações");
            ui.add(DragValue::new(&mut s.speed).range(0.1..=10.0).speed(0.05).suffix("×"));
            ui.end_row();

            ui.label("Repetições");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut s.repeat).range(0..=1_000_000));
                ui.weak(if s.repeat == 0 { "infinito" } else { "0 = infinito" });
            });
            ui.end_row();

            ui.label("Atalho do script");
            let mut hk = s.hotkey.unwrap_or_default();
            hotkey_ui(ui, &mut hk, "script_hk");
            s.hotkey = (hk.vk != 0).then_some(hk);
            ui.end_row();

            ui.label("Gatilho");
            ui.vertical(|ui| {
                let (mut kind, mut delay) = match s.trigger {
                    Trigger::Manual => (0, 0.0),
                    Trigger::OnStartup { delay_s } => (1, delay_s),
                    Trigger::OnWindowOpen { delay_s } => (2, delay_s),
                };
                ui.radio_value(&mut kind, 0, "Manual (botão ▶ ou atalho)");
                ui.radio_value(&mut kind, 1, "Ao iniciar o UltraTask (com \"Iniciar com o Windows\" = ao ligar o PC), assim que a janela-alvo existir");
                ui.radio_value(&mut kind, 2, "Sempre que uma nova janela-alvo abrir (ex.: ao iniciar o app/jogo)");
                if kind != 0 {
                    ui.horizontal(|ui| {
                        ui.label("Esperar");
                        ui.add(DragValue::new(&mut delay).range(0.0..=3600.0).speed(0.5).suffix(" s"));
                        ui.weak("antes de começar (tempo para o app carregar)");
                    });
                    if s.target.is_none() {
                        ui.colored_label(Color32::from_rgb(230, 160, 30), "⚠ gatilhos precisam de uma janela-alvo");
                    }
                }
                s.trigger = match kind {
                    1 => Trigger::OnStartup { delay_s: delay },
                    2 => Trigger::OnWindowOpen { delay_s: delay },
                    _ => Trigger::Manual,
                };
            });
            ui.end_row();

            ui.label("Debug");
            ui.checkbox(&mut s.trace, "Registrar cada bloco no log ao rodar");
            ui.end_row();

            ui.label("Tamanho gravado");
            match s.client_size {
                Some((w, h)) => {
                    ui.horizontal(|ui| {
                        ui.label(format!("{w}×{h} — a janela é ajustada para esse tamanho antes de tocar"));
                        if ui.button("Esquecer").clicked() {
                            s.client_size = None;
                        }
                    });
                }
                None => {
                    ui.weak("—");
                }
            }
            ui.end_row();
        });
        if pick_target {
            self.pick_req = Some(Pick::Target);
        }
    }

    fn modals(&mut self, ctx: &egui::Context) {
        if let Some((i, purpose, list)) = &self.chooser {
            let (i, purpose) = (*i, *purpose);
            let mut choice = None;
            let mut cancel = false;
            egui::Modal::new(Id::unique("chooser")).show(ctx, |ui| {
                ui.heading("Várias janelas casam com a regra — qual usar?");
                for w in list {
                    if ui.button(w.label()).clicked() {
                        choice = Some(w.clone());
                    }
                }
                ui.separator();
                if ui.button("Cancelar").clicked() {
                    cancel = true;
                }
            });
            if let Some(w) = choice {
                self.chooser = None;
                self.launch(i, purpose, w, ctx);
            } else if cancel {
                self.chooser = None;
            }
        }

        if self.show_settings {
            let mut open = true;
            let before = self.settings.clone();
            egui::Window::new("Ajustes").open(&mut open).resizable(false).show(ctx, |ui| {
                let st = &mut self.settings;
                egui::Grid::new("settings").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Gravar / parar gravação");
                    hotkey_ui(ui, &mut st.record, "hk_rec");
                    ui.end_row();
                    ui.label("Tocar / parar script selecionado");
                    hotkey_ui(ui, &mut st.play, "hk_play");
                    ui.end_row();
                    ui.label("Parar tudo (pânico)");
                    hotkey_ui(ui, &mut st.panic, "hk_panic");
                    ui.end_row();
                    ui.label("Modo Foco: retomar após parado por");
                    ui.add(DragValue::new(&mut st.idle_resume_s).range(0.5..=30.0).speed(0.1).suffix(" s"));
                    ui.end_row();
                    ui.label("OCR: ampliar a região");
                    ui.add(DragValue::new(&mut st.ocr_scale).range(1..=4).suffix("×"));
                    ui.end_row();
                    ui.label("Tema");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut st.theme, Theme::System, "Sistema");
                        ui.radio_value(&mut st.theme, Theme::Light, "Claro");
                        ui.radio_value(&mut st.theme, Theme::Dark, "Escuro");
                    });
                    ui.end_row();
                    ui.label("Segundo plano");
                    ui.checkbox(&mut st.start_with_windows, "Iniciar com o Windows (escondido na bandeja)");
                    ui.end_row();
                });
                ui.weak("Fechar a janela só esconde o UltraTask na bandeja; gatilhos e atalhos continuam ativos. Para sair: bandeja > Sair.");
                ui.separator();
                ui.weak(format!("Scripts ficam em {}", model::base_dir().join("scripts").display()));
            });
            self.show_settings = open;
            if self.settings.theme != before.theme {
                ctx.set_theme(theme_pref(self.settings.theme));
            }
            if self.settings.start_with_windows != before.start_with_windows {
                match win::set_autostart(self.settings.start_with_windows) {
                    Ok(()) => self.say(if self.settings.start_with_windows { "UltraTask vai iniciar com o Windows" } else { "UltraTask não inicia mais com o Windows" }),
                    Err(e) => {
                        self.say(e);
                        self.settings.start_with_windows = before.start_with_windows;
                    }
                }
            }
            if self.settings != before {
                if let Err(e) = model::save_settings(&self.settings) {
                    self.say(format!("Erro ao salvar ajustes: {e}"));
                }
                self.register_hotkeys();
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.hide_frames > 0 {
            self.hide_frames -= 1;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            ctx.request_repaint();
        }
        while let Ok(o) = self.hook_rx.try_recv() {
            match o {
                Out::Hotkey(hooks::RECORD_HOTKEY) => self.toggle_record(ctx),
                Out::Hotkey(hooks::PLAY_HOTKEY) => match self.sel {
                    Some(i) => self.toggle_play(i, ctx),
                    None => self.say("Nenhum script selecionado"),
                },
                Out::Hotkey(hooks::PANIC_HOTKEY) => self.panic(),
                Out::Hotkey(id) => self.toggle_play((id - hooks::SCRIPT_HOTKEY) as usize, ctx),
                Out::Recorded(ev) => self.finish_record(ev, ctx),
                Out::Picked { x, y, root } => self.on_pick(x, y, root, ctx),
                Out::Error(e) => self.say(e),
            }
        }
        while let Ok(l) = self.log_rx.try_recv() {
            self.say(l);
        }
        while let Ok(m) = self.tray_rx.try_recv() {
            match m {
                TrayMsg::Show => self.show(ctx),
                TrayMsg::Panic => self.panic(),
                TrayMsg::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        self.poll_triggers();
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self._tray.is_some() {
            self.save_dirty();
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        if self.runs.iter().any(|r| !r.done.load(Ordering::Relaxed)) || self.recording.is_some() {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_dirty();
        }
        egui::Panel::top("top").show(ui, |ui| self.toolbar(ui, &ctx));
        egui::Panel::bottom("bottom").resizable(true).default_size(170.0).show(ui, |ui| self.bottom(ui));
        egui::Panel::left("left").resizable(true).default_size(240.0).show(ui, |ui| self.library(ui));
        egui::CentralPanel::default().show(ui, |ui| self.center(ui, &ctx));
        self.modals(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_dirty();
        self.panic();
    }
}

// ---------- block editor ----------

struct EditCtx<'a> {
    names: &'a [String],
    pick_req: &'a mut Option<Pick>,
    pick_result: &'a mut Option<(Id, PickData)>,
}

impl EditCtx<'_> {
    fn take(&mut self, id: Id) -> Option<PickData> {
        match *self.pick_result {
            Some((pid, d)) if pid == id => {
                *self.pick_result = None;
                Some(d)
            }
            _ => None,
        }
    }
}

fn rename_calls(blocks: &mut [Block], old: &str, new: &str) -> bool {
    let mut hit = false;
    for b in blocks {
        match b {
            Block::Call { script } if script == old => {
                *script = new.into();
                hit = true;
            }
            Block::If { then, else_, .. } => hit |= rename_calls(then, old, new) | rename_calls(else_, old, new),
            Block::Repeat { body, .. } | Block::While { body, .. } => hit |= rename_calls(body, old, new),
            _ => {}
        }
    }
    hit
}

fn blocks_ui(ui: &mut Ui, blocks: &mut Vec<Block>, id: Id, ec: &mut EditCtx) {
    editor::tree_ui(ui, blocks, id, &mut |ui, b, bid| block_fields(ui, b, bid, ec));
}

fn expr_edit(ui: &mut Ui, s: &mut String, width: f32) {
    let ok = model::expr_error(s).is_none();
    let mut te = egui::TextEdit::singleline(s).desired_width(width);
    if !ok {
        te = te.text_color(Color32::from_rgb(230, 60, 60));
    }
    ui.add(te).on_hover_text(if ok { "número ou expressão (ex.: x + 10, rand(1, 5))" } else { "expressão inválida" });
}

fn xy_ui(ui: &mut Ui, x: &mut String, y: &mut String, id: Id, ec: &mut EditCtx) -> Option<[u8; 3]> {
    ui.label("x");
    expr_edit(ui, x, 90.0);
    ui.label("y");
    expr_edit(ui, y, 90.0);
    if ui.small_button("📍").on_hover_text("Capturar clicando na janela-alvo").clicked() {
        *ec.pick_req = Some(Pick::Point(id));
    }
    if let Some(PickData::Point(px, py, c)) = ec.take(id) {
        *x = px.to_string();
        *y = py.to_string();
        return Some(c);
    }
    None
}

fn region_ui(ui: &mut Ui, r: &mut Region, id: Id, ec: &mut EditCtx) {
    for (l, v) in [("x", &mut r.x), ("y", &mut r.y), ("larg", &mut r.w), ("alt", &mut r.h)] {
        ui.label(l);
        ui.add(DragValue::new(v));
    }
    if ui.small_button("📍").on_hover_text("Capturar a região com dois cliques").clicked() {
        *ec.pick_req = Some(Pick::Region(id, None));
    }
    if let Some(PickData::Region(nr)) = ec.take(id) {
        *r = nr;
    }
}

fn key_combo(ui: &mut Ui, vk: &mut u16, id: impl std::hash::Hash + std::fmt::Debug, allow_none: bool) {
    ComboBox::from_id_salt(id).selected_text(if *vk == 0 { "—".into() } else { key_name(*vk) }).height(320.0).show_ui(ui, |ui| {
        if allow_none {
            ui.selectable_value(vk, 0, "—");
        }
        for k in all_keys() {
            ui.selectable_value(vk, k, key_name(k));
        }
    });
}

fn hotkey_ui(ui: &mut Ui, hk: &mut Hotkey, id: &str) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut hk.ctrl, "Ctrl");
        ui.checkbox(&mut hk.shift, "Shift");
        ui.checkbox(&mut hk.alt, "Alt");
        ui.checkbox(&mut hk.win, "Win");
        key_combo(ui, &mut hk.vk, id, true);
    });
}

fn enum_combo<T: PartialEq + Copy>(ui: &mut Ui, v: &mut T, id: Id, opts: &[(T, &str)]) {
    let cur = opts.iter().find(|o| o.0 == *v).map_or("?", |o| o.1);
    ComboBox::from_id_salt(id).selected_text(cur).show_ui(ui, |ui| {
        for (o, l) in opts {
            ui.selectable_value(v, *o, *l);
        }
    });
}

const BTNS: &[(Btn, &str)] = &[(Btn::Left, "esquerdo"), (Btn::Right, "direito"), (Btn::Middle, "meio")];

/// Returns true when "Explodir" was clicked on a recording block.
fn block_fields(ui: &mut Ui, b: &mut Block, id: Id, ec: &mut EditCtx) -> bool {
    match b {
        Block::Recording { events, speed } => {
            let secs = events.last().map_or(0, |e| e.t) as f32 / 1000.0;
            ui.label(format!("{} eventos, {secs:.1}s", events.len()));
            ui.add(DragValue::new(speed).range(0.1..=10.0).speed(0.05).prefix("velocidade ").suffix("×"));
            return ui.button("Explodir em passos").on_hover_text("Converte em Clique/Tecla/Esperar editáveis (descarta movimentos soltos)").clicked();
        }
        Block::Move { x, y } => {
            xy_ui(ui, x, y, id, ec);
        }
        Block::Click { x, y, button, kind, jitter_px } => {
            enum_combo(ui, button, id.with("b"), BTNS);
            enum_combo(ui, kind, id.with("k"), &[(ClickKind::Click, "clique"), (ClickKind::Double, "duplo"), (ClickKind::Down, "pressionar"), (ClickKind::Up, "soltar")]);
            xy_ui(ui, x, y, id, ec);
            ui.add(DragValue::new(jitter_px).range(0..=100).prefix("± ").suffix(" px"));
        }
        Block::Key { vk, action } => {
            key_combo(ui, vk, id.with("vk"), false);
            enum_combo(ui, action, id.with("a"), &[(KeyAction::Press, "apertar"), (KeyAction::Down, "pressionar"), (KeyAction::Up, "soltar")]);
        }
        Block::Type { text } => {
            ui.add(egui::TextEdit::singleline(text).desired_width(320.0).hint_text("texto; use {variavel} ou {expressão}"));
        }
        Block::Scroll { x, y, delta } => {
            xy_ui(ui, x, y, id, ec);
            ui.add(DragValue::new(delta).range(-2400..=2400).speed(10).prefix("delta "));
            ui.weak("(120 = um passo; negativo = para baixo)");
        }
        Block::Wait { ms, jitter_ms } => {
            expr_edit(ui, ms, 80.0);
            ui.label("ms");
            ui.add(DragValue::new(jitter_ms).range(0..=60_000).prefix("± ").suffix(" ms"));
        }
        Block::WaitUntil { cond, timeout_ms, on_timeout } => {
            cond_ui(ui, cond, id.with("c"), ec);
            ui.label("timeout");
            expr_edit(ui, timeout_ms, 70.0);
            ui.label("ms, então");
            enum_combo(ui, on_timeout, id.with("t"), &[(OnTimeout::Stop, "parar com erro"), (OnTimeout::Continue, "continuar")]);
        }
        Block::If { cond, .. } | Block::While { cond, .. } => cond_ui(ui, cond, id.with("c"), ec),
        Block::Repeat { times, .. } => {
            expr_edit(ui, times, 60.0);
            ui.label("vezes");
        }
        Block::SetVar { name, expr } => {
            ui.add(egui::TextEdit::singleline(name).desired_width(100.0));
            ui.label("=");
            expr_edit(ui, expr, 240.0);
        }
        Block::OcrToVar { name, region } => {
            ui.add(egui::TextEdit::singleline(name).desired_width(100.0));
            ui.label("= texto da região");
            region_ui(ui, region, id, ec);
        }
        Block::Call { script } => {
            ComboBox::from_id_salt(id.with("call")).selected_text(if script.is_empty() { "escolha…" } else { script.as_str() }).show_ui(ui, |ui| {
                for n in ec.names {
                    ui.selectable_value(script, n.clone(), n);
                }
            });
        }
        Block::Log { text } => {
            ui.add(egui::TextEdit::singleline(text).desired_width(320.0).hint_text("ex.: contador = {contador}"));
        }
        Block::Breakpoint => {
            ui.weak("pausa a execução aqui; veja as variáveis no painel e clique ▶ para continuar");
        }
        Block::Stop => {
            ui.weak("encerra o script aqui");
        }
    }
    false
}

fn cond_ui(ui: &mut Ui, c: &mut Condition, id: Id, ec: &mut EditCtx) {
    ui.checkbox(&mut c.negate, "não");
    let kinds: [(&str, fn() -> CondKind); 6] = [
        ("pixel tem a cor", || CondKind::Pixel { x: "0".into(), y: "0".into(), color: [255, 255, 255], tol: 10 }),
        ("texto (OCR) contém", || CondKind::OcrContains { region: Region { x: 0, y: 0, w: 200, h: 40 }, text: String::new() }),
        ("janela existe", || CondKind::WindowExists),
        ("janela está em foco", || CondKind::WindowForeground),
        ("título casa com", || CondKind::TitleMatches("*".into())),
        ("expressão", || CondKind::Expr("contador < 10".into())),
    ];
    let same = |a: &CondKind, b: &CondKind| std::mem::discriminant(a) == std::mem::discriminant(b);
    let cur = kinds.iter().find(|k| same(&k.1(), &c.kind)).map_or("?", |k| k.0);
    ComboBox::from_id_salt(id.with("kind")).selected_text(cur).show_ui(ui, |ui| {
        for (l, make) in kinds {
            if ui.selectable_label(same(&make(), &c.kind), l).clicked() && !same(&make(), &c.kind) {
                c.kind = make();
            }
        }
    });
    match &mut c.kind {
        CondKind::Pixel { x, y, color, tol } => {
            if let Some(picked) = xy_ui(ui, x, y, id, ec) {
                *color = picked;
            }
            egui::color_picker::color_edit_button_srgb(ui, color);
            ui.add(DragValue::new(tol).range(0..=255).prefix("tolerância "));
        }
        CondKind::OcrContains { region, text } => {
            region_ui(ui, region, id, ec);
            ui.add(egui::TextEdit::singleline(text).desired_width(140.0).hint_text("texto"));
        }
        CondKind::TitleMatches(p) => {
            ui.add(egui::TextEdit::singleline(p).desired_width(200.0));
        }
        CondKind::Expr(s) => expr_edit(ui, s, 240.0),
        CondKind::WindowExists | CondKind::WindowForeground => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every non-ASCII char inside a string literal must exist in egui's bundled fonts,
    /// otherwise it renders as an empty box.
    #[test]
    fn ui_glyphs_exist() {
        let ctx = egui::Context::default();
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear(); // loads the fonts
        let src = [include_str!("ui.rs"), include_str!("model.rs"), include_str!("runner.rs"), include_str!("main.rs"), include_str!("editor.rs"), include_str!("tree.rs")].concat();
        let mut chars = std::collections::BTreeSet::new();
        for line in src.lines().filter(|l| !l.trim_start().starts_with("//")) {
            let mut in_str = false;
            let mut prev = ' ';
            for c in line.chars() {
                if c == '"' && prev != '\\' {
                    in_str = !in_str;
                } else if in_str && !c.is_ascii() {
                    chars.insert(c);
                }
                prev = c;
            }
        }
        let missing: String = ctx.fonts_mut(|f| chars.into_iter().filter(|&c| !f.has_glyph(&egui::FontId::proportional(14.0), c) || !f.has_glyph(&egui::FontId::monospace(14.0), c)).collect());
        assert!(missing.is_empty(), "sem glifo: {missing:?}");
    }
}
