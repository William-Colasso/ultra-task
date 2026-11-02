# 15: Pixel e OCR em segundo plano com Windows Graphics Capture

**O que construir:** No modo Segundo plano, as condições de pixel e OCR capturam a janela com PrintWindow, já com PW_RENDERFULLCONTENT. Em janelas DirectX ou DirectComposition encobertas, como jogos em janela e alguns Chromium, a imagem pode vir preta.

Com este ticket, quando a captura vier toda preta (ou de uma cor só) e a janela não estiver minimizada, a execução passa a usar o Windows Graphics Capture (CreateForWindow) para aquela janela até o fim.
- Janela minimizada continua sem captura: a condição dá falso com aviso, como hoje.
- No Windows 10 pode aparecer a borda amarela de captura. Onde o sistema permitir, desligue-a.

Detalhes na pesquisa: `../pesquisa-input-paralelo.md`, R3b.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Com uma janela do Chrome e um jogo (ou amostra) DirectX em janela, ambos cobertos por outra janela, uma condição de pixel e um OCR leem o valor certo em Segundo plano.
- [ ] A imagem capturada com a janela coberta é praticamente igual à capturada com ela visível. O teste fica ignorado por padrão, porque mexe em janelas reais.
- [x] Apps comuns continuam no PrintWindow, sem custo extra.
- [x] Se houver dependência nova, ela compila nos alvos dos tickets 07 e 08 sem toolchain C além do MSVC. Onde o WGC não existir, fica o PrintWindow.
- [x] O README atualiza a limitação "jogos DirectX aparecem pretos".

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu` + Wine; veja `../AMBIENTE-LINUX.md`).

**Commits**
- `b873562` feat(captura): Windows Graphics Capture quando o PrintWindow vem de uma cor só

**Arquivos tocados**
- `src/wgc.rs` (novo): decisões puras (`is_uniform`, `Policy`, `Step`, `Probe`, `client_offset`), a captura com troca (`BgCapture`) e a sessão WGC (`Wgc`).
- `src/runner.rs`: `WinIo` ganhou `cap: wgc::BgCapture` e `capture()`; `pixel` e `ocr` passam por ele. O modo Foco continua no `win::capture_client` (cópia da tela).
- `src/main.rs`: `mod wgc;`.
- `src/ui.rs`: `wgc.rs` entrou na lista do teste de glifos.
- `Cargo.toml`: features novas do `windows`, uma por linha e em ordem alfabética: `Graphics`, `Graphics_Capture`, `Graphics_DirectX`, `Graphics_DirectX_Direct3D11`, `Win32_Graphics_Direct3D`, `Win32_Graphics_Direct3D11`, `Win32_Graphics_Dwm`, `Win32_Graphics_Dxgi`, `Win32_Graphics_Dxgi_Common`, `Win32_System_WinRT_Direct3D11`, `Win32_System_WinRT_Graphics_Capture`. O `Cargo.lock` não mudou.
- `README.md`: limitação "jogos DirectX aparecem pretos" reescrita; a linha do `--include-ignored` cita o teste novo.

**Testes adicionados** (`src/wgc.rs`)
- `uniform_detection`, `minimized_never_captures`, `common_apps_stay_on_printwindow`, `switches_to_wgc_until_the_end`, `uniform_window_reprobes_rarely`, `missing_wgc_keeps_printwindow`, `client_offset_inside_frame`: rodam no `cargo test`.
- `e2e_wgc_covered_matches_visible` (`#[ignore]`): janela própria com um EDIT, capturada com PrintWindow visível e depois com WGC coberta por uma janela topmost; pelo menos 98% dos pixels têm de bater (tolerância de 8 por canal). Ao fim, minimiza a janela e confere que `BgCapture` devolve erro.

**Como foi verificado aqui**
- `cargo test --target x86_64-pc-windows-gnu`: 29 passaram, 5 ignorados.
- `cargo check` em `i686-pc-windows-gnu` e `aarch64-pc-windows-gnullvm` sem erro nem aviso.
- [x] Apps comuns no PrintWindow sem custo extra: `common_apps_stay_on_printwindow` (imagem com conteúdo nunca dispara o teste do WGC) e leitura do código. O custo a mais é o `is_uniform`, que para no primeiro pixel diferente.
- [x] Dependência nova: não há. O WGC vem do crate `windows` já usado. Onde o WGC não existe, `missing_wgc_keeps_printwindow` cobre a decisão. No Wine (sem WGC), o `e2e_wgc_covered_matches_visible` sob `xvfb-run` falhou em `Wgc::new` com "não existe neste Windows", que é o caminho que deixa o PrintWindow. Para os alvos Win7 do ticket 08: `CreateDirect3D11DeviceFromDXGIDevice` (só existe no Windows 8+) é carregada com `GetProcAddress`. O `objdump -p` do exe confirma que do `d3d11.dll` só `D3D11CreateDevice` é importada estaticamente; ela existe no Windows 7 SP1.
- [x] README: conferido o texto novo em "Limitações conhecidas".

