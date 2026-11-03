# 11: Integração: banco SQLite embutido

**O que construir:** Um novo destino de integração, SQLite. Ele aponta para um arquivo .db, criado se faltar, e roda um SQL com parâmetros `:variavel`, ligados aos valores das variáveis com os tipos preservados. O SQL nunca recebe interpolação de texto, para não haver injeção.

O resultado depende do comando:
- **SELECT:** devolve as linhas como JSON (uma lista de objetos) na variável de resultado. Exemplo: `json(r, "/0/nome")`.
- **Outros comandos:** põem em `r_status` o número de linhas afetadas.

O SQLite vai embutido no exe, sem DLL extra, e precisa compilar nos alvos dos tickets 06 e 07.

**Bloqueado por:** 09 (Integrações: faixa por eventos e REST bidirecional).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Testes com um .db temporário cobrem:
  - inserir valores de variáveis;
  - ler de volta para uma variável;
  - extrair um campo com `json()`;
  - um valor com aspas e `;`, que não quebra nem injeta SQL.
- [x] Gravações no mesmo arquivo vindas de integrações diferentes não dão "database is locked".
- [x] O validador, a skill (com snippet validado) e o README cobrem o destino.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine), conforme `AMBIENTE-LINUX.md`. Os três critérios foram verificados aqui; o status fica `needs-local-verification` porque o build MSVC (x64, x86 e ARM64) com o C do SQLite só dá para provar no Windows.

**Commits**
- `563429e` feat: destino de integração SQLite embutido
- (este registro) docs: rastreabilidade do ticket 11

**Arquivos tocados**
- Novos: `src/integrations/sql.rs` (comum aos destinos SQL: `Cell`, `Cell::from_value`, `rows_to_json`, `param_names`, `bind`, `validate_sql`) e `src/integrations/sqlite.rs` (destino `Sqlite`, `exec`, trava por arquivo, testes).
- Compartilhados (edições pequenas):
  - `Cargo.toml`/`Cargo.lock`: `rusqlite = { version = "0.41.1", features = ["bundled"] }` (traz `libsqlite3-sys` 0.39.1 com o SQLite em C; `sqlite-wasm-rs`/`rsqlite-vfs` entram no lock mas só valem para wasm).
  - `src/integrations/mod.rs`: `pub mod sql; pub mod sqlite;`, variante `Dest::Sqlite`, braço em `Dest::get`; a assinatura de `Destination::prepare` passou de `(body, eval)` para `(body, vars: &HashMapContext)`, porque os destinos SQL precisam dos valores tipados. Os outros destinos (`http.rs`, `file.rs`, `command.rs`, `clipboard.rs`) montam o mesmo `eval` com `eval_text(vars, …)` na primeira linha; `engine.rs` passa `vars`.
  - `src/integrations/editor.rs`: entrada "SQLite" em `DESTS`, braço em `dest_ui`, `sql_edit` (campo do SQL, reaproveitado pelo ODBC) e `sqlite_ui`.
  - `src/integrations/tests.rs`: `ClockIo::texts` virou `pub(super)`.
  - `src/ui.rs`: `integrations/sql.rs` e `integrations/sqlite.rs` na lista do teste de glifos.
  - `skills/ultratask-scripts/SKILL.md` (SQLite em *Outros destinos*, com snippet) e `README.md` (lista de destinos).

**Testes adicionados**
- `src/integrations/sql.rs`: `params_skip_strings_comments_and_casts`, `bind_keeps_types_and_never_interpolates`, `rows_json_keeps_column_order_and_types`, `validate`.
- `src/integrations/sqlite.rs`: `json_contract_and_validation`, `insert_select_json_and_no_injection`, `concurrent_writers_do_not_lock`, `waits_for_other_connection`, `errors_go_to_log`.

