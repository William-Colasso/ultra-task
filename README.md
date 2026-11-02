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
   - edite, reordene ou adicione passos;
   - use **Explodir em passos** para transformar uma gravação em blocos editáveis;
   - use 📍 para capturar coordenadas, cor ou região clicando na janela.

### Modos
| Modo | Como funciona | Use para |
|---|---|---|
| Segundo plano | `PostMessage` direto para a janela; o cursor real não se mexe e scripts rodam em paralelo | apps desktop clássicos (Win32, WinForms, WPF) |
| Foco | traz a janela para frente e usa `SendInput` com scancodes; um script por vez; pausa enquanto você mexe no mouse/teclado | jogos e apps que ignoram o modo Segundo plano |

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
  - apps que checam a posição real do cursor (Chromium/Electron e o próprio egui/winit);
  - janelas elevadas (nesse caso, use "Reiniciar como administrador").
- Combinações com Ctrl/Alt/Shift no Segundo plano são instáveis (`GetKeyState` reflete o teclado real).
- Jogos com mouse relativo (câmera FPS) podem não girar com movimentos absolutos.
- Pixel e OCR no Segundo plano usam `PrintWindow`. Jogos DirectX podem devolver uma imagem preta (no modo Foco funciona).
- Jogos online com anti-cheat podem detectar input simulado e banir a conta.

## Testes
```bash
cargo test                      # interpretador, debug, validação, explode, glob, interpolação, JSON, lParam
cargo test -- --include-ignored # + E2E real (janela EDIT em segundo plano + OCR) e registro numa chave temporária do HKCU
```
