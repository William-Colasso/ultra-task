# 08: Windows 7 e 8.1 (melhor esforço)

**O que construir:** Builds para os alvos Win7 do Rust (`x86_64-win7-windows-msvc` e `i686-win7-windows-msvc`), que exigem toolchain nightly com `build-std`. Com eles, o UltraTask abre e toca scripts em Segundo plano e em Foco no Windows 7 SP1 e no 8.1.

Recursos que não existem nesses sistemas ficam desligados, com uma mensagem clara em vez de derrubar o app. Exemplos: o OCR do WinRT e a renderização por software do ticket 06, se ela depender do DirectX 12.

O README diz o que funciona em cada versão do Windows. Se uma dependência não suportar esses alvos, documente qual é e pare ali: este ticket é de melhor esforço.

**Bloqueado por:** 07 (Builds de 32 bits e ARM64 nativo).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Os dois builds Win7 compilam com um comando documentado.
- [ ] Numa VM com Windows 7 SP1 ou 8.1, o app abre, grava e toca em Segundo plano e em Foco. Se não houver VM, este passo fica com o usuário.
- [ ] Recursos ausentes mostram uma mensagem e não derrubam o app.
- [x] O README tem uma tabela de recursos por versão do Windows.

## Rastreabilidade

Feito num container Linux (nightly + `-Z build-std`, alvos `*-win7-windows-gnu` e `*-win7-windows-msvc` só com `check`), ver `../AMBIENTE-LINUX.md`.

**Resultado do melhor esforço:** o código compila para os alvos Win7, mas o exe **não carrega no Windows 7**: a crate `windows`/`windows-core` 0.62 importa estaticamente `combase.dll` e `api-ms-win-core-winrt-*.dll`, que só existem a partir do Windows 8. Conforme o ticket, parei aí para o 7. O mesmo build serve para o **Windows 8.1**, onde essas DLLs existem; nele, os recursos ausentes (OCR, WGC, Direct3D 12) já falham com mensagem.

**Commits**
- (este commit) feat: builds Win7/8.1 com nightly e mensagem do OCR fora do Windows 10

**Arquivos tocados**
- `.cargo/config.toml` (compartilhado): alias `build-win7` (`cargo +nightly build-win7` = `build --release -Z build-std` nos dois alvos `*-win7-windows-msvc`). Os rustflags do ticket 06 (`+crt-static`) e do 07 (manifesto no x86) valem também para eles (`target_env = "msvc"`, `target_arch = "x86"`).
- `src/compat.rs`: `CLASS_NOT_REGISTERED` e `ocr_failure_message`: se criar o `OcrEngine` falha com `REGDB_E_CLASSNOTREG` (a classe não existe, caso do 8.1), a mensagem diz que o OCR só existe no Windows 10/11 em vez de ensinar a instalar idioma.
- `src/win.rs` (1 linha em `ocr()`): usa `ocr_failure_message` com o código do erro.
- `README.md`: tabela "Recursos por versão do Windows" em Requisitos; subseção de build "Windows 7 e 8.1 (melhor esforço)" com o comando e o motivo de o 7 não abrir.

**Testes**
- `compat::tests::ocr_choice` ganhou os casos de `ocr_failure_message` (classe não registrada = mensagem de "só Windows 10 e 11", sem `Add-WindowsCapability`; outro código = mensagem antiga).
- `real_ocr_reads_or_explains` no Wine agora imprime "OCR indisponível: o OCR do Windows (Windows.Media.Ocr) só existe no Windows 10 e 11…": o Wine também não tem a classe e devolve `REGDB_E_CLASSNOTREG`, igual ao 8.1.
- `cargo test --target x86_64-pc-windows-gnu`: tudo passa, sem warnings.

