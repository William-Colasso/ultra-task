# 05: Exportar script como .exe que só executa

**O que construir:** Um botão "📦 Exportar .exe" na lista de scripts abre "Salvar como" e grava um executável. Esse .exe é uma cópia do próprio UltraTask e leva embutidos:
- o script;
- todos os scripts que ele chama com Chamar script, em cadeia;
- os ajustes de reprodução: atalho de pânico, retomada após input físico e escala do OCR.

Se o ticket 09 já estiver na `main`, as integrações do script vão junto.

Ao abrir o .exe exportado:
- nenhuma janela de edição aparece;
- ele acha a janela-alvo pela regra do script. Se ela não existir, espera que apareça, e o ícone da bandeja mostra "Aguardando janela…";
- toca com o modo e as repetições do script;
- a bandeja oferece "Parar", e o atalho de pânico funciona;
- ao terminar, sai com código 0 (ok), 1 (erro) ou 2 (parado);
- se foi aberto num terminal, mostra o log ali.

O .exe exportado não lê nem grava os ajustes nem a pasta de scripts do app. Ele ignora o gatilho e o atalho do script, e não conflita com o UltraTask aberto nem com outros .exe exportados.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Ao exportar e abrir o .exe com duplo clique, a macro roda na janela-alvo e o processo sai sozinho.
- [x] A exportação é recusada, com uma mensagem que diz o motivo, quando falta um script chamado ou quando o validador reprova o script.
- [x] Um teste unitário cobre:
  - a ida e volta do conteúdo embutido;
  - um exe sem nada embutido, que abre o app normal.
