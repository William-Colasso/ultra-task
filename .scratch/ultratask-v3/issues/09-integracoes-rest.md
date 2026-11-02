# 09: Integrações: faixa por eventos e REST bidirecional (tracer)

**O que construir:** Uma aba "Integrações" no script, ao lado de Blocos e Config e fora da lista de blocos. Cada integração tem:
- **Nome.**
- **Gatilho:** dispara quando uma das variáveis escolhidas é definida, a cada N segundos ou ao terminar a execução.
- **Destino:** este ticket entrega o REST, com método, URL com `{…}`, cabeçalhos e timeout.
- **Formato do envio:** JSON das variáveis escolhidas (números continuam números), linha CSV, ou texto com `{…}`.
- **Variável de resultado:** opcional.

As integrações rodam em paralelo e nunca travam o fluxo principal. O fluxo reage pelas variáveis de resultado. Para uma variável de resultado `r`:
- `r` recebe o corpo da resposta;
- `r_ok` e `r_status` (o status HTTP, ou 0 em erro de rede) dizem como foi;
- `r_pendente` vira verdadeiro no instante em que o gatilho dispara, ainda dentro do bloco que definiu a variável. Assim, `Esperar até !r_pendente` não tem corrida.

Uma nova função de expressão, `json(texto, "/caminho")`, lê um campo de um JSON por JSON Pointer. Exemplos: `json(endereco, "/logradouro")` ou `json(r, "/itens/0/preco")`.

**Regras**
- As variáveis de resultado existem desde o início da execução.
- Respostas não disparam outras integrações, para não haver laços. Para encadear, o fluxo define uma variável.
- Só as integrações do script em execução ficam ativas; as de scripts chamados com Chamar, não.
- Parar descarta as respostas pendentes.
- O gatilho "ao terminar" roda depois do último bloco, e a espera por ele é limitada pelo timeout.
- Os erros vão para o log da execução.
- O HTTPS usa o repositório de certificados do Windows e não pode exigir toolchain C além do MSVC, porque os tickets 07 e 08 dependem disso.

**Contrato do JSON.** Ele fixa o formato para os tickets 10 a 13:

```json
"integrations": [{
  "name": "ViaCEP",
  "when": { "OnSet": ["cep"] },
  "dest": { "Http": { "method": "GET", "url": "https://viacep.com.br/ws/{cep}/json/", "headers": [], "timeout_ms": 10000 } },
  "payload": { "Json": ["cep"] },
  "result": "endereco"
}]
```

- `when` também pode ser `{ "Every": { "s": 5.0 } }` ou `"OnEnd"`.
- `payload` também pode ser `{ "Csv": ["a", "b"] }` ou `{ "Text": "pedido {n}" }`.
- `result` pode ser `null`.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] A aba lista as integrações do script e permite criar, editar e remover. Elas são salvas no JSON do script.
- [x] O validador rejeita:
  - campos desconhecidos;
  - nomes de variável inválidos;
  - `{…}` com expressão inválida;
  - uma integração disparada pela própria variável de resultado.
- [x] Um teste ponta a ponta, com um servidor HTTP local de teste, cobre este fluxo:
  1. o fluxo define `cep`;
  2. espera `!endereco_pendente`;
  3. `json(endereco, "/logradouro")` dá o valor esperado;
  4. um Se ramifica nesse valor.
- [x] Um servidor lento (2 s) não atrasa os blocos seguintes do fluxo.
- [x] Os gatilhos "a cada N s" e "ao terminar" funcionam, e os três formatos de envio têm teste.
- [ ] As variáveis de resultado aparecem no painel Execuções, e os erros aparecem no log.
- [x] A skill documenta `integrations` e `json()` com snippets validados por `--validate`, e o README ganha uma seção.

## Rastreabilidade

Feito num container Linux (alvo `x86_64-pc-windows-gnu`, testes no Wine 9.0), conforme `AMBIENTE-LINUX.md`.

**Commits**
- `905c4d8` feat: integrações REST em paralelo ao fluxo e função json()
- (este registro) docs: rastreabilidade do ticket 09

