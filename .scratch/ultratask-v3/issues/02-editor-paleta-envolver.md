# 02: Editor: paleta de blocos, "Envolver em…" e recolher containers

**O que construir:**
- **Paleta.** Ao lado da árvore de blocos, uma paleta lista todos os tipos de bloco, agrupados pelas categorias e cores do ticket 01.
  - Arrastar um item da paleta para qualquer vaga da árvore insere ali um bloco novo.
  - Clicar num item o acrescenta no fim.
  - O antigo menu "➕ bloco" deixa de existir.
- **Envolver.** O menu de contexto (botão direito) de um cartão oferece "Envolver em Se / Repetir / Enquanto", que põe o bloco dentro de um container novo, no mesmo lugar.
- **Recolher.** Containers podem ser recolhidos e expandidos. Recolhidos, mostram quantos blocos têm dentro.

**Bloqueado por:** 01 (Editor: arrastar blocos para dentro e para fora de containers).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Todos os tipos de bloco aparecem na paleta, por categoria.
- [x] Arrastar da paleta para uma vaga aninhada vazia insere o bloco novo nela.
- [x] "Envolver em…" mantém a posição do bloco e o põe no corpo (ou no "então") do container novo. A operação tem teste unitário.
- [x] Recolher e expandir só mudam a tela; o JSON do script não muda.
- [x] O teste de glifos passa.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine; veja `../AMBIENTE-LINUX.md`), em cima do ticket 01 no mesmo branch.

**Commits**
- `3e38132` feat(editor): paleta de blocos, "Envolver em…" e recolher containers

**Arquivos tocados**
- `src/tree.rs`: `wrap`, `count_inside`, `Category::ALL`.
- `src/editor.rs`: `palette_ui`; `Drag::New`; menu de contexto do cartão (Envolver em…, Duplicar, Excluir); botão ⏷/⏵ e contagem "(N blocos dentro)"; o menu "➕ bloco" saiu.
- `src/ui.rs`: aba Blocos ganhou `Panel::left("palette")` com a paleta; o id da árvore passou a incluir o nome do script (o estado de recolhido não vaza entre scripts).
- `src/model.rs`: só o comentário de `Block::templates` (agora alimenta a paleta).
- `README.md`: "Uso rápido" descreve paleta, alça ☰, "Envolver em…" e recolher.

**Testes adicionados**
- `tree::tests::wrap_keeps_position`, `tree::tests::counts_blocks_inside`.
- `editor::tests::palette_lists_every_block_type_by_category`, `palette_click_appends_and_drag_inserts_into_nested_empty_slot`, `collapse_only_changes_the_view`, `wrap_op_on_nested_block`, `context_menu_wraps_in_place` (egui sem janela, com eventos sintéticos de mouse, como no ticket 01).

**Como cada critério foi verificado** (`cargo test --target x86_64-pc-windows-gnu`: 41 passam, 2 ignorados que já existiam)
- Todos os tipos na paleta, por categoria: `palette_lists_every_block_type_by_category` (um item por `Block::templates()`, agrupados na ordem de `Category::ALL`). Como a paleta lê `templates()` e `tree::category`, blocos novos de outros tickets aparecem sozinhos (na categoria Ação, se ninguém os classificar).
- Arrastar da paleta para vaga aninhada vazia: `palette_click_appends_and_drag_inserts_into_nested_empty_slot` arrasta "Mostrar no log" para o corpo vazio de um Repetir dentro do "então" de um Se; o mesmo teste confere o clique, que põe no fim.
- "Envolver em…": `tree::wrap_keeps_position` (o container ocupa o lugar do bloco; o bloco vai para o "então" do Se ou o corpo de Repetir/Enquanto), `wrap_op_on_nested_block`, e `context_menu_wraps_in_place` (botão direito no cartão abre o menu com "Envolver em Se/Repetir/Enquanto"; clicar em Repetir envolve no lugar).
- Recolher só muda a tela: `collapse_only_changes_the_view` clica ⏷ e ⏵ e confere que a vaga some e volta enquanto o JSON dos blocos fica byte a byte igual. O estado fica em `ui.data` (memória temporária do egui).
- Teste de glifos: `ui::tests::ui_glyphs_exist` passa com `editor.rs` e `tree.rs` na lista. Os glifos novos (`☰`, `⏷`, `⏵`) foram escolhidos testando candidatos nas fontes do egui.

**Decisões de projeto**
- O menu de contexto fica no fundo do cartão (`UiBuilder::sense(click)`), que o egui registra abaixo dos widgets internos; por isso o título do cartão não é selecionável. Botão direito em cima de um campo de texto abre o menu do campo, não o do cartão.
- "Envolver em…" lista os blocos de `templates()` que têm vagas, então um container novo de outro ticket entra no menu sem mudar este código.
- Um container recolhido não mostra as vagas, então também não aceita soltar dentro; soltar antes/depois dele continua valendo.
- O estado de recolhido segue a posição na árvore (id do cartão), não o bloco: ao mover um container recolhido, ele aparece expandido no novo lugar.

### Pendente no Windows
Todos os critérios foram verificados aqui; falta só o que o container não prova:
- `cargo build --release` com o toolchain MSVC.
- Olhar a tela (`cargo run --release`): a paleta aparece à esquerda da árvore, com os cinco grupos (Ação, Espera, Controle, Variáveis, Depuração) nas cores dos cartões, nos temas claro e escuro (botão ☀/🌙); arrastar "Esperar" da paleta para o corpo vazio de um Repetir mostra a linha indicadora e o bloco cai ali; botão direito num cartão → "Envolver em Enquanto" põe o bloco no corpo do Enquanto no mesmo lugar; ⏷ recolhe um Repetir e mostra "(N blocos dentro)", e o 💾 Salvar não acende só por recolher.

### Verificação no Windows (2026-11-03)
Em 2026-11-03, numa máquina Windows 11 com o toolchain `stable-x86_64-pc-windows-msvc` (branch `main-vqbyri` @ 1903fe5):
- `cargo test`: 166 ok, 0 falhas, 11 ignorados;
- `cargo build --release`: ok (SQLite `bundled` compilado com `cl.exe`); exe de 15,9 MB que só importa DLLs do sistema;
- `cargo test -- --ignored --skip cdp_real_browser --skip real_database_round_trip`: 9 ok, 0 falhas.

O build de release com MSVC passou. Falta conferir a aparência na tela.
