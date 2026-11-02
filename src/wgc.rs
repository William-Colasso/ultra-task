//! Pixel e OCR em segundo plano com Windows Graphics Capture (ticket 15).
//!
//! O Segundo plano captura com PrintWindow. Janelas DirectX/DirectComposition encobertas podem
//! sair pretas (ou de uma cor só). Quando isso acontece com a janela não minimizada, testamos o
//! WGC (`CreateForWindow`); se ele enxerga conteúdo, a execução passa a usá-lo para aquela janela
//! até o fim. Onde o WGC não existe (Windows 7/8, Windows 10 antes do 1903), fica o PrintWindow.
use crate::model::Mode;
use crate::win::{self, Hwnd, Image, hw};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, POINT, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::IsIconic;
use windows::core::{HRESULT, IInspectable, Interface, s, w};

/// Enquanto o PrintWindow continuar de uma cor só e o WGC também, testamos o WGC de novo só
/// depois deste intervalo (a tela pode estar carregando). Condições consultam a cada 100 ms.
pub const REPROBE_MS: u64 = 5000;

/// True quando todos os pixels têm a mesma cor (o alfa não conta). Imagem vazia conta como
/// uniforme: não há o que ler nela.
pub fn is_uniform(img: &Image) -> bool {
    let mut px = img.bgra.chunks_exact(4);
    let Some(first) = px.next() else { return true };
    px.all(|p| p[..3] == first[..3])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    PrintWindow,
    Wgc,
}

/// O que fazer na próxima captura.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Janela minimizada: nenhuma das duas captura; a condição dá falso com aviso, como antes.
    Refuse,
    PrintWindow,
    Wgc,
}

/// Resultado de um teste do WGC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    /// O WGC viu conteúdo: troca para ele até o fim.
    Content,
    /// O WGC também viu uma cor só: a janela é mesmo assim, fica no PrintWindow.
    Uniform,
    /// O WGC não existe ou falhou: fica no PrintWindow, sem tentar de novo.
    Failed,
}

/// Decisões puras da captura em segundo plano de uma janela, durante uma execução.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub backend: Backend,
    pub wgc_unavailable: bool,
    pub last_probe: Option<u64>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy { backend: Backend::PrintWindow, wgc_unavailable: false, last_probe: None }
    }
}

impl Policy {
    pub fn step(&self, minimized: bool) -> Step {
        match (minimized, self.backend) {
            (true, _) => Step::Refuse,
            (false, Backend::PrintWindow) => Step::PrintWindow,
            (false, Backend::Wgc) => Step::Wgc,
        }
    }

    /// Depois de um PrintWindow: testar o WGC? Só quando a imagem veio de uma cor só, o WGC não
    /// falhou antes e o último teste já tem `REPROBE_MS`. Apps comuns nunca chegam aqui.
    pub fn should_probe(&self, uniform: bool, now_ms: u64) -> bool {
        self.backend == Backend::PrintWindow && uniform && !self.wgc_unavailable && self.last_probe.is_none_or(|t| now_ms.saturating_sub(t) >= REPROBE_MS)
    }

    pub fn on_probe(&mut self, p: Probe, now_ms: u64) {
        self.last_probe = Some(now_ms);
        match p {
            Probe::Content => self.backend = Backend::Wgc,
            Probe::Uniform => {}
            Probe::Failed => self.wgc_unavailable = true,
        }
    }

    /// O WGC parou de funcionar no meio da execução (ex.: a janela recriou a superfície).
    pub fn on_wgc_error(&mut self) {
        self.backend = Backend::PrintWindow;
        self.wgc_unavailable = true;
    }
}

/// Captura em segundo plano de uma execução: PrintWindow, trocando para WGC quando preciso.
#[derive(Default)]
pub struct BgCapture {
    pub policy: Policy,
    wgc: Option<Wgc>,
}