**Arquivos tocados**
- Novos: `src/integrations/mod.rs` (modelo, trait `Destination`, formatos, `json()`, validação), `src/integrations/http.rs` (destino REST + WinHTTP), `src/integrations/engine.rs` (disparos, threads, variáveis de resultado), `src/integrations/editor.rs` (aba), `src/integrations/tests.rs`.
- Compartilhados (edições pequenas):
  - `src/model.rs`: campo `Script.integrations` (padrão `[]`, omitido ao salvar quando vazio); `is_ident` virou `pub`; `validate` chama `integrations::validate`.
  - `src/runner.rs`: campo `Interp.integ`; `json()` registrada em `Interp::new`; `start_integrations`/`finish_integrations` (públicos) e `pump_integrations`/`var_set`; chamadas em `run_blocks`, `sleep_until`, `wait_if_paused`, `SetVar` e `OcrToVar`; `spawn` liga as integrações e roda o "ao terminar".
  - `src/ui.rs`: variante `Tab::Integrations` e aba "Integrações"; o teste de glifos lê os 4 arquivos novos.
  - `src/main.rs`: `mod integrations;`.
  - `Cargo.toml`: feature `Win32_Networking_WinHttp` do crate `windows` (nenhum crate novo; `Cargo.lock` não mudou).
  - `skills/ultratask-scripts/SKILL.md` (seção *Integrações*, `json()`, campo `integrations`) e `README.md` (seção *Integrações*, linha de testes).

**Testes adicionados** (`src/integrations/tests.rs`)
- `json_contract`, `rejects_unknown_fields`, `validator`, `payload_formats`, `json_function`, `url_and_request`, `editor_renders_every_variant`, `result_vars_and_fire_errors_without_network`;
- com servidor HTTP local (`TcpListener` na própria suíte, uma thread por conexão): `e2e_viacep_flow`, `e2e_slow_server_does_not_block_flow`, `e2e_every_and_on_end`, `e2e_payload_formats_on_the_wire`, `e2e_errors_go_to_log`, `e2e_no_loops_and_called_scripts_inactive`, `e2e_stop_discards`;
- `#[ignore]`: `https_real_viacep` (HTTPS real em viacep.com.br; precisa de Windows e internet).

**Como cada critério marcado foi verificado**
- *Validador*: `validator` confere os caminhos de erro de nomes inválidos (`when.OnSet[k]`, `payload.Csv[k]`, `result`), `{…}` inválido (URL, cabeçalho, `payload.Text`) e integração disparada pela própria variável de resultado (inclusive `r_pendente`); `rejects_unknown_fields` cobre campo desconhecido em integração, `Every`, `Http`, método e formato. No exe: `wine ultratask.exe --validate Ruim.json` imprimiu os 3 `ERRO` esperados e saiu com 1.
- *Ponta a ponta*: `e2e_viacep_flow` passa no Wine: `SetVar cep` dispara um GET real via WinHTTP para o servidor local; o bloco seguinte vê `endereco_pendente = true` (servidor responde em 300 ms); `WaitUntil !endereco_pendente`; `json(endereco, "/logradouro")` dá "Praça da Sé"; o `If` digita "rua Praça da Sé". Um segundo CEP cai no ramo `json(endereco, "/erro") == true`.
- *Servidor lento*: `e2e_slow_server_does_not_block_flow`: com `/lento` (2 s), o bloco seguinte roda em < 300 ms, um `Wait 100` termina em < 600 ms e a resposta só chega depois de 1,9 s.
- *Gatilhos e formatos*: `e2e_every_and_on_end` (a cada 0,25 s durante ~1,2 s dá de 3 a 6 envios; o "ao terminar" envia `terminou com n=4` e a execução espera a resposta, que aparece em `fim`/`fim_ok`); `payload_formats` (puro) e `e2e_payload_formats_on_the_wire` (método, `Content-Type` e corpo exatos no servidor para JSON com números como números, CSV com aspas e texto interpolado).
- *Skill/README*: os dois blocos JSON da seção *Integrações* da skill foram extraídos para arquivos e passaram em `wine target/x86_64-pc-windows-gnu/debug/ultratask.exe --validate "Consulta CEP.json" Fragmento.json` (`OK` nos dois, saída 0), depois de `cargo build --target x86_64-pc-windows-gnu`.
- `cargo test --target x86_64-pc-windows-gnu`: 33 passaram, 3 ignorados. Os testes de integração rodaram 5 vezes seguidas sem falha.

