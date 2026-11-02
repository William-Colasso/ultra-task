# 12: Integração: ODBC (bancos instalados no PC)

**O que construir:** Um novo destino de integração, ODBC, para bancos que já estão no PC, como SQL Server, MySQL, PostgreSQL e Access. Ele aceita uma string de conexão ou um DSN e usa o gerenciador ODBC do próprio Windows.

O SQL, os parâmetros `:variavel` e o formato do resultado seguem o mesmo contrato do SQLite (ticket 11), reaproveitando a conversão de linhas para JSON.

**Bloqueado por:** 11 (Integração: banco SQLite embutido).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Uma falha de conexão vai para o log e deixa `r_ok` falso. Um teste com string de conexão inválida cobre isso.
- [x] Um teste real contra um banco ODBC insere, lê de volta e extrai um campo com `json()`. Se a máquina não tiver banco nem driver, peça ao usuário um DSN de teste e deixe este passo marcado como pendente.
- [x] O validador, a skill (com snippet validado) e o README cobrem o destino.
- [x] O README avisa que uma senha na string de conexão fica em texto puro no JSON do script.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine), conforme `AMBIENTE-LINUX.md`. Os quatro critérios foram verificados aqui. O status fica `needs-local-verification` porque o gerenciador ODBC usado foi o do Wine (que repassa ao unixODBC), não o `odbc32.dll` do Windows, e o build MSVC não roda aqui.

**Commits**
- `f9e249b` feat: destino de integração ODBC (bancos instalados no PC)
- (este registro) docs: rastreabilidade do ticket 12

**Arquivos tocados**
- Novo: `src/integrations/odbc.rs` (destino `Odbc`, `connection_string`, `exec`, leitura tipada das colunas, testes).
- Compartilhados (edições pequenas):
  - `Cargo.toml`/`Cargo.lock`: `odbc-api = { version = "29.3.0", default-features = false, features = ["odbc_version_3_80"] }` (traz `odbc-sys`, `widestring`, `atoi`; sem a feature `prompt`, que puxaria o winit). Linka com `odbc32`, que vem em todo Windows.
  - `src/integrations/mod.rs`: `pub mod odbc;`, variante `Dest::Odbc`, braço em `Dest::get`.
  - `src/integrations/editor.rs`: entrada "ODBC (bancos instalados)" em `DESTS`, braço em `dest_ui` e `odbc_ui` (reaproveita `sql_edit` do ticket 11).
  - `src/ui.rs`: `integrations/odbc.rs` na lista do teste de glifos.
  - `skills/ultratask-scripts/SKILL.md` (ODBC em *Outros destinos*, com snippet e aviso da senha) e `README.md` (destino ODBC com o aviso da senha; linha dos testes ignorados).

**Testes adicionados** (`src/integrations/odbc.rs`)
- `json_contract_and_validation`, `dsn_or_connection_string`, `invalid_connection_goes_to_log`;
- `#[ignore]`: `real_database_round_trip` (precisa de `ULTRATASK_ODBC_TESTE`).

