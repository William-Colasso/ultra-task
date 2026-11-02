# Pesquisa: macros em paralelo em apps que rejeitam input em background

Relatório do agente de pesquisa da rodada 3 (2026-11-02), mantido como veio. Ele embasa os tickets:
- 14: R3a;
- 15: R3b;
- 16 e 17: R1;
- 18: R2.

Nenhuma técnica sozinha cobre todos os casos. Três resolvem a maior parte sem tocar no mouse ou teclado reais: UI Automation, Chrome DevTools Protocol (CDP) e duas correções pequenas no modo Background. Para jogos, as saídas são uma VM por instância ou injeção de DLL. Revezar o foreground funciona, mas só serve como "modo AFK".

## 1. Ranking

| # | Técnica | Apps que destrava | Mantém PC livre? | Requisitos | Complexidade | Risco |
|---|---|---|---|---|---|---|
| 1 | UI Automation: Invoke, Toggle, Value, SelectionItem, ExpandCollapse, Text [1] | Win32, WinForms, WPF, UWP/WinUI, Qt ≥5.11 [2], Chromium ≥126 e Electron, eframe/egui via AccessKit [3]. Java Swing só pela Java Access Bridge [4]. Jogos não. | Sim, sem nenhum input | Mesma integridade do alvo. Alvo admin exige UltraTask elevado ou uiAccess (assinado, instalado em Program Files) [5]. Rust: crate `uiautomation` [6] | M | Controles custom sem padrões; Invoke pode bloquear |
| 2 | CDP: `Input.dispatch*` e `insertText` [7] | Chromium, Electron, WebView2, CEF | Sim | Alvo lançado com `--remote-debugging-port`. No Chrome ≥136, também `--user-data-dir` próprio [8] | M | Quem acessa a porta controla o perfil |
| 3 | AttachThreadInput + SetKeyboardState [9][10] | Ctrl/Alt/Shift + tecla em apps que leem GetKeyState [11] | Sim | Mesmo desktop; alvo não travado. Rust: features `Win32_UI_Input_KeyboardAndMouse` e `Win32_System_Threading` | S | Não afeta GetAsyncKeyState nem RawInput |
| 4 | PrintWindow com PW_RENDERFULLCONTENT, com Windows Graphics Capture (WGC) como reserva [12][13] | Pixel/OCR de Chromium, DirectComposition e DirectX encobertos | Sim | Windows 8.1+ / Windows 10 1903+ | S/M | Janela minimizada não é capturada [14]; a borda amarela só sai em 20348+ com consentimento [15] |
| 5 | VM Hyper-V ou sessões RDS, com o UltraTask em modo Focus dentro [17][18] | Tudo, inclusive DirectInput e RawInput | Sim | Windows Pro/Enterprise. Várias sessões só em Windows Server com RDSH (pago) | M (operacional) | RAM e GPU; RDP minimizado para de renderizar (`RemoteDesktop_SuppressWhenMinimized=2`) [19] |
| 6 | Revezamento do foreground (time-slicing) [20–22] | Tudo que o modo Focus já faz | **Não** | Usuário ocioso | M | Pisca; tecla digitada pelo usuário cai no alvo |
| 7 | Injeção de DLL + hooks [23] | Jogos e apps que checam GetForegroundWindow, GetCursorPos, GetAsyncKeyState, RawInput ou XInput | Sim | Uma DLL por arquitetura. Rust: `dll-syringe` (exige nightly) [24] + `retour` [25] | L | Antivírus/EDR, pois é técnica de malware catalogada [26]; crashes |
| 8 | InjectTouchInput / InjectSyntheticPointerInput [27][28] | Apps touch visíveis na tela | Parcial: o cursor fica, mas a janela é ativada [29] | Windows 8+ / Windows 10 1809+ | M | O ponto precisa estar visível |

