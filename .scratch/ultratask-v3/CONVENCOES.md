# Convenções dos tickets da rodada 3

Vale para todo ticket em `issues/`. Cada ticket é feito por um agente num branch `ticket/NN-slug` criado a partir de `main`, de preferência num worktree próprio.

## Pronto quando
- Todos os critérios do ticket estão marcados com `[x]`, e o arquivo do ticket, no branch, diz `**Status:** done`.
- `cargo test` passa. Se o ticket mexe em input real, captura de tela ou registro, `cargo test -- --include-ignored` também passa.
- `cargo build --release` termina sem erro.
- Se o formato do JSON dos scripts mudou: `skills/ultratask-scripts/SKILL.md` e o README estão atualizados, e cada snippet novo passa em `ultratask.exe --validate`. Rode `cargo build` antes do `--validate`, porque o `cargo test` não reconstrói o exe.
- Textos da UI em pt-BR. Commits no padrão Conventional Commits, em pt-BR.

## Ambiente
- `cargo` e `rustup` ficam em `%USERPROFILE%\.cargo\bin`, que não está no PATH do shell do agente. Use o caminho completo ou acrescente a pasta ao PATH.
- O toolchain é `stable-x86_64-pc-windows-msvc`. O VS Build Tools 2022 tem os toolsets x64 e x86, mas não o ARM64.
- O usuário pode estar com o UltraTask aberto a partir do `target\release` da pasta principal. Cada worktree tem o próprio `target`, então essa trava não afeta o seu build.

## Armadilhas que o código não conta
- **Glifos.** O teste de glifos lê uma lista fixa de arquivos-fonte; inclua nela todo arquivo novo que tenha texto de UI. Use só glifos que existem nas fontes do egui: `⬅⬆➡⬇` funcionam, mas `←↑→↓` e `●` aparecem como quadrado.
- **Expressões.** O evalexpr aceita expressão incompleta (`x >`) ao compilar. Valide com `model::expr_error`, que confere a aridade.
- **JSON dos scripts.**
  - Usa `deny_unknown_fields`.
  - Os enums são externamente marcados, e as variantes sem dados viram string (`"Stop"`).
  - Os campos numéricos dos blocos são strings de expressão.
  - Todo campo novo precisa de valor padrão, para que os scripts antigos continuem válidos.
- **egui 0.37.**
  - O app implementa `App::ui(&mut self, ui, frame)` e `logic(ctx, frame)`. O `logic` roda mesmo com a janela escondida.
  - Os painéis usam `egui::Panel::left/top/bottom(..).show(ui, ..)`.
  - O egui já traz arrastar e soltar (`dnd_drag_source`, `dnd_drop_zone`).
- **winit 0.30.** O event loop não pode ser recriado no mesmo processo.
- **eframe.** Mostra a janela depois do primeiro frame, mesmo com `with_visible(false)`. É por isso que existe o `hide_frames`.
- **Segundo plano.**
  - O atraso de 20 ms entre pressionar e soltar uma tecla mantém WM_KEYDOWN e WM_CHAR em ordem.
  - Apps egui/winit, Chromium e Electron ignoram clique enviado por PostMessage.
- **Features do `windows`.** No `Cargo.toml`, fica uma feature por linha, em ordem alfabética, para que os merges entre tickets não conflitem.
- **Agentes em paralelo.** Outros agentes mexem no mesmo código ao mesmo tempo.
  - Ponha código novo em módulos novos.
  - Nos arquivos compartilhados (modelo do script, interpretador, UI principal), faça edições pequenas e localizadas.
  - Não refatore o que o ticket não pede.

A pesquisa que embasa os tickets 14 a 18 está em `pesquisa-input-paralelo.md`.