**Decisões de projeto**
- Gatilho: imagem toda de uma cor (alfa ignorado). Isso inclui "toda preta", e uma imagem vazia também conta.
- Uma janela comum de uma cor só (por exemplo, uma página em branco) não pode prender a execução no WGC. O WGC é testado e só vira o caminho se enxergar conteúdo (`Probe::Content`). Se ele também vier de uma cor só, a execução fica no PrintWindow e testa de novo no máximo a cada 5 s (`REPROBE_MS`), porque a tela pode estar carregando. Se o WGC não existir ou falhar, não testa mais naquela execução, e o log avisa uma vez.
- Depois da troca, o WGC vale até o fim da execução. Se falhar no meio, volta ao PrintWindow com aviso e não tenta de novo.
- Janela minimizada: `Step::Refuse` vem antes de qualquer captura, inclusive com o WGC ligado. O erro é o mesmo de antes ("janela-alvo minimizada: não dá para capturar"), então a condição dá falso com aviso.
- A sessão WGC fica aberta durante a execução. O `FrameArrived` (pool free-threaded, 2 buffers) guarda só o quadro mais novo, então o pool nunca enche. Sem quadro novo (janela parada), vale o último lido. No primeiro quadro, a espera é de até 1 s.
- Recorte: o quadro do WGC cobre `DWMWA_EXTENDED_FRAME_BOUNDS`, e a área cliente sai de `ClientToScreen(0,0)` menos o canto desses limites (`client_offset`).
- Borda amarela: `SetIsBorderRequired(false)` e `SetIsCursorCaptureEnabled(false)`, com erros ignorados. No Windows 10 antes do 2104, a borda continua.
- Dispositivo D3D11: hardware, com WARP de reserva, e `D3D11_CREATE_DEVICE_BGRA_SUPPORT`.

### Merge com `main-vqbyri`
Só conflitos de texto, sem mudança de comportamento: features do WGC intercaladas em ordem alfabética com `Win32_Networking_WinHttp`, `Win32_System_Com` e `Win32_UI_Accessibility`; `mod wgc;` e `wgc.rs` na lista de glifos em ordem alfabética; o campo `cap` do `WinIo` convive com o `uia` novo. No README, a linha do `--include-ignored` junta os testes dos dois lados. Depois do merge, `cargo test --target x86_64-pc-windows-gnu` passou com 107 testes e 7 ignorados, sem warnings, e o JSON do roteiro continua passando no `--validate`.

### Pendente no Windows
Preparação: num worktree com este branch, `cargo build` e `cargo test` (toolchain `stable-x86_64-pc-windows-msvc`), num Windows 10 1903+ ou 11.
1. **Teste ignorado:** `cargo test wgc -- --ignored` (e depois `cargo test -- --include-ignored` inteiro). `e2e_wgc_covered_matches_visible` tem que passar. No Windows 10, anote se a borda amarela aparece.
2. **Chrome e jogo DirectX cobertos:** salve este script como `t15-pixel-ocr.json` na pasta de scripts e confira com `ultratask.exe --validate t15-pixel-ocr.json`. O script já foi validado no container.
   ```json
   {
     "name": "t15-pixel-ocr",
     "target": { "exe": "chrome.exe", "title": "*" },
     "mode": "Background",
     "blocks": [
       { "OcrToVar": { "name": "t", "region": { "x": 0, "y": 0, "w": 600, "h": 200 } } },
       { "Log": { "text": "OCR: {t}" } }
     ]
   }
   ```
   - Abra no Chrome uma página com texto grande no topo (por exemplo, `data:text/html,<h1>ULTRATASK 15</h1>`) e cubra o Chrome com outra janela.
   - Rode o script. O log tem que mostrar `OCR: ULTRATASK 15`; se o PrintWindow vier preto, a linha "usando Windows Graphics Capture nesta janela até o fim" aparece antes.
   - Para pixel, crie no editor uma condição "Se pixel" num ponto de cor conhecida e confira o ramo tomado com a janela coberta.
   - Repita com um jogo DirectX em janela, ou com uma amostra como o `D3D11Tutorial` do SDK ou qualquer app DirectX em modo janela: troque `exe` no JSON e a região para um texto ou cor fixos do jogo.
3. **Apps comuns:** rode o mesmo script contra o Bloco de Notas com texto. A linha do WGC não pode aparecer no log.
4. **Minimizada:** minimize o alvo e rode. O log mostra "janela-alvo minimizada: não dá para capturar", e a condição dá falso.
5. **Windows 7/8 (ticket 08, melhor esforço):** no build Win7, rode o passo 2 contra o Bloco de Notas. O exe tem que abrir e, se a captura vier de uma cor só, o log avisa "Windows Graphics Capture indisponível" e segue com PrintWindow.
6. Ao fim, marque os critérios e mude o **Status** para `done`.
