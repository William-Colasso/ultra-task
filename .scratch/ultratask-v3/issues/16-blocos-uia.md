# 16: Blocos UI Automation: agir em elementos sem mouse nem teclado (tracer)

**O que construir:** Um novo bloco, "Elemento (UIA)", age direto num controle da janela-alvo pela UI Automation do Windows, sem input nenhum. Por isso roda em paralelo de verdade e com a janela coberta. Vale inclusive para apps que ignoram o Segundo plano: Chromium 126 ou mais novo, Electron com acessibilidade ligada, egui via AccessKit, WPF, WinForms, UWP e Qt.

- **Captura.** O elemento é capturado com 📍: clicar no controle grava a identificação dele (AutomationId, Name, ControlType, ClassName) e o ponto clicado.
- **Ações:**
  - acionar (Invoke);
  - alternar (Toggle);
  - definir valor (aceita `{…}`);
  - selecionar;
  - expandir/recolher;
  - ler o valor ou o nome para uma variável.
- **Elemento não encontrado.** Se o elemento não aparecer dentro de um timeout, o bloco para com erro. Uma opção do bloco troca isso por clicar no ponto capturado, como o Clique faz hoje, com aviso no log.
- **Condição.** Uma nova condição, "elemento", testa se ele existe, se o nome ou o valor contém um texto, e se está marcado.

Restrições:
- **Threads.** Chamadas UIA nunca rodam na thread da UI do UltraTask nem na thread dos hooks, porque um hook lento trava o input do sistema inteiro.
- **Timeout.** Invoke pode bloquear em alguns apps, então use timeout.
- **Apps elevados.** Elementos de apps elevados exigem o UltraTask elevado; o banner atual já cobre esse caso.
- **Biblioteca.** O crate `uiautomation` 0.25 usa a mesma versão do `windows` que o projeto. A alternativa é usar o COM direto.

Detalhes na pesquisa: `../pesquisa-input-paralelo.md`, R1.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Três alvos, cobertos por outras janelas, são acionados enquanto o usuário digita em outro app:
  - o Bloco de Notas ou um app WinForms;
  - um app WPF;
  - um Electron com `--force-renderer-accessibility`, ou o próprio UltraTask.

  O efeito é conferido lendo o valor pela UIA. O cursor e a janela ativa não mudam.
- [ ] Elemento não encontrado gera erro claro no log, ou o clique de reserva quando a opção estiver ligada.
- [x] A escolha do elemento a partir da identificação gravada é lógica pura, com testes unitários sobre uma árvore falsa.
- [x] O validador, a skill (com snippets validados) e o README cobrem o bloco e a condição.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu` + Wine; veja `../AMBIENTE-LINUX.md`). O Wine não tem UI Automation de verdade, então o que depende dela fica para a máquina Windows.

### Commits
- `e19e993` feat: bloco e condição "Elemento (UIA)" pela UI Automation
- o commit seguinte, `docs(ticket 16): …`, só atualiza este arquivo.

### Arquivos tocados
- `src/uia.rs` (novo): tipos do JSON (`UiaElem`, `UiaId`, `UiaAction`, `UiaTest`), escolha do elemento (`matches`, `choose`), cadeia do clique (`pick_from_chain`, `suggest_action`, `captured`), `Query`/`Op`/`Reply`, `FakeTree` (só em testes), `Worker<C>` (thread própria com timeout por chamada) e a parte COM (`Ctx`, `query`, `capture`, `capture_async`).
- `src/runner/uia_block.rs` (novo, submódulo do runner): execução do bloco (espera, reserva, avisos) e da condição.
- `src/model.rs`: `Block::Uia`, `CondKind::Uia`, título, resumo, modelo no menu "+ bloco", validação.
- `src/runner.rs`: `Io::uia` com implementação padrão ("UI Automation indisponível"), braços no `run_block`/`cond`, campo `WinIo.uia` (worker) e `WinIo::uia` (sem `gate()`, porque não há input).
- `src/ui.rs`: `Pick::Uia`, `PickData::Uia` (o `PickData` deixou de ser `Copy`), campo `uia_pick` do `App` (resultado do 📍 vindo de outra thread), editor do bloco e da condição, lista de arquivos do teste de glifos.
- `src/main.rs`: `mod uia;`.
- `Cargo.toml`: features `Win32_System_Com` e `Win32_UI_Accessibility` (uma por linha, em ordem alfabética).
- `skills/ultratask-scripts/SKILL.md`, `README.md`.

### Testes adicionados
- `uia::tests::choose_by_identification`: árvore falsa com dois botões "OK" em painéis diferentes; AutomationId vence nome mudado; glob no nome; ControlType errado; identificação vazia; desempate por ancestrais, depois ponto, depois ordem do documento; busca só por ClassName.
- `uia::tests::chain_and_suggestion`: clique no TextBlock de dentro de um botão WPF sobe para o botão; Edit fica como está; ação sugerida por ControlType e padrões; nomes de ControlType.
- `uia::tests::fake_tree_ops`, `uia::tests::serde_defaults_and_roundtrip`, `uia::tests::validation_and_description`.
- `uia::tests::worker_times_out_and_recovers`: chamada travada (1,5 s) estoura o timeout de 100 ms e a próxima chamada usa uma thread nova; erro de inicialização é repassado.
- `runner::uia_block::tests::actions_reads_and_conditions`: SetValue com `{n}`, Toggle, ReadValue, ReadName, Invoke e as condições Checked/ValueContains/Exists com Io falso; nenhum input é enviado.
- `runner::uia_block::tests::waits_for_the_element_then_errors_or_falls_back`: elemento que aparece em 600 ms é achado; se nunca aparece, `elemento Button #btnSalvar não encontrado em 1000 ms`; com reserva, aviso no log e Move/Down/Up no ponto (7, 8).
- `runner::uia_block::tests::hung_app_and_missing_uia`: Invoke que trava só avisa; leitura que trava é erro; Io sem UIA dá erro no bloco e falso (com aviso) na condição.
- `model::tests::uia_validation`: padrões do JSON e caminhos de erro do validador.
- `uia::tests::real_uia_on_own_window` (`#[ignore]`): UIA real numa janela Win32 própria (checkbox + edit), inclusive a captura por ponto.

