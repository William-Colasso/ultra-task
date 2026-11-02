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
- `"Focus"`: traz a janela para frente e usa o mouse/teclado reais (`SendInput` por scancode). Roda um script por vez. Serve para jogos e qualquer app que ignore o modo Background. Combinações como Ctrl+S são confiáveis neste modo e no CDP.
- **Navegador (CDP)**: preencha o campo `cdp` (veja *Modo Navegador (CDP)*); `mode` e `simulate_focus` passam a ser ignorados. O UltraTask abre o app e controla a página pelo Chrome DevTools Protocol, sem mexer no mouse/teclado reais, com a janela coberta e com várias instâncias em paralelo. Prefira este modo para Chrome, Edge, apps Electron e apps WebView2.

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
  "human_mouse": false,
  "blocks": [],
  "integrations": []
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
| `integrations` | chamadas REST em paralelo ao fluxo (veja *Integrações*) | `[]` |
| `cdp` | liga o modo Navegador (CDP); veja abaixo. Omita nos outros modos | ausente |

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

### Modo Navegador (CDP)

```json
"cdp": { "exe": "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", "args": "https://exemplo.com", "port": 0, "webview2": false, "keep_open": false }
```

| Campo | Significado | Padrão |
|---|---|---|
| `exe` | o programa que o UltraTask abre (`chrome.exe`, `msedge.exe`, o exe do app Electron ou WebView2). No JSON, `\\` em cada barra. `""` = conectar a um app já aberto com `--remote-debugging-port` na `port` | `""` |
| `args` | resto da linha de comando (ex.: a URL); argumentos com espaço vão entre `\"...\"` | `""` |
| `port` | `0` = porta livre a cada execução (permite várias instâncias). Obrigatória quando `exe` é `""` | `0` |
| `webview2` | o app embute WebView2: a porta vai por `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` | `false` |
| `keep_open` | não fecha o app no fim (o perfil temporário fica no disco) | `false` |

- A página é escolhida pelo `target.title`, comparado com o título da **página** (`document.title`), não com o da janela. No Chrome, use `"*Parte do título*"` em vez de `"Título - Google Chrome"`. `target` pode ser `null`: usa a primeira página.
- Coordenadas continuam na área cliente da janela, como nos outros modos.
- Com `exe` preenchido, `trigger` precisa ser `"Manual"`: cada execução abre o app de novo.
- `Type` usa `Input.insertText`; `Key` aceita combinações (Ctrl `Down`, S `Press`, Ctrl `Up` = Ctrl+S). `Pixel`, `OcrContains` e `OcrToVar` usam a captura da página.
- `human_mouse` e `integrations` valem também aqui: o trajeto humano vira uma sequência de eventos de mouse dentro da página. Blocos e condições `Uia` **não** funcionam neste modo: o bloco para com erro e a condição dá falso.

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
  - O campo opcional `elements` guarda o elemento UIA de cada clique gravado: `"elements":[{"event":1,"elem":ELEM,"action":"Invoke"}]`. `event` é o índice, em `events`, do botão pressionado (`{"Button":["Left",true,x,y]}`); `action` (`"Invoke"`, `"Toggle"` ou `"Select"`) só aparece quando o elemento pode ser acionado sem mouse. Gravações sem `elements` continuam válidas.
  - A reprodução da gravação ignora `elements`. Ele só serve ao **Explodir em passos** do app: cliques esquerdos simples com `action` viram blocos `Uia` com `"fallback_click": true` (clique no mesmo ponto se o elemento sumir); os demais continuam `Click`. Não edite `elements` à mão.

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
- funções do UltraTask: `rand(a, b)` (inteiro sorteado entre a e b, inclusive), `contem(texto, trecho)` (busca sem diferenciar maiúsculas) e `json(texto, "/caminho")` (lê um campo de um texto JSON; veja *Integrações*);
- funções nativas: `min`, `max`, `floor`, `round`, `ceil`, `len`, `if(cond, a, b)`, `str::to_lowercase`, `str::to_uppercase`, `str::trim`, `str::from`, `str::substring`, `math::abs`, `math::sqrt`.

Variáveis são globais na execução (inclusive dentro de `Call`). Uma variável só existe depois do primeiro `SetVar`/`OcrToVar`; ler antes disso é erro em tempo de execução.

