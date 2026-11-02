---
name: ultratask-scripts
description: Gera e edita scripts JSON do UltraTask (macros de mouse/teclado presas a uma janela-alvo, com blocos, condições e expressões). Use ao criar, editar, corrigir ou validar um arquivo da pasta scripts\ do UltraTask, ou ao transformar a descrição de uma automação num script do UltraTask.
---

# Scripts do UltraTask

Um script do UltraTask é um arquivo JSON. Ele descreve a janela-alvo e uma árvore de blocos (cliques, teclas, esperas, condições, laços, variáveis) que o app executa nessa janela.

## Passos

1. **Ache a pasta.** Os scripts ficam em `scripts\` ao lado do `ultratask.exe` (num build de desenvolvimento: `target\debug\scripts\` ou `target\release\scripts\`). Se você não sabe onde está o exe, pergunte ao usuário. Para editar, leia o arquivo existente primeiro: o app grava o JSON numa linha só, mas aceita qualquer formatação.
2. **Junte os fatos que só o usuário tem:**
   - a janela-alvo: nome do `.exe` e título;
   - o modo (veja *Modos*);
   - as coordenadas.

   As coordenadas são pixels físicos relativos à **área cliente** da janela-alvo. Elas vêm do usuário (botão 📍 do app, que preenche x/y/cor clicando na janela) ou de um script/gravação que já exista. Quando faltar uma coordenada, escreva o bloco com `"0"` e diga ao usuário qual campo ele precisa capturar com 📍.
3. **Escreva o JSON** seguindo a *Referência*. O arquivo se chama `<name>.json`, com os caracteres `<>:"/\|?*` de `name` trocados por `_`.
4. **Valide** cada arquivo escrito:
   ```bash
   "C:/caminho/ultratask.exe" --validate "C:/caminho/scripts/Meu script.json"
   ```
   No PowerShell, termine o comando com `| Out-String` para esperar a saída.

   O passo termina quando o validador imprime `OK <arquivo>` para todo arquivo e sai com código 0. Cada `ERRO` traz o caminho JSON do problema (ex.: `blocks[2].then[0].x`). Corrija e rode de novo.
5. **Avise o usuário** para clicar em 🔄 (Recarregar) na lista de scripts do app. Se o script estava aberto e com alterações não salvas no app, ele pode sobrescrever o seu arquivo; peça para recarregar antes de editar.

## Modos

- `"Background"`: envia mensagens direto para a janela. O cursor real não se mexe e vários scripts rodam em paralelo. Serve para apps desktop clássicos (Win32, WinForms, WPF, Bloco de Notas, ERPs).
- `"Focus"`: traz a janela para frente e usa o mouse/teclado reais (`SendInput` por scancode). Roda um script por vez. Serve para jogos, Chrome/Edge, apps Electron e qualquer app que ignore o modo Background. Combinações como Ctrl+S são confiáveis só neste modo.

## Referência

### Script (nível de cima)

Campos omitidos usam o padrão; campos desconhecidos são **rejeitados**.

```json
{
  "name": "Login diário",
  "target": { "exe": "Notepad.exe", "title": "*Bloco de Notas*" },
  "mode": "Background",
  "simulate_focus": false,
  "speed": 1.0,
  "repeat": 1,
  "hotkey": { "ctrl": true, "shift": false, "alt": true, "win": false, "vk": 49 },
  "client_size": [800, 600],
  "trigger": { "OnWindowOpen": { "delay_s": 3.0 } },
  "trace": false,
  "human_mouse": false,
  "blocks": []
}
```