### Como cada critério marcado foi verificado
- **Escolha do elemento é lógica pura, testada sobre árvore falsa:** `choose`/`matches`/`pick_from_chain` não chamam COM. Testes acima; `cargo test --target x86_64-pc-windows-gnu` no Wine: 28 passaram, 3 ignorados.
- **Validador, skill e README:** `model::tests::uia_validation`; na skill, a seção *Elementos (UIA)*, a condição `Uia` e o exemplo "Pedido via UIA"; no README, a seção *Elementos (UI Automation)*. Depois de `cargo build --target x86_64-pc-windows-gnu`, os exemplos completos da skill foram extraídos para arquivos e passaram em `wine target/x86_64-pc-windows-gnu/debug/ultratask.exe --validate "Pedido via UIA.json" "Notas numeradas.json"` (OK nos dois, código 0).

### Decisões de projeto
- **COM direto pelo crate `windows` 0.62**, sem o crate `uiautomation`: só duas features novas e nenhuma dependência nova.
- **Uma ida e volta por busca:** `BuildUpdatedCache` com `TreeScope_Subtree` traz a árvore inteira (ControlView) com AutomationId, Name, ControlType, ClassName e retângulo. O achatamento para em 20 000 elementos.
- **Regras de escolha:** `automation_id` e `control_type` precisam ser iguais quando gravados; `name` (glob, sem diferenciar maiúsculas) só precisa casar quando não há `automation_id`; `class_name` só é obrigatório quando é o que sobra. Desempate: nome/classe/ancestrais, depois conter o ponto capturado, depois distância ao ponto, depois ordem do documento.
- **Threads e timeout:** cada execução tem um `Worker` (thread MTA) criado no primeiro uso, e cada chamada tem 3 s (`CALL_TIMEOUT`). Se estourar, a thread é abandonada e a próxima chamada cria outra. O 📍 usa outra thread (`capture_async`) e entrega o resultado por canal ao `logic` da UI.
- **Timeout numa ação** (Invoke que abre uma janela modal) vira aviso e o script segue, porque a ação foi entregue. Em leitura, é erro.
- **Reserva só para "não encontrado"**, como pede o ticket. Padrão ausente ou UIA indisponível continuam sendo erro.
- **A condição faz uma busca só**, sem esperar; para esperar, usa-se `WaitUntil`, que já repete a cada 100 ms.
- **O ponto capturado** (`x`, `y`) fica dentro de `elem`, em inteiros como no `Region`, porque serve ao bloco (reserva) e à condição (desempate).
- **`Io::uia` tem implementação padrão**, para que os `Io` falsos de outros tickets continuem compilando.

### Pendente no Windows
Pré-requisito: `cargo build --release` (MSVC) e `cargo test -- --include-ignored` na pasta do worktree.
1. **Teste real:** `cargo test real_uia_on_own_window -- --ignored` passa (Toggle e SetValue lidos de volta pela UIA; a captura do checkbox sugere `Toggle`).
2. **Três alvos cobertos, com o usuário digitando em outro app:**
   1. Abra (a) o Bloco de Notas ou um app WinForms, (b) um app WPF com campo de texto e botão ou caixa de seleção, e (c) um Electron iniciado com `--force-renderer-accessibility` (ou o próprio UltraTask, rodando de outra pasta).
   2. Para cada um, crie um script `Background` com alvo nessa janela. Adicione **Elemento (UIA)** capturado com 📍 num campo de texto, ação *definir valor* `teste {1+1}`; outro num botão ou caixa (acionar/alternar); outro com *ler valor para* `v` no campo; e **Mostrar no log** `{v}`.
   3. Deixe rodando um PowerShell que imprime a posição do cursor e o título da janela ativa a cada 200 ms.
   4. Cubra as três janelas com outra (um navegador maximizado), comece a digitar nela e dispare os três scripts pelos atalhos.
   5. Esperado: o log mostra `teste 2` nos três; o que você digita continua indo só para a janela da frente; a posição do cursor e a janela ativa no PowerShell não mudam por causa dos scripts.
3. **Elemento não encontrado:** troque o AutomationId/nome de um bloco por algo inexistente, timeout `1000`. Sem a opção, o log mostra `erro: elemento … não encontrado em 1000 ms`. Com **se não achar, clicar no ponto**, aparece `aviso: elemento … não encontrado; clicando no ponto capturado (x, y)` e o clique acontece nesse ponto. (A lógica já está coberta por `waits_for_the_element_then_errors_or_falls_back`; falta ver com a UIA real.)
4. **Aparência:** confira o editor do bloco e da condição (popup da identificação, combo de ações e de testes) nos temas claro e escuro.