### Integrações (`integrations`)

Uma integração envia dados para fora **em paralelo**: os blocos seguintes não esperam. O fluxo reage pelas variáveis de resultado.

```json
"integrations": [{
  "name": "ViaCEP",
  "when": { "OnSet": ["cep"] },
  "dest": { "Http": { "method": "GET", "url": "https://viacep.com.br/ws/{cep}/json/", "headers": [], "timeout_ms": 10000 } },
  "payload": { "Json": ["cep"] },
  "result": "endereco"
}]
```

| Campo | Valores |
|---|---|
| `name` | texto não vazio; aparece no log |
| `when` | `{"OnSet": ["a", "b"]}`: quando um bloco `SetVar`/`OcrToVar` define uma dessas variáveis. `{"Every": {"s": 5.0}}`: a cada N segundos enquanto o script roda (pula se o envio anterior ainda não voltou). `"OnEnd"`: uma vez depois do último bloco; a execução espera a resposta até `timeout_ms` |
| `dest` | `{"Http": {"method", "url", "headers", "timeout_ms"}}`. `method`: `"GET"`, `"POST"`, `"PUT"`, `"PATCH"` ou `"DELETE"`. `url` e os valores de `headers` aceitam `{expr}`. `headers` é uma lista de pares: `[["Authorization", "Bearer {token}"]]`. `timeout_ms` é um **número** (não string). Outros destinos: veja *Outros destinos* abaixo |
| `payload` | `{"Json": ["a", "b"]}` envia `{"a": …, "b": …}` (números continuam números). `{"Csv": ["a", "b"]}` envia uma linha CSV. `{"Text": "pedido {n}"}` envia o texto interpolado. Em `GET` o corpo não vai: ponha os valores na `url` |
| `result` | nome de variável ou `null` |

Para `"result": "r"`, o app cria, desde o início da execução:
- `r`: o corpo da resposta (texto);
- `r_ok`: `true` quando o status é 2xx;
- `r_status`: o status HTTP, ou `0` em erro de rede (cada destino em *Outros destinos* diz o que vai aqui);
- `r_pendente`: vira `true` no instante do disparo (ainda dentro do bloco que definiu a variável) e `false` quando a resposta chega.

Regras:
- Espere a resposta com `WaitUntil` em `"!r_pendente"`, depois confira `r_ok`.
- Leia campos com `json(r, "/logradouro")` ou `json(r, "/itens/0/preco")` (JSON Pointer). Texto, número e booleano voltam como valores; objeto e lista voltam como texto JSON; campo ausente, `null` ou `r` vazio voltam como `""`.
- `when.OnSet` não pode conter a própria variável de resultado (nem `r_ok`, `r_status`, `r_pendente`).
- Respostas não disparam integrações. Para encadear, defina uma variável com `SetVar` depois da resposta.
- Só valem as integrações do script que está rodando; as de scripts chamados com `Call` ficam inativas.
- Parar o script descarta as respostas pendentes e não roda o `"OnEnd"`.
- Os erros (rede, status fora de 2xx, `{expr}` inválida) vão para o log e não param o fluxo.
- O HTTPS usa os certificados e o proxy do Windows.

Exemplo: consulta o CEP e digita a rua, ou avisa se não achou:

```json
{
  "name": "Consulta CEP",
  "target": { "exe": "Notepad.exe", "title": "*Bloco de Notas*" },
  "integrations": [{
    "name": "ViaCEP",
    "when": { "OnSet": ["cep"] },
    "dest": { "Http": { "method": "GET", "url": "https://viacep.com.br/ws/{cep}/json/", "headers": [], "timeout_ms": 10000 } },
    "payload": { "Json": ["cep"] },
    "result": "endereco"
  }, {
    "name": "Relatório",
    "when": "OnEnd",
    "dest": { "Http": { "method": "POST", "url": "https://exemplo.com/api/relatorio", "headers": [["Authorization", "Bearer {token}"]], "timeout_ms": 5000 } },
    "payload": { "Text": "cep {cep}: {rua}" },
    "result": null
  }],
  "blocks": [
    { "SetVar": { "name": "token", "expr": "\"abc123\"" } },
    { "SetVar": { "name": "rua", "expr": "\"\"" } },
    { "SetVar": { "name": "cep", "expr": "\"01001000\"" } },
    { "WaitUntil": { "cond": { "kind": { "Expr": "!endereco_pendente" }, "negate": false }, "timeout_ms": "15000", "on_timeout": "Stop" } },
    { "SetVar": { "name": "rua", "expr": "json(endereco, \"/logradouro\")" } },
    { "If": {
        "cond": { "kind": { "Expr": "endereco_ok && rua != \"\"" }, "negate": false },
        "then": [ { "Type": { "text": "Rua: {rua}" } } ],
        "else_": [ { "Log": { "text": "CEP não encontrado (status {endereco_status})" } } ] } }
  ]
}
```