| Campo | Significado | Padrão |
|---|---|---|
| `target` | `exe` compara sem diferenciar maiúsculas. `title` é um glob (`*` e `?`). `null` para sub-rotinas chamadas com `Call`: elas usam a janela de quem chamou | `null` |
| `simulate_focus` | só no Background: finge que a janela está ativa (alguns apps e jogos exigem isso) | `false` |
| `speed` | multiplica a velocidade dos blocos `Recording` e dos trajetos do `human_mouse` | `1.0` |
| `repeat` | quantas vezes rodar `blocks`; `0` = infinito | `1` |
| `hotkey` | atalho global; os 5 campos são obrigatórios; `vk` segue a tabela de *Teclas* | `null` |
| `client_size` | `[w, h]` da área cliente em que as coordenadas foram medidas. Antes de tocar, o app redimensiona a janela para esse tamanho. Use `null` quando não souber | `null` |
| `trigger` | `"Manual"`, `{"OnStartup":{"delay_s":N}}` (uma vez quando o UltraTask abre e a janela existe) ou `{"OnWindowOpen":{"delay_s":N}}` (a cada nova janela-alvo). Gatilhos exigem `target` | `"Manual"` |
| `trace` | registra cada bloco no log (debug) | `false` |
| `human_mouse` | movimento humano do mouse: `Move`, `Click` e `Scroll` levam o ponteiro da posição atual até o alvo por uma curva com aceleração, freada e às vezes uma correção, em vez de saltar. O clique cai no ponto do bloco (± `jitter_px`). Vale nos dois modos; `Recording` não muda. Use `true` quando o app ou jogo estranha cursor que se teletransporta ou quando um arrasto (`Down` → `Move` → `Up`) precisa de movimentos intermediários | `false` |

Exemplo de `human_mouse`: um arrasto no Paint que deixa um traço curvo, no modo Foco.

```json
{
  "name": "Traço no Paint",
  "target": { "exe": "mspaint.exe", "title": "*Paint*" },
  "mode": "Focus",
  "human_mouse": true,
  "blocks": [
    { "Click": { "x": "200", "y": "300", "button": "Left", "kind": "Down", "jitter_px": 0 } },
    { "Move": { "x": "700", "y": "450" } },
    { "Click": { "x": "700", "y": "450", "button": "Left", "kind": "Up", "jitter_px": 0 } }
  ]
}
```

### Blocos

As variantes sem dados são **strings**; as outras são objetos de uma chave só. Os campos marcados *expr* são **strings** com uma expressão (veja *Expressões*), mesmo quando são só um número: `"x": "120"`. Todo campo listado é obrigatório.

```json
{ "Click": { "x": "120", "y": "45", "button": "Left", "kind": "Click", "jitter_px": 0 } }
{ "Move": { "x": "120", "y": "45" } }
{ "Scroll": { "x": "300", "y": "200", "delta": -120 } }
{ "Key": { "vk": 13, "action": "Press" } }
{ "Type": { "text": "Olá {nome}, pedido {n + 1}" } }
{ "Wait": { "ms": "500", "jitter_ms": 100 } }
{ "WaitUntil": { "cond": COND, "timeout_ms": "10000", "on_timeout": "Stop" } }
{ "If": { "cond": COND, "then": [], "else_": [] } }
{ "Repeat": { "times": "3", "body": [] } }
{ "While": { "cond": COND, "body": [] } }
{ "SetVar": { "name": "contador", "expr": "contador + 1" } }
{ "OcrToVar": { "name": "texto", "region": { "x": 10, "y": 10, "w": 200, "h": 30 } } }
{ "Call": { "script": "Login" } }
{ "Log": { "text": "contador = {contador}" } }
"Breakpoint"
"Stop"
```

- `Click`:
  - `x`, `y` são *expr*;
  - `button`: `"Left"` | `"Right"` | `"Middle"`;
  - `kind`: `"Click"` | `"Double"` | `"Down"` | `"Up"` (para arrastar: `Down`, depois `Move`s, depois `Up`);
  - `jitter_px` sorteia ±N px.
- `Key`: `action` é `"Press"` | `"Down"` | `"Up"`. Para uma combinação: `Key` Ctrl `Down`, `Key` S `Press`, `Key` Ctrl `Up`.
- `Type` digita texto Unicode; `{expr}` é trocado pelo valor e `{{`/`}}` viram chaves literais. `Log` usa a mesma interpolação e escreve no log do app.
- `Wait`: `ms` é *expr*; `jitter_ms` sorteia ±N ms.
- `WaitUntil`: confere a condição a cada 100 ms. `timeout_ms` é *expr*. `on_timeout`: `"Stop"` encerra com erro, `"Continue"` segue adiante.
- `Repeat.times` é *expr*. `While` repete enquanto a condição for verdadeira.
- `SetVar`: `name` é um identificador (letras, dígitos, `_`; não começa com dígito); `expr` é *expr*. `OcrToVar` guarda o texto lido (string, sem espaços nas pontas).
- `Call` roda outro script da mesma pasta, pelo `name` dele, com a mesma janela e as mesmas variáveis.
- `Breakpoint` pausa até o usuário clicar ▶ no painel de execuções, onde ele vê as variáveis. `Stop` encerra o script.
- `Recording` é uma gravação feita pelo app. Copie-a de scripts existentes em vez de escrever à mão: `{"Recording":{"events":[{"t":0,"ev":{"Move":[10,20]}}, …],"speed":1.0}}`.

