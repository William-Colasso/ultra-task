# 10: Integração: arquivo de texto

**O que construir:** Um novo destino de integração, Arquivo. O caminho aceita `{…}`, e um caminho relativo parte da pasta do exe. Pastas que faltam são criadas. Há três modos:
- **Acrescentar** e **sobrescrever** gravam no formato de envio da integração:
  - JSON: uma linha por envio;
  - CSV: uma linha de cabeçalho quando o arquivo é novo;
  - texto: gravado como está.
- **Ler** põe o conteúdo do arquivo na variável de resultado.

**Bloqueado por:** 09 (Integrações: faixa por eventos e REST bidirecional).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Testes numa pasta temporária cobrem:
  - acrescentar;
  - sobrescrever;
  - cabeçalho CSV gravado uma vez só;
  - ler o arquivo para uma variável.
- [x] Erro de permissão ou caminho inválido vai para o log e deixa `r_ok` falso.
- [x] O validador, a skill (com snippet validado) e o README cobrem o destino.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine), conforme `AMBIENTE-LINUX.md`. Os três critérios foram verificados aqui; o status fica `needs-local-verification` só pela aparência da aba e pelo teste ignorado de somente leitura (passos abaixo).

**Commits**
- `f8627fc` feat: destino de integração Arquivo (acrescentar, sobrescrever, ler)
- (este registro) docs: rastreabilidade do ticket 10

**Arquivos tocados**
- Novo: `src/integrations/file.rs` (destino `File`, `FileMode`, `resolve`, `io_error`, testes).
- Compartilhados (edições pequenas):
  - `src/integrations/mod.rs`: `pub mod file;`, variante `Dest::File`, braço em `Dest::get`, campo `Body.header` (linha de cabeçalho do CSV, preenchida só por `Payload::Csv`).
  - `src/integrations/editor.rs`: entrada "Arquivo" em `DESTS`, braço em `dest_ui` e `file_ui`.
  - `src/integrations/tests.rs`: os ajudantes do e2e (`ctx`, `set`, `ty`, `expr`, `wait_until`, `Run`, `run`, `var`, `ClockIo`) viraram `pub(super)` para os testes dos destinos novos; um `let Dest::Http(h) = …` ganhou `else { panic!() }`.
  - `src/ui.rs`: `integrations/file.rs` na lista do teste de glifos.
  - `skills/ultratask-scripts/SKILL.md` (subseção *Outros destinos* com o Arquivo e snippet) e `README.md` (lista de destinos).

**Testes adicionados** (`src/integrations/file.rs`, módulo `tests`)
- `relative_path_starts_at_exe_folder`, `json_contract_and_validation`, `append_overwrite_csv_header_and_read`, `csv_header_only_once`, `errors_go_to_log_and_r_ok_false`;
- `#[ignore]`: `read_only_file_is_permission_error` (o Wine roda como root e ignora o atributo somente leitura).

**Como cada critério marcado foi verificado**
- *Testes em pasta temporária* (`std::env::temp_dir()`, no Wine `C:\users\root\Temp`): `append_overwrite_csv_header_and_read` roda pelo fluxo (`Interp` + engine) cinco integrações disparadas por `SetVar n` três vezes: JSON acrescentado vira três linhas (`{"n":1,"nome":"Zé, o \"bom\""}` …), numa subpasta `sub/pasta` que não existia; CSV acrescentado tem `n,nome` uma vez só e três linhas com aspas escapadas; texto acrescentado dá `[1][2][3]`; sobrescrever termina com `n=3`; e o modo Ler põe o CSV inteiro em `conteudo`, com `conteudo_ok = true` e `conteudo_status` = bytes. `csv_header_only_once` confere que um CSV que já tem conteúdo não ganha outro cabeçalho e que sobrescrever grava cabeçalho + linha.
- *Erros*: `errors_go_to_log_and_r_ok_false` usa caminho inválido (`C:\nome<invalido>|?.txt` ➜ log `caminho inválido`), leitura de arquivo inexistente (➜ `não encontrado`) e gravar sobre uma pasta (➜ `sem permissão`, que é o `ERROR_ACCESS_DENIED` do Windows). As três deixam `r_ok = false` e `r_status = 0`, e as três mensagens vão para o log da execução com o nome da integração. Rodou 3 vezes seguidas sem falha.
- *Validador/skill/README*: `json_contract_and_validation` (contrato, padrões, `deny_unknown_fields`, modo desconhecido, `path` vazio e `{…}` inválido). No exe, depois de `cargo build --target x86_64-pc-windows-gnu`: o snippet da skill passou em `wine ultratask.exe --validate "Planilha de pedidos.json"` (`OK`, saída 0); um arquivo com `"path": "{x +}"` deu `ERRO … integrations[0].dest.File.path: {x +}: operador sem operando` e um com `"mode": "Apagar"` deu `ERRO … unknown variant` (saída 1).
- `cargo test --target x86_64-pc-windows-gnu`: 85 passaram, 4 ignorados, sem warnings.

**Decisões de projeto**
- Contrato: `{"File": {"path": "…", "mode": "Append" | "Overwrite" | "Read"}}`, padrão `{"path": "saida.txt", "mode": "Append"}`.
- O cabeçalho do CSV vem num campo novo `Body.header`, para não reparsear o texto; é gravado quando o arquivo está vazio ou não existe.
- Linhas de JSON e CSV terminam em CRLF (padrão do Windows e do RFC 4180); texto não ganha quebra de linha.
- `r_status` = bytes gravados ou lidos; `r` = conteúdo lido (o BOM UTF-8 é removido; bytes inválidos viram `�`). O destino não tem `timeout_ms` no JSON; para a espera do "ao terminar" vale 10 s.
- As mensagens de erro são em pt-BR para não encontrado, sem permissão e caminho inválido (códigos 123/161); as outras usam a mensagem do sistema.

### Pendente no Windows
Na máquina Windows (toolchain `stable-x86_64-pc-windows-msvc`), no branch deste ticket:
1. `cargo test` e `cargo test -- --include-ignored`: `integrations::file::tests::read_only_file_is_permission_error` deve passar (gravar num arquivo com atributo somente leitura dá `sem permissão`, `r_ok` falso e o conteúdo fica intacto). `cargo build --release` sem erro.
2. Aba: `target\release\ultratask.exe` ➜ ➕ Novo ➜ **Integrações** ➜ **➕ Nova integração** ➜ Destino **Arquivo**. Confira o combo Acrescentar/Sobrescrever/Ler, o campo do caminho (fica vermelho com `{x +}`) e a dica embaixo; sem quadrados no lugar de glifos. Copie o snippet "Planilha de pedidos" da skill para `target\release\scripts\Planilha de pedidos.json`, 🔄, ▶ Tocar duas vezes: `target\release\relatorios\pedidos.csv` deve ter `pedido,cliente,valor` uma vez e duas linhas `1001,Ana,19.9`, e o painel **Execuções** deve mostrar `planilha` com esse conteúdo e `planilha_ok = true`.