#### Outros destinos

Todos seguem as mesmas regras de gatilho, envio e variáveis de resultado. Em erro, `r_ok` fica `false`, `r_status` fica `0` e a mensagem vai para o log.

**Arquivo** (`File`): `{"File": {"path": "dados/{dia}.csv", "mode": "Append"}}`.
- `path` aceita `{expr}`; um caminho relativo parte da pasta de dados: a pasta do `ultratask.exe` (ou do `.exe` exportado), ou `%LOCALAPPDATA%\UltraTask` quando ela não aceita escrita (a mesma pasta de `scripts\`). As pastas que faltam são criadas.
- `mode`: `"Append"` (acrescenta no fim), `"Overwrite"` (troca o conteúdo) ou `"Read"` (lê o arquivo inteiro para `r`; o `payload` é ignorado).
- Ao gravar: `Json` grava uma linha por envio; `Csv` grava a linha com os nomes das variáveis quando o arquivo é novo (ou vazio) e depois uma linha por envio; `Text` grava o texto como está, sem quebra de linha. As linhas de `Json` e `Csv` terminam em CRLF.
- `r_status`: bytes gravados ou lidos. `r`: o conteúdo lido (vazio ao gravar).

```json
{
  "name": "Planilha de pedidos",
  "integrations": [{
    "name": "Grava pedido",
    "when": { "OnSet": ["pedido"] },
    "dest": { "File": { "path": "relatorios/pedidos.csv", "mode": "Append" } },
    "payload": { "Csv": ["pedido", "cliente", "valor"] },
    "result": null
  }, {
    "name": "Lê pedidos",
    "when": "OnEnd",
    "dest": { "File": { "path": "relatorios/pedidos.csv", "mode": "Read" } },
    "payload": { "Text": "" },
    "result": "planilha"
  }],
  "blocks": [
    { "SetVar": { "name": "cliente", "expr": "\"Ana\"" } },
    { "SetVar": { "name": "valor", "expr": "19.9" } },
    { "SetVar": { "name": "pedido", "expr": "1001" } }
  ]
}
```

**Comando** (`Command`): `{"Command": {"program": "cmd", "args": ["/c", "echo {n}"], "timeout_ms": 10000}}`.
- Roda o programa sem janela de console, com a pasta de dados como pasta de trabalho (veja o Arquivo). `program` (caminho ou nome no PATH) e cada item de `args` aceitam `{expr}`. Para um comando do `cmd`, ponha a linha inteira num só argumento depois de `"/c"`. Evite aspas dentro dos argumentos do `cmd`: o escape do Windows e o do `cmd` são diferentes.
- O envio (`payload`) vai pelo **stdin**. O **stdout**, sem espaços nas pontas, vai para `r`; o **código de saída** vai para `r_status`; `r_ok` é `true` só com código 0 (outro código vai para o log, com o stderr).
- Se passar de `timeout_ms`, o processo (e o que ele abriu) é encerrado e o log diz `tempo esgotado`.
- Cuidado: um valor de variável dentro de `args` vira parte da linha de comando. Para dados vindos de fora, prefira mandar pelo stdin.

**Área de transferência** (`Clipboard`): `{"Clipboard": {"mode": "Write"}}`.
- `"Write"` põe o envio na área de transferência (como texto). `"Read"` lê o texto atual para `r` (o `payload` é ignorado; vazio quando não há texto).
- `r_status`: número de caracteres escritos ou lidos.

```json
{
  "name": "Procura e copia",
  "integrations": [{
    "name": "findstr",
    "when": { "OnSet": ["texto"] },
    "dest": { "Command": { "program": "cmd", "args": ["/c", "findstr /n {chave}"], "timeout_ms": 5000 } },
    "payload": { "Text": "{texto}" },
    "result": "achou"
  }, {
    "name": "Copia",
    "when": { "OnSet": ["achou_txt"] },
    "dest": { "Clipboard": { "mode": "Write" } },
    "payload": { "Text": "{achou_txt}" },
    "result": null
  }],
  "blocks": [
    { "SetVar": { "name": "chave", "expr": "\"pedido\"" } },
    { "SetVar": { "name": "texto", "expr": "\"pedido 42\"" } },
    { "WaitUntil": { "cond": { "kind": { "Expr": "!achou_pendente" }, "negate": false }, "timeout_ms": "6000", "on_timeout": "Stop" } },
    { "If": {
        "cond": { "kind": { "Expr": "achou_ok" }, "negate": false },
        "then": [ { "SetVar": { "name": "achou_txt", "expr": "achou" } } ],
        "else_": [ { "Log": { "text": "não achou (código {achou_status})" } } ] } }
  ]
}
```

**SQLite** (`Sqlite`): `{"Sqlite": {"path": "dados.db", "sql": "SELECT nome FROM clientes WHERE id = :id", "timeout_ms": 10000}}`.
- `path`: o arquivo `.db`, criado se faltar (com as pastas). Aceita `{expr}`; relativo parte da pasta de dados (veja o Arquivo). O SQLite vai embutido no exe (também no `.exe` exportado).
- `sql`: **um** comando. Os valores entram como `:variavel`, ligados ao valor atual da variável com o tipo preservado (inteiro, número, texto; booleano vira 1/0). O `sql` **não** aceita `{…}` e nunca recebe texto colado, então aspas e `;` nos valores não quebram nem injetam SQL. Não use `?`.
- SELECT (ou qualquer comando que devolve colunas): `r` recebe as linhas como lista JSON de objetos, ex.: `[{"id":1,"nome":"Ana"}]`; leia com `json(r, "/0/nome")`. `r_status` = número de linhas. NULL vira `null` (e `json()` devolve `""`).
- Outros comandos (INSERT, UPDATE, CREATE…): `r_status` (e `r`) = linhas afetadas.
- Uma variável usada como `:variavel` precisa existir no instante do disparo; senão o envio falha e vai para o log.
- Integrações diferentes gravando no mesmo arquivo esperam a vez (até `timeout_ms`), sem "database is locked".

```json
{
  "name": "Cadastro local",
  "integrations": [{
    "name": "Tabela",
    "when": { "OnSet": ["iniciar"] },
    "dest": { "Sqlite": { "path": "dados/clientes.db", "sql": "CREATE TABLE IF NOT EXISTS clientes (id INTEGER PRIMARY KEY, nome TEXT, saldo REAL)", "timeout_ms": 5000 } },
    "payload": { "Json": [] },
    "result": "tabela"
  }, {
    "name": "Insere",
    "when": { "OnSet": ["nome"] },
    "dest": { "Sqlite": { "path": "dados/clientes.db", "sql": "INSERT INTO clientes (nome, saldo) VALUES (:nome, :saldo)", "timeout_ms": 5000 } },
    "payload": { "Json": [] },
    "result": "ins"
  }, {
    "name": "Consulta",
    "when": { "OnSet": ["buscar"] },
    "dest": { "Sqlite": { "path": "dados/clientes.db", "sql": "SELECT id, nome, saldo FROM clientes WHERE nome = :buscar ORDER BY id DESC LIMIT 1", "timeout_ms": 5000 } },
    "payload": { "Json": [] },
    "result": "linhas"
  }],
  "blocks": [
    { "SetVar": { "name": "iniciar", "expr": "1" } },
    { "WaitUntil": { "cond": { "kind": { "Expr": "!tabela_pendente" }, "negate": false }, "timeout_ms": "6000", "on_timeout": "Stop" } },
    { "SetVar": { "name": "saldo", "expr": "10.5" } },
    { "SetVar": { "name": "nome", "expr": "\"O'Brien; DROP TABLE clientes\"" } },
    { "WaitUntil": { "cond": { "kind": { "Expr": "!ins_pendente" }, "negate": false }, "timeout_ms": "6000", "on_timeout": "Stop" } },
    { "SetVar": { "name": "buscar", "expr": "nome" } },
    { "WaitUntil": { "cond": { "kind": { "Expr": "!linhas_pendente" }, "negate": false }, "timeout_ms": "6000", "on_timeout": "Stop" } },
    { "If": {
        "cond": { "kind": { "Expr": "linhas_ok && linhas_status > 0" }, "negate": false },
        "then": [ { "Log": { "text": "id {json(linhas, \"/0/id\")}, saldo {json(linhas, \"/0/saldo\")}" } } ],
        "else_": [ { "Log": { "text": "não achou" } } ] } }
  ]
}
```

**ODBC** (`Odbc`): `{"Odbc": {"connection": "MeuBanco", "sql": "SELECT nome FROM clientes WHERE id = :id", "timeout_ms": 15000}}`.
- Para bancos já instalados no PC (SQL Server, MySQL, PostgreSQL, Access…), pelo gerenciador ODBC do Windows. Precisa do driver ODBC do banco instalado.
- `connection`: o nome de um DSN (configurado em "Fontes de dados ODBC" do Windows) ou uma string de conexão, ex.: `Driver={ODBC Driver 18 for SQL Server};Server=.;Database=loja;Trusted_Connection=yes;TrustServerCertificate=yes`. Vai como está: **não** aceita `{expr}`, porque as chaves fazem parte da sintaxe do ODBC.
- **Senha**: uma senha na `connection` (`PWD=…`) fica em texto puro no JSON do script. Prefira um DSN com autenticação do Windows (`Trusted_Connection=yes`) ou guarde a senha no DSN.
- `sql`, `:variavel`, `timeout_ms` e o resultado seguem o mesmo contrato do **SQLite**: SELECT ➜ `r` = lista JSON de objetos e `r_status` = número de linhas; outros comandos ➜ `r_status` = linhas afetadas. `DECIMAL`/`NUMERIC` viram número quando cabem; datas voltam como texto.
- Falha de conexão (DSN inexistente, driver ausente, senha errada, servidor fora) vai para o log e deixa `r_ok` falso.

```json
{
  "name": "Estoque no SQL Server",
  "integrations": [{
    "name": "Consulta estoque",
    "when": { "OnSet": ["codigo"] },
    "dest": { "Odbc": { "connection": "Driver={ODBC Driver 18 for SQL Server};Server=.;Database=loja;Trusted_Connection=yes;TrustServerCertificate=yes", "sql": "SELECT descricao, quantidade FROM produtos WHERE codigo = :codigo", "timeout_ms": 15000 } },
    "payload": { "Json": [] },
    "result": "produto"
  }],
  "blocks": [
    { "SetVar": { "name": "codigo", "expr": "\"A-100\"" } },
    { "WaitUntil": { "cond": { "kind": { "Expr": "!produto_pendente" }, "negate": false }, "timeout_ms": "20000", "on_timeout": "Stop" } },
    { "If": {
        "cond": { "kind": { "Expr": "produto_ok && produto_status > 0" }, "negate": false },
        "then": [ { "Type": { "text": "{json(produto, \"/0/descricao\")}: {json(produto, \"/0/quantidade\")}" } } ],
        "else_": [ { "Log": { "text": "produto não encontrado ou banco fora do ar" } } ] } }
  ]
}
```

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

## Exemplo no modo Navegador (CDP)

Abre o Chrome numa página de busca, digita, salva com Ctrl+S e espera um pixel verde de confirmação:

```json
{
  "name": "Busca no navegador",
  "target": { "exe": "chrome.exe", "title": "*Exemplo*" },
  "cdp": { "exe": "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", "args": "https://exemplo.com", "port": 0, "webview2": false, "keep_open": false },
  "blocks": [
    { "Click": { "x": "400", "y": "180", "button": "Left", "kind": "Click", "jitter_px": 0 } },
    { "Type": { "text": "relatório {rand(1, 9)}" } },
    { "Key": { "vk": 17, "action": "Down" } },
    { "Key": { "vk": 83, "action": "Press" } },
    { "Key": { "vk": 17, "action": "Up" } },
    { "WaitUntil": {
        "cond": { "kind": { "Pixel": { "x": "40", "y": "160", "color": [30, 160, 60], "tol": 20 } }, "negate": false },
        "timeout_ms": "10000", "on_timeout": "Stop" } }
  ]
}
```

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
