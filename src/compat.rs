//! Rodar em qualquer Windows 10/11 x64 sem instalar nada (ticket 06).
//!
//! - **Renderização:** OpenGL (glow) é o caminho normal. Sem GPU, o Windows só oferece o
//!   OpenGL 1.1 do "GDI Generic" e o egui_glow recusa (precisa de 2.0+). Como o event loop do
//!   winit não pode ser recriado no mesmo processo, o exe se relança com `--render-software`,
//!   que abre a janela com wgpu/Direct3D 12. Sem GPU, o D3D12 usa o WARP, o rasterizador por
//!   software que vem no Windows 10/11.
//! - **Pasta sem escrita:** scripts e ajustes vão para `%LOCALAPPDATA%\UltraTask`.
//! - **OCR ausente:** mensagem que diz como instalar um idioma com OCR.
//!
//! As decisões ficam em funções puras, testadas abaixo; o resto só chama o Windows.
use eframe::egui_wgpu::{self, wgpu};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

// ---------- renderização ----------

/// Argumento que liga a renderização por software (wgpu/D3D12, WARP sem GPU).
pub const SOFTWARE_ARG: &str = "--render-software";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Render {
    OpenGl,
    Software,
}

pub fn render_from_args(args: &[String]) -> Render {
    if args.iter().any(|a| a == SOFTWARE_ARG) { Render::Software } else { Render::OpenGl }
}

/// Depois que `run_native` falhou: os argumentos para relançar o exe em modo software, ou
/// `None` se não vale relançar (já estava em software, ou o app chegou a abrir e a falha veio
/// depois, quando relançar reabriria uma janela que o usuário já usou).
pub fn relaunch_args(args: &[String], render: Render, app_started: bool) -> Option<Vec<String>> {
    if render == Render::Software || app_started {
        return None;
    }
    let mut a = args.to_vec();
    a.push(SOFTWARE_ARG.into());
    Some(a)
}

/// Escolhe o adaptador do wgpu: `adapters` traz (tipo, aceita a superfície da janela).
/// Prefere uma GPU de verdade (o OpenGL pode ter falhado só por driver ruim); sem nenhuma,
/// usa o WARP (`DeviceType::Cpu`).
pub fn pick_adapter(adapters: &[(wgpu::DeviceType, bool)]) -> Option<usize> {
    let ok = |cpu: bool| adapters.iter().position(|&(t, s)| s && (t == wgpu::DeviceType::Cpu) == cpu);
    ok(false).or_else(|| ok(true))
}

/// Configura o renderizador do eframe. Com as features `glow` e `wgpu` juntas, o padrão do
/// eframe é wgpu, então o OpenGL é escolhido explicitamente aqui.
pub fn apply_render(render: Render, opts: &mut eframe::NativeOptions) {
    match render {
        Render::OpenGl => opts.renderer = eframe::Renderer::Glow,
        Render::Software => {
            opts.renderer = eframe::Renderer::Wgpu;
            let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
            setup.instance_descriptor.backends = wgpu::Backends::DX12;
            setup.native_adapter_selector = Some(Arc::new(|adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
                let list: Vec<_> = adapters.iter().map(|a| (a.get_info().device_type, surface.is_none_or(|s| a.is_surface_supported(s)))).collect();
                pick_adapter(&list).map(|i| adapters[i].clone()).ok_or_else(|| "nenhum adaptador Direct3D 12 (nem o WARP) aceita a janela".to_string())
            }));
            opts.wgpu_options.wgpu_setup = egui_wgpu::WgpuSetup::CreateNew(setup);
        }
    }
}

/// Linha para o log quando a janela abriu pelo caminho de software.
pub fn render_note(cc: &eframe::CreationContext<'_>) -> Option<String> {
    let rs = cc.wgpu_render_state.as_ref()?;
    let info = rs.adapter.get_info();
    let kind = if info.device_type == wgpu::DeviceType::Cpu { "por software" } else { "pela GPU" };
    Some(format!("OpenGL indisponível: a janela usa Direct3D 12 {kind} ({})", info.name))
}

/// Relança o exe com `args`. Fecha antes o evento de instância única, senão o novo processo
/// acharia este e sairia.
pub fn relaunch(args: &[String], show_event: isize) -> std::io::Result<()> {
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(show_event as *mut _));
    }
    std::process::Command::new(std::env::current_exe()?).args(args).spawn().map(|_| ())
}

