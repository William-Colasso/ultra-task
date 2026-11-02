//! Aba "Integrações" do editor de scripts.
use super::*;
use eframe::egui::{self, Color32, ComboBox, DragValue, Id, Ui};

const RED: Color32 = Color32::from_rgb(230, 60, 60);

/// Destinos oferecidos no editor: rótulo + valor novo. Um destino novo entra aqui e em `dest_ui`.
const DESTS: &[(&str, fn() -> Dest)] = &[("REST (HTTP/HTTPS)", || Dest::Http(HttpDest::default()))];

pub fn integrations_ui(ui: &mut Ui, list: &mut Vec<Integration>) {
    ui.weak("As integrações rodam em paralelo e nunca travam os blocos. O fluxo reage pelas variáveis de resultado (ex.: Esperar até !r_pendente).");
    ui.add_space(6.0);
    let mut remove = None;
    for (i, it) in list.iter_mut().enumerate() {
        let id = ui.scope_id().with(("integracao", i));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Nome");
                ui.add(egui::TextEdit::singleline(&mut it.name).desired_width(220.0));
                if ui.small_button("✖").on_hover_text("Remover integração").clicked() {
                    remove = Some(i);
                }
            });
            egui::Grid::new(id.with("grid")).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Gatilho");
                ui.vertical(|ui| when_ui(ui, &mut it.when, id.with("when")));
                ui.end_row();
                ui.label("Destino");
                ui.vertical(|ui| dest_ui(ui, &mut it.dest, id.with("dest")));
                ui.end_row();
                ui.label("Envio");
                ui.vertical(|ui| payload_ui(ui, &mut it.payload, id.with("payload")));
                ui.end_row();
                ui.label("Resultado");
                ui.vertical(|ui| result_ui(ui, &mut it.result));
                ui.end_row();
            });
            let mut errs = vec![];
            validate(std::slice::from_ref(it), &mut errs);
            for e in errs {
                ui.colored_label(RED, format!("⚠ {}", e.trim_start_matches("integrations[0].")));
            }
        });
        ui.add_space(6.0);
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    if ui.button("➕ Nova integração").clicked() {
        list.push(Integration::default());
    }
}

/// Lista de nomes de variáveis, um campo por nome.
fn names_ui(ui: &mut Ui, names: &mut Vec<String>, id: Id) {
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        for (k, n) in names.iter_mut().enumerate() {
            let bad = !crate::model::is_ident(n);
            let mut te = egui::TextEdit::singleline(n).desired_width(90.0).id(id.with(k));
            if bad {
                te = te.text_color(RED);
            }
            ui.add(te);
            if ui.small_button("✖").on_hover_text("Tirar esta variável").clicked() {
                remove = Some(k);
            }
        }
        if ui.small_button("➕ variável").clicked() {
            names.push(String::new());
        }
    });
    if let Some(k) = remove {
        names.remove(k);
    }
}

fn interp_edit(ui: &mut Ui, s: &mut String, width: f32, multiline: bool) {
    let err = interp_error(s);
    let mut te = if multiline { egui::TextEdit::multiline(s).desired_rows(2) } else { egui::TextEdit::singleline(s) }.desired_width(width);
    if err.is_some() {
        te = te.text_color(RED);
    }
    ui.add(te).on_hover_text(err.unwrap_or_else(|| "aceita {expressão}".into()));
}

fn when_ui(ui: &mut Ui, w: &mut When, id: Id) {
    let kinds = ["Ao definir variável", "A cada N segundos", "Ao terminar a execução"];
    let cur = match w {
        When::OnSet(_) => 0,
        When::Every { .. } => 1,
        When::OnEnd => 2,
    };
    let mut sel = cur;
    ComboBox::from_id_salt(id).selected_text(kinds[cur]).show_ui(ui, |ui| {
        for (k, l) in kinds.iter().enumerate() {
            ui.selectable_value(&mut sel, k, *l);
        }
    });
    if sel != cur {
        *w = [When::OnSet(vec![]), When::Every { s: 5.0 }, When::OnEnd][sel].clone();
    }
    match w {
        When::OnSet(names) => {
            ui.weak("dispara quando um bloco define uma destas variáveis:");
            names_ui(ui, names, id.with("names"));
        }
        When::Every { s } => {
            ui.add(DragValue::new(s).range(0.1..=86400.0).speed(0.1).prefix("a cada ").suffix(" s"));
        }
        When::OnEnd => {
            ui.weak("depois do último bloco; a espera é limitada pelo timeout");
        }
    }
}

