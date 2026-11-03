# 14: Segundo plano: Ctrl, Alt e Shift confiáveis

**O que construir:** Hoje, no modo Segundo plano, combinações como Ctrl+S falham. O app-alvo consulta o estado real do teclado (GetKeyState), e as teclas postadas não o alteram.

Com este ticket, enquanto a execução mantiver um modificador pressionado, cada tecla enviada à janela-alvo leva esse estado de modificadores falsificado só para a thread do alvo. A técnica é a do pywinauto: AttachThreadInput + SetKeyboardState, com o estado restaurado logo depois. O teclado real do usuário e a janela ativa não são afetados.

Cuidados levantados pela pesquisa (`../pesquisa-input-paralelo.md`, R3a):
- a thread da execução precisa ter uma fila de mensagens;
- o attach zera o estado das teclas;
- um attach numa thread travada congela as duas. Pule janelas que não respondem, como faz o ControlSend do AutoHotkey;
- GetAsyncKeyState e RawInput continuam vendo o hardware. Isso é limitação conhecida, não bug.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Ctrl+S, enviado em Segundo plano para o Bloco de Notas (ou um app WinForms) coberto por outra janela, abre o diálogo Salvar em 20 de 20 tentativas.
- [x] Durante o teste, o Ctrl real nunca aparece pressionado para o resto do sistema (GetAsyncKeyState), e a janela ativa do usuário não muda.
- [x] Uma janela-alvo travada não congela a execução: a combinação falha com um aviso no log.
- [x] Teclas sem modificador seguem o caminho de hoje, e o teste E2E existente continua verde.
- [x] O README tira esse item das limitações conhecidas, ou explica o que ainda não funciona.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu` + Wine; veja `../AMBIENTE-LINUX.md`).

**Commits**
- `029b940` feat(segundo-plano): Ctrl, Alt e Shift falsificados só para a thread do alvo

**Arquivos tocados**
- `src/bgkeys.rs` (novo): decisões puras (`held_mods`, `forged_state`, `plan`, `combo_name`) e a parte Win32 (`is_hung`, `post_with_mods`).
- `src/win.rs`: `Injector` ganhou `pub notes: Vec<String>`; o ramo de tecla do Segundo plano passa por `bgkeys::plan`.
- `src/runner.rs`: `WinIo` ganhou `say` (escreve no log da execução) e `last_note`; `send` repassa os avisos do Injector ao log, sem repetir o mesmo aviso seguido.
- `src/main.rs`: `mod bgkeys;`.
- `src/ui.rs`: `bgkeys.rs` entrou na lista do teste de glifos.
- `README.md`: limitação de Ctrl/Alt/Shift reescrita; linha do `--include-ignored` cita os testes novos.

**Testes adicionados** (`src/bgkeys.rs`)
- `held_mods_sides`, `forged_state_sets_and_clears`, `plan_paths`, `combo_names`: rodam no `cargo test`.
- `e2e_ctrl_s_forged` (`#[ignore]`): janela própria com wndproc que anota `GetKeyState(VK_CONTROL)` ao receber o `S` postado; 20 tentativas; confere que `GetAsyncKeyState(VK_CONTROL)` real fica solto e que a janela ativa não muda.
- `e2e_hung_target_does_not_freeze` (`#[ignore]`): alvo cuja thread para de bombear por 3 s; Ctrl+S tem que voltar em menos de 1 s, com aviso "não responde ... Ctrl+S".

**Como foi verificado aqui**
- `cargo test --target x86_64-pc-windows-gnu`: 22 passaram, 4 ignorados (no commit do ticket).
- `cargo check` também em `i686-pc-windows-gnu` e `aarch64-pc-windows-gnullvm`.
- [x] README: conferido o texto novo em "Limitações conhecidas".
- O JSON do roteiro abaixo passou em `ultratask.exe --validate` (build debug, Wine).
- Indicativo, não conta como critério: com `xvfb-run -a cargo test --target x86_64-pc-windows-gnu bgkeys -- --ignored`, os dois testes E2E passaram no Wine. O Wine não reproduz o Windows com fidelidade.

**Decisões de projeto**
- Só forja quando a tecla não é modificador e há modificador preso pela execução (`plan`). O próprio Ctrl continua indo como `WM_KEYDOWN` postado, como antes. Teclas sem modificador não pagam nada a mais: a checagem de travamento só roda quando vai haver attach.
- O estado montado liga o bit 0x80 no VK genérico e no do lado (`VK_CONTROL` + `VK_LCONTROL`/`VK_RCONTROL`, idem Shift e Alt) e desliga o dos modificadores não presos, para a combinação sair exata. Os bits de alternância (Caps Lock) ficam como estão. Ctrl/Alt genéricos com o bit de estendida viram o lado direito.
- Travamento: `IsHungAppWindow` ou `SendMessageTimeoutW(WM_NULL, SMTO_ABORTIFHUNG | SMTO_BLOCK, 150 ms)` sem resposta. Nesse caso a tecla não é postada (postar sem modificador digitaria um "s" solto) e o log avisa uma vez.
- Sequência por tecla: fila de mensagens criada com `PeekMessageW`; `AttachThreadInput`; `GetKeyboardState` (o attach zera o estado); `SetKeyboardState` forjado; `PostMessageW`; `SendMessageTimeoutW(WM_NULL)` como ponto de sincronia (o alvo só atende mensagem enviada no próximo `GetMessage`, depois de tirar a tecla da fila); mais 10 ms; estado restaurado; attach desfeito.
- Se o attach falhar (por exemplo, alvo elevado ou da mesma thread), a tecla é postada sem modificadores e o log avisa.
- O aviso sai do `Injector` por `notes`, e não muda o trait `Io`, para não mexer no interpretador.
- Limitação conhecida, no README: `GetAsyncKeyState` e RawInput continuam vendo o hardware.