**Como foi verificado**
- `rustup toolchain install nightly --component rust-src` (cargo 1.101.0-nightly 2026-10-27).
- `cargo +nightly check -Z build-std --target x86_64-win7-windows-gnu`, `--target i686-win7-windows-gnu` e `--target x86_64-win7-windows-msvc --target i686-win7-windows-msvc`: todos compilam sem warnings; nenhuma dependência quebra nesses alvos.
- `cargo +nightly build -Z build-std --target x86_64-win7-windows-gnu`: linka. `x86_64-w64-mingw32-objdump -p` do exe: a std do alvo Win7 não importa mais `bcryptprimitives.dll` (`ProcessPrng`, Windows 10) nem `api-ms-win-core-synch-l1-2-0.dll` (`WaitOnAddress`, Windows 8); usa `SystemFunction036`. Todas as funções importadas de kernel32/user32/gdi32/shell32/advapi32/ole32/oleaut32/propsys/uiautomationcore/dwmapi/d3d11/dxgi/winhttp/comdlg32/comctl32/setupapi/userenv/imm32/uxtheme/ws2_32/opengl32 existem no Windows 7 (a mais nova é de Win7: `RegisterTouchWindow`, `QueryDisplayConfig`, `SetWindowDisplayAffinity`, `Shell_NotifyIconGetRect`, `ChangeWindowMessageFilterEx`). As que não existem no 7 são só: `combase.dll!CoIncrementMTAUsage`, `combase.dll!CoCreateFreeThreadedMarshaler`, `api-ms-win-core-winrt-l1-1-0.dll!RoGetActivationFactory`/`RoInitialize` e `api-ms-win-core-winrt-error-l1-1-0.dll!RoOriginateErrorW`, todas declaradas com `windows_link::link!` em `windows-core-0.62.2/src/imp/` e `windows-result` (dependências do UltraTask, do wgpu-hal e do eframe/winit via accesskit).
- Já carregados em tempo de execução (não estão na tabela de importação): `d3d12.dll`/`dcomp.dll` (wgpu, ticket 06) e `CreateDirect3D11DeviceFromDXGIDevice` (WGC, ticket 15). No 8.1, o WGC falha em `GraphicsCaptureSession::IsSupported` e a captura continua no `PrintWindow` com aviso no log; o `--render-software` mostra a caixa de erro do ticket 06.
- README: lido depois da edição (critério 4 marcado).

**Decisões**
- Não tentei `/DELAYLOAD` das DLLs do WinRT para o Windows 7: além de não dar para testar aqui, o `accesskit_windows` implementa objetos COM com `#[implement]`, e o `windows-core` chama `CoCreateFreeThreadedMarshaler` quando alguém (a própria UI Automation do sistema) pede `IMarshal` a eles; com delay-load, isso derrubaria o app no 7 fora de qualquer recurso que dê para desligar. É exatamente o caso "dependência não suporta: documente e pare".
- O exe normal (alvos tier 1) não roda no 8.1 (a std do Rust exige Windows 10 desde a 1.78); por isso a tabela manda usar o build Win7 também no 8.1.

### Pendente no Windows
1. **Build** (critério 1). No "Developer PowerShell for VS 2022", na raiz do repo: `rustup toolchain install nightly --component rust-src`, `rustup target add --toolchain nightly x86_64-pc-windows-msvc` (não precisa, o build-std recompila a std; só se o rustup reclamar) e `cargo +nightly build-win7`. Esperado: `target\x86_64-win7-windows-msvc\release\ultratask.exe` e `target\i686-win7-windows-msvc\release\ultratask.exe`. `dumpbin /dependents` em cada um: sem `VCRUNTIME140.dll`, `api-ms-win-crt-*`, `bcryptprimitives.dll` e `api-ms-win-core-synch-l1-2-0.dll`; com `combase.dll` e `api-ms-win-core-winrt-*` (o bloqueio do 7).
2. **Windows 8.1** (critério 2, critério 3). Numa VM com Windows 8.1 (x64 para o exe x64; x86 ou x64 para o x86): copiar o exe e abrir. Criar um script com o Bloco de Notas como alvo, gravar digitação, tocar em Segundo plano e em Foco. Depois: um bloco "Ler texto (OCR)" deve parar a execução com "OCR indisponível: o OCR do Windows (Windows.Media.Ocr) só existe no Windows 10 e 11…" e o app continua aberto; uma condição de pixel numa janela que fica preta no PrintWindow deve dar o aviso "Windows Graphics Capture indisponível"; `ultratask.exe --render-software` deve mostrar a caixa "não conseguiu abrir a janela (Direct3D 12 / WARP)" e sair sem travar.
3. **Windows 7 SP1** (critério 2). Numa VM com Windows 7 SP1: abrir o exe Win7. Esperado hoje: o Windows recusa com "combase.dll não encontrada" (ou "ponto de entrada não encontrado"). Se for preciso suportar o 7, o próximo passo é a decisão de projeto acima (delay-load + guardas), num ticket próprio.