- [ ] Dois .exe exportados em Segundo plano rodam ao mesmo tempo, com o UltraTask aberto.
- [ ] "Parar" na bandeja e o atalho de pânico interrompem a execução e soltam teclas e botões presos.
- [x] O README explica a exportação e avisa que antivírus podem estranhar executáveis de macro.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu` + Wine), ver `../AMBIENTE-LINUX.md`.

**Commits**
- `bd4f49d` feat: exportar script como .exe que só executa
- A feature `Win32_UI_Controls_Dialogs` do `windows` (para o "Salvar como") entrou no `Cargo.toml` junto do commit do ticket 06 (`ae46e5b`).

**Arquivos tocados**
- Novo: `src/export.rs` (payload, cadeia de "Chamar script", validação da exportação, "Salvar como", player do .exe exportado).
- Compartilhados (edições pequenas): `src/main.rs` (`mod export` e, antes de tudo, `read_own_payload()` desvia para o player), `src/runner.rs` (`RunHandle::exit_code` e `runner::exit_code()`), `src/ui.rs` (botão "📦 Exportar .exe" na lista; `export.rs` no teste de glifos), `README.md` (seção "Exportar como .exe").

**Testes adicionados** (`src/export.rs`)
- `payload_roundtrip`: ida e volta do conteúdo embutido (script, scripts chamados, atalho de pânico, retomada, escala do OCR) na memória e num arquivo lendo só o fim; reexportar troca o payload em vez de empilhar.
- `exe_without_payload_opens_the_app`: bytes sem rodapé e o próprio exe de teste não têm payload (`main` segue para o editor).
- `broken_payload_is_an_error_not_the_app`: rodapé com tamanho impossível ou JSON ruim viram erro; `deny_unknown_fields` + valores padrão no payload.
- `real_exe_export_roundtrip`: `write_exported` copia o exe real, embute e lê de volta; recusa sobrescrever o exe em uso.
- `chain_collects_nested_calls_once`: cadeia de "Chamar script" dentro de Se/Repetir/Enquanto, sem repetir e sem o principal.
- `export_is_refused_with_a_reason`: recusa com o motivo quando falta script chamado (em qualquer nível), quando o validador reprova, sem janela-alvo ou sem blocos; quando passa, leva a cadeia e os ajustes e descarta gatilho e atalho.
- `panic_combo_and_names`: leitura do atalho de pânico por estado das teclas, nome padrão do .exe, códigos de saída 0/1/2.

**Como foi verificado aqui**
- `cargo test --target x86_64-pc-windows-gnu` no Wine: tudo passa.
- Recusa com motivo: `export_is_refused_with_a_reason` cobre a decisão e o texto; o botão só repassa a mensagem ao log (`Exportação recusada: …`).
- README: lido após a edição.
- Ponta a ponta no Wine + Xvfb (sem marcar critério, porque não é o Windows real): exe de debug com payload anexado no mesmo formato (script Python no scratchpad) e alvo `notepad.exe`:
  - aberto antes do Bloco de Notas: imprime `aguardando janela notepad.exe / "*"`; ao abrir o Bloco de Notas, acha a janela, roda o script chamado (`🐞 soma 2`) e sai com código 0;
  - com bloco Parar: sai com 2; com expressão que não existe: sai com 1;
  - dois .exe exportados (Esperar 3 s + Digitar) rodando ao mesmo tempo, com o UltraTask (editor) aberto: os dois terminam com 0 e o editor continua aberto.

**Decisões de projeto**
- Payload = JSON do `Payload` + tamanho (u64 LE) + marcador `ULTRATK\x01` no fim do exe. O Windows ignora bytes depois das seções do PE. A leitura só busca os últimos 16 bytes, então um exe normal abre o editor sem custo perceptível. Exportar a partir de um exe que já tem payload troca o payload.
- O payload guarda os scripts inteiros (não só os nomes), com `deny_unknown_fields` e valores padrão como o JSON dos scripts. Gatilho e atalho do script são descartados ao exportar.
- A validação usa o mesmo `model::validate` do editor e do `--validate`, para o principal e para cada chamado, com os nomes da biblioteca como scripts conhecidos; erro prefixado com o nome do script (`'a' (chamado): blocks[0].script: …`).
- Atalho de pânico no exportado: `GetAsyncKeyState` a cada 30 ms em vez de `RegisterHotKey`, porque um atalho registrado é exclusivo do sistema: o UltraTask aberto e cada .exe exportado precisam reagir ao mesmo atalho ao mesmo tempo. Falta confirmar no Windows (passo 5) que o estado assíncrono das teclas continua visível enquanto o UltraTask aberto tem o mesmo atalho registrado.
- O player não chama `win::single_instance`, nem `model::base_dir`/`load_settings`/`load_scripts`, nem registra atalhos. Só sobe os hooks de baixo nível no modo Foco (para pausar quando você mexe no mouse/teclado).
- Com várias janelas casando com a regra, usa a primeira (não há interface para escolher).
- "Salvar como" com `GetSaveFileNameW` (comdlg32, DLL do sistema), sem dependência nova.
- **Ponto de entrada do ticket 09 (integrações):** elas ficam no `Script` (`integrations`), então já viajam em `Payload.script`/`Payload.called` sem mudança. Se o ticket 09 guardar algo fora do `Script`, o lugar está comentado no doc de `Payload` em `src/export.rs` (campo novo com padrão, preenchido em `build_payload` e repassado em `player_settings`/`run_player`). Quando o 09 entrar na `main`, acrescente em `export_is_refused_with_a_reason` (ou num teste novo) um script com integração e confira que ela sai igual em `split_payload`.

**Merge com `main-vqbyri`** (tickets 01-04, 09, 14-17)
- Conflitos resolvidos em `Cargo.toml`, `README.md`, `src/main.rs` e `src/ui.rs`, mantendo os dois lados: `mod compat`/`mod export` entram na lista em ordem alfabética, `compat.rs`/`export.rs` entram na lista de glifos (um por linha), e a seção "Exportar como .exe" do README fica antes de "Integrações".
- Integrações (ticket 09): `Script.integrations` vai junto no payload, e `build_payload` usa o `model::validate` atual, que já chama `integrations::validate`. Teste novo `integrations_travel_and_are_validated`: as integrações do principal e dos chamados fazem a ida e volta, e uma integração disparada pela própria variável de resultado faz a exportação ser recusada. O README agora diz que as integrações vão junto.
- Código de saída: no `spawn`, o `finish_integrations` roda antes de `code.store(exit_code(&result))`, e o `result` continua sendo o dos blocos. Os erros de integração vão só para o log, então a saída continua 0/1/2. No Wine, depois do merge, um exportado com "Chamar script" saiu com 0.
- `cargo test --target x86_64-pc-windows-gnu`: 121 passaram, 0 falharam, 7 ignorados, sem warnings.

### Pendente no Windows
Preparação: `cargo build --release`; abrir `target\release\ultratask.exe`; criar o script "Teste exportação" com alvo `notepad.exe` / `*`, modo Segundo plano, blocos `Esperar 2000` → `Digitar "ok {1+1}"` → `Chamar script "Sub"`; criar "Sub" com `Digitar " sub"`.
1. **Duplo clique.** Selecionar "Teste exportação" > **📦 Exportar .exe** > salvar na Área de Trabalho. Fechar o Bloco de Notas e dar duplo clique no .exe: nenhuma janela de edição abre, a dica do ícone na bandeja diz "Aguardando janela…". Abrir o Bloco de Notas: em ~2 s aparece "ok 2 sub" e o ícone some (processo encerrado, conferir no Gerenciador de Tarefas). No PowerShell: `(Start-Process "$env:USERPROFILE\Desktop\Teste exportação.exe" -Wait -PassThru).ExitCode` deve dar `0` com o Bloco de Notas aberto.
2. **Recusa.** Mudar o "Chamar script" para um nome que não existe e exportar: o log mostra "Exportação recusada: 'Teste exportação': blocks[2].script: script … não encontrado na pasta" e o "Salvar como" não abre. Pôr `x >` num Esperar: recusa com "expressão inválida".
3. **Log no terminal.** Num `cmd`, rodar `start /wait "" "%USERPROFILE%\Desktop\Teste exportação.exe" & echo %errorlevel%`: o log (`[Teste exportação] janela-alvo: …`, `terminou`) aparece no terminal e o código é 0.
4. **Dois exportados em paralelo.** Exportar também um segundo script (alvo `mspaint.exe` ou outro Bloco de Notas, modo Segundo plano, `Repetir 20` com `Digitar "b"` + `Esperar 300`). Com o UltraTask aberto, abrir os dois .exe juntos: os dois digitam ao mesmo tempo, o editor continua usável, e cada processo sai sozinho no fim.
5. **Parar e pânico.** Exportar um script em modo Foco com `Tecla Shift Down` → `Esperar 30000` → `Tecla Shift Up`. Abrir, esperar começar, clicar no ícone da bandeja > **Parar**: o processo sai com código 2 e o Shift não fica preso (digitar no Bloco de Notas sai em minúsculas). Repetir com o atalho de pânico (`Ctrl+Shift+Alt+X`) em vez do menu, com o UltraTask aberto e também tocando algo: o exportado e o UltraTask param juntos. Repetir no modo Segundo plano com `Clique` tipo `Down` em vez da tecla: o botão é solto.
6. **Ajustes embutidos.** Mudar o atalho de pânico em ⚙ Ajustes para outro (ex.: `Ctrl+Shift+Alt+Q`), exportar e conferir que o exportado para com o novo atalho e não com o antigo.

### Ajuste na integração 05 × 18 (feito por quem integrou os branches)
- `cdp::spawn` passou a preencher `RunHandle.exit_code` (campo do ticket 05) com `runner::exit_code`, para o código de saída 0/1/2 valer também no modo Navegador.
- O player do .exe exportado (`export::run_player`) agora, quando o script tem `cdp`, chama `cdp::spawn` direto em vez de esperar a janela-alvo (no CDP o próprio UltraTask abre o app).
- Verificado no container: `cargo test --target x86_64-pc-windows-gnu` com 143 ok, 8 ignorados.
- Pendente no Windows: exportar um script com `cdp` (ex.: "Busca no navegador" da skill), abrir o .exe e conferir que o app abre, a macro roda e o processo sai com 0.