fn dest_ui(ui: &mut Ui, d: &mut Dest, id: Id) {
    let cur = d.get().kind();
    let label = DESTS.iter().find(|(_, mk)| mk().get().kind() == cur).map_or(cur, |x| x.0);
    ComboBox::from_id_salt(id).selected_text(label).show_ui(ui, |ui| {
        for (l, mk) in DESTS {
            let fresh = mk();
            if ui.selectable_label(fresh.get().kind() == cur, *l).clicked() && fresh.get().kind() != cur {
                *d = fresh;
            }
        }
    });
    match d {
        Dest::Http(h) => http_ui(ui, h, id),
    }
}

fn http_ui(ui: &mut Ui, h: &mut HttpDest, id: Id) {
    ui.horizontal(|ui| {
        ComboBox::from_id_salt(id.with("method")).width(80.0).selected_text(h.method.as_str()).show_ui(ui, |ui| {
            for m in Method::ALL {
                ui.selectable_value(&mut h.method, m, m.as_str());
            }
        });
        interp_edit(ui, &mut h.url, 380.0, false);
    });
    ui.horizontal(|ui| {
        ui.label("timeout");
        ui.add(DragValue::new(&mut h.timeout_ms).range(1..=600_000).speed(100.0).suffix(" ms"));
        if h.method == Method::Get {
            ui.weak("(GET não envia corpo; use {variável} na URL)");
        }
    });
    let mut remove = None;
    for (k, (n, v)) in h.headers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(n).desired_width(140.0).hint_text("Cabeçalho"));
            ui.label(":");
            interp_edit(ui, v, 240.0, false);
            if ui.small_button("✖").on_hover_text("Remover cabeçalho").clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        h.headers.remove(k);
    }
    if ui.small_button("➕ cabeçalho").clicked() {
        h.headers.push((String::new(), String::new()));
    }
}

fn payload_ui(ui: &mut Ui, p: &mut Payload, id: Id) {
    let kinds = ["JSON das variáveis", "Linha CSV", "Texto com {…}"];
    let cur = match p {
        Payload::Json(_) => 0,
        Payload::Csv(_) => 1,
        Payload::Text(_) => 2,
    };
    let mut sel = cur;
    ComboBox::from_id_salt(id).selected_text(kinds[cur]).show_ui(ui, |ui| {
        for (k, l) in kinds.iter().enumerate() {
            ui.selectable_value(&mut sel, k, *l);
        }
    });
    if sel != cur {
        // Mantém as variáveis escolhidas ao trocar entre JSON e CSV.
        let names = match p {
            Payload::Json(n) | Payload::Csv(n) => std::mem::take(n),
            Payload::Text(_) => vec![],
        };
        *p = match sel {
            0 => Payload::Json(names),
            1 => Payload::Csv(names),
            _ => Payload::Text(String::new()),
        };
    }
    match p {
        Payload::Json(n) | Payload::Csv(n) => names_ui(ui, n, id.with("names")),
        Payload::Text(t) => interp_edit(ui, t, 380.0, true),
    }
}

fn result_ui(ui: &mut Ui, r: &mut Option<String>) {
    ui.horizontal(|ui| {
        let mut on = r.is_some();
        if ui.checkbox(&mut on, "guardar a resposta em").changed() {
            *r = on.then(|| "resposta".to_string());
        }
        if let Some(name) = r {
            let bad = !crate::model::is_ident(name);
            let mut te = egui::TextEdit::singleline(name).desired_width(120.0);
            if bad {
                te = te.text_color(RED);
            }
            ui.add(te);
        }
    });
    if let Some(n) = r {
        ui.weak(format!("cria {n}, {n}_ok, {n}_status (0 = erro de rede) e {n}_pendente"));
    }
}
