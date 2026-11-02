#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod compat;
mod export;
mod hooks;
mod model;
mod runner;
mod ui;
mod win;

use std::sync::Arc;
use std::sync::mpsc::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayMsg {
    Show,
    Panic,
    Quit,
}

/// 32×32 orange disc with a white "play" triangle, generated so we ship no image assets.
fn icon_rgba() -> Vec<u8> {
    let mut px = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let (fx, fy) = (x as f32 - 15.5, y as f32 - 15.5);
            let i = (y * 32 + x) * 4;
            if fx * fx + fy * fy <= 15.5 * 15.5 {
                let tri = x >= 11 && x <= 23 && (fy.abs() as f32) <= (x as f32 - 11.0) * -0.62 + 7.5;
                let c = if tri { [255, 255, 255] } else { [232, 89, 12] };
                px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    px
}

pub fn make_tray(tx: Sender<TrayMsg>, wake: Arc<dyn Fn() + Send + Sync>) -> Option<TrayIcon> {
    let (show, panic, quit) = (MenuItem::new("Mostrar", true, None), MenuItem::new("Parar tudo", true, None), MenuItem::new("Sair", true, None));
    let menu = Menu::new();
    menu.append_items(&[&show, &panic, &quit]).ok()?;
    let ids = (show.id().clone(), panic.id().clone());
    let (t1, w1) = (tx.clone(), wake.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let m = if e.id == ids.0 {
            TrayMsg::Show
        } else if e.id == ids.1 {
            TrayMsg::Panic
        } else {
            TrayMsg::Quit
        };
        let _ = t1.send(m);
        w1();
    }));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            let _ = tx.send(TrayMsg::Show);
            wake();
        }
    }));
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("UltraTask")
        .with_icon(Icon::from_rgba(icon_rgba(), 32, 32).ok()?)
        .build()
        .ok()
}

fn main() -> eframe::Result {
    // Um .exe exportado (ticket 05) só toca o script embutido; o editor nem abre.
    match export::read_own_payload() {
        Some(Ok(p)) => std::process::exit(export::run_player(p)),
        Some(Err(e)) => {
            compat::error_box(&format!("Este .exe exportado está corrompido: {e}"));
            std::process::exit(1);
        }
        None => {}
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--validate") {
        // Release builds have no console of their own; borrow the caller's for the report.
        unsafe {
            let _ = windows::Win32::System::Console::AttachConsole(windows::Win32::System::Console::ATTACH_PARENT_PROCESS);
        }
        std::process::exit(model::validate_files(&args[1..]));
    }
    // A second launch just pops up the running instance (e.g. the hidden autostart one).
    let Some(show_event) = win::single_instance() else { return Ok(()) };
    let hidden = args.iter().any(|a| a == "--tray");
    let render = compat::render_from_args(&args);
    let mut opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("UltraTask")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([640.0, 420.0])
            .with_icon(eframe::egui::IconData { rgba: icon_rgba(), width: 32, height: 32 }),
        ..Default::default()
    };
    compat::apply_render(render, &mut opts);
    // OpenGL ausente (VM/RDP sem GPU) faz o run_native falhar antes de criar o app. O event loop
    // do winit não pode ser recriado, então o fallback para software é relançar o exe.
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s2 = started.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            "UltraTask",
            opts,
            Box::new(move |cc| {
                s2.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(Box::new(ui::App::new(cc, Some(show_event), hidden)))
            }),
        )
    }));
    let err = match result {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(e)) => e.to_string(),
        Err(_) => "pânico ao abrir a janela".to_string(),
    };
    if let Some(a) = compat::relaunch_args(&args, render, started.load(std::sync::atomic::Ordering::SeqCst))
        && compat::relaunch(&a, show_event).is_ok()
    {
        return Ok(());
    }
    compat::error_box(&format!("O UltraTask não conseguiu abrir a janela ({}): {err}", if render == compat::Render::Software { "Direct3D 12 / WARP" } else { "OpenGL" }));
    std::process::exit(1);
}
