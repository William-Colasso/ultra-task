# 18: Modo CDP para Chromium, Electron e WebView2

**O que construir:** Um terceiro modo de execução, "Navegador (CDP)", para apps Chromium, Electron e WebView2, que ignoram os cliques do Segundo plano. O modo é mais uma implementação da interface de I/O do interpretador, ao lado do Segundo plano e do Foco.

**Como abre e conecta.** O script ganha o comando que abre o app. Ao tocar, o UltraTask:
1. lança o app com depuração remota:
   - `--remote-debugging-port`;
   - um `--user-data-dir` próprio por instância;
   - as flags que impedem o Chromium de congelar janelas cobertas.

   Para WebView2, a porta vai pela variável de ambiente `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`.
2. conecta pelo Chrome DevTools Protocol, só em 127.0.0.1;
3. escolhe a página cujo título casa com a regra da janela-alvo.

**Como age.**
- Clique, Mover, Rolar, Tecla e Digitar viram eventos do protocolo, sem tocar no mouse nem no teclado reais. Tecla aceita modificadores, então Ctrl+S funciona.
- As coordenadas dos blocos continuam na área cliente. O modo converte para o viewport, descontando a barra do navegador e o devicePixelRatio.
- Pixel e OCR usam a captura de tela do próprio protocolo.

Várias instâncias rodam ao mesmo tempo, cada uma com a sua porta e o seu perfil.

Detalhes na pesquisa: `../pesquisa-input-paralelo.md`, R2.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Três instâncias sobrepostas de um app Electron (ou do Chrome) recebem cliques e textos ao mesmo tempo. O resultado é conferido no DOM, e o cursor e a janela ativa não mudam.
- [x] Ctrl+S, ou outro atalho com modificador, funciona.
- [ ] Uma condição de pixel funciona com a janela coberta.
- [ ] A porta só escuta em 127.0.0.1 e o perfil é descartável. O README explica o risco de deixar a porta aberta.
- [ ] A conexão é sem TLS, só localhost, e a dependência de WebSocket compila nos alvos dos tickets 07 e 08 sem toolchain C além do MSVC.
- [x] O validador, a skill (com snippets validados) e o README cobrem o modo.

## Rastreabilidade

Feito no container Linux (alvo `x86_64-pc-windows-gnu` + Wine), branch `worktree-agent-a1e39c5601a46018f`.

### Commits
- `0104381` feat: modo Navegador (CDP) para Chromium, Electron e WebView2
- commit seguinte: docs: rastreabilidade do ticket 18 (modo CDP)

### Arquivos tocados
- Novos: `src/cdp/mod.rs`, `src/cdp/client.rs`, `src/cdp/input.rs`, `src/cdp/launch.rs`, `src/cdp/ui.rs`.
- Compartilhados (edições pequenas):
  - `Cargo.toml`/`Cargo.lock`: `png = "0.18.1"` (já estava no lock, vindo de outra dependência) e `tungstenite = { version = "0.30.0", default-features = false, features = ["handshake"] }`;
  - `src/main.rs`: `mod cdp;`;
  - `src/model.rs`: struct `CdpLaunch`, campo `Script.cdp: Option<CdpLaunch>` (omitido no JSON quando `None`) e 3 linhas em `validate`;
  - `src/ui.rs`: desvio para `play_cdp` em `start_with_target` e `play_on`, `play_cdp` + `push_run` (a poda de `runs` saiu de `play_on` para `push_run`), terceira opção em Config → Modo, lista do teste de glifos (agora um arquivo por linha, em ordem alfabética, com os 5 arquivos de `src/cdp/`);
  - `README.md`: linha na tabela de modos, seção "Modo Navegador (CDP)" com "Risco da porta de depuração", limitações e testes;
  - `skills/ultratask-scripts/SKILL.md`: modo CDP, campo `cdp`, exemplo completo.
- `src/runner.rs` e `src/win.rs` **não** foram tocados.

