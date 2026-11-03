# 17: Gravação guarda o elemento UIA de cada clique

**O que construir:** Ao gravar, cada clique também registra o elemento UIA sob o cursor, com a mesma identificação do ticket 16. Esse elemento é resolvido fora da thread dos hooks, sem atrasar a gravação.

"Explodir em passos" transforma os cliques em elementos acionáveis (Invoke, Toggle, Select) em blocos "Elemento (UIA)", com o clique por coordenada como reserva. Os demais cliques continuam como Clique. A reprodução da gravação bruta não muda.

**Bloqueado por:** 16 (Blocos UI Automation).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Gravar cliques em botões de um app WinForms ou WPF e explodir gera blocos UIA que funcionam com a janela coberta.
- [x] O hook nunca espera pela UIA, então a gravação não fica mais lenta. Registre no ticket o teste ou a medição que comprova isso.
- [x] Scripts gravados antes deste ticket continuam válidos e tocam igual.
- [x] A skill e o README cobrem o novo campo da gravação.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu` + Wine; veja `../AMBIENTE-LINUX.md`), em cima do ticket 16 no mesmo branch. O Wine não tem UI Automation de verdade.

### Commits
- `9d698e2` feat: gravação guarda o elemento UIA de cada clique
- o commit seguinte, `docs(ticket 17): …`, só atualiza este arquivo.

### Arquivos tocados
- `src/uia_rec.rs` (novo): `RecElem` (item de `Recording.elements`), `rec_elements` (n-ésimo botão pressionado → índice em `events`), `upgrade_clicks` (pós-passo do `explode`), `ClickCollector` (thread coletora) e `click_collector` (resolve cada clique com `uia::capture` num `Worker`, timeout de 2 s).
- `src/model.rs`: campo `elements` no `Block::Recording` (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`) e validação de `elements[i].event`.
- `src/hooks.rs`: `Recorder` ganhou `clicks: ClickCollector` e `downs`; cada botão pressionado gravado faz `clicks.click(..)`; `Out::Recorded` passou a levar `(Vec<Event>, Vec<RecElem>)`; ao parar, a coletora termina na thread dela, guarda o resultado em `FINISHED` e acorda a thread dos hooks, que emite `Out::Recorded`.
- `src/ui.rs`: `finish_record` recebe os elementos; **Explodir em passos** chama `uia_rec::upgrade_clicks(runner::explode(..), ..)`; o bloco de gravação mostra quantos cliques viram UIA.
- `src/runner.rs`: só `..` no padrão do `Recording` e um teste.
- `src/main.rs`: `mod uia_rec;`.
- `skills/ultratask-scripts/SKILL.md`, `README.md`.

### Testes adicionados
- `uia_rec::tests::collector_never_blocks_the_hook`: **medição do critério do hook.** Uma coletora com resolvedor que leva 20 ms por clique recebe 200 cliques. O teste exige que os 200 enfileiramentos somem menos de 20 ms (o tempo de uma única resolução). No Wine, em três execuções: 200 cliques em 74 µs, 75 µs e 110 µs; pior clique 9 a 12 µs. Também confere que o resultado sai completo e em ordem, e que um resolvedor que entra em pânico não perde a gravação.
- `uia_rec::tests::recorded_clicks_become_uia_blocks`: clique em botão (Invoke) e em caixa (Toggle) viram `Uia` com reserva; clique duplo, arraste, clique direito e clique em texto sem ação continuam `Click`; os `Wait` não mudam; sem elementos, a saída é idêntica à do `explode`.
- `uia_rec::tests::rec_elem_json`: formato do `RecElem`, `action` opcional, campo desconhecido rejeitado.
- `model::tests::old_recordings_unchanged`: um script gravado antes do ticket é lido, validado e regravado byte a byte; com `elements`, também valida; `event` que não aponta para botão pressionado é erro.
- `runner::tests::recording_with_elements_plays_the_same`: a mesma gravação, com e sem `elements`, envia exatamente os mesmos eventos nos mesmos tempos (Io falso).

### Como cada critério marcado foi verificado
- **O hook nunca espera pela UIA:** no `mouse_proc` (thread dos hooks), o único trabalho novo é `ClickCollector::click`, um `Sender::send` num canal sem limite. A leitura do elemento roda na thread coletora, que chama um `Worker` UIA com timeout. A medição está em `collector_never_blocks_the_hook` (acima). Ao parar a gravação, a thread dos hooks também não espera: quem termina é a coletora, que avisa por `PostThreadMessageW` (`wake_thread`).
- **Scripts antigos válidos e tocando igual:** `old_recordings_unchanged` (leitura, validação, JSON idêntico) e `recording_with_elements_plays_the_same`. O código da reprodução do `Recording` não mudou (só ganhou `..` no padrão).
- **Skill e README:** a skill descreve `elements` (formato, `event`, `action`, efeito no Explodir); o README fala da gravação na seção *Elementos (UI Automation)*. O script `Gravacao com elementos.json` (gravação com `elements`) e os exemplos da skill passaram em `wine target/x86_64-pc-windows-gnu/debug/ultratask.exe --validate …` (OK, código 0), depois de `cargo build --target x86_64-pc-windows-gnu`.
- `cargo test --target x86_64-pc-windows-gnu`: 33 passaram, 3 ignorados.

