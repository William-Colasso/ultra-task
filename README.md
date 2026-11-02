# UltraTask

UltraTask é um gravador e reprodutor de macros para Windows, inspirado no TinyTask e escrito todo em Rust.
A diferença principal é que cada script roda **numa janela-alvo** e, no modo Segundo plano, não toma o seu mouse nem o teclado.

## Requisitos
- Windows 10 ou 11 de 64 bits. Não há nada para instalar: basta copiar o `ultratask.exe` sozinho para qualquer pasta.
  - Não depende do runtime do Visual C++ (o CRT vai embutido no exe) e só usa DLLs que vêm com o Windows.
  - Funciona sem placa de vídeo (VM, área de trabalho remota, Windows Sandbox sem vGPU). Se o OpenGL não abrir, o UltraTask se reabre sozinho com `--render-software`, que desenha a janela com Direct3D 12 por software (WARP, que vem no Windows). Esse argumento também pode ser passado à mão.
  - Os blocos e condições de OCR precisam de um idioma com OCR no Windows. Sem nenhum, eles falham com uma mensagem que diz como instalar: **Configurações > Hora e idioma > Idioma e região > Português (Brasil) > Opções de idioma > Reconhecimento óptico de caracteres**, ou, no PowerShell como administrador, `Add-WindowsCapability -Online -Name "Language.OCR~~~pt-BR~0.0.1.0"`.
- Scripts e ajustes ficam ao lado do exe. Se essa pasta não aceita escrita (ex.: `C:\Program Files`), o UltraTask avisa no log e usa `%LOCALAPPDATA%\UltraTask`.

## Build

Requer Rust (toolchain `stable-msvc`) e o Visual Studio Build Tools com C++. O `.cargo/config.toml` liga o CRT estático (`+crt-static`) nos alvos MSVC.

```bash
cargo build --release
```