**Como cada critério marcado foi verificado**
- *Testes com .db temporário* (`insert_select_json_and_no_injection`, pelo fluxo, `.db` numa subpasta nova de `%TEMP%`): cria a tabela; insere `nome`/`saldo`/`vip` das variáveis (`"Ana"`, `10.5`, `true`); insere `O'Brien "Zé"; DROP TABLE clientes; --` com `-3.25` e `false`; lê de volta com `WHERE nome = :nome OR id > :ler` para `r`, que vale exatamente `[{"id":1,"nome":"Ana","saldo":10.5,"vip":1,"obs":":nome fica como texto"},{"id":2,"nome":"O'Brien \"Zé\"; DROP TABLE clientes; --","saldo":-3.25,"vip":0,…}]` (tipos preservados; o `':nome'` dentro de aspas no SQL não virou parâmetro); `json(r, "/0/nome") == "Ana"` leva o Se a digitar `ok Ana`; `json(r, "/1/saldo")` dá `-3.25`; `ins_status = 1`; um `count(*)` no fim dá 2 (a tabela não caiu). `bind_keeps_types_and_never_interpolates` confere que o SQL enviado só tem `?` e que o valor com `'); DROP TABLE t; --` vai como parâmetro.
- *Sem "database is locked"*: `concurrent_writers_do_not_lock`: três integrações disparadas pela mesma variável gravam no mesmo arquivo 40 vezes, sem nenhum log de erro; depois 4 threads × 50 `exec` simultâneos, zero falhas; total de 320 linhas. `waits_for_other_connection`: outra conexão (fora da trava interna, como outro processo) segura `BEGIN EXCLUSIVE` por 400 ms, e a gravação espera pelo busy timeout e termina com sucesso.
- *Validador/skill/README*: `json_contract_and_validation` (contrato, padrões, `deny_unknown_fields`, `{…}` inválido no `path`, `?` no SQL, timeout 0) e `errors_go_to_log` (tabela inexistente ➜ `SQLite "…": no such table: nao_existe`; `:nao_definida` ➜ `SQL: a variável 'nao_definida' (:nao_definida) não existe`; `r_ok` falso). No exe, depois de `cargo build --target x86_64-pc-windows-gnu`: o snippet "Cadastro local" da skill passou em `wine ultratask.exe --validate "Cadastro local.json"` (`OK`, saída 0); um arquivo com `path` vazio, `?` no SQL e `timeout_ms: 0` deu os 3 `ERRO` esperados (saída 1).
- *Embutido, sem DLL*: `x86_64-w64-mingw32-objdump -p ultratask.exe` não lista nenhuma DLL de SQLite.
- *Outros alvos* (só `cargo check`, que compila o C do SQLite mas não linka): `i686-pc-windows-gnu` e `aarch64-pc-windows-gnullvm` passaram usando o `clang` do container (`CC_<alvo>=clang`, `CFLAGS_<alvo>="--target=<i686|aarch64>-w64-mingw32 -isystem /usr/share/mingw-w64/include"`), porque não há `i686-w64-mingw32-gcc` aqui.
- `cargo test --target x86_64-pc-windows-gnu`: 100 passaram, 5 ignorados, sem warnings (rodou 2 vezes).

**Decisões de projeto**
- *SQLite em C embutido* (`rusqlite` + `bundled`): o `libsqlite3-sys` compila o `sqlite3.c` com o compilador C do próprio toolchain do Rust, via `cc`: o `cl.exe` no MSVC, o mingw aqui. Isso cumpre "sem toolchain C além do MSVC", porque o MSVC já é o toolchain. Para o ARM64 (ticket 07), o mesmo componente ARM64 do VS Build Tools que o Rust já exige para linkar também compila o SQLite. Não há SQLite puro em Rust maduro; a alternativa seria a DLL do sistema (`winsqlite3.dll`, só no Windows 10+ e sem garantia de versão), que o ticket descarta.
- *Parâmetros*: o `:variavel` é trocado por `?` por um analisador próprio (`sql::bind`), que pula textos (`'…'`, `"…"`, `` `…` ``, `[…]`), comentários (`--`, `/* */`) e o `::` do PostgreSQL. Assim o SQLite e o ODBC (ticket 12, que só aceita `?`) seguem o mesmo contrato. Os valores são lidos das variáveis no instante do disparo, na thread do fluxo. `?` no SQL é erro de validação. Booleano vira 1/0; tupla vira texto JSON; `Empty` vira NULL.
- *Resultado*: se o comando devolve colunas (SELECT, `RETURNING`, `PRAGMA`), `r` = lista JSON de objetos na ordem das colunas (`sql::rows_to_json`, reaproveitada pelo ODBC) e `r_status` = número de linhas. Senão, `r_status` e `r` = linhas afetadas. BLOB vira texto hexadecimal.
- *Concorrência*: cada disparo abre a própria conexão; uma trava por arquivo (caminho em minúsculas) faz um comando de cada vez dentro do processo, e o `busy_timeout = timeout_ms` cobre outro processo. Só um comando por disparo (o `prepare` do rusqlite usa só o primeiro e dá erro se sobrar texto).
- *Contrato*: `{"Sqlite": {"path": "dados.db", "sql": "…", "timeout_ms": 10000}}`; o `path` aceita `{…}` e é relativo à pasta do exe (mesma regra do destino Arquivo); o `sql` não aceita `{…}`.

