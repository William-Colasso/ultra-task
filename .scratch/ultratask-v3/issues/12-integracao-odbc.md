# 12: Integração: ODBC (bancos instalados no PC)

**O que construir:** Um novo destino de integração, ODBC, para bancos que já estão no PC, como SQL Server, MySQL, PostgreSQL e Access. Ele aceita uma string de conexão ou um DSN e usa o gerenciador ODBC do próprio Windows.

O SQL, os parâmetros `:variavel` e o formato do resultado seguem o mesmo contrato do SQLite (ticket 11), reaproveitando a conversão de linhas para JSON.

**Bloqueado por:** 11 (Integração: banco SQLite embutido).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Uma falha de conexão vai para o log e deixa `r_ok` falso. Um teste com string de conexão inválida cobre isso.
- [ ] Um teste real contra um banco ODBC insere, lê de volta e extrai um campo com `json()`. Se a máquina não tiver banco nem driver, peça ao usuário um DSN de teste e deixe este passo marcado como pendente.
- [ ] O validador, a skill (com snippet validado) e o README cobrem o destino.
- [ ] O README avisa que uma senha na string de conexão fica em texto puro no JSON do script.
