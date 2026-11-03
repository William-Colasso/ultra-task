# 03: Cola de comandos nos campos editáveis e Manual no app

**O que construir:**
- **Catálogo.** Um catálogo único descreve cada função de expressão, com assinatura, descrição curta e exemplo:
  - as do UltraTask: `rand`, `contem`;
  - as nativas que a documentação promete: `min`, `max`, `floor`, `round`, `ceil`, `len`, `if`, `str::…` e `math::…`.

  O catálogo também cobre os operadores e a sintaxe `{expressão}` dos textos.
- **Cola.** Ela aparece junto ao campo quando:
  - um campo de expressão está em foco (Definir variável, x/y, tempos, condição por expressão);
  - o cursor está dentro de `{…}` num texto (Digitar, Mostrar no log).

  A cola lista as variáveis do script e as funções que casam com a palavra sendo digitada, com a descrição de cada uma. Setas e Enter, ou um clique, inserem o item no cursor. Se a expressão estiver inválida, a cola mostra o erro.
- **Manual.** Um botão "📖 Manual" na barra, e um "?" junto aos campos, abrem uma janela com as seções Expressões, Blocos, Condições, Teclas e Atalhos.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Digitar `ra` num campo de expressão sugere `rand(a, b)` com a descrição, e escolher a sugestão insere `rand(` no cursor.
- [x] A cola lista as variáveis definidas em qualquer nível do script: Definir variável, Ler texto e, se o ticket 09 já estiver na `main`, as variáveis de resultado das integrações.
- [x] Dentro de `{…}`, em Digitar e Mostrar no log, a cola funciona igual. Fora das chaves, ela não aparece.
- [x] O Manual abre pela barra e pelo "?". O "?" leva direto à seção Expressões.
- [x] Um teste roda o exemplo de cada função do catálogo num interpretador novo e falha se algum der erro.
- [x] Um teste falha se uma função registrada pelo UltraTask faltar no catálogo.
- [x] Um teste falha se a skill de scripts não citar alguma função do catálogo.
- [x] O teste de glifos passa.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine), conforme `AMBIENTE-LINUX.md`.

### Commits
- `aedb358` feat: cola de comandos nos campos de expressão e janela Manual
- o commit seguinte, `docs: rastreabilidade do ticket 03`, traz este registro.

### Arquivos tocados
- `src/catalog.rs` (novo): `FUNCTIONS`, `OPERATORS`, `TEXT_SYNTAX`; lógica pura da cola: `brace_span`, `query`, `suggestions`, `apply`, `script_vars`.
- `src/cola.rs` (novo): widget `cola::field` (TextEdit + lista flutuante; ⬆ ⬇ Enter ou clique; erro da expressão; "?" para o Manual).
- `src/manual.rs` (novo): `Section`, `Manual { open, section }`, `entries` (conteúdo puro) e `window`.
- `src/runner.rs`: o registro de `rand`/`contem` saiu de `Interp::new` para `pub fn custom_functions()` + `pub fn new_context()`, sem mudar o comportamento.
- `src/ui.rs`:
  - campo `App::manual` e botão "📖 Manual" na barra;
  - `EditCtx` ganhou `vars` e `manual`;
  - `expr_edit` passou a receber `id` e `ec` e usa a cola;
  - Digitar e Mostrar no log usam `text_edit`, com a cola dentro de `{…}`;
  - "?" (`help_btn`) junto a Digitar, Mostrar no log, Definir variável e condição por expressão;
  - a lista do teste de glifos inclui `catalog.rs`, `cola.rs` e `manual.rs`.
- `src/main.rs`: `mod catalog; mod cola; mod manual;`.

### Testes adicionados
- `catalog::tests::every_example_runs`: roda o exemplo de cada função, e de cada operador, num `runner::new_context()` novo. Também confere cada exemplo com `model::expr_error`.
- `catalog::tests::registered_functions_are_cataloged`: toda função de `runner::custom_functions()` está no catálogo com `ours: true`, e vice-versa.
- `catalog::tests::skill_mentions_every_function`: `skills/ultratask-scripts/SKILL.md` cita `` `nome `` de cada função.
- `catalog::tests::{suggests_rand_and_inserts, filter_rules, braces, query_reports_errors, vars_from_every_level_and_calls}`.
- `cola::tests::{enter_inserts_suggestion, text_fields_only_inside_braces}`: dirigem o widget num `egui::Context` sem janela (foco, `Event::Text`, ⬇, Enter).
- `manual::tests::{sections_have_content, ask_opens_on_section}`.
- `ui::tests::help_button_opens_expressions`: um clique simulado no "?" abre o Manual em Expressões.

### Como cada critério marcado foi verificado
`CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo test --target x86_64-pc-windows-gnu`: 31 testes passaram e 2 foram ignorados (os `#[ignore]` que já existiam). `cargo build --target x86_64-pc-windows-gnu` terminou sem avisos.
- **`ra` sugere `rand(a, b)` com a descrição:** `suggests_rand_and_inserts`. A inserção de `rand(` no cursor pelo teclado, no widget real, é coberta por `enter_inserts_suggestion`, que também continua digitando depois do `(`.
- **Variáveis em qualquer nível:** `vars_from_every_level_and_calls` cobre If/Repeat/While aninhados, Definir variável, Ler texto e os scripts chamados por Chamar script. O ticket 09 não está na base; o ponto de extensão está marcado em `catalog::block_vars`.
- **Dentro e fora de `{…}`:** `braces` testa a lógica, e `text_fields_only_inside_braces` testa o widget com `FieldKind::Text`, o mesmo de Digitar e Mostrar no log.
- **Catálogo, skill e glifos:** os testes de nomes acima e `ui::tests::ui_glyphs_exist`.

