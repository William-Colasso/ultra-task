# 03: Cola de comandos nos campos editáveis e Manual no app

**O que construir:**
- **Catálogo.** Um catálogo único descreve cada função de expressão, com assinatura, descrição curta e exemplo:
  - as do UltraTask: `rand`, `contem`;
  - as nativas que a documentação promete: `min`, `max`, `floor`, `round`, `ceil`, `len`, `if`, `str::…` e `math::…`.

  O catálogo também cobre os operadores e a sintaxe `{expressão}` dos textos.
- **Cola.** Ela aparece junto ao campo quando:
  - um campo de expressão está em foco (Definir variável, x/y, tempos, condição por expressão);
  - o cursor está dentro de `{…}` num texto (Digitar, Mostrar no log).

  A cola lista as variáveis do script e as funções que casam com a palavra sendo digitada, com a descrição de cada uma. Setas e Enter, ou um clique, inserem o item no cursor. Se a expressão estiver inválida, a cola mostra o erro.
- **Manual.** Um botão "📖 Manual" na barra, e um "?" junto aos campos, abrem uma janela com as seções Expressões, Blocos, Condições, Teclas e Atalhos.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Digitar `ra` num campo de expressão sugere `rand(a, b)` com a descrição, e escolher a sugestão insere `rand(` no cursor.
- [ ] A cola lista as variáveis definidas em qualquer nível do script: Definir variável, Ler texto e, se o ticket 09 já estiver na `main`, as variáveis de resultado das integrações.
- [ ] Dentro de `{…}`, em Digitar e Mostrar no log, a cola funciona igual. Fora das chaves, ela não aparece.
- [ ] O Manual abre pela barra e pelo "?". O "?" leva direto à seção Expressões.
- [ ] Um teste roda o exemplo de cada função do catálogo num interpretador novo e falha se algum der erro.
- [ ] Um teste falha se uma função registrada pelo UltraTask faltar no catálogo.
- [ ] Um teste falha se a skill de scripts não citar alguma função do catálogo.
- [ ] O teste de glifos passa.
