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
- **destino**:
  - REST (HTTP/HTTPS), com método, URL com `{…}`, cabeçalhos e timeout;
  - Arquivo: acrescenta, sobrescreve ou lê um arquivo de texto (a leitura vai para a variável de resultado). O caminho aceita `{…}`, um caminho relativo parte da pasta do exe, e as pastas que faltam são criadas. JSON grava uma linha por envio, CSV grava o cabeçalho quando o arquivo é novo, e texto vai como está;
  - Comando: roda um programa sem janela de console, com argumentos que aceitam `{…}`. O envio vai pelo stdin, o stdout vai para a variável de resultado e o código de saída para `r_status`; se o timeout estoura, o processo é encerrado;
  - Área de transferência: escreve o envio ou lê o texto atual para a variável de resultado;
  - SQLite: um banco num arquivo `.db` (criado se faltar), com o SQLite embutido no exe. O SQL usa `:variavel`, ligado ao valor da variável com o tipo preservado e sem interpolação de texto (sem injeção). Um SELECT põe as linhas em JSON na variável de resultado (`json(r, "/0/nome")`); os outros comandos põem as linhas afetadas em `r_status`;
  - ODBC: bancos já instalados no PC (SQL Server, MySQL, PostgreSQL, Access…) pelo gerenciador ODBC do Windows, com um DSN ou uma string de conexão. O SQL e o resultado seguem o mesmo contrato do SQLite. **Atenção:** uma senha na string de conexão (`PWD=…`) fica em texto puro no JSON do script; prefira um DSN ou a autenticação do Windows (`Trusted_Connection=yes`);
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
- Combinações com Ctrl/Alt/Shift no Segundo plano são instáveis (`GetKeyState` reflete o teclado real).
- Jogos com mouse relativo (câmera FPS) podem não girar com movimentos absolutos.
- Pixel e OCR no Segundo plano usam `PrintWindow`. Jogos DirectX podem devolver uma imagem preta (no modo Foco funciona).
- Jogos online com anti-cheat podem detectar input simulado e banir a conta.

## Testes
```bash
cargo test                      # interpretador, debug, validação, explode, glob, interpolação, JSON, lParam, integrações (servidor HTTP local)
cargo test -- --include-ignored # + E2E real (janela EDIT em segundo plano + OCR), registro numa chave temporária do HKCU, HTTPS real (viacep.com.br), área de transferência, arquivo somente leitura e banco ODBC real (defina ULTRATASK_ODBC_TESTE com um DSN ou uma string de conexão)
```
