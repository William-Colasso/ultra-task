# UltraTask

UltraTask é um gravador e reprodutor de macros para Windows, inspirado no TinyTask e escrito todo em Rust.
A diferença principal é que cada script roda **numa janela-alvo** e, no modo Segundo plano, não toma o seu mouse nem o teclado.

## Build

Requer Rust (toolchain `stable-msvc`) e o Visual Studio Build Tools com C++.

```bash
cargo build --release
```

O resultado é um único `target/release/ultratask.exe`. Os scripts ficam em `scripts\` ao lado do .exe, então dá para copiar a pasta e levar tudo junto.

## Uso rápido

1. Clique em **➕ Novo**. Na aba **Config**, use **🎯 Clicar na janela** ou **Escolher da lista** para definir a janela-alvo.
2. **⏺ Gravar** (ou `Ctrl+Shift+Alt+R`): use a janela-alvo normalmente e aperte o atalho de novo para parar.
3. **▶ Tocar** (ou `Ctrl+Shift+Alt+P`). Para tudo, use `Ctrl+Shift+Alt+X`.
4. Na aba **Blocos**:
   - edite, reordene ou adicione passos;
   - use **Explodir em passos** para transformar uma gravação em blocos editáveis;
   - use 📍 para capturar coordenadas, cor ou região clicando na janela.

### Modos
| Modo | Como funciona | Use para |
|---|---|---|
| Segundo plano | `PostMessage` direto para a janela; o cursor real não se mexe e scripts rodam em paralelo | apps desktop clássicos (Win32, WinForms, WPF) |
| Foco | traz a janela para frente e usa `SendInput` com scancodes; um script por vez; pausa enquanto você mexe no mouse/teclado | jogos e apps que ignoram o modo Segundo plano |
| Navegador (CDP) | o UltraTask abre o app com depuração remota e manda os eventos direto para a página pelo Chrome DevTools Protocol; o cursor real não se mexe, a janela pode ficar coberta e vários scripts rodam em paralelo | Chrome, Edge, apps Electron e WebView2 |

### Modo Navegador (CDP)
Em **Config → Modo**, escolha **Navegador (CDP)** e preencha:
- **Programa**: o exe do app (`chrome.exe`, `msedge.exe`, o exe do app Electron ou do app WebView2). Vazio = conectar a um app que você já abriu com `--remote-debugging-port`, na **Porta** informada.
- **Argumentos**: o resto da linha de comando, como a URL. Use aspas em argumentos com espaço.
- **Porta**: `0` escolhe uma porta livre a cada execução, o que permite várias instâncias ao mesmo tempo.
- **App WebView2**: marque para apps que embutem o WebView2.
- **Deixar o app aberto no fim**: por padrão o UltraTask fecha o app e apaga o perfil quando o script termina.

Ao tocar, o UltraTask:
1. abre o app com `--remote-debugging-port=<porta>`, `--remote-debugging-address=127.0.0.1`, um `--user-data-dir` temporário só desta execução (em `%TEMP%\UltraTask-cdp\`) e as flags que impedem o Chromium de congelar janelas cobertas (`--disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling`). No WebView2, a porta e essas flags vão pela variável `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, e o perfil por `WEBVIEW2_USER_DATA_FOLDER`;
2. conecta em `127.0.0.1` (WebSocket sem TLS) e escolhe a página cujo título (`document.title`) casa com o **Título** da janela-alvo. Sem janela-alvo, usa a primeira página;
3. roda os blocos: Clique, Mover, Rolar, Tecla e Digitar viram `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent` (com Ctrl/Alt/Shift, então Ctrl+S funciona) e `Input.insertText`. Pixel e OCR usam `Page.captureScreenshot`.

As coordenadas continuam relativas à área cliente da janela, como nos outros modos (o 📍 funciona igual). O modo desconta a barra do navegador (abas, endereço, menu do Electron) e divide pelo `devicePixelRatio`.

Limitações: só a página é controlada, não a barra do navegador nem diálogos nativos (salvar arquivo, alertas do sistema). Apps de instância única que repassam a abertura para outro processo podem fechar logo ao abrir.