impl BgCapture {
    /// Imagem da área cliente e, se houve troca ou falha do WGC, um aviso para o log.
    pub fn capture(&mut self, hwnd: Hwnd, now_ms: u64) -> Result<(Image, Option<String>), String> {
        let minimized = unsafe { IsIconic(hw(hwnd)).as_bool() };
        match self.policy.step(minimized) {
            Step::Refuse => Err("janela-alvo minimizada: não dá para capturar".into()),
            Step::Wgc => match self.wgc.as_mut().ok_or_else(|| "WGC não iniciado".to_string()).and_then(|c| c.grab(hwnd)) {
                Ok(img) => Ok((img, None)),
                Err(e) => {
                    self.policy.on_wgc_error();
                    self.wgc = None;
                    let img = win::capture_client(hwnd, Mode::Background)?;
                    Ok((img, Some(format!("aviso: Windows Graphics Capture falhou ({e}); voltando ao PrintWindow"))))
                }
            },
            Step::PrintWindow => {
                let img = win::capture_client(hwnd, Mode::Background)?;
                if !self.policy.should_probe(is_uniform(&img), now_ms) {
                    return Ok((img, None));
                }
                let probe = Wgc::new(hwnd).and_then(|mut c| c.grab(hwnd).map(|i| (c, i)));
                match probe {
                    Ok((c, wimg)) if !is_uniform(&wimg) => {
                        self.policy.on_probe(Probe::Content, now_ms);
                        self.wgc = Some(c);
                        Ok((wimg, Some("captura de uma cor só no PrintWindow: usando Windows Graphics Capture nesta janela até o fim".into())))
                    }
                    Ok(_) => {
                        self.policy.on_probe(Probe::Uniform, now_ms);
                        Ok((img, None))
                    }
                    Err(e) => {
                        self.policy.on_probe(Probe::Failed, now_ms);
                        Ok((img, Some(format!("aviso: captura de uma cor só e Windows Graphics Capture indisponível ({e}); seguindo com PrintWindow"))))
                    }
                }
            }
        }
    }
}

/// `CreateDirect3D11DeviceFromDXGIDevice` só existe no Windows 8+; carregar dinamicamente
/// mantém o exe abrindo no Windows 7 (ticket 08).
fn rt_device(dxgi: &IDXGIDevice) -> Result<IDirect3DDevice, String> {
    type F = unsafe extern "system" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> HRESULT;
    unsafe {
        let lib = LoadLibraryW(w!("d3d11.dll")).map_err(|e| e.message())?;
        let f: F = std::mem::transmute(GetProcAddress(lib, s!("CreateDirect3D11DeviceFromDXGIDevice")).ok_or("sem CreateDirect3D11DeviceFromDXGIDevice")?);
        let mut out = std::ptr::null_mut();
        f(dxgi.as_raw(), &mut out).ok().map_err(|e| e.message())?;
        if out.is_null() {
            return Err("dispositivo Direct3D nulo".into());
        }
        IInspectable::from_raw(out).cast().map_err(|e| e.message())
    }
}

fn d3d_device() -> Result<ID3D11Device, String> {
    let make = |t: D3D_DRIVER_TYPE| unsafe {
        let mut dev = None;
        D3D11CreateDevice(None, t, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut dev), None, None).ok()?;
        dev
    };
    make(D3D_DRIVER_TYPE_HARDWARE).or_else(|| make(D3D_DRIVER_TYPE_WARP)).ok_or_else(|| "não consegui criar o dispositivo Direct3D 11".into())
}

/// Sessão WGC de uma janela. O FrameArrived guarda só o quadro mais novo, então o pool nunca
/// enche; sem quadro novo (janela parada), vale o último lido.
pub struct Wgc {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    rt: IDirect3DDevice,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    size: SizeInt32,
    latest: Arc<Mutex<Option<Direct3D11CaptureFrame>>>,
    last: Option<Image>,
}

const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;

