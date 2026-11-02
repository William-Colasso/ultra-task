# 13: Integração: comando externo e área de transferência

**O que construir:** Dois novos destinos de integração.
- **Comando.** Roda um programa sem abrir janela de console. Os argumentos aceitam `{…}`.
  - O envio vai pelo stdin.
  - O stdout, sem espaços nas pontas, vai para a variável de resultado.
  - O código de saída vai para `r_status`.
  - Se o timeout estourar, o processo é encerrado.
- **Área de transferência.** Escreve o envio, ou lê o conteúdo atual para a variável de resultado.

**Bloqueado por:** 09 (Integrações: faixa por eventos e REST bidirecional).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] Um teste com `cmd /c` confere que:
  - o stdout chega na variável;
  - o código de saída chega em `r_status`;
  - um comando que trava é encerrado no timeout.
- [ ] Um teste faz a ida e volta na área de transferência. Ele fica ignorado por padrão, porque mexe na área de transferência real.
- [x] O validador, a skill (com snippet validado) e o README cobrem os dois destinos.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine), conforme `AMBIENTE-LINUX.md`.

**Commits**
- `874eb55` feat: destinos de integração Comando e Área de transferência
- (este registro) docs: rastreabilidade do ticket 13

**Arquivos tocados**
- Novos: `src/integrations/command.rs` (destino `Command`, `run`, `decode`, job object, testes) e `src/integrations/clipboard.rs` (destino `Clipboard`, `ClipMode`, `write`/`read`, testes).
- Compartilhados (edições pequenas):
  - `src/integrations/mod.rs`: `pub mod clipboard; pub mod command;`, variantes `Dest::Command` e `Dest::Clipboard`, braços em `Dest::get`.
  - `src/integrations/editor.rs`: entradas "Comando" e "Área de transferência" em `DESTS` (agora `pub(super)`), braços em `dest_ui`, `command_ui` e `clipboard_ui`.
  - `src/integrations/tests.rs`: `editor_renders_every_variant` também desenha uma integração para cada entrada de `DESTS`.
  - `src/ui.rs`: os 2 arquivos novos na lista do teste de glifos.
  - `Cargo.toml`: features `Win32_Globalization` (OEM ➜ UTF-16 do stdout), `Win32_System_DataExchange` e `Win32_System_Memory` e `Win32_System_Ole` (área de transferência), `Win32_System_JobObjects` (encerrar o processo com os filhos). Nenhum crate novo; o `Cargo.lock` não mudou.
  - `skills/ultratask-scripts/SKILL.md` (Comando e Área de transferência em *Outros destinos*, com snippet) e `README.md` (lista de destinos e linha dos testes ignorados).

**Testes adicionados**
- `src/integrations/command.rs`: `json_contract_and_validation`, `cmd_stdout_stdin_and_exit_code`, `hung_command_is_killed_on_timeout`, `missing_program_goes_to_log`, `decode_utf8_or_oem`.
- `src/integrations/clipboard.rs`: `json_contract`; `#[ignore]`: `clipboard_round_trip`.

**Como cada critério marcado foi verificado**
- *Teste com `cmd /c`* (no `cmd.exe` do Wine): `cmd_stdout_stdin_and_exit_code` roda pelo fluxo três integrações disparadas por `SetVar n = 21`: `cmd /c "echo   valor {n * 2}  "` deixa `r = "valor 42"` (pontas aparadas), `r_ok = true`, `r_status = 0`; `cmd /c "findstr pedido"` recebe pelo stdin `outra linha\r\npedido 21\r\n` e devolve `pedido 21`; `cmd /c "echo parcial& exit 7"` deixa `x = "parcial"`, `x_ok = false`, `x_status = 7` e o log `integração 'sai7': "cmd" saiu com código 7`. `hung_command_is_killed_on_timeout` roda `cmd /c "for /l %i in (0,0,1) do @echo x>>arquivo"` com timeout de 700 ms: volta entre 0,7 s e 2,5 s com `tempo esgotado (700 ms): o processo "cmd" foi encerrado`, e o arquivo para de crescer (mesmo tamanho 400 ms depois), o que prova que o processo morreu. A suíte inteira rodou 3 vezes seguidas sem falha.
- *Validador/skill/README*: `json_contract_and_validation` e `json_contract` (contrato, padrões, `deny_unknown_fields`, modo desconhecido, programa vazio, `{…}` inválido em argumento, timeout 0). No exe, depois de `cargo build --target x86_64-pc-windows-gnu`: o snippet "Procura e copia" da skill passou em `wine ultratask.exe --validate "Procura e copia.json"` (`OK`, saída 0); um arquivo com programa vazio, `echo {x +}` e `timeout_ms: 0` deu os 3 `ERRO` esperados, e `"mode": "Colar"` deu `unknown variant` (saída 1).
- `cargo test --target x86_64-pc-windows-gnu`: 91 passaram, 5 ignorados, sem warnings.

