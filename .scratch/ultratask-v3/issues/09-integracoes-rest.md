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

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] A aba lista as integrações do script e permite criar, editar e remover. Elas são salvas no JSON do script.
- [ ] O validador rejeita:
  - campos desconhecidos;
  - nomes de variável inválidos;
  - `{…}` com expressão inválida;
  - uma integração disparada pela própria variável de resultado.
- [ ] Um teste ponta a ponta, com um servidor HTTP local de teste, cobre este fluxo:
  1. o fluxo define `cep`;
  2. espera `!endereco_pendente`;
  3. `json(endereco, "/logradouro")` dá o valor esperado;
  4. um Se ramifica nesse valor.
- [ ] Um servidor lento (2 s) não atrasa os blocos seguintes do fluxo.
- [ ] Os gatilhos "a cada N s" e "ao terminar" funcionam, e os três formatos de envio têm teste.
- [ ] As variáveis de resultado aparecem no painel Execuções, e os erros aparecem no log.
- [ ] A skill documenta `integrations` e `json()` com snippets validados por `--validate`, e o README ganha uma seção.