### Pendente no Windows
Na máquina Windows (toolchain `stable-x86_64-pc-windows-msvc`), no branch deste ticket:
1. `cargo test` e `cargo build --release`: confirme que o `libsqlite3-sys` compila com o `cl.exe` (a saída do build mostra `Compiling libsqlite3-sys`) e que não aparece nenhum `sqlite3.dll` em `target\release`. `dumpbin /dependents target\release\ultratask.exe` não deve listar SQLite.
2. Alvos do ticket 07: `cargo build --release --target i686-pc-windows-msvc` (usa o toolset x86 já instalado). Depois de instalar o componente ARM64 do VS Build Tools: `cargo build --release --target aarch64-pc-windows-msvc`. Os dois devem compilar o SQLite sem erro.
3. Copie o snippet "Cadastro local" da skill para `target\release\scripts\Cadastro local.json`, 🔄, ▶ Tocar. O log deve mostrar `id 1, saldo 10.5`; rode de novo: `id 2, saldo 10.5`. Abra `target\release\dados\clientes.db` num visualizador de SQLite (ex.: DB Browser for SQLite) e confira as linhas com `O'Brien; DROP TABLE clientes` como texto. No painel **Execuções**, `linhas` mostra o JSON e `ins_status = 1`.
4. Aba: Destino **SQLite** mostra arquivo, SQL em fonte monoespaçada (fica vermelho com `?`; a dica lista os `:parâmetros`) e timeout, sem quadrados no lugar de glifos.

### Merge com `main-vqbyri`
Depois dos tickets 10 a 13, o branch recebeu `git merge main-vqbyri` (tickets 05, 06, 14 a 18).
- Conflitos resolvidos mantendo os dois lados: em `Cargo.toml`, dependências e features do `windows` em ordem alfabética, uma por linha; em `README.md`, a linha dos testes ignorados; no `Cargo.lock`, ficou o lado de `main-vqbyri`, e o `cargo` acrescentou `rusqlite`/`odbc-api`. O teste de glifos em `ui.rs` juntou sem conflito, um arquivo por linha, em ordem.
- *Pasta de dados (ticket 06)*: `model::base_dir()` agora é `compat::data_dir()`, ou seja, a pasta do exe ou `%LOCALAPPDATA%\UltraTask` quando a do exe não aceita escrita. Decisão: os caminhos relativos dos destinos Arquivo e SQLite e a pasta de trabalho do Comando **acompanham** essa pasta, porque é onde dá para gravar e onde ficam `scripts\` e `ultratask.json`. Não houve mudança de código, porque os destinos já usavam `base_dir()`. A skill, o README, a dica da aba e os comentários agora dizem "pasta de dados".
- *Exe exportado (ticket 05)*: os destinos novos guardam tudo no próprio `Script` (`integrations[].dest`), que já viaja no `Payload` pelo serde. Não precisam de nada fora do `Script`. No exe exportado, um caminho relativo parte da pasta de dados *do exe exportado* (a pasta dele, ou `%LOCALAPPDATA%\UltraTask`), que é o comportamento esperado para um exe portátil. O SQLite vai embutido também no exe exportado, que é o mesmo binário. O ODBC usa o `odbc32.dll` do sistema, e a string de conexão viaja no `Script`, com a senha em texto puro se houver.
- Verificação depois do merge: `cargo test --target x86_64-pc-windows-gnu` passou 2 vezes, com 166 testes passando, 11 ignorados e nenhum warning. Rodou num `WINEPREFIX` próprio, porque o `~/.wine` compartilhado estava sem `drive_c` e todo exe falhava com `could not load kernel32.dll`. O `real_database_round_trip` passou de novo com o driver SQLite3 ODBC. Os 10 snippets completos da skill passaram em `wine ultratask.exe --validate` (`LC_ALL=C.UTF-8`, nome do arquivo igual ao `name`).

### Verificação no Windows (2026-11-03)
Em 2026-11-03, numa máquina Windows 11 com o toolchain `stable-x86_64-pc-windows-msvc` (branch `main-vqbyri` @ 1903fe5):
- `cargo test`: 166 ok, 0 falhas, 11 ignorados;
- `cargo build --release`: ok (SQLite `bundled` compilado com `cl.exe`); exe de 15,9 MB que só importa DLLs do sistema;
- `cargo test -- --ignored --skip cdp_real_browser --skip real_database_round_trip`: 9 ok, 0 falhas.

O build de release com MSVC passou, com o SQLite `bundled` compilado pelo `cl.exe`. Falta conferir a aparência da aba.