/// Caixa de erro para quando nem o caminho de software abriu (o exe não tem console).
pub fn error_box(msg: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let text: Vec<u16> = msg.encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "UltraTask".encode_utf16().chain([0]).collect();
    unsafe {
        MessageBoxW(None, windows::core::PCWSTR(text.as_ptr()), windows::core::PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR);
    }
}

// ---------- pasta de dados ----------

#[derive(Clone, Debug, PartialEq)]
pub struct DataDir {
    pub path: PathBuf,
    /// Aviso para o log quando a pasta do exe não aceita escrita.
    pub warning: Option<String>,
}

/// Onde ficam scripts e ajustes: ao lado do exe (portátil, como sempre foi) ou, se essa pasta
/// não aceita escrita (ex.: Arquivos de Programas), em `%LOCALAPPDATA%\UltraTask`.
pub fn choose_data_dir(exe_dir: &Path, exe_dir_writable: bool, local_appdata: Option<&Path>) -> DataDir {
    if exe_dir_writable {
        return DataDir { path: exe_dir.to_path_buf(), warning: None };
    }
    match local_appdata.filter(|p| !p.as_os_str().is_empty()) {
        Some(l) => {
            let path = l.join("UltraTask");
            let warning = Some(format!("A pasta do UltraTask ({}) não aceita escrita: scripts e ajustes ficam em {}", exe_dir.display(), path.display()));
            DataDir { path, warning }
        }
        None => DataDir {
            path: exe_dir.to_path_buf(),
            warning: Some(format!("A pasta do UltraTask ({}) não aceita escrita e %LOCALAPPDATA% não existe: scripts e ajustes não serão salvos", exe_dir.display())),
        },
    }
}

/// Cria e apaga um arquivo de teste: é o único jeito confiável de saber (ACLs, mídia somente
/// leitura, pasta de rede) se dá para gravar ali.
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".ultratask-escrita-{}.tmp", std::process::id()));
    let ok = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    if ok {
        let _ = std::fs::remove_file(&probe);
    }
    ok
}

/// Decidida uma vez por processo, na primeira vez que alguém precisa da pasta.
pub fn data_dir() -> &'static DataDir {
    static DIR: OnceLock<DataDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        choose_data_dir(&exe_dir, dir_writable(&exe_dir), local.as_deref())
    })
}

// ---------- OCR ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OcrChoice {
    /// Um idioma do perfil do usuário tem OCR.
    UserProfile,
    /// Nenhum idioma do perfil tem OCR, mas outro idioma com OCR está instalado: usa o primeiro.
    FirstInstalled,
    /// Nenhum idioma com OCR: falha com [`ocr_missing_message`].
    Missing,
}

pub fn choose_ocr(profile_engine_ok: bool, installed_langs: usize) -> OcrChoice {
    match (profile_engine_ok, installed_langs) {
        (true, _) => OcrChoice::UserProfile,
        (false, 0) => OcrChoice::Missing,
        (false, _) => OcrChoice::FirstInstalled,
    }
}

/// `REGDB_E_CLASSNOTREG`: a classe WinRT não existe neste Windows. É o que o Windows 8.1 devolve
/// para `Windows.Media.Ocr.OcrEngine`, que só veio no Windows 10 (ticket 08).
pub const CLASS_NOT_REGISTERED: i32 = 0x8004_0154_u32 as i32;

/// Mensagem do OCR quando criar o motor falhou com `code`: sem o OCR no sistema, instalar um
/// idioma não adianta, então a mensagem diz isso em vez de ensinar a instalar.
pub fn ocr_failure_message(code: i32, detail: &str) -> String {
    if code == CLASS_NOT_REGISTERED {
        "OCR indisponível: o OCR do Windows (Windows.Media.Ocr) só existe no Windows 10 e 11. \
         Os blocos e condições de OCR não funcionam nesta versão do Windows."
            .to_string()
    } else {
        ocr_missing_message(detail)
    }
}