impl Wgc {
    pub fn new(hwnd: Hwnd) -> Result<Self, String> {
        let e = |e: windows::core::Error| e.message();
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
            return Err("não existe neste Windows".into());
        }
        let device = d3d_device()?;
        let ctx = unsafe { device.GetImmediateContext() }.map_err(e)?;
        let rt = rt_device(&device.cast::<IDXGIDevice>().map_err(e)?)?;
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(e)?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hw(hwnd)) }.map_err(e)?;
        let size = item.Size().map_err(e)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&rt, FORMAT, 2, size).map_err(e)?;
        let latest = Arc::new(Mutex::new(None));
        let slot = latest.clone();
        pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |p, _| {
            if let Some(p) = p.as_ref() {
                while let Ok(f) = p.TryGetNextFrame() {
                    // Soltar o quadro anterior devolve o buffer ao pool.
                    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
                }
            }
            Ok(())
        }))
        .map_err(e)?;
        let session = pool.CreateCaptureSession(&item).map_err(e)?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        // Tira a borda amarela onde o sistema deixa (Windows 10 2104+/11); antes disso, falha calado.
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture().map_err(e)?;
        Ok(Wgc { device, ctx, rt, pool, session, size, latest, last: None })
    }

    /// Área cliente atual. Espera até 1 s pelo primeiro quadro.
    pub fn grab(&mut self, hwnd: Hwnd) -> Result<Image, String> {
        let deadline = Instant::now() + Duration::from_secs(1);
        let frame = loop {
            let f = self.latest.lock().unwrap_or_else(|e| e.into_inner()).take();
            if f.is_some() || self.last.is_some() || Instant::now() > deadline {
                break f;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if let Some(f) = frame {
            let img = self.read(&f)?;
            let cs = f.ContentSize().map_err(|e| e.message())?;
            drop(f);
            if cs != self.size {
                self.size = cs;
                let _ = self.pool.Recreate(&self.rt, FORMAT, 2, cs);
            }
            self.last = Some(img);
        }
        let full = self.last.as_ref().ok_or("nenhum quadro em 1 s")?;
        Ok(full.crop(client_region(hwnd)?))
    }

    /// Copia o quadro para a CPU (textura de staging) como BGRA, no tamanho do conteúdo.
    fn read(&self, f: &Direct3D11CaptureFrame) -> Result<Image, String> {
        let e = |e: windows::core::Error| e.message();
        unsafe {
            let access: IDirect3DDxgiInterfaceAccess = f.Surface().map_err(e)?.cast().map_err(e)?;
            let tex: ID3D11Texture2D = access.GetInterface().map_err(e)?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            tex.GetDesc(&mut desc);
            let cs = f.ContentSize().map_err(e)?;
            let (w, h) = ((cs.Width.max(0) as u32).min(desc.Width), (cs.Height.max(0) as u32).min(desc.Height));
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;
            desc.MipLevels = 1;
            desc.ArraySize = 1;
            let mut staging = None;
            self.device.CreateTexture2D(&desc, None, Some(&mut staging)).map_err(e)?;
            let staging = staging.ok_or("textura de staging nula")?;
            self.ctx.CopyResource(&staging, &tex);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut map)).map_err(e)?;
            let mut bgra = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as usize {
                let row = std::slice::from_raw_parts((map.pData as *const u8).add(y * map.RowPitch as usize), w as usize * 4);
                bgra.extend_from_slice(row);
            }
            self.ctx.Unmap(&staging, 0);
            Ok(Image { w: w as i32, h: h as i32, bgra })
        }
    }
}