**Como cada critério marcado foi verificado**
- *Falha de conexão*: `invalid_connection_goes_to_log` roda, pelo fluxo, um DSN inexistente e um `Driver={Driver Que Nao Existe};…`. O gerenciador ODBC do Wine respondeu com erro (`State: IM002 … Data source name not found and no default driver specified`); as duas integrações ficam com `r_ok = false`, `r_status = 0` e `r = ""`, o Se digita `falhou`, e o log tem exatamente duas linhas, `integração 'dsn': ODBC: não conectou: …` e `integração 'driver': ODBC: …` (sem quebra de linha).
- *Teste real*: a máquina não tem banco nem driver do Windows. Para rodar o teste mesmo assim, instalei no container o driver SQLite3 do unixODBC (`apt-get install libsqliteodbc`; só no container, nada no repositório) e rodei `ULTRATASK_ODBC_TESTE="Driver=SQLite3;Database=<scratchpad>/odbc-teste.db" cargo test --target x86_64-pc-windows-gnu integrations::odbc -- --include-ignored`. O caminho foi o exe Windows ➜ `odbc32.dll` do Wine ➜ unixODBC ➜ driver SQLite3 ODBC, e `real_database_round_trip` passou. O teste cria a tabela `ut_teste_<pid>`, insere `(1, "Ana", 10.5)` e `(2, "O'Brien; DROP TABLE x; --", -3.25)` pelas variáveis, lê de volta com `WHERE id >= :ler` e confere com `json()` que `r_status == 2`, `/0/nome == "Ana"`, `/1/nome` é o texto com aspas e `;`, `/1/id == 2` e `/0/saldo == 10.5`. Depois apaga a tabela, e `ins_status` deu 1. Falta repetir no Windows com um banco de verdade (passo 2 abaixo).
- *Validador/skill/README*: `json_contract_and_validation` cobre o contrato, os padrões, `deny_unknown_fields`, conexão vazia, SQL vazio, `?` no SQL e timeout 0. Também confere que `Driver={SQL Server}` não é tratado como expressão. No exe, depois de `cargo build --target x86_64-pc-windows-gnu`, o snippet "Estoque no SQL Server" da skill passou em `wine ultratask.exe --validate "Estoque no SQL Server.json"` (`OK`), junto com os três snippets dos tickets 10, 11 e 13. Um arquivo com conexão vazia, `?` e `timeout_ms: 0` deu os 3 `ERRO` esperados, e a segunda integração dele (`Driver={SQL Server};Server=.`) não deu erro.
- *Aviso da senha*: o README (lista de destinos, item ODBC) diz que `PWD=…` fica em texto puro no JSON do script e sugere DSN ou `Trusted_Connection=yes`. A skill e a dica da aba repetem o aviso.
- `objdump -p ultratask.exe` lista `ODBC32.dll`, o gerenciador do sistema. `cargo check` passou também em `i686-pc-windows-gnu` e `aarch64-pc-windows-gnullvm`, com `CC=clang` para o C do SQLite.
- `cargo test --target x86_64-pc-windows-gnu`: 103 passaram e 6 ficaram ignorados, sem warnings. A suíte rodou 2 vezes.

**Decisões de projeto**
- *Crate*: `odbc-api`, que no Windows usa as funções UTF-16 (`SQLDriverConnectW` e parâmetros `VarWCharBox`), então acentos funcionam em qualquer driver. O ambiente ODBC é o global do crate (`odbc_api::environment()`), e cada disparo abre e fecha a própria conexão.
- *Conexão*: um texto sem `=` vira `DSN=<texto>`, e o resto vai como está. A `connection` **não** aceita `{…}`, porque as chaves são sintaxe do ODBC (`Driver={SQL Server}`) e a interpolação as quebraria. Com isso, a senha não pode vir de uma variável. O timeout de login e o de consulta saem de `timeout_ms` (arredondado para cima, em segundos); se o driver não aceitar o de consulta, ele é ignorado.
- *Contrato igual ao SQLite*: o ticket 11 já tinha `sql::bind` (`:variavel` ➜ `?` + valores tipados) e `sql::rows_to_json`, e o ODBC reaproveita os dois. Os parâmetros são ligados assim: `Int` ➜ `i64`, `Float` ➜ `f64`, texto ➜ UTF-16, `Empty` ➜ NULL de texto e booleano ➜ 1/0. Na leitura, as colunas `INTEGER`/`SMALLINT`/`BIGINT`/`TINYINT`/`BIT` viram inteiro e `REAL`/`DOUBLE`/`FLOAT` viram número. `DECIMAL`/`NUMERIC` viram inteiro (escala 0) ou número quando o texto converte; os binários viram hexadecimal e o resto vira texto, inclusive datas. SELECT ➜ `r_status` = linhas; outros comandos ➜ `SQLRowCount`.
- As mensagens de erro do driver vão para o log numa linha só, com o prefixo `ODBC:` (`não conectou:` quando a falha é na conexão).

