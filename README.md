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
   - arraste blocos da paleta à esquerda para a árvore (ou clique num deles para pôr no fim);
   - arraste um cartão pela alça ☰ para reordenar ou pôr dentro de Se/Repetir/Enquanto; o botão direito no cartão oferece "Envolver em…", e ⏷/⏵ recolhe um container;
   - use **Explodir em passos** para transformar uma gravação em blocos editáveis;
   - use 📍 para capturar coordenadas, cor ou região clicando na janela.

### Modos
| Modo | Como funciona | Use para |
|---|---|---|
| Segundo plano | `PostMessage` direto para a janela; o cursor real não se mexe e scripts rodam em paralelo | apps desktop clássicos (Win32, WinForms, WPF) |
| Foco | traz a janela para frente e usa `SendInput` com scancodes; um script por vez; pausa enquanto você mexe no mouse/teclado | jogos e apps que ignoram o modo Segundo plano |

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
  - apps que checam a posição real do cursor (Chromium/Electron e o próprio egui/winit);
  - janelas elevadas (nesse caso, use "Reiniciar como administrador").
- Combinações com Ctrl/Alt/Shift no Segundo plano: o estado dos modificadores é falsificado só para a thread do alvo (`GetKeyState`). Apps que leem o teclado com `GetAsyncKeyState` ou RawInput continuam vendo o teclado real e ignoram o modificador. Se o alvo não responde, a combinação não é enviada e o log avisa.
- Jogos com mouse relativo (câmera FPS) podem não girar com movimentos absolutos.
- Pixel e OCR no Segundo plano usam `PrintWindow`. Se a imagem vier toda preta (ou de uma cor só), como em jogos DirectX e alguns Chromium encobertos, a execução passa a usar o Windows Graphics Capture para aquela janela até o fim (Windows 10 1903 ou mais novo). No Windows 10 pode aparecer uma borda amarela na janela capturada; onde o sistema deixa, ela é desligada. Sem o Windows Graphics Capture (Windows 7/8), fica a imagem preta (no modo Foco funciona). Janela minimizada não é capturada: a condição dá falso com aviso.
- Jogos online com anti-cheat podem detectar input simulado e banir a conta.

## Testes
```bash
cargo test                      # interpretador, debug, validação, explode, glob, interpolação, JSON, lParam, integrações (servidor HTTP local), UIA (árvore falsa)
cargo test -- --include-ignored # + E2E real (janela EDIT em segundo plano + OCR, UI Automation numa janela própria, Ctrl+S em segundo plano, alvo travado, captura WGC com a janela coberta), registro numa chave temporária do HKCU e HTTPS real (viacep.com.br)
```