**Hooks, conceitualmente.** É o que o ProtoInput/Nucleus Co-op faz. A DLL entra no processo criado suspenso e intercepta GetCursorPos, GetAsyncKeyState, GetKeyState, GetRawInputData, foco, XInput e DirectInput. Esses hooks devolvem um estado virtual por instância, alimentado por pipe [23].

## 2. Top 3 para o UltraTask

### R1 — Bloco "UIA" (ação + condição), capturado na gravação
- **Onde:** novo tipo de bloco, executado numa thread MTA sem janelas. A Microsoft avisa que chamadas UIA feitas na thread de UI podem travar o app [30], e o UltraTask é egui. O crate `uiautomation` 0.25 já usa `windows ^0.62.2`, a mesma versão do projeto [6].
- **Gravação:** a cada clique, `ElementFromPoint` [31]. Salvar AutomationId, Name, ControlType, ClassName e ancestrais junto com as coordenadas.
- **Execução:** `ElementFromHandle(hwnd do script)`, depois `FindFirst`, depois o padrão. Sem match, volta ao PostMessage atual. Evitar `SetFocus`, porque ativa a janela. A doc diz que `Invoke` deve retornar na hora, mas que isso depende da implementação [32], então usar timeout.
- **Condição UIA:** existe / Name / Value / ToggleState. Substitui pixel/OCR onde houver árvore.
- **Chromium/Electron:** UIA nativo a partir do Chrome 126, em rollout gradual [33]. O Electron só expõe a árvore com tecnologia assistiva ativa ou com `app.setAccessibilitySupportEnabled` [34]. Dá para forçar com `--force-renderer-accessibility` [35].
- **Como verificar:** ver os padrões no Accessibility Insights. Rodar 3 alvos (WinForms, WPF, Electron) encobertos enquanto você digita em outro app. Ler o efeito pelo ValuePattern e confirmar que `GetCursorPos` e `GetForegroundWindow` não mudaram.

### R2 — Backend de input "CDP"
- **Onde:** backend escolhido por script; o UltraTask passa a lançar o alvo.
- **Lançamento:** `--remote-debugging-port=<porta> --user-data-dir=<pasta da instância>`, mais `--disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling` [36]. Sem isso, janela coberta para de renderizar e o JS é estrangulado [37].
  - WebView2: definir `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=N` no ambiente do processo filho; o CDP usa uma porta só, compartilhada por todos os controles WebView2 do app [38].
  - Electron documenta a flag, e nenhum fuse documentado a cobre [39][40].
- **Protocolo:** `GET /json/list`, depois WebSocket com `tungstenite` + `serde_json` (síncrono, sem tokio).
  - Chamar `Emulation.setFocusEmulationEnabled`; segundo uma issue do Playwright, ele faz isso por padrão [41].
  - `dispatchMouseEvent` usa pixels CSS relativos ao viewport, então dividir pelo devicePixelRatio [7].
  - `dispatchKeyEvent` aceita `modifiers` (Ctrl=2), o que resolve Ctrl+S; `insertText` é experimental [7].
  - Para condições de pixel, `Page.captureScreenshot`.
- **Como verificar:** 3 instâncias sobrepostas rodando juntas; checar o DOM via `Runtime.evaluate`; cursor e foreground intactos.
- **Risco:** a porta expõe o perfil, e foi esse o motivo da mudança no Chrome 136 [8]. Escutar só em 127.0.0.1 e usar perfis descartáveis.

### R3 — Duas correções pequenas no backend Background
- **(a) Modificadores**, como no `send_keystrokes` do pywinauto [9]:
  1. `AttachThreadInput(worker, alvo)`. Pular janelas travadas, como faz o ControlSend do AutoHotkey [42]. O worker precisa ter fila de mensagens, e o attach reseta o estado das teclas [43].
  2. `GetKeyboardState`, ligar o bit 0x80 em `VK_CONTROL`/`VK_LCONTROL`, `SetKeyboardState`.
  3. PostMessage de `WM_KEYDOWN`/`WM_KEYUP` (`WM_SYSKEY*` para Alt).
  4. Esperar uns 10 ms, restaurar o estado e desfazer o attach.
  - Funciona porque SetKeyboardState escreve a tabela que GetKeyState lê [10], enquanto GetAsyncKeyState continua refletindo o hardware [11].
  - Attach numa thread travada congela as duas [44].
  - O ControlSend padrão do AutoHotkey manda modificadores globais e interfere na janela ativa [45].
