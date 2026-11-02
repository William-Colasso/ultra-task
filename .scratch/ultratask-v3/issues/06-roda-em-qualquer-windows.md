# 06: Rodar em qualquer Windows 10/11 x64 sem instalar nada

**O que construir:** O exe de release roda numa instalação limpa do Windows 10 ou 11 de 64 bits, copiado sozinho, sem instalar nada:
- **Sem runtime.** Não depende do runtime do Visual C++.
- **Sem GPU.** A janela abre em máquinas sem aceleração de vídeo (VM, RDP, Windows Sandbox sem vGPU), com renderização por software. O OpenGL continua sendo o caminho normal, porque os tickets 07 e 08 dependem dele.
- **Sem OCR.** Se não houver um idioma com OCR instalado, os blocos e condições de OCR falham com uma mensagem que diz como instalar, em vez de travar.
- **Pasta sem escrita.** Se a pasta do exe não aceitar escrita (ex.: Arquivos de Programas), o app avisa e guarda scripts e ajustes em `%LOCALAPPDATA%\UltraTask`.

O event loop do winit não pode ser recriado no mesmo processo. Por isso, se abrir com OpenGL falhar, a troca de renderização precisa relançar o exe ou ser decidida antes de abrir a janela.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] O exe só importa DLLs do sistema (confira com `dumpbin /dependents` ou equivalente).
- [ ] O app abre, grava e toca um script no Windows Sandbox com a vGPU desligada, ou numa VM sem 3D. Se não houver como testar, peça esse teste ao usuário.
- [ ] Sem OCR disponível, aparece a mensagem e o app não entra em pânico.
- [ ] Numa pasta sem escrita, o app avisa e usa o local alternativo. Numa pasta normal, o comportamento é igual ao de hoje.
- [x] O README lista os requisitos: Windows 10/11 e nada para instalar.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu` + Wine), ver `../AMBIENTE-LINUX.md`.

**Commits**
- `ae46e5b` feat: roda em qualquer Windows 10/11 x64 sem instalar nada

**Arquivos tocados**
- Novos: `src/compat.rs`, `.cargo/config.toml`.
- Compartilhados (edições pequenas): `Cargo.toml` (feature `wgpu_no_default_features` no eframe, dependência `wgpu` só com `dx12`/`std`/`wgsl`, feature `Globalization` do `windows`), `Cargo.lock`, `src/main.rs` (`mod compat`, escolha do renderizador e relançamento), `src/model.rs` (`base_dir()` passa a vir de `compat::data_dir()`), `src/win.rs` (criação do motor de OCR em `ocr()`), `src/ui.rs` (2 avisos no log em `App::new`; `compat.rs` entra no teste de glifos), `README.md` (seção Requisitos).

**Testes adicionados** (`src/compat.rs`)
- `render_choice_and_relaunch`: `--render-software` liga o modo software; falha do OpenGL antes de o app abrir relança com os mesmos argumentos + `--render-software`; nunca relança em laço nem depois de o app abrir.
- `adapter_choice`: prefere GPU que aceite a janela; sem GPU, WARP (`DeviceType::Cpu`).
- `data_dir_choice`: pasta com escrita = igual a hoje, sem aviso; sem escrita = `%LOCALAPPDATA%\UltraTask` com aviso; sem `LOCALAPPDATA` = fica na pasta do exe, com aviso.
- `writable_probe_leaves_nothing_behind`: o teste de escrita não deixa arquivo; pasta inexistente = sem escrita.
- `ocr_choice`: perfil com OCR / outro idioma com OCR / nenhum (mensagem com o caminho em Configurações e o `Add-WindowsCapability`).
- `real_ocr_reads_or_explains`: o OCR real nunca entra em pânico; ou lê, ou devolve a mensagem que começa com "OCR".

**Como foi verificado aqui**
- `cargo test --target x86_64-pc-windows-gnu` no Wine: tudo passa.
- README: lido após a edição (critério marcado).
- DLLs (alvo gnu, critério ainda `[ ]` por faltar o MSVC): `x86_64-w64-mingw32-objdump -p target/x86_64-pc-windows-gnu/debug/ultratask.exe | grep 'DLL Name'` lista só DLLs do Windows: advapi32, api-ms-win-core-synch-l1-2-0, api-ms-win-core-winrt-error-l1-1-0, api-ms-win-core-winrt-l1-1-0, bcryptprimitives, combase, comctl32, dwmapi, dxgi, gdi32, imm32, kernel32, msvcrt, ntdll, ole32, oleaut32, OPENGL32, propsys, rpcrt4, setupapi, shell32, uiautomationcore, user32, userenv, uxtheme, ws2_32. Nada de libgcc/libwinpthread/vcruntime. Do `dxgi.dll` só vem `CreateDXGIFactory1` (existe desde o Windows 7); `d3d12.dll`, `dcomp.dll` e `d3dcompiler_47.dll` o wgpu carrega em tempo de execução (libloading), então não entram na tabela de importação.
- CRT estático: `cargo check --target x86_64-pc-windows-msvc -v` mostra `-C target-feature=+crt-static` nas chamadas do rustc (o check para depois por falta da std MSVC no container); no alvo gnu a flag não aparece.
- Fallback de renderização no Wine + Xvfb: com `MESA_GL_VERSION_OVERRIDE=1.1` (simula o OpenGL 1.1 do "GDI Generic"), o egui_glow recusa, o processo se relança como `ultratask.exe --render-software` (visto no `ps`) e, como o Wine não tem Direct3D 12, mostra a caixa de erro "O UltraTask não conseguiu abrir a janela (Direct3D 12 / WARP): …" em vez de travar ou relançar em laço. Sem o override, a janela abre normalmente pelo OpenGL.
- Pasta sem escrita no Wine: exe copiado para uma pasta com `chattr +i`; o log mostra "A pasta do UltraTask (Z:\…\ro) não aceita escrita: scripts e ajustes ficam em C:\users\root\AppData\Local\UltraTask".
- OCR no Wine (não tem `Windows.Media.Ocr`): `real_ocr_reads_or_explains` imprime a mensagem de instalação.

**Decisões de projeto**
- Renderização por software = wgpu com só o backend DX12. O Windows não traz OpenGL 2+ por software (só o 1.1 do GDI Generic), e Mesa/ANGLE exigiriam DLLs junto do exe. O WARP vem no Windows 10/11. O OpenGL continua o padrão (`compat::apply_render` força `Renderer::Glow`, porque com as duas features o padrão do eframe seria wgpu: **quem montar `NativeOptions` em outro lugar precisa chamar `compat::apply_render`**).
- A troca é por relançamento (`--render-software`), porque o event loop do winit não pode ser recriado. Só relança se o app nunca chegou a ser criado e se ainda não estava em software; o `run_native` roda dentro de `catch_unwind` porque o eframe usa `expect` na escolha da configuração do GL. Antes de relançar, o evento de instância única é fechado, senão o filho acharia o pai e sairia.
- No modo software, o seletor de adaptador prefere uma GPU que aceite a janela (o OpenGL pode ter falhado só por driver) e cai no WARP.
- Pasta de dados decidida uma vez por processo (`OnceLock`) com um arquivo de teste criado e apagado na pasta do exe. Não copia scripts que já estejam na pasta do exe para o `%LOCALAPPDATA%` (o usuário pode copiar à mão). O exe x64 não sofre a virtualização de arquivos do UAC (só processos 32 bits sem manifesto sofrem); o ticket 07 deve conferir isso no build de 32 bits.
- OCR: se nenhum idioma do perfil tem OCR mas outro idioma com OCR está instalado, usa o primeiro deles em vez de falhar.
- Custo: o wgpu aumenta o tempo de build e o tamanho do exe; não exige toolchain C (`cargo tree -i cc` vazio para os alvos gnu e msvc).

**Merge com `main-vqbyri`** (tickets 01-04, 09, 14-17)
- `Cargo.toml`: as features do `windows` foram unidas, uma por linha e em ordem alfabética (`Globalization` antes de `Graphics*`; `Win32_UI_Controls_Dialogs` antes de `Win32_UI_HiDpi`, que antes estava fora de ordem). O `Cargo.lock` foi mesclado sem conflito e o `cargo check` não o alterou.
- WGC (ticket 15, Direct3D 11) e o fallback por wgpu/DX12 usam a mesma `windows` 0.62.2 (`cargo tree -d` não mostra uma segunda versão; o wgpu-hal depende dela), e as features só se somam.
- Imports do exe gnu depois do merge, ainda só DLLs do sistema: entram `comdlg32` (Salvar como, ticket 05), `d3d11` (WGC) e `winhttp` (ticket 09). Lista completa: advapi32, api-ms-win-core-synch-l1-2-0, api-ms-win-core-winrt-error-l1-1-0, api-ms-win-core-winrt-l1-1-0, bcryptprimitives, combase, comctl32, comdlg32, d3d11, dwmapi, dxgi, gdi32, imm32, kernel32, msvcrt, ntdll, ole32, oleaut32, OPENGL32, propsys, rpcrt4, setupapi, shell32, uiautomationcore, user32, userenv, uxtheme, winhttp, ws2_32.
- `NativeOptions` só é montado em `src/main.rs`, que continua chamando `compat::apply_render`.
- `cargo test --target x86_64-pc-windows-gnu`: 121 passaram, 0 falharam, 7 ignorados, sem warnings.

### Pendente no Windows
1. **DLLs do exe MSVC.** No "Developer PowerShell for VS 2022", na raiz do repo: `cargo build --release` e `dumpbin /dependents target\release\ultratask.exe`. Esperado: só DLLs do Windows, como a lista acima, e **nenhuma** `VCRUNTIME140.dll`, `MSVCP140.dll` ou `api-ms-win-crt-*.dll`. Se aparecer alguma, conferir se o `.cargo/config.toml` foi lido (`cargo build --release -v` deve mostrar `+crt-static`).
2. **Sem GPU.** Ativar o Windows Sandbox (Recursos do Windows) e criar `semgpu.wsb` com `<Configuration><VGpu>Disable</VGpu><MappedFolders><MappedFolder><HostFolder>C:\caminho\target\release</HostFolder><ReadOnly>true</ReadOnly></MappedFolder></MappedFolders></Configuration>`. Abrir o `.wsb`, copiar `ultratask.exe` da pasta mapeada para a Área de Trabalho do Sandbox e dar duplo clique. Esperado: a janela abre (o log mostra "OpenGL indisponível: a janela usa Direct3D 12 por software (Microsoft Basic Render Driver)"); criar um script com o Bloco de Notas como alvo, gravar digitação, tocar em Segundo plano e ver o texto aparecer. Se não houver Sandbox, uma VM do Hyper-V/VirtualBox sem aceleração 3D serve. Também rodar o exe direto da pasta mapeada (somente leitura) para cobrir o passo 4 no mesmo Sandbox.
3. **Sem OCR.** No Sandbox (ou numa VM) em inglês sem idioma de OCR extra, ou removendo o OCR com `Get-WindowsCapability -Online -Name "Language.OCR*" | Remove-WindowsCapability -Online` (PowerShell admin; reinstale depois com `Add-WindowsCapability -Online -Name "Language.OCR~~~pt-BR~0.0.1.0"`): criar um script com um bloco "Ler texto (OCR)" e um "Esperar até" com condição de OCR, tocar. Esperado: o log mostra "OCR indisponível: … Add-WindowsCapability …", o bloco de OCR termina a execução com erro, a condição só avisa e conta como falsa, e o app continua aberto. Rodar também `cargo test -- --include-ignored` numa máquina com OCR instalado para o `e2e_background_edit`.
4. **Pasta sem escrita.** Copiar o exe para `C:\Program Files\UltraTask\` (precisa de admin para copiar) e abrir **sem** elevar. Esperado: o log mostra "A pasta do UltraTask (C:\Program Files\UltraTask) não aceita escrita: scripts e ajustes ficam em C:\Users\<você>\AppData\Local\UltraTask"; criar um script e mudar um ajuste; conferir que `%LOCALAPPDATA%\UltraTask\scripts\*.json` e `%LOCALAPPDATA%\UltraTask\ultratask.json` foram criados e que nada apareceu em Program Files. Depois, numa pasta normal (ex.: Área de Trabalho), conferir que `scripts\` e `ultratask.json` continuam ao lado do exe e que não há aviso.