**Decisões de projeto**
- Contratos: `{"Command": {"program": "cmd", "args": ["/c", "echo ok"], "timeout_ms": 10000}}` e `{"Clipboard": {"mode": "Write" | "Read"}}` (esses são os padrões).
- *Sem janela*: `std::process::Command` com `creation_flags(CREATE_NO_WINDOW)`; stdin, stdout e stderr em pipes, lidos em threads (um envio grande não trava). A pasta de trabalho é a do exe, como os caminhos relativos do destino Arquivo.
- *Timeout*: o processo vai para um job object logo depois do `spawn`, e o timeout chama `TerminateJobObject`, que encerra também os filhos (ex.: `cmd /c ping …`). Sem `KILL_ON_JOB_CLOSE`: o que o comando abriu de propósito (`start notepad`) continua depois de um término normal. Se o job não puder ser criado, cai no `Child::kill`. Um filho aberto antes da atribuição ao job (janela de microssegundos) escapa; aceitável.
- *Resultado*: `r_ok` só com código 0; com outro código, o stdout continua em `r` e o log mostra o código e até 200 caracteres do stderr. Se o processo termina mas um filho em segundo plano segura o stdout aberto, a espera vai até o fim do timeout e o resultado é erro.
- *Texto*: stdout em UTF-8 quando válido; senão, decodificado pela página OEM (`CP_OEMCP`), que é a do `cmd`.
- *Argumentos*: o escape é o do `std` (regras do MSVCRT). Para o `cmd`, a skill recomenda a linha inteira num só argumento e sem aspas internas, e mandar dados de fora pelo stdin (interpolar em `args` permite injeção de comando no `cmd`).
- *Área de transferência*: `OpenClipboard(NULL)` com até 50 tentativas de 20 ms (outro programa pode estar com ela aberta), `CF_UNICODETEXT`; `r_status` = caracteres.

### Pendente no Windows
Na máquina Windows (toolchain `stable-x86_64-pc-windows-msvc`), no branch deste ticket:
1. `cargo test` e `cargo build --release` sem erro.
2. *Área de transferência* (critério 2): `cargo test clipboard_round_trip -- --include-ignored`. Deve passar: escreve `Pedido nº 7\r\nçã ok` pelo fluxo (`w_ok = true`, `w_status = 18`), lê de volta para `c` com o mesmo texto e `c_status = 18`, e restaura o conteúdo anterior da área de transferência. Depois, cole (Ctrl+V) no Bloco de Notas para confirmar que o texto anterior voltou.
3. *Sem janela*: copie o snippet "Procura e copia" da skill para `target\release\scripts\Procura e copia.json`, 🔄, ▶ Tocar. Nenhuma janela de console pode piscar; o painel **Execuções** deve mostrar `achou = "1:pedido 42"`, `achou_ok = true`, `achou_status = 0`; Ctrl+V no Bloco de Notas cola `1:pedido 42`. Troque `{chave}` por `nada` (expr `"\"nada\""`): `achou_status = 1`, `achou_ok = false`, e o log mostra `"cmd" saiu com código 1`.
4. *Filhos no timeout*: num script com `{"Command": {"program": "cmd", "args": ["/c", "ping -n 30 127.0.0.1"], "timeout_ms": 1000}}`, rode e confira no Gerenciador de Tarefas que nenhum `PING.EXE` sobra depois de 1 s (o log mostra `tempo esgotado (1000 ms)`).
5. Aba: Destino **Comando** (campos programa/arg/➕ argumento/timeout, ✖ remove argumento) e **Área de transferência** (combo Escrever/Ler) aparecem sem quadrados no lugar de glifos.