impl Drop for Wgc {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// Onde a área cliente fica dentro do quadro do WGC, que cobre os limites visíveis da janela
/// (DWMWA_EXTENDED_FRAME_BOUNDS, sem a borda invisível de redimensionar).
fn client_region(hwnd: Hwnd) -> Result<crate::model::Region, String> {
    let (cw, ch) = win::client_size(hwnd).ok_or("janela inválida")?;
    let mut b = RECT::default();
    let mut p = POINT::default();
    unsafe {
        DwmGetWindowAttribute(hw(hwnd), DWMWA_EXTENDED_FRAME_BOUNDS, &mut b as *mut _ as *mut _, size_of::<RECT>() as u32).map_err(|e| e.message())?;
        let _ = ClientToScreen(hw(hwnd), &mut p);
    }
    Ok(client_offset(b, p, cw, ch))
}

/// Parte pura de `client_region`.
pub fn client_offset(bounds: RECT, client_origin: POINT, cw: i32, ch: i32) -> crate::model::Region {
    crate::model::Region { x: client_origin.x - bounds.left, y: client_origin.y - bounds.top, w: cw, h: ch }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(px: &[[u8; 4]]) -> Image {
        Image { w: px.len() as i32, h: 1, bgra: px.concat() }
    }

    #[test]
    fn uniform_detection() {
        assert!(is_uniform(&img(&[])));
        assert!(is_uniform(&img(&[[0, 0, 0, 255]; 5])), "toda preta");
        assert!(is_uniform(&img(&[[12, 200, 7, 255], [12, 200, 7, 0]])), "uma cor só, alfa não conta");
        assert!(!is_uniform(&img(&[[0, 0, 0, 255], [0, 0, 1, 255]])));
        assert!(!is_uniform(&img(&[[0, 0, 0, 255], [0, 0, 0, 255], [255, 255, 255, 255]])));
    }

    #[test]
    fn minimized_never_captures() {
        let mut p = Policy::default();
        assert_eq!(p.step(true), Step::Refuse);
        p.on_probe(Probe::Content, 0);
        assert_eq!(p.step(true), Step::Refuse, "nem com o WGC ligado");
        assert_eq!(p.step(false), Step::Wgc);
    }

    #[test]
    fn common_apps_stay_on_printwindow() {
        let p = Policy::default();
        assert_eq!(p.step(false), Step::PrintWindow);
        assert!(!p.should_probe(false, 0), "imagem com conteúdo: sem custo extra");
        assert!(p.should_probe(true, 0), "uma cor só: testar o WGC");
    }

    #[test]
    fn switches_to_wgc_until_the_end() {
        let mut p = Policy::default();
        p.on_probe(Probe::Content, 100);
        assert_eq!(p.backend, Backend::Wgc);
        assert!(!p.should_probe(true, 100_000), "já no WGC: não testa de novo");
        assert_eq!(p.step(false), Step::Wgc);
    }

    #[test]
    fn uniform_window_reprobes_rarely() {
        let mut p = Policy::default();
        p.on_probe(Probe::Uniform, 1000);
        assert_eq!(p.backend, Backend::PrintWindow);
        assert!(!p.should_probe(true, 1000 + REPROBE_MS - 1));
        assert!(p.should_probe(true, 1000 + REPROBE_MS));
    }

    #[test]
    fn missing_wgc_keeps_printwindow() {
        let mut p = Policy::default();
        p.on_probe(Probe::Failed, 0);
        assert!(!p.should_probe(true, 1_000_000), "WGC indisponível: nunca mais testa");
        assert_eq!(p.step(false), Step::PrintWindow);
        let mut p = Policy::default();
        p.on_probe(Probe::Content, 0);
        p.on_wgc_error();
        assert_eq!(p.step(false), Step::PrintWindow);
        assert!(!p.should_probe(true, 1_000_000));
    }

    #[test]
    fn client_offset_inside_frame() {
        let b = RECT { left: 100, top: 50, right: 900, bottom: 650 };
        let r = client_offset(b, POINT { x: 101, y: 81 }, 798, 568);
        assert_eq!((r.x, r.y, r.w, r.h), (1, 31, 798, 568));
    }

    /// Janela própria pintada (GDI) com um padrão, coberta por outra: o WGC deve ver o mesmo que
    /// o PrintWindow vê com ela visível. Para DirectX/Chrome, veja o roteiro no ticket 15.
    #[test]
    #[ignore = "abre janelas de verdade e usa Windows Graphics Capture; rode no Windows com cargo test -- --ignored"]
    fn e2e_wgc_covered_matches_visible() {
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::*;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let inst = Some(HINSTANCE(GetModuleHandleW(None).unwrap().0));
            let style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
            let top = CreateWindowExW(Default::default(), w!("STATIC"), w!("ultratask-e2e-wgc"), style, 200, 200, 400, 300, None, None, inst, None).unwrap();
            let _ = CreateWindowExW(Default::default(), w!("EDIT"), w!("WGC 123"), WS_CHILD | WS_VISIBLE | WS_BORDER, 10, 10, 200, 40, Some(top), None, inst, None);
            let cover = CreateWindowExW(WS_EX_TOPMOST, w!("STATIC"), w!("capa"), style, 150, 150, 600, 500, None, None, inst, None).unwrap();
            let _ = ShowWindow(cover, SW_HIDE);
            tx.send((top.0 as isize, cover.0 as isize)).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let (top, cover) = rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(400));
        let visible = win::capture_client(top, Mode::Background).unwrap();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hw(cover), windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE);
        }
        std::thread::sleep(Duration::from_millis(400));
        let mut c = Wgc::new(top).unwrap();
        let covered = c.grab(top).unwrap();
        assert_eq!((covered.w, covered.h), (visible.w, visible.h));
        let same = visible.bgra.chunks_exact(4).zip(covered.bgra.chunks_exact(4)).filter(|(a, b)| a[..3].iter().zip(&b[..3]).all(|(x, y)| x.abs_diff(*y) <= 8)).count();
        let ratio = same as f64 / (visible.w * visible.h) as f64;
        assert!(ratio > 0.98, "só {:.1}% dos pixels batem", ratio * 100.0);
        assert!(!is_uniform(&covered));
        // Minimizada: sem captura, como antes.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hw(top), windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
        }
        std::thread::sleep(Duration::from_millis(300));
        assert!(BgCapture::default().capture(top, 0).is_err());
    }
}
