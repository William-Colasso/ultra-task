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

Em qualquer modo, o bloco `Uia` (veja *Elementos (UIA)*) age direto num controle, sem mouse nem teclado. Ele roda em paralelo e com a janela coberta, inclusive em apps que ignoram o Background (Chromium 126+, Electron com acessibilidade, WPF, WinForms, UWP, Qt, egui). Prefira `Uia` a `Click` quando o controle tiver nome ou AutomationId.

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
  "blocks": []
}
```

| Campo | Significado | Padrão |
|---|---|---|
| `target` | `exe` compara sem diferenciar maiúsculas. `title` é um glob (`*` e `?`). `null` para sub-rotinas chamadas com `Call`: elas usam a janela de quem chamou | `null` |
| `simulate_focus` | só no Background: finge que a janela está ativa (alguns apps e jogos exigem isso) | `false` |
| `speed` | multiplica a velocidade dos blocos `Recording` | `1.0` |
| `repeat` | quantas vezes rodar `blocks`; `0` = infinito | `1` |
| `hotkey` | atalho global; os 5 campos são obrigatórios; `vk` segue a tabela de *Teclas* | `null` |
| `client_size` | `[w, h]` da área cliente em que as coordenadas foram medidas. Antes de tocar, o app redimensiona a janela para esse tamanho. Use `null` quando não souber | `null` |
| `trigger` | `"Manual"`, `{"OnStartup":{"delay_s":N}}` (uma vez quando o UltraTask abre e a janela existe) ou `{"OnWindowOpen":{"delay_s":N}}` (a cada nova janela-alvo). Gatilhos exigem `target` | `"Manual"` |
| `trace` | registra cada bloco no log (debug) | `false` |

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
{ "Uia": { "elem": ELEM, "action": "Invoke", "timeout_ms": "5000", "fallback_click": false } }
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
- `Uia` age num elemento da janela-alvo pela UI Automation; veja *Elementos (UIA)*. Só `elem` e `action` precisam aparecer: `timeout_ms` vale `"5000"` e `fallback_click` vale `false` quando omitidos.
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
{ "Uia": { "elem": ELEM, "test": "Exists" } }
```

- `Pixel`: cor RGB com tolerância por canal.
- `OcrContains`: busca o texto sem diferenciar maiúsculas.
- `Uia`: `test` é `"Exists"`, `{"NameContains": "texto"}`, `{"ValueContains": "texto"}` (sem diferenciar maiúsculas) ou `"Checked"` (caixa marcada). Faz uma busca só, sem esperar; para esperar o elemento, use a condição dentro de `WaitUntil`. Se o elemento não existir ou a UIA falhar, dá falso.
- Pixel e OCR capturam a janela-alvo; com a janela minimizada a condição dá falso. No modo Background, jogos DirectX podem aparecer pretos: nesse caso use `"Focus"`.

### Elementos (UIA)

`ELEM` identifica um controle dentro da janela-alvo. Todos os campos são opcionais, mas pelo menos um entre `automation_id`, `name`, `control_type` e `class_name` precisa estar preenchido:

```json
{ "automation_id": "btnSalvar", "name": "Salvar", "control_type": "Button", "class_name": "", "x": 120, "y": 45 }
```

| Campo | Como casa |
|---|---|
| `automation_id` | igual, diferenciando maiúsculas. Quando preenchido, `name` só desempata (o texto do botão pode mudar) |
| `name` | glob (`*`, `?`), sem diferenciar maiúsculas. Obrigatório casar quando não há `automation_id` |
| `control_type` | igual ao tipo da UIA, sem o prefixo: `Button`, `Edit`, `CheckBox`, `RadioButton`, `ComboBox`, `ListItem`, `TabItem`, `TreeItem`, `MenuItem`, `Hyperlink`, `Text`, `Pane`, `Window`, `Document`… |
| `class_name` | desempata; só é obrigatório casar quando não há `automation_id` nem `name` |
| `ancestors` | lista opcional de `{automation_id, name, control_type, class_name}`, do pai para cima. Só desempata. Omita ao escrever à mão |
| `x`, `y` | ponto (área cliente) onde o elemento foi capturado: desempata elementos iguais e é onde vai o clique de reserva |

Quem tem os valores é o usuário: o botão 📍 do bloco ou da condição preenche tudo clicando no controle. Ferramentas como o Accessibility Insights ou o Inspect do SDK do Windows também mostram AutomationId, Name e ControlType. Se você não sabe a identificação, escreva o que souber (por exemplo, só `name` e `control_type`) e peça ao usuário para conferir com 📍.

`action` é um destes:

| Ação | Faz |
|---|---|
| `"Invoke"` | aciona (botão, link, item de menu) |
| `"Toggle"` | alterna uma caixa de seleção |
| `{"SetValue": "texto {expr}"}` | define o valor de um campo; interpola `{expr}` como `Type` |
| `"Select"` | seleciona (item de lista, aba, botão de opção) |
| `"Expand"` / `"Collapse"` | expande ou recolhe (árvore, combo, menu) |
| `{"ReadValue": "var"}` | guarda o valor do campo (string) na variável |
| `{"ReadName": "var"}` | guarda o nome do elemento (string) na variável |

- O bloco procura o elemento a cada 200 ms até `timeout_ms` (*expr*). Sem achar, para com erro; com `"fallback_click": true`, clica em (`x`, `y`) como o `Click` faz e avisa no log.
- O elemento precisa oferecer o padrão da ação (Invoke, Toggle, Value, SelectionItem, ExpandCollapse); senão, o bloco para com erro.
- Se o app demorar mais de 3 s para responder a uma ação (por exemplo, um `Invoke` que abre uma janela modal), o bloco avisa no log e segue. Em `ReadValue`/`ReadName`, isso é erro.
- Apps rodando como administrador só respondem se o UltraTask também estiver elevado.

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

## Exemplo com UI Automation

Preenche um formulário de um app WinForms sem tocar no mouse nem no teclado, marca "Urgente", salva e confere a mensagem de status:

```json
{
  "name": "Pedido via UIA",
  "target": { "exe": "Pedidos.exe", "title": "Pedidos*" },
  "mode": "Background",
  "blocks": [
    { "SetVar": { "name": "n", "expr": "rand(100, 999)" } },
    { "Uia": { "elem": { "automation_id": "txtCliente", "control_type": "Edit" }, "action": { "SetValue": "Cliente {n}" } } },
    { "If": {
        "cond": { "kind": { "Uia": { "elem": { "automation_id": "chkUrgente", "control_type": "CheckBox" }, "test": "Checked" } }, "negate": true },
        "then": [ { "Uia": { "elem": { "automation_id": "chkUrgente", "control_type": "CheckBox" }, "action": "Toggle" } } ],
        "else_": [] } },
    { "Uia": { "elem": { "name": "Salvar", "control_type": "Button", "x": 320, "y": 410 }, "action": "Invoke", "timeout_ms": "3000", "fallback_click": true } },
    { "WaitUntil": {
        "cond": { "kind": { "Uia": { "elem": { "automation_id": "lblStatus" }, "test": { "NameContains": "salvo" } } }, "negate": false },
        "timeout_ms": "10000", "on_timeout": "Stop" } },
    { "Uia": { "elem": { "automation_id": "lblStatus" }, "action": { "ReadName": "status" } } },
    { "Log": { "text": "pedido {n}: {status}" } }
  ]
}
```

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