### Decisões de projeto
- Toda a lógica da cola é pura, em `catalog.rs`. `cola.rs` só desenha e trata teclado e ponteiro. Os índices são em caracteres, como o cursor do egui.
- **Inserção.** Uma função insere `nome(` e uma variável insere o nome. A cola troca a palavra antes do cursor (letras, dígitos, `_`, `::`).
- **Filtro.**
  - `trim` também acha `str::trim`.
  - Palavra vazia lista tudo.
  - Um número, ou o cursor dentro de aspas, não sugere nada.
  - Uma variável já digitada inteira some da lista.
- **Teclado e mouse.** ⬆ ⬇ Enter são consumidos antes do TextEdit, e só quando a cola tem sugestões. A cola continua aberta enquanto o ponteiro está sobre ela; assim, o clique numa sugestão funciona mesmo que o campo perca o foco ao apertar o botão. Depois do clique, o foco volta ao campo.
- `script_vars` também segue Chamar script, porque as variáveis são globais na execução. Há proteção contra recursão.
- No evalexpr, `len` e `str::substring` contam bytes; o catálogo avisa.
- O botão da barra alterna o Manual na última seção vista. O "?" abre sempre em Expressões.
- `manual::block_help` tem um braço `_` (com `#[allow(unreachable_patterns)]`), para que variantes de `Block` vindas de outros tickets compilem no merge. O teste `sections_have_content` acusa bloco sem texto.
- O formato do JSON não mudou. SKILL.md e README não precisaram de alteração: a skill já cita todas as funções do catálogo.

### Complemento na integração com o ticket 09
- No merge do 09 (`merge: ticket 09`), `catalog::script_vars` passou a listar as variáveis de resultado das integrações (`r`, `r_ok`, `r_status`, `r_pendente`); teste `catalog::tests::integration_result_vars_are_listed`.
- `json()` foi para `runner::custom_functions()` e para `catalog::FUNCTIONS` (antes era registrada à parte em `integrations::register_functions`, que saiu).

### Pendente no Windows
1. `cargo build --release` (MSVC) e `cargo test` no Windows.
2. Manual pela barra e pelo "?" (critério 4), e aparência:
   1. `cargo run`; crie um script e abra a aba Blocos.
   2. Clique em "📖 Manual" na barra. A janela "📖 Manual" abre com as abas Expressões, Blocos, Condições, Teclas e Atalhos. Troque para Teclas e feche. Clique de novo: ela reabre em Teclas.
   3. Adicione "Definir variável" e clique no "?" à direita do campo: o Manual abre em Expressões. Repita com o "?" de Digitar, de Mostrar no log e de uma condição "expressão" num "Se".
   4. Confira que nenhum texto aparece como quadrado e que tudo fica legível nos temas claro e escuro.
3. Cola com mouse, e visual:
   1. No campo de expressão de "Definir variável", digite `ra`. A lista aparece logo abaixo do campo, com `rand(a, b)` e a descrição. Clique na sugestão: o campo fica `rand(`, com o cursor depois do `(`, e o foco continua no campo.
   2. Digite `x >` num campo x: a cola mostra `⚠` e o erro em vermelho.
   3. Em "Digitar", digite `oi {con`: a lista mostra as variáveis e `contem(texto, trecho)`. Mova o cursor para antes do `{`: a lista some.
   4. Defina duas variáveis dentro de um "Repetir" e confira que ambas aparecem na cola de um campo de outro bloco.

### Verificação visual no Windows (2026-11-03)
Feita no exe de release MSVC aberto pelo usuário. A janela foi capturada com PrintWindow e os controles foram acionados pela UI Automation (AccessKit), sem mexer no mouse nem alterar o script aberto.

O botão 📖 Manual da barra abre a janela com as seções Expressões, Blocos, Condições, Modos, Teclas e Atalhos. O "?" de um campo abre o Manual já em Expressões. Falta escolher uma sugestão da cola com o mouse.
