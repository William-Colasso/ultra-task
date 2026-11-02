# 11: Integração: banco SQLite embutido

**O que construir:** Um novo destino de integração, SQLite. Ele aponta para um arquivo .db, criado se faltar, e roda um SQL com parâmetros `:variavel`, ligados aos valores das variáveis com os tipos preservados. O SQL nunca recebe interpolação de texto, para não haver injeção.

O resultado depende do comando:
- **SELECT:** devolve as linhas como JSON (uma lista de objetos) na variável de resultado. Exemplo: `json(r, "/0/nome")`.
- **Outros comandos:** põem em `r_status` o número de linhas afetadas.

O SQLite vai embutido no exe, sem DLL extra, e precisa compilar nos alvos dos tickets 06 e 07.

**Bloqueado por:** 09 (Integrações: faixa por eventos e REST bidirecional).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Testes com um .db temporário cobrem:
  - inserir valores de variáveis;
  - ler de volta para uma variável;
  - extrair um campo com `json()`;
  - um valor com aspas e `;`, que não quebra nem injeta SQL.
- [ ] Gravações no mesmo arquivo vindas de integrações diferentes não dão "database is locked".
- [ ] O validador, a skill (com snippet validado) e o README cobrem o destino.