### Testes adicionados
- `cdp::client::tests`: `picks_page_by_title`, `ws_paths_ignore_the_advertised_host`, `http_parsing`, `replies_and_evaluate`, `client_against_fake_server` (servidor falso local: HTTP `/json/list` + handshake WebSocket real + JSON-RPC com eventos intercalados e resposta de erro).
- `cdp::input::tests`: `geometry_subtracts_bar_and_dpr`, `click_becomes_pressed_released_in_css_px`, `double_click_counts`, `wheel_flips_sign`, `ctrl_s_carries_modifier_and_no_text`, `typing_keys_and_shift`, `key_table`, `modifiers_reach_mouse_and_release_buttons`.
- `cdp::launch::tests`: `splits_like_a_command_line`, `chromium_command_line`, `webview2_goes_through_environment`, `ports_and_profiles_are_per_instance`, `validation`.
- `cdp::tests`: `window_of_the_launched_process_wins`, `script_json_and_validator`, `base64`, `screenshot_png_to_bgra`, e `cdp_real_browser` (`#[ignore]`: precisa de Chromium aberto; lê `ULTRATASK_CDP_PORTS`).
- `cargo test --target x86_64-pc-windows-gnu` (Wine): **40 passed, 0 failed, 3 ignored**.

### Como cada critério marcado foi verificado
- **Ctrl+S funciona:** o teste `cdp_real_browser`, compilado para Windows e rodando no Wine, conectou ao mesmo tempo em 3 Chromium 1243 reais (headless, Linux, `/opt/pw-browsers`), abertos com as mesmas flags de `launch::plan`. Em cada um, o interpretador rodou:
  - Clique no input;
  - Digitar `olá <porta> `;
  - Shift Down + `1` Press + Shift Up;
  - Ctrl Down + S Press + Ctrl Up;
  - clique Duplo;
  - `WaitUntil` com condição Pixel.

  O DOM confirmou que:
  - o valor ficou `olá <porta> !` (Shift virou `!`);
  - o handler `keydown` com `ctrlKey && key === 's'` disparou;
  - o `dblclick` chegou em (300,300).

  O Pixel bateu `rgb(200,30,60)` via `Page.captureScreenshot`. Comando: `ULTRATASK_CDP_PORTS=9331,9332,9333 cargo test --target x86_64-pc-windows-gnu -- --ignored cdp_real` → `ok`.
- **Validador, skill e README:** `cargo build --target x86_64-pc-windows-gnu` e `wine ultratask.exe --validate` nos snippets da skill (exemplo "Busca no navegador" e o bloco `cdp` isolado) → `OK`, saída 0. Casos negativos dão `ERRO` com caminho JSON:
  - `cdp.args`: aspas sem fechar;
  - `trigger`: gatilho junto com `cdp.exe`;
  - `cdp.port`: sem exe e sem porta;
  - `cdp.webview2`: sem exe;
  - campo desconhecido (`porta`).

### Verificado no Linux, mas sem marcar (depende do Windows)
- Protótipo descartável nativo do Linux (fora do projeto, inclui `client.rs`, `input.rs`, `launch.rs` e `model.rs` por `#[path]`). Rodou em 3 threads; cada uma:
  1. chamou `free_port`, `profile_dir`, `plan` e `spawn` com o Chromium real;
  2. achou a página pelo título (~1,2 s);
  3. clicou, digitou e mandou Shift+A pelo `InputState`;
  4. mandou `Browser.close` pela conexão da página. O processo saiu em ~1,3 s com status 0, o perfil foi apagado e a porta fechou.
- `/proc/net/tcp` mostrou as portas de depuração escutando só em `0100007F` (127.0.0.1).
- A dependência compila sem toolchain C: `cargo check -p tungstenite -p png -p getrandom@0.4.3` passou em `i686-pc-windows-gnu` e `aarch64-pc-windows-gnullvm`. `cargo tree -e build` não mostra `cc`, `cmake`, `bindgen` nem `pkg-config`, e não há TLS (`default-features = false`, só `handshake`).

### Decisões de projeto
- **Modo como campo, não como variante de `Mode`.** `Script.cdp: Option<CdpLaunch>` liga o modo. `Mode::Cdp` obrigaria a editar 7 `match` em `win.rs` (Injector, captura), justamente onde os tickets 14 e 15 mexem. Com o campo, `mode` e `simulate_focus` ficam ignorados no CDP, e a skill diz isso.
- **Run thread própria (`cdp::spawn`).** Reaproveita `Interp`, `Io`, `Halt`, `Debug` e `RunHandle` públicos do runner, sem editar `runner.rs`. O laço de repetição (12 linhas) foi duplicado de propósito para não tocar no runner.
- **Página pelo título da página** (`document.title`, glob da regra da janela-alvo), não pelo da janela.
  - Sem regra, ou com título vazio, usa a primeira `page`/`webview`.
  - DevTools e workers ficam fora.
  - O UltraTask espera até 30 s a página aparecer, porque o título muda ao carregar.