- **(b) Captura:** `PrintWindow(hwnd, hdc, PRINT_WINDOW_FLAGS(2))`. É a PW_RENDERFULLCONTENT, flag não documentada que o WebRTC usa para conteúdo DirectComposition [12]. O windows-rs não tem a constante, mas o tipo é newtype de u32 [46]. Se a imagem ainda vier preta, usar WGC (`CreateForWindow`) [13] via o crate `windows-capture` [16].
  - *Nota da rodada 3: o UltraTask já usa `PRINT_WINDOW_FLAGS(3)` (PW_CLIENTONLY | PW_RENDERFULLCONTENT); falta só o WGC, que é o ticket 15.*
- **Como verificar:**
  - (a) Ctrl+S num app WinForms em background: o diálogo "Salvar" deve aparecer, e `GetAsyncKeyState(VK_CONTROL)` real nunca deve ficar pressionado.
  - (b) Chrome e um jogo em janela, encobertos: o frame não deve sair preto e deve ser praticamente igual ao capturado com a janela visível.

### Revezamento do foreground: velocidade e quanto pisca
- **Sequência de cada fatia:**
  1. Esperar o usuário ficar ocioso (GetLastInputInfo + hook low-level; `LLKHF_INJECTED` separa o seu input do dele [47]).
  2. Salvar foreground e cursor.
  3. `SetForegroundWindow`, que é restrito pelo sistema [20]. O AutoHotkey tenta 6 vezes em 60 ms e depois manda Alt [21].
  4. Um único `SendInput` com mover, down e up; ele não é intercalado com o input do usuário [48].
  5. Restaurar foreground e cursor.
- **Velocidade:** o AutoHotkey espera 100 ms depois de ativar uma janela, por confiabilidade [22]. Daí ~100–200 ms por fatia, ~5–10 ações/s somando todos os scripts. É estimativa minha, não medi.
- **Flicker:** cada fatia troca ativação, z-order e cursor, e isso é visível.

## 3. O que NÃO funciona
- **Teclado via PostMessage** não atualiza o estado de shift [49]; por isso a R3a.
- **Postar `WM_POINTER*`**: `GetPointerInfo` só tem dados da última mensagem de ponteiro que o sistema entregou à thread; fora disso retorna ERROR_NO_DATA [50].
- **Postar `WM_INPUT`**: o lParam é um handle HRAWINPUT do sistema, lido por GetRawInputData [51].
  - WM_INPUT real só chega a uma janela em background se o app se registrou com `RIDEV_INPUTSINK` [52].
  - DirectInput com `DISCL_FOREGROUND` solta o dispositivo ao perder o foco [53].
  - Só hooks resolvem esses casos [23].
- **HID virtual**: o VHF exige driver em modo kernel [54] e o ViGEmBus foi aposentado [55]. Pela minha inferência, o input entraria na fila do sistema e seguiria o foreground, então não paraleliza.
- **Touch/pen injection**: vai para o desktop da sessão [27], acerta o que estiver visível no ponto e ativa a janela [29]. Na prática é revezamento de foreground sem mover o cursor.
- **`CreateDesktop`**: só o input desktop recebe input, e mensagens só passam entre processos do mesmo desktop [56]. No máximo repete o Background atual.
- **Desktops virtuais**: o Chromium trata janelas neles como ocluídas [37].
- **Outras sessões ou sandboxes**: workstation só tem uma sessão ativa [18]; o Windows Sandbox roda uma instância por vez [57]; o Sandboxie isola programas, não o input [58].
- **Jogos online com anti-cheat**: fora de escopo, com risco de ban [59].

