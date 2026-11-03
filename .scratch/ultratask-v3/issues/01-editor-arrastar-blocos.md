# 01: Editor: arrastar blocos para dentro e para fora de containers

**O que construir:** Na aba Blocos, cada bloco vira um cartão com uma alça de arrastar e uma cor por categoria:
- **ação:** Clique, Mover, Tecla, Digitar, Rolar, gravação;
- **espera:** Esperar, Esperar até;
- **controle:** Se, Repetir, Enquanto, Chamar script, Parar;
- **variáveis:** Definir variável, Ler texto;
- **depuração:** Mostrar no log, Breakpoint.

Arrastar um cartão e soltá-lo entre outros cartões, ou numa vaga de container ("então"/"senão" de Se, corpo de Repetir/Enquanto), move o bloco para lá. As vagas aparecem mesmo vazias ("solte blocos aqui"), e um indicador mostra onde o bloco vai cair. Soltar um container dentro dele mesmo ou de um descendente é recusado.

Este ticket só move blocos que já existem; criar blocos arrastando é o ticket 02.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Mover um bloco para frente ou para trás na mesma lista o deixa exatamente onde o indicador mostrou.
- [x] Um bloco pode ir para o corpo vazio de um Repetir, para o "então" e o "senão" de um Se, e voltar ao nível de cima.
- [x] Soltar um container nele mesmo ou num descendente é recusado: o indicador mostra que não pode, e a árvore não muda.
- [x] Mover marca o script como alterado (💾 Salvar), e o JSON salvo guarda o novo aninhamento.
- [ ] 📍, Explodir, duplicar e excluir continuam funcionando em blocos aninhados.
- [x] As cores funcionam nos temas claro e escuro.
- [x] A operação de mover na árvore é uma função pura, fora da UI, com testes unitários para estes casos:
  - mesma lista, para frente e para trás;
  - para dentro de uma vaga filha;
  - para fora, até o pai;
  - recusa de soltar num descendente.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine; veja `../AMBIENTE-LINUX.md`).

**Commits**
- `eb5d77c` feat(editor): arrastar blocos para dentro e para fora de containers

**Arquivos tocados**
- `src/tree.rs` (novo): `Slot`, `Pos`, `slots`/`slot`/`slot_mut`, `get`, `is_inside`, `can_drop`, `insert`, `remove`, `splice`, `duplicate`, `move_block`, `Category` + `category`.
- `src/editor.rs` (novo): cartões coloridos com alça ☰, vagas "solte blocos aqui", indicador de destino, aplicação das operações (`tree_ui`).
- `src/ui.rs`: `blocks_ui` passou a só chamar `editor::tree_ui` (com `block_fields` como callback); saiu o `enum Act`; `editor.rs` e `tree.rs` entraram na lista do teste de glifos.
- `src/main.rs`: `mod editor;` e `mod tree;`.

**Testes adicionados**
- `tree::tests::move_forward_in_same_list`, `move_backward_in_same_list`, `move_to_own_place_is_noop`, `move_into_empty_child_slots`, `move_into_slot_of_later_sibling_after_shift`, `move_out_to_parent`, `refuses_drop_into_itself_or_descendant`, `refuses_missing_spots`, `insert_into_nested_empty_slot`, `duplicate_remove_splice_nested`, `categories`, `nesting_survives_json`.
- `editor::tests::drag_within_same_list_lands_at_indicator`, `drag_into_empty_slots_and_back_out`, `drag_container_into_itself_is_refused`, `card_actions_work_on_nested_blocks`. Os três `drag_*` rodam o `tree_ui` de verdade num `egui::Context` sem janela, com eventos sintéticos de mouse (mover, apertar na alça, arrastar, soltar), e conferem o destino que o indicador mostrava antes de soltar e a árvore depois.