### Pendente no Windows
Na máquina Windows (toolchain `stable-x86_64-pc-windows-msvc`), no branch deste ticket:
1. `cargo test` e `cargo build --release` sem erro. `invalid_connection_goes_to_log` deve passar também com o `odbc32.dll` do Windows (a mensagem será `[Microsoft][ODBC Driver Manager] Data source name not found…`).
2. *Teste real* (critério 2, no gerenciador do Windows): o pedido de um DSN de teste fica com o usuário. Basta um destes:
   - **Access**, sem instalar nada além do driver que vem com o Office ou com o Access Database Engine. Crie um `.accdb` vazio e rode `set ULTRATASK_ODBC_TESTE=Driver={Microsoft Access Driver (*.mdb, *.accdb)};Dbq=C:\caminho\teste.accdb`.
   - **SQL Server Express/LocalDB**, com o "ODBC Driver 17/18 for SQL Server": `set ULTRATASK_ODBC_TESTE=Driver={ODBC Driver 18 for SQL Server};Server=(localdb)\MSSQLLocalDB;Database=master;Trusted_Connection=yes;TrustServerCertificate=yes`.

   Depois rode `cargo test real_database_round_trip -- --include-ignored`. O teste deve passar e apagar a tabela `ut_teste_<pid>` no fim.
3. Aba: o destino **ODBC (bancos instalados)** mostra o campo de conexão (com a dica de exemplo), o aviso da senha, o SQL (fica vermelho com `?`) e o timeout, sem quadrados no lugar de glifos.
4. (Opcional) Com o banco do passo 2, adapte o snippet "Estoque no SQL Server" da skill (string de conexão e tabela `produtos`), ponha em `target\release\scripts\` e rode com o Bloco de Notas aberto. Ele deve digitar `descrição: quantidade`. Com o servidor parado, o log deve mostrar `ODBC: não conectou: …` e a execução segue até o fim.

### Merge com `main-vqbyri`
Depois dos tickets 10 a 13, o branch recebeu `git merge main-vqbyri` (tickets 05, 06, 14 a 18).
- Conflitos resolvidos mantendo os dois lados: em `Cargo.toml`, dependências e features do `windows` em ordem alfabética, uma por linha; em `README.md`, a linha dos testes ignorados; no `Cargo.lock`, ficou o lado de `main-vqbyri`, e o `cargo` acrescentou `rusqlite`/`odbc-api`. O teste de glifos em `ui.rs` juntou sem conflito, um arquivo por linha, em ordem.
- *Pasta de dados (ticket 06)*: `model::base_dir()` agora é `compat::data_dir()`, ou seja, a pasta do exe ou `%LOCALAPPDATA%\UltraTask` quando a do exe não aceita escrita. Decisão: os caminhos relativos dos destinos Arquivo e SQLite e a pasta de trabalho do Comando **acompanham** essa pasta, porque é onde dá para gravar e onde ficam `scripts\` e `ultratask.json`. Não houve mudança de código, porque os destinos já usavam `base_dir()`. A skill, o README, a dica da aba e os comentários agora dizem "pasta de dados".
- *Exe exportado (ticket 05)*: os destinos novos guardam tudo no próprio `Script` (`integrations[].dest`), que já viaja no `Payload` pelo serde. Não precisam de nada fora do `Script`. No exe exportado, um caminho relativo parte da pasta de dados *do exe exportado* (a pasta dele, ou `%LOCALAPPDATA%\UltraTask`), que é o comportamento esperado para um exe portátil. O SQLite vai embutido também no exe exportado, que é o mesmo binário. O ODBC usa o `odbc32.dll` do sistema, e a string de conexão viaja no `Script`, com a senha em texto puro se houver.
- Verificação depois do merge: `cargo test --target x86_64-pc-windows-gnu` passou 2 vezes, com 166 testes passando, 11 ignorados e nenhum warning. Rodou num `WINEPREFIX` próprio, porque o `~/.wine` compartilhado estava sem `drive_c` e todo exe falhava com `could not load kernel32.dll`. O `real_database_round_trip` passou de novo com o driver SQLite3 ODBC. Os 10 snippets completos da skill passaram em `wine ultratask.exe --validate` (`LC_ALL=C.UTF-8`, nome do arquivo igual ao `name`).
