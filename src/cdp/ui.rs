//! Config tab section of the "Navegador (CDP)" mode.
use crate::model::{CdpLaunch, Trigger};
use eframe::egui::{Color32, DragValue, TextEdit, Ui};

pub const MODE_LABEL: &str = "Navegador (CDP) — Chrome, Edge, Electron e WebView2, sem usar seu mouse/teclado";

pub fn config_ui(ui: &mut Ui, c: &mut CdpLaunch, trigger: Trigger) {
    ui.horizontal(|ui| {
        ui.label("Programa  ");
        ui.add(TextEdit::singleline(&mut c.exe).hint_text(r"C:\...\chrome.exe (vazio = conectar a um app já aberto)").desired_width(360.0));
    });
    ui.horizontal(|ui| {
        ui.label("Argumentos");
        ui.add(TextEdit::singleline(&mut c.args).hint_text("https://exemplo.com").desired_width(360.0));
    });
    ui.horizontal(|ui| {
        ui.label("Porta     ");
        ui.add(DragValue::new(&mut c.port).range(0..=65535));
        ui.weak(if c.port == 0 { "0 = uma porta livre por execução (várias instâncias juntas)" } else { "fixa: só uma instância por vez" });
    });
    ui.checkbox(&mut c.webview2, "App WebView2 (a porta vai por WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS)");
    ui.checkbox(&mut c.keep_open, "Deixar o app aberto no fim (o perfil temporário fica no disco)");
    for e in super::validate(c, trigger) {
        ui.colored_label(Color32::from_rgb(230, 160, 30), format!("⚠ {e}"));
    }
    ui.weak("Ao tocar, abre o app com depuração remota em 127.0.0.1 e um perfil temporário próprio, e escolhe a página cujo título casa com o Título da janela-alvo.");
    ui.weak("Clique, Mover, Rolar, Tecla e Digitar viram eventos do protocolo; Pixel e OCR usam a captura da própria página, mesmo com a janela coberta.");
    ui.colored_label(Color32::from_rgb(230, 160, 30), "⚠ Enquanto o app roda, qualquer programa deste PC pode controlá-lo pela porta. Use perfis descartáveis.");
}