#### Risco da porta de depuração
Enquanto o app roda, **qualquer programa deste PC** que conecte na porta controla o app: lê cookies e sessões, navega e executa JavaScript como você. Foi por isso que o Chrome 136 passou a exigir um `--user-data-dir` próprio para abrir a porta. Por isso o UltraTask:
- abre a porta só em `127.0.0.1`, que não aceita conexões de outras máquinas;
- usa um perfil temporário por execução e apaga esse perfil quando o script termina.

Não conecte o modo (Programa vazio) a um navegador aberto com o seu perfil do dia a dia. Não deixe o app aberto com a porta além do necessário, e nunca exponha a porta na rede (`--remote-debugging-address=0.0.0.0` ou redirecionamento de porta).

### Expressões
Os campos numéricos e as condições aceitam expressões ([evalexpr](https://docs.rs/evalexpr)).
- Exemplos: `contador + 1`, `x * 2`, `rand(100, 300)`, `contem(texto, "OK")`, `a > 3 && b == "fim"`.
- Os blocos **Digitar** e **Mostrar no log** interpolam `{expressão}`.

### Debug
- **Mostrar no log**: escreve um texto com variáveis, como `contador = {contador}`.
- **Breakpoint**: pausa a execução.
- No painel **Execuções**, cada execução mostra o bloco atual e as variáveis, e tem ⏸/▶ para pausar e continuar a qualquer momento.
- As execuções terminadas ficam na lista para você conferir os valores finais.
- **Config → Debug** registra no log cada bloco executado.

### Gatilhos e segundo plano
Na aba **Config → Gatilho**, um script pode rodar:
- **ao iniciar o UltraTask**: uma vez, assim que a janela-alvo existir;
- **sempre que uma nova janela-alvo abrir**: por exemplo, ao abrir o app ou o jogo.

Os dois aceitam um atraso em segundos.

Em **⚙ Ajustes**, a opção **Iniciar com o Windows** faz o UltraTask subir escondido na bandeja (`ultratask.exe --tray`) ao entrar no Windows. Fechar a janela também só esconde o app; gatilhos e atalhos continuam ativos. Abrir o exe de novo mostra a instância que já está rodando.

O tema (sistema/claro/escuro) fica em Ajustes ou no botão ☀/🌙 da barra.

## Scripts gerados por IA
[`skills/ultratask-scripts/SKILL.md`](skills/ultratask-scripts/SKILL.md) é uma *Agent Skill* que ensina agentes de IA a escrever o JSON dos scripts. Para instalar no Claude Code, copie a pasta para `~/.claude/skills/` (ou para `.claude/skills/` de um projeto). O agente valida o que escreveu com:

```bash
ultratask.exe --validate "scripts/Meu script.json"
```

O validador imprime `OK` ou `ERRO <caminho JSON>: <motivo>` e sai com código 1 se houver erro. Depois, clique em 🔄 na lista de scripts para recarregar.

## Limitações conhecidas
- O modo Segundo plano depende do app. Ele não funciona em:
  - jogos com DirectInput/RawInput;
  - apps que checam a posição real do cursor (Chromium/Electron e o próprio egui/winit). Para Chromium, Electron e WebView2, use o modo Navegador (CDP);
  - janelas elevadas (nesse caso, use "Reiniciar como administrador").
- Combinações com Ctrl/Alt/Shift no Segundo plano são instáveis (`GetKeyState` reflete o teclado real).
- Jogos com mouse relativo (câmera FPS) podem não girar com movimentos absolutos.
- Pixel e OCR no Segundo plano usam `PrintWindow`. Jogos DirectX podem devolver uma imagem preta (no modo Foco funciona).
- Jogos online com anti-cheat podem detectar input simulado e banir a conta.

## Testes
```bash
cargo test                      # interpretador, debug, validação, explode, glob, interpolação, JSON, lParam, CDP (tradução, coordenadas, lançamento, cliente contra servidor falso)
cargo test -- --include-ignored # + E2E real (janela EDIT em segundo plano + OCR) e registro numa chave temporária do HKCU
# CDP contra Chromium de verdade (um por porta, abertos com --remote-debugging-port e título "UT..."):
ULTRATASK_CDP_PORTS=9331,9332,9333 cargo test -- --ignored cdp_real
```