**Decisões de projeto**
- *Transporte HTTP: WinHTTP pelo crate `windows`* (já dependência do projeto; só uma feature nova). Usa o repositório de certificados e o proxy do Windows (`WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`, com recuo para `DEFAULT_PROXY`), não traz crate novo e não compila C, o que atende os tickets 07/08. `ureq` + `native-tls`/schannel também serviria, mas traria uma árvore de dependências maior, e o padrão dele (rustls/ring) exige compilador C. Endereços de loopback vão sem proxy. O WinHTTP do Wine funciona para HTTP: os testes e2e passam nele. O HTTPS real não dá para provar aqui, porque o container não resolve DNS (`https_real_viacep` falha no Wine com "servidor não encontrado (DNS) (WinHTTP 12007)") e porque o Wine não usa o repositório de certificados do Windows.
- *Extensão para os tickets 10 a 13*: um destino novo é uma variante em `integrations::Dest`, um módulo que implementa `Destination` (`kind`, `validate`, `prepare`, `timeout_ms`), um braço em `Dest::get`, e a entrada em `DESTS` e em `dest_ui` no `editor.rs`. Gatilhos, formatos, variáveis de resultado, log e validação comum não mudam. `prepare` roda na thread do fluxo, resolve os `{…}` no instante do disparo e devolve um `Job` (`FnOnce() -> Outcome + Send`) que roda numa thread própria. O `Outcome` traz `ok`, `status` (0 sem resposta), `body` e `error`.
- *Contrato*: exatamente o do ticket. `headers` é uma lista de pares `["Nome", "valor"]` (o valor aceita `{…}`); `method` é `"GET" | "POST" | "PUT" | "PATCH" | "DELETE"`; `timeout_ms` é número (como no exemplo do ticket), não string de expressão. Todos os campos têm padrão.
- *Semântica*: GET não envia corpo, e os valores vão na URL. O `Content-Type` sai do formato quando o usuário não define o cabeçalho. A URL é escapada só onde é inválida (espaço, acentos, aspas), mantendo `/ ? & =`. Se uma integração é disparada de novo antes da resposta, só a resposta do último disparo é aplicada. "A cada N s" pula o disparo se o anterior está pendente e usa o relógio do fluxo (`Io::now_ms`), então não conta as pausas do modo Foco. "Ao terminar" roda também quando o script termina com erro ou com o bloco Parar, mas não quando o usuário para. A espera vai até o maior `timeout_ms` + 1 s. O JSON enviado mantém a ordem das variáveis escolhidas. `json()` devolve `""` para campo ausente, `null` ou texto vazio e dá erro para texto que não é JSON.
- Respostas são aplicadas pela thread do fluxo (entre blocos, a cada fatia de 20 ms das esperas e durante pausas), sem passar por `on_set`, e por isso não disparam integrações.

### Pendente no Windows
Na máquina Windows (toolchain `stable-x86_64-pc-windows-msvc`), no branch deste ticket:
1. `cargo test` e `cargo test -- --include-ignored`. Com internet, `https_real_viacep` deve passar: HTTPS via WinHTTP com o repositório de certificados do Windows, digitando "Praça da Sé". `cargo build --release` sem erro.
2. *Aba Integrações* (critério 1): `target\release\ultratask.exe` ➜ ➕ Novo ➜ aba **Integrações** ➜ **➕ Nova integração**. Preencha Nome "ViaCEP", Gatilho "Ao definir variável" com ➕ variável `cep`, Destino REST com GET e URL `https://viacep.com.br/ws/{cep}/json/`, Envio "JSON das variáveis" com `cep`, e marque "guardar a resposta em" `endereco`. Confira que nenhum aviso ⚠ vermelho fica na tela. Ctrl+S e abra `target\release\scripts\<nome>.json`: o bloco `integrations` deve ter o formato do contrato. Crie uma segunda integração, remova-a com ✖, salve e confira que sumiu do JSON. Clique 🔄 e confira que a aba mostra o que foi salvo. Confira também a aparência: campos alinhados e sem quadrados no lugar de glifos.
3. *Painel Execuções e log* (critério 6): copie o exemplo "Consulta CEP" da skill para `target\release\scripts\Consulta CEP.json`, abra o Bloco de Notas, clique 🔄 e ▶ Tocar. O Bloco de Notas deve receber "Rua: Praça da Sé", e o painel **Execuções** ➜ "variáveis" deve mostrar `endereco`, `endereco_ok = true`, `endereco_status = 200` e `endereco_pendente = false`. A integração "Relatório" aponta para `exemplo.com`, então o log deve mostrar `integração 'Relatório': falhou (status …)` ou um erro de rede, e a execução deve terminar mesmo assim. Troque a URL por `https://viacep.com.br/ws/{cep}/jsonx/` e rode de novo: o log deve mostrar `falhou (status 4xx)` e o Bloco de Notas não recebe nada, porque cai no `Log` do else. Desligue a rede e rode: o log deve mostrar `servidor não encontrado (DNS)` ou `não conectou ao servidor`, e o script segue até o fim.
4. (Opcional) Atrás de um proxy corporativo/PAC, repita o passo 3 para confirmar que o `AUTOMATIC_PROXY` do WinHTTP pega a configuração do sistema.