- **Coordenadas.**
  - Offset vertical = altura da área cliente − `innerHeight × devicePixelRatio`. A janela vem do processo lançado e, em último caso, da regra.
  - Offset horizontal = 0.
  - Os pontos em CSS são `(x − off)/dpr`. Pixel/OCR usam `(x − off)` direto na captura, que vem em pixels físicos.
  - A geometria é relida a cada 500 ms. Sem janela, o offset é 0.
- **Teclas.**
  - Os modificadores ficam no `InputState` (Alt=1, Ctrl=2, Meta=4, Shift=8).
  - Com Ctrl, Alt ou Meta pressionados, a tecla vira `rawKeyDown` sem `text`: atalho, sem digitar.
  - Pontuação segue o layout US. `Digitar` usa `Input.insertText`.
- **Clique duplo.** O Chromium não deduz a contagem de cliques, então o `InputState` calcula `clickCount` (≤ 500 ms e ≤ 4 px).
- **Rolagem.** `deltaY = −delta × 100/120`, sinal invertido em relação ao Windows.
- **Porta e perfil.**
  - Porta 0 = porta livre por execução, escolhida com `bind 127.0.0.1:0`.
  - Perfil em `%TEMP%\UltraTask-cdp\<script>-<porta>`, apagado no fim, a não ser com `keep_open`.
  - Para fechar: `Browser.close` (no WebView2, não), espera de 5 s e `kill`.
- **WebView2.** Além de `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, define `WEBVIEW2_USER_DATA_FOLDER` por instância. Sem isso, as instâncias dividiriam o mesmo processo de navegador e a segunda ignoraria a porta.
- **Cliente.**
  - Sempre reconecta em `ws://127.0.0.1:<porta><caminho>` e ignora o host anunciado.
  - Manda `Host: 127.0.0.1:<porta>` e não manda `Origin`.
  - Liga a emulação de foco (`Emulation.setFocusEmulationEnabled`) ao conectar.
- **Gatilhos.** Com `cdp.exe`, só `Manual`. `OnWindowOpen` abriria o app em laço, porque cada execução cria uma janela nova. Sem `exe` (anexar), os gatilhos continuam valendo.
- **Manual (`manual::block_help`).** Não existe neste branch (vem do ticket 03), e o CDP é um modo, não um bloco. No merge com o 03, vale acrescentar ao Manual uma seção sobre o modo.

### Merge com `main-vqbyri`
A base trouxe os tickets 01-04, 09 e 14-17, e o merge foi feito por `git merge`, sem rebase.

- **Conflitos resolvidos mantendo os dois lados:**
  - `src/main.rs`: os `mod`, em ordem alfabética;
  - `src/model.rs`: `human_mouse` e `cdp` em `Script` e no `Default`;
  - `src/ui.rs`: a lista do teste de glifos, com os arquivos de `src/cdp/` incluídos e a lista toda reordenada em ordem alfabética (`bgkeys` passou para antes de `catalog`);
  - `README.md`: a seção de testes;
  - `skills/ultratask-scripts/SKILL.md`: as linhas `human_mouse`/`integrations`/`cdp` na tabela do script, e os dois exemplos (CDP e UIA).
- **Adaptações do `CdpIo` e do `cdp::spawn`:**
  - `pointer`: começa da última posição enviada; antes do primeiro evento, do meio da página. Não há cursor real.
  - `uia`: devolve erro claro (`UIA_UNAVAILABLE`, "UI Automation não funciona no modo Navegador (CDP); use Clique, Pixel ou OCR"). O bloco para com esse erro e a condição dá falso com aviso.
  - O laço de `cdp::spawn` liga `human_mouse` e chama `start_integrations`/`finish_integrations`, como o `runner::spawn`. Os vários `Move` do trajeto humano viram `mouseMoved`.
- **Manual:** seção nova **Modos** em `src/manual.rs` (Segundo plano, Foco, Navegador (CDP) e o risco da porta). O teste `sections_have_content` agora também exige o termo "Navegador (CDP)".
- **Skill:** nota de que `human_mouse` e `integrations` valem no CDP, e `Uia` não.
- **Verificação depois do merge:**
  - `cargo test --target x86_64-pc-windows-gnu`: **129 passed, 0 failed, 8 ignored**, sem warnings (nem no `cargo build`);
  - `cdp_real_browser` de novo contra 3 Chromium reais: `ok`. O teste agora roda com `human_mouse` ligado, confere no DOM mais de 4 `mousemove` e que o bloco UIA falha com `UIA_UNAVAILABLE`;
  - todos os scripts completos da skill dão `OK` no `--validate` com `LC_ALL=C.UTF-8` e o nome do arquivo igual ao `name`: "Busca no navegador", "Consulta CEP", "Login diário", "Notas numeradas", "Pedido via UIA", "Traço no Paint" e o bloco `cdp`.