## 4. Fontes
1. https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternsoverview
2. https://qt.io/blog/2018/05/22/qt-5-11-released
3. https://docs.rs/crate/eframe/latest/features
4. https://docs.oracle.com/javase/accessbridge/2.0.2/introduction.htm
5. https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
6. https://docs.rs/uiautomation/latest/uiautomation/
7. https://github.com/ChromeDevTools/devtools-protocol/blob/master/pdl/domains/Input.pdl
8. https://developer.chrome.com/blog/remote-debugging-port
9. https://github.com/pywinauto/pywinauto/blob/atspi/pywinauto/controls/hwndwrapper.py
10. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setkeyboardstate
11. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeystate
12. https://webrtc.googlesource.com/src/+/f89110d67902e787f6745ad2b52f7f09fc808512%5E%21
13. https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow
14. https://webrtc.googlesource.com/src/+/main/modules/desktop_capture/win/wgc_capture_session.cc
15. https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired
16. https://docs.rs/windows-capture
17. https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/get-started/install-hyper-v
18. https://docs.uipath.com/robot/standalone/latest/admin-guide/windows-sessions ; https://docs.uipath.com/robot/standalone/2023.4/user-guide/setting-up-windows-server-for-high-density-robots
19. https://support.smartbear.com/testcomplete/docs/ver-14-6/testing-with/running/via-rdp/in-minimized-window.html
20. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow
21. https://www.autohotkey.com/docs/v2/lib/WinActivate.htm
22. https://www.autohotkey.com/docs/v2/lib/SetWinDelay.htm
23. https://github.com/Ilyaki/ProtoInput (hooks em /src/ProtoInput/ProtoInputHooks)
24. https://github.com/OpenByteDev/dll-syringe
25. https://docs.rs/retour
26. https://attack.mitre.org/techniques/T1055/001/
27. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injecttouchinput
28. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injectsyntheticpointerinput
29. https://learn.microsoft.com/en-us/windows/win32/inputmsg/wm-pointeractivate
30. https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading
31. https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation-elementfrompoint
32. https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationinvokepattern-invoke
33. https://developer.chrome.com/blog/windows-uia-support
34. https://www.electronjs.org/docs/latest/tutorial/accessibility
35. https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/resources/accessibility/accessibility.html
36. https://github.com/GoogleChrome/chrome-launcher/blob/main/docs/chrome-flags-for-tools.md
37. https://github.com/chromium/chromium/blob/main/docs/windows_native_window_occlusion_tracking.md
38. https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code
39. https://www.electronjs.org/docs/latest/api/command-line-switches
40. https://www.electronjs.org/docs/latest/tutorial/fuses
41. https://github.com/microsoft/playwright/issues/41873
42. https://github.com/AutoHotkey/AutoHotkey/blob/alpha/source/keyboard_mouse.cpp
43. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-attachthreadinput
44. https://devblogs.microsoft.com/oldnewthing/20130619-00/?p=4043
45. https://www.autohotkey.com/docs/v2/lib/ControlSend.htm
46. https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Storage/Xps/struct.PRINT_WINDOW_FLAGS.html
47. https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct
48. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
49. https://devblogs.microsoft.com/oldnewthing/20050530-11/?p=35513
50. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getpointerinfo
51. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getrawinputdata
52. https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice
53. https://learn.microsoft.com/en-us/previous-versions/windows/desktop/ee417921(v=vs.85)
54. https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-
55. https://github.com/nefarius/ViGEmBus
56. https://learn.microsoft.com/en-us/windows/win32/winstation/desktops
57. https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-faq
58. https://github.com/sandboxie-plus/Sandboxie
59. https://partner.steamgames.com/documentation/vac