### Condições (`COND`)

```json
{ "kind": KIND, "negate": false }
```

`negate: true` inverte o resultado. `KIND` é um destes:

```json
{ "Pixel": { "x": "10", "y": "10", "color": [255, 0, 0], "tol": 10 } }
{ "OcrContains": { "region": { "x": 0, "y": 0, "w": 300, "h": 40 }, "text": "Pronto" } }
"WindowExists"
"WindowForeground"
{ "TitleMatches": "*Nível 3*" }
{ "Expr": "contador < 10 && contem(texto, \"OK\")" }
```

- `Pixel`: cor RGB com tolerância por canal.
- `OcrContains`: busca o texto sem diferenciar maiúsculas.
- Pixel e OCR capturam a janela-alvo; com a janela minimizada a condição dá falso. No modo Background, jogos DirectX podem aparecer pretos: nesse caso use `"Focus"`.

### Expressões

São [evalexpr](https://docs.rs/evalexpr):
- números `3`, `2.5`; strings `"texto"` (dentro do JSON: `\"texto\"`); booleanos `true`/`false`;
- operadores `+ - * / % ^`, `== != < > <= >=`, `&& || !`. A divisão de inteiros trunca. `+` concatena duas strings.
- funções do UltraTask: `rand(a, b)` (inteiro sorteado entre a e b, inclusive) e `contem(texto, trecho)` (busca sem diferenciar maiúsculas);
- funções nativas: `min`, `max`, `floor`, `round`, `ceil`, `len`, `if(cond, a, b)`, `str::to_lowercase`, `str::to_uppercase`, `str::trim`, `str::from`, `str::substring`, `math::abs`, `math::sqrt`.

Variáveis são globais na execução (inclusive dentro de `Call`). Uma variável só existe depois do primeiro `SetVar`/`OcrToVar`; ler antes disso é erro em tempo de execução.

### Teclas (`vk`, decimal)

| Teclas | Códigos |
|---|---|
| A–Z | 65–90 |
| 0–9 | 48–57 |
| F1–F12 | 112–123 |
| Enter, Esc, Tab, Espaço, Backspace | 13, 27, 9, 32, 8 |
| ←, ↑, →, ↓ | 37, 38, 39, 40 |
| Insert, Delete, Home, End, PgUp, PgDn | 45, 46, 36, 35, 33, 34 |
| Shift, Ctrl, Alt, Win | 16, 17, 18, 91 |
| Num0–Num9 | 96–105 |

Para outras teclas, use o código *Virtual-Key* do Windows.

## Exemplo completo

Toda vez que o Bloco de Notas abre, espera 2 s, digita três linhas numeradas e confere pelo OCR se elas apareceram:

```json
{
  "name": "Notas numeradas",
  "target": { "exe": "Notepad.exe", "title": "*Bloco de Notas*" },
  "mode": "Background",
  "trigger": { "OnWindowOpen": { "delay_s": 2.0 } },
  "blocks": [
    { "SetVar": { "name": "n", "expr": "1" } },
    { "While": {
        "cond": { "kind": { "Expr": "n <= 3" }, "negate": false },
        "body": [
          { "Type": { "text": "Linha {n}" } },
          { "Key": { "vk": 13, "action": "Press" } },
          { "SetVar": { "name": "n", "expr": "n + 1" } },
          { "Wait": { "ms": "rand(150, 300)", "jitter_ms": 0 } }
        ] } },
    { "OcrToVar": { "name": "lido", "region": { "x": 0, "y": 0, "w": 400, "h": 120 } } },
    { "If": {
        "cond": { "kind": { "Expr": "contem(lido, \"Linha 3\")" }, "negate": false },
        "then": [ { "Log": { "text": "ok: {lido}" } } ],
        "else_": [ { "Log": { "text": "OCR não achou: {lido}" } }, "Breakpoint" ] } }
  ]
}
```