### Pendente no Windows
Pré-requisitos: build MSVC do branch (`cargo build --release`), Chrome ou Edge instalado e um app Electron qualquer. Para o Electron, por exemplo: `npm create electron-app teste` (ou o VS Code com `--user-data-dir` próprio).

1. **3 instâncias sobrepostas, cliques e textos ao mesmo tempo, DOM, cursor e janela ativa.**
   1. Salve `C:\ut\t.html`:
      ```html
      <title>UT teste</title><input id=i style="position:fixed;left:0;top:0;width:300px;height:40px"><div id=b style="position:fixed;left:0;top:100px;width:50px;height:50px;background:rgb(200,30,60)"></div><script>log=[];addEventListener('keydown',e=>{if(e.ctrlKey&&e.key==='s'){e.preventDefault();log.push('ctrl+s')}})</script>
      ```
   2. Crie 3 scripts iguais, com nomes diferentes:
      - `target`: `{"exe":"chrome.exe","title":"UT*"}`;
      - `cdp`: `{"exe":"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe","args":"--new-window file:///C:/ut/t.html"}`;
      - blocos: Clique em (20, 20 + altura da barra), Digitar `instância {rand(1,999)}` e Esperar 20000. Para achar a altura da barra, capture o ponto com 📍 numa janela aberta pelo próprio script com `keep_open`.
      - `repeat` 0, para eles ficarem rodando.
   3. Toque os 3 (atalhos ou ▶). Arraste as janelas para que se cubram e deixe outro app (Bloco de Notas) em primeiro plano, digitando nele.
   4. Confirme:
      - cada Chrome mostra o texto no input;
      - o cursor não se mexeu;
      - o Bloco de Notas continua ativo e recebendo o que você digita;
      - pelo DOM: abra `http://127.0.0.1:<porta>/json/list` (a porta aparece no log: "conectado ... em 127.0.0.1:<porta>"), entre no `devtoolsFrontendUrl` e rode `document.getElementById('i').value`.
   5. Repita com o app Electron: `cdp.exe` = exe do app, `args` vazio, `target.title` = título da página.
2. **(Complemento do Ctrl+S, já verificado no Chromium.)**
   1. No script do passo 1, acrescente Tecla Ctrl Down, Tecla S Press e Tecla Ctrl Up. No DevTools, `log` deve dar `["ctrl+s"]`.
   2. Num app Electron com menu Arquivo → Salvar (Ctrl+S), confira se o atalho do menu dispara. Se não disparar, anote a limitação no README: os aceleradores do processo principal podem não receber eventos do CDP.
3. **Pixel com a janela coberta.**
   1. Crie um script com `WaitUntil` com Pixel em (25, 125 + barra), cor `[200, 30, 60]` e timeout 5000, seguido de Log `achou`.
   2. Cubra a janela do Chrome com outra (não minimize) e toque. O log deve mostrar `achou`.
   3. Repita com OCR (`OcrToVar` numa região com texto).
4. **Porta só em 127.0.0.1 e perfil descartável.**
   1. Durante uma execução, rode `netstat -ano | findstr <porta>`: só deve aparecer `127.0.0.1:<porta> ... LISTENING`, nunca `0.0.0.0`.
   2. Durante a execução, `dir %TEMP%\UltraTask-cdp` mostra a pasta `<script>-<porta>`. No fim do script (sem `keep_open`), o Chrome fecha e a pasta some em até ~10 s. Com `keep_open`, o log informa o caminho do perfil e a pasta fica.
   3. Teste também o WebView2: um app WebView2 com `webview2: true`. Confira a porta no `netstat` e que `WEBVIEW2_USER_DATA_FOLDER` aponta para a pasta temporária, ou seja, que o app não usa a pasta de dados de sempre.
5. **WebSocket sem TLS, só localhost, compilando nos alvos dos tickets 07 e 08.**
   1. Instale o componente ARM64 do VS Build Tools, como pede o ticket 07. Depois rode `cargo check --target i686-pc-windows-msvc` e `cargo check --target aarch64-pc-windows-msvc`.
   2. Com o nightly do ticket 08, rode `cargo +nightly check -Z build-std --target x86_64-win7-windows-msvc`. O `getrandom` 0.4, dependência do `tungstenite`, usa `RtlGenRandom` nos alvos win7.
   3. Confirme com `cargo tree -p tungstenite -e features` que não há `native-tls` nem `rustls`.