### Decisões de projeto
- **Elementos fora dos eventos:** `Event` continua `Copy` e igual; o elemento vai num campo novo do `Recording`, indexado pelo evento do botão pressionado. Gravações antigas não mudam nem no JSON regravado.
- **Ordinal do clique, não índice:** o hook só conhece o n-ésimo botão pressionado, e o `clean_recording` só descarta eventos de tecla. Por isso o n-ésimo botão pressionado é o mesmo antes e depois da limpeza (`rec_elements`).
- **Explodir como pós-passo:** `upgrade_clicks` não mexe no `explode`. Ele usa o fato de que os blocos do `explode` consomem os botões pressionados em ordem (Click/Down um, Double dois, Up nenhum). Só cliques esquerdos simples viram UIA; o clique duplo continua clique, porque dois Invoke/Select não equivalem a "abrir".
- **Qual elemento:** o mesmo `uia::capture` do 📍 (ElementFromPoint, sobe até 3 níveis procurando Invoke/Toggle/Select, ancestrais até a janela-alvo). A ação sugerida segue o ControlType: CheckBox → Toggle; RadioButton/ListItem/TabItem/TreeItem/DataItem → Select; senão Invoke, Toggle ou Select, nessa ordem.
- **Corrida conhecida:** o hook roda antes de o app receber o botão pressionado, e a leitura começa logo em seguida. Num botão que fecha a janela já no pressionar, o elemento pode sumir antes da leitura; nesse caso o clique fica como `Click`.
- **Parar a gravação** pode atrasar o `Out::Recorded` até os cliques pendentes serem lidos (no máximo 2 s por clique travado). O hook não espera.

### Merge com `main-vqbyri` (tickets 01–04 e 09)
Commit de merge no mesmo branch (`merge: main-vqbyri nos tickets 16 e 17`). Adaptações:
- **Explodir em passos** agora fica em `src/editor.rs` (`Op::Explode`), que passou a chamar `uia_rec::upgrade_clicks(runner::explode(..), ..)`; o `blocks_ui` antigo do `ui.rs` saiu no merge.
- `model::tests::old_recordings_unchanged` compara byte a byte só o bloco `Recording`: o `Script` ganhou campos novos em outros tickets (`human_mouse`, `integrations`), que aparecem ao regravar o script inteiro.
- Teste do editor ajustado para o campo `elements` do `Recording`.
- Depois do merge: `cargo test --target x86_64-pc-windows-gnu` com 96 passando, 4 ignorados e sem warnings.

### Pendente no Windows
1. Rode `cargo test -- --include-ignored` (inclui `real_uia_on_own_window`, do ticket 16, que exercita o mesmo `capture`).
2. **Gravar e explodir com a janela coberta:**
   1. Abra um app WinForms ou WPF com pelo menos um botão, uma caixa de seleção e um item de lista (ex.: um formulário de teste do Visual Studio).
   2. No UltraTask, crie um script `Background` com esse app como alvo, grave (`Ctrl+Shift+Alt+R`) clicando no botão, na caixa e no item, e pare a gravação.
   3. Confira no bloco de gravação a linha "N cliques viram Elemento (UIA) ao explodir" e clique em **Explodir em passos**. Esperado: os três cliques viraram **Elemento (UIA)** (acionar/alternar/selecionar), com **se não achar, clicar no ponto** ligado; os demais cliques continuam **Clique**.
   4. Volte o app ao estado inicial, cubra-o com outra janela maximizada, digite nela e dispare o script. Esperado: o botão é acionado, a caixa muda e o item fica selecionado (confira depois, ou com uma condição **elemento (UIA)** *está marcado*); o cursor e a janela ativa não mudam.
3. **Gravação não fica mais lenta:** grave cliques rápidos (dez cliques seguidos) num app WPF pesado; o cursor não pode engasgar. Para medir, compare com o mesmo teste no build anterior a este ticket: o tempo entre os eventos gravados (`t` no JSON) deve ser o mesmo dos cliques reais.
4. **Script antigo:** abra um script gravado antes deste ticket, toque e compare com o comportamento antes da atualização; salve e confira que o JSON não ganhou `elements`.