### Merge com `main-vqbyri`
Só conflitos de texto, sem mudança de comportamento: `mod bgkeys;` e `bgkeys.rs` na lista de glifos (agora um arquivo por linha) entraram em ordem alfabética; no `WinIo`, os campos `say`/`last_note` convivem com o `uia` novo. No README, a linha do `--include-ignored` junta os testes dos dois lados. Depois do merge, `cargo test --target x86_64-pc-windows-gnu` passou com 107 testes e 7 ignorados, sem warnings, e o JSON do roteiro continua passando no `--validate`.

### Pendente no Windows
Preparação: num worktree com este branch, `cargo build` e `cargo test` (toolchain `stable-x86_64-pc-windows-msvc`).
1. **Testes ignorados:** `cargo test bgkeys -- --ignored` e depois `cargo test -- --include-ignored`. Devem passar `e2e_ctrl_s_forged`, `e2e_hung_target_does_not_freeze` e o `win::tests::e2e_background_edit` existente (critério "teclas sem modificador seguem o caminho de hoje").
2. **Ctrl+S no Bloco de Notas, 20 de 20:** salve este script como `t14-ctrl-s.json` na pasta de scripts e confira com `ultratask.exe --validate t14-ctrl-s.json`:
   ```json
   {
     "name": "t14-ctrl-s",
     "target": { "exe": "Notepad.exe", "title": "*" },
     "mode": "Background",
     "blocks": [
       { "Key": { "vk": 17, "action": "Down" } },
       { "Key": { "vk": 83, "action": "Press" } },
       { "Key": { "vk": 17, "action": "Up" } }
     ]
   }
   ```
   Abra o Bloco de Notas, digite algo e cubra-o com outra janela (por exemplo, o Explorer). Rode o script; o diálogo "Salvar como" tem que abrir. Feche-o com Esc, cubra de novo e repita até 20 vezes: 20 de 20. Se o Bloco de Notas novo do Windows 11 ignorar até tecla sem modificador postada, use o `notepad.exe` clássico ou um app WinForms (por exemplo, um formulário com `KeyPreview = true` que abre um `SaveFileDialog` no Ctrl+S, com `exe` ajustado no JSON).
3. **Ctrl real e janela ativa:** durante o passo 2, deixe outro app em foco (por exemplo, o Bloco de Notas de um segundo arquivo) e digite nele: nada sai em maiúscula/atalho, e o foco não muda. Com o PowerShell, rode em paralelo um loop que lê `GetAsyncKeyState(0x11)` a cada 5 ms (via `Add-Type` com `[DllImport("user32.dll")]`) e anote se o bit 0x8000 aparece; não pode aparecer. O teste `e2e_ctrl_s_forged` já confere as duas coisas na janela própria.
4. **Alvo travado:** além do teste do passo 1, rode o script do passo 2 contra uma janela travada de propósito (por exemplo, um app WinForms com um botão que faz `Thread.Sleep(10000)` na thread da UI; clique nele antes). A execução não pode congelar, e o log tem que mostrar "aviso: a janela-alvo não responde; Ctrl+S não foi enviado".
5. Ao fim, marque os critérios e mude o **Status** para `done`.

### Verificação no Windows (2026-11-03)
Em 2026-11-03, numa máquina Windows 11 com o toolchain `stable-x86_64-pc-windows-msvc` (branch `main-vqbyri` @ 1903fe5):
- `cargo test`: 166 ok, 0 falhas, 11 ignorados;
- `cargo build --release`: ok (SQLite `bundled` compilado com `cl.exe`); exe de 15,9 MB que só importa DLLs do sistema;
- `cargo test -- --ignored --skip cdp_real_browser --skip real_database_round_trip`: 9 ok, 0 falhas.

`e2e_ctrl_s_forged` passou: 20 Ctrl+S falsificados numa janela de teste coberta, com `GetAsyncKeyState(VK_CONTROL)` sempre solto e a janela ativa sem mudar. `e2e_hung_target_does_not_freeze` e o `win::tests::e2e_background_edit` antigo também passaram. Falta o roteiro manual do Ctrl+S 20/20 no Bloco de Notas.
