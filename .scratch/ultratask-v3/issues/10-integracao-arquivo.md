# 10: Integração: arquivo de texto

**O que construir:** Um novo destino de integração, Arquivo. O caminho aceita `{…}`, e um caminho relativo parte da pasta do exe. Pastas que faltam são criadas. Há três modos:
- **Acrescentar** e **sobrescrever** gravam no formato de envio da integração:
  - JSON: uma linha por envio;
  - CSV: uma linha de cabeçalho quando o arquivo é novo;
  - texto: gravado como está.
- **Ler** põe o conteúdo do arquivo na variável de resultado.

**Bloqueado por:** 09 (Integrações: faixa por eventos e REST bidirecional).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Testes numa pasta temporária cobrem:
  - acrescentar;
  - sobrescrever;
  - cabeçalho CSV gravado uma vez só;
  - ler o arquivo para uma variável.
- [ ] Erro de permissão ou caminho inválido vai para o log e deixa `r_ok` falso.
- [ ] O validador, a skill (com snippet validado) e o README cobrem o destino.