O resultado é um único `target/release/ultratask.exe`. Os scripts ficam em `scripts\` ao lado do .exe, então dá para copiar a pasta e levar tudo junto.

## Uso rápido

1. Clique em **➕ Novo**. Na aba **Config**, use **🎯 Clicar na janela** ou **Escolher da lista** para definir a janela-alvo.
2. **⏺ Gravar** (ou `Ctrl+Shift+Alt+R`): use a janela-alvo normalmente e aperte o atalho de novo para parar.
3. **▶ Tocar** (ou `Ctrl+Shift+Alt+P`). Para tudo, use `Ctrl+Shift+Alt+X`.
4. Na aba **Blocos**:
   - arraste blocos da paleta à esquerda para a árvore (ou clique num deles para pôr no fim);
   - arraste um cartão pela alça ☰ para reordenar ou pôr dentro de Se/Repetir/Enquanto; o botão direito no cartão oferece "Envolver em…", e ⏷/⏵ recolhe um container;
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

**Config → Mouse → Movimento humano do mouse** (`"human_mouse": true` no JSON): Mover, Clique, Rolar e arrastos levam o ponteiro por uma curva, acelerando e freando, com duração que cresce com a distância e às vezes passando um pouco do ponto e corrigindo. O clique cai exatamente no alvo (mais o ± px do bloco). No Segundo plano, a janela recebe os movimentos intermediários. A velocidade do script escala o trajeto; gravações tocam como foram gravadas.

### Elementos (UI Automation)
O bloco **Elemento (UIA)** age direto num controle da janela-alvo pela UI Automation do Windows, sem mouse nem teclado. Por isso roda em paralelo e com a janela coberta, inclusive em apps que ignoram o modo Segundo plano (Chromium 126+, Electron com acessibilidade ligada, WPF, WinForms, UWP, Qt e egui).
- Capture o elemento com 📍: o clique grava AutomationId, Name, ControlType, ClassName e o ponto clicado. A identificação pode ser editada clicando nela.
- Ações: acionar, alternar, definir valor (aceita `{expressão}`), selecionar, expandir, recolher, e ler o valor ou o nome para uma variável.
- O bloco espera o elemento aparecer até o timeout. Sem achar, para com erro ou, com **se não achar, clicar no ponto**, clica no ponto capturado e avisa no log.
- A condição **elemento (UIA)** testa se ele existe, se o nome ou o valor contém um texto, e se está marcado.
- As chamadas UIA rodam numa thread própria, com timeout de 3 s por chamada. Um app que trava no "acionar" (uma janela modal, por exemplo) gera só um aviso.
- Apps elevados exigem o UltraTask elevado.
- Ao gravar, cada clique também guarda o elemento UIA sob o cursor (campo `elements` da gravação). Ele é lido numa thread à parte, então a gravação não fica mais lenta. **Explodir em passos** transforma os cliques em botões, caixas e itens (Invoke, Toggle, Select) em blocos **Elemento (UIA)** com o clique no mesmo ponto como reserva; os outros cliques continuam como **Clique**. A reprodução da gravação bruta não muda.

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

### Exportar como .exe
**📦 Exportar .exe** (na lista de scripts) grava um executável que só toca o script selecionado. Ele é uma cópia do UltraTask que leva embutidos:
- o script e todos os scripts que ele chama com **Chamar script**, em cadeia, com as integrações de cada um (só as do script principal ficam ativas, como no app);
- os ajustes de reprodução: atalho de pânico, retomada do modo Foco e ampliação do OCR.

A exportação é recusada, com o motivo no log, se o script não tem janela-alvo ou blocos, se falta algum script chamado ou se o validador reprova algum deles.

Ao abrir o .exe exportado:
- nenhuma janela de edição aparece; ele procura a janela-alvo pela regra do script e, se ela não existir, espera que apareça (o ícone da bandeja mostra "Aguardando janela…");
- toca com o modo e as repetições do script, ignorando o gatilho e o atalho dele;
- **Parar** no menu da bandeja ou o atalho de pânico interrompem e soltam as teclas e botões presos;
- sai com código **0** (terminou), **1** (erro) ou **2** (parado). Aberto num terminal, mostra o log ali. Como é um app de janela, use `start /wait meu.exe` no cmd ou `(Start-Process .\meu.exe -Wait -PassThru).ExitCode` no PowerShell para esperar e ler o código.

Ele não lê nem grava os ajustes nem a pasta de scripts do UltraTask, e não conflita com o UltraTask aberto nem com outros .exe exportados (vários podem rodar ao mesmo tempo no modo Segundo plano).

> **Antivírus:** executáveis de macro que simulam teclado e mouse e não são assinados costumam ser estranhados por antivírus e pelo SmartScreen ("O Windows protegeu o computador"). Se for distribuir o .exe, avise quem vai usar ou assine o arquivo.

### Integrações
A aba **Integrações** de cada script liga o fluxo a serviços externos sem travá-lo. Cada integração tem:
- **gatilho**: quando um bloco define certas variáveis, a cada N segundos, ou ao terminar a execução;
- **destino**: por enquanto REST (HTTP/HTTPS), com método, URL com `{…}`, cabeçalhos e timeout;
- **envio**: JSON das variáveis escolhidas, uma linha CSV ou um texto com `{…}`;
- **resultado** (opcional): a variável `r` recebe o corpo da resposta; `r_ok`, `r_status` e `r_pendente` dizem como foi.

O envio roda em paralelo. Para usar a resposta, ponha **Esperar até** `!r_pendente` e leia os campos com `json(r, "/caminho")`, por exemplo `json(endereco, "/logradouro")`. Os erros vão para o log, e as variáveis aparecem no painel **Execuções**. O HTTPS usa o WinHTTP do Windows, com os certificados e o proxy do sistema.

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
- Combinações com Ctrl/Alt/Shift no Segundo plano: o estado dos modificadores é falsificado só para a thread do alvo (`GetKeyState`). Apps que leem o teclado com `GetAsyncKeyState` ou RawInput continuam vendo o teclado real e ignoram o modificador. Se o alvo não responde, a combinação não é enviada e o log avisa.
- Jogos com mouse relativo (câmera FPS) podem não girar com movimentos absolutos.
- Pixel e OCR no Segundo plano usam `PrintWindow`. Se a imagem vier toda preta (ou de uma cor só), como em jogos DirectX e alguns Chromium encobertos, a execução passa a usar o Windows Graphics Capture para aquela janela até o fim (Windows 10 1903 ou mais novo). No Windows 10 pode aparecer uma borda amarela na janela capturada; onde o sistema deixa, ela é desligada. Sem o Windows Graphics Capture (Windows 7/8), fica a imagem preta (no modo Foco funciona). Janela minimizada não é capturada: a condição dá falso com aviso.
- Jogos online com anti-cheat podem detectar input simulado e banir a conta.

## Testes
```bash
cargo test                      # interpretador, debug, validação, explode, glob, interpolação, JSON, lParam, integrações (servidor HTTP local), UIA (árvore falsa), CDP (tradução, coordenadas, lançamento, cliente contra servidor falso)
cargo test -- --include-ignored # + E2E real (janela EDIT em segundo plano + OCR, UI Automation numa janela própria, Ctrl+S em segundo plano, alvo travado, captura WGC com a janela coberta), registro numa chave temporária do HKCU e HTTPS real (viacep.com.br)
# CDP contra Chromium de verdade (um por porta, abertos com --remote-debugging-port e título "UT..."):
ULTRATASK_CDP_PORTS=9331,9332,9333 cargo test -- --ignored cdp_real
```