pub fn ocr_missing_message(detail: &str) -> String {
    let mut m = "OCR indisponível: nenhum idioma com reconhecimento de texto (OCR) está instalado no Windows. \
                 Instale em Configurações > Hora e idioma > Idioma e região > Português (Brasil) > Opções de idioma > \
                 Reconhecimento óptico de caracteres, ou no PowerShell como administrador: \
                 Add-WindowsCapability -Online -Name \"Language.OCR~~~pt-BR~0.0.1.0\""
        .to_string();
    if !detail.trim().is_empty() {
        m += &format!(" ({})", detail.trim());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn render_choice_and_relaunch() {
        assert_eq!(render_from_args(&args(&["--tray"])), Render::OpenGl);
        assert_eq!(render_from_args(&args(&["--tray", SOFTWARE_ARG])), Render::Software);
        // OpenGL falhou antes de o app abrir: relança em software, mantendo os argumentos.
        assert_eq!(relaunch_args(&args(&["--tray"]), Render::OpenGl, false), Some(args(&["--tray", SOFTWARE_ARG])));
        // Nunca relança em laço, nem depois de o app ter aberto.
        assert_eq!(relaunch_args(&args(&[SOFTWARE_ARG]), Render::Software, false), None);
        assert_eq!(relaunch_args(&[], Render::OpenGl, true), None);
    }

    #[test]
    fn adapter_choice() {
        use wgpu::DeviceType::*;
        assert_eq!(pick_adapter(&[(Cpu, true), (DiscreteGpu, true)]), Some(1));
        assert_eq!(pick_adapter(&[(Cpu, true), (IntegratedGpu, false)]), Some(0), "GPU que não aceita a janela não serve");
        assert_eq!(pick_adapter(&[(Cpu, true)]), Some(0), "VM sem GPU: WARP");
        assert_eq!(pick_adapter(&[(Cpu, false)]), None);
        assert_eq!(pick_adapter(&[]), None);
    }

    #[test]
    fn data_dir_choice() {
        let exe = Path::new(r"C:\Program Files\UltraTask");
        let local = Path::new(r"C:\Users\ana\AppData\Local");
        // Pasta normal: igual a antes, sem aviso.
        assert_eq!(choose_data_dir(exe, true, Some(local)), DataDir { path: exe.into(), warning: None });
        let d = choose_data_dir(exe, false, Some(local));
        assert_eq!(d.path, local.join("UltraTask"));
        assert!(d.warning.as_deref().is_some_and(|w| w.contains("não aceita escrita") && w.contains(r"Local\UltraTask")), "{d:?}");
        let d = choose_data_dir(exe, false, None);
        assert_eq!(d.path, exe);
        assert!(d.warning.is_some());
        assert_eq!(choose_data_dir(exe, false, Some(Path::new(""))).path, exe);
    }

    #[test]
    fn writable_probe_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("ultratask-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(dir_writable(&dir));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(&dir).unwrap();
        assert!(!dir_writable(&dir), "pasta que não existe não aceita escrita");
    }

    #[test]
    fn ocr_choice() {
        assert_eq!(choose_ocr(true, 0), OcrChoice::UserProfile);
        assert_eq!(choose_ocr(false, 2), OcrChoice::FirstInstalled);
        assert_eq!(choose_ocr(false, 0), OcrChoice::Missing);
        let m = ocr_missing_message("");
        assert!(m.contains("Add-WindowsCapability") && m.contains("Configurações") && !m.ends_with(')'));
        assert!(ocr_missing_message("classe não registrada").ends_with("(classe não registrada)"));
        // Windows 8.1: a classe do OCR não existe; instalar idioma não resolve.
        let m = ocr_failure_message(CLASS_NOT_REGISTERED, "Classe não registrada");
        assert!(m.starts_with("OCR") && m.contains("Windows 10 e 11") && !m.contains("Add-WindowsCapability"), "{m}");
        assert_eq!(ocr_failure_message(0x8000_4005_u32 as i32, "x"), ocr_missing_message("x"));
    }

    /// O OCR de verdade nunca entra em pânico: lê (Windows com idioma de OCR) ou devolve uma
    /// mensagem que começa com "OCR" (Wine e Windows sem idioma de OCR caem aqui).
    #[test]
    fn real_ocr_reads_or_explains() {
        let img = crate::win::Image { w: 64, h: 32, bgra: vec![255; 64 * 32 * 4] };
        match crate::win::ocr(&img) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{e}");
                assert!(e.starts_with("OCR"), "{e}");
            }
        }
    }
}
