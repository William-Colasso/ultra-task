# 13: Integração: comando externo e área de transferência

**O que construir:** Dois novos destinos de integração.
- **Comando.** Roda um programa sem abrir janela de console. Os argumentos aceitam `{…}`.
  - O envio vai pelo stdin.
  - O stdout, sem espaços nas pontas, vai para a variável de resultado.
  - O código de saída vai para `r_status`.
  - Se o timeout estourar, o processo é encerrado.
- **Área de transferência.** Escreve o envio, ou lê o conteúdo atual para a variável de resultado.

**Bloqueado por:** 09 (Integrações: faixa por eventos e REST bidirecional).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Um teste com `cmd /c` confere que:
  - o stdout chega na variável;
  - o código de saída chega em `r_status`;
  - um comando que trava é encerrado no timeout.
- [ ] Um teste faz a ida e volta na área de transferência. Ele fica ignorado por padrão, porque mexe na área de transferência real.
- [ ] O validador, a skill (com snippet validado) e o README cobrem os dois destinos.