**Como cada critério marcado foi verificado** (`cargo test --target x86_64-pc-windows-gnu`, 34 passam, 2 ignorados que já existiam)
- Frente/trás na mesma lista: `drag_within_same_list_lands_at_indicator` (o `Pos` do indicador antes de soltar é exatamente onde o bloco fica) + `move_forward/backward_in_same_list`.
- Corpo vazio de Repetir, "então"/"senão" de Se e volta ao topo: `drag_into_empty_slots_and_back_out` + `move_into_empty_child_slots`, `move_out_to_parent`.
- Recusa em si mesmo/descendente: `drag_container_into_itself_is_refused` (o alvo sai com `ok = false`, que pinta a linha de vermelho e o texto "não pode soltar dentro dele mesmo"; a árvore fica igual) + `refuses_drop_into_itself_or_descendant`.
- Marca como alterado e JSON guarda o aninhamento: o `App::center` (sem mudança) compara o script com um clone de antes do quadro e liga `dirty` (💾 Salvar) quando difere; os testes `drag_*` mostram que o arrastar altera `blocks` dentro do próprio quadro. `nesting_survives_json` serializa e lê de volta o `Script` depois de um mover.
- Função pura com testes: `tree::move_block` e os testes listados acima cobrem os quatro casos pedidos.

**Decisões de projeto**
- Endereço de bloco/destino: `Pos { list: Vec<(índice, Slot)>, index }`. O destino é lido na árvore *antes* do mover; `move_block` corrige os índices deslocados pela remoção (mesma lista e listas dentro de irmãos posteriores).
- Soltar no próprio lugar ou logo depois dele é no-op (não marca alterado).
- A alça ☰ é o único ponto de arraste, para não brigar com os campos de texto do cartão. Na lista mais interna sob o ponteiro, o índice vem do centro vertical dos cartões; a lista do topo aceita o soltar em toda a área visível da rolagem.
- ⬆/⬇/📋/🗑 continuam no cartão e agora passam pelas mesmas funções de `tree.rs`. O menu "➕ bloco" continuou no fim de cada lista neste commit; o ticket 02 o trocou pela paleta.
- `tree::category` usa `_ => Ação` para blocos que outros tickets acrescentarem; o corpo de Repetir/Enquanto ganhou o rótulo "faça:".
- Glifo da alça: `⠿` não existe nas fontes do egui (o teste de glifos pegou); ficou `☰`.

### Pendente no Windows
- **📍, Explodir, duplicar e excluir em blocos aninhados.** Duplicar/excluir/explodir aninhados estão cobertos por `card_actions_work_on_nested_blocks` e `duplicate_remove_splice_nested`; falta o 📍, que precisa de janela real. Passos: `cargo run --release`; criar script com janela-alvo (ex.: Bloco de Notas); arrastar da paleta um Repetir e, para dentro dele, um Clique (com o ticket 02 no branch, o "➕ bloco" já não existe); clicar no 📍 do Clique aninhado e clicar na janela-alvo: x/y do Clique aninhado mudam. Gravar uma macro curta, arrastar o bloco de gravação para dentro do Repetir, clicar "Explodir em passos": os passos ficam dentro do Repetir. 📋 e 🗑 no Clique aninhado duplicam/excluem só ele.
- **Cores nos temas claro e escuro.** Alternar o tema com o botão ☀/🌙 da barra de cima com um script que tenha um bloco de cada categoria (Clique, Esperar, Se, Definir variável, Mostrar no log): as cinco cores (azul, âmbar, roxo, verde, rosa) se distinguem, o texto dos cartões é legível nos dois temas, e a alça ☰ aparece na cor da categoria.
- **Teste manual do arrastar** (o automático usa eventos sintéticos): arrastar pela ☰ um bloco entre outros, para dentro de um Repetir vazio e de volta; arrastar um Se para o próprio "senão" mostra a linha vermelha e não muda nada; depois de mover, 💾 Salvar fica ativo e o JSON do script, na pasta indicada em ⚙ ("Scripts ficam em …"), mostra o novo aninhamento.
- `cargo build --release` no toolchain MSVC.

### Verificação visual no Windows (2026-11-03)
Feita no exe de release MSVC aberto pelo usuário. A janela foi capturada com PrintWindow e os controles foram acionados pela UI Automation (AccessKit), sem mexer no mouse nem alterar o script aberto.

Cartões com cor por categoria, alça e vagas "então/senão" aninhadas aparecem certos nos dois temas. Falta testar 📍, Explodir, duplicar e excluir num bloco aninhado.
