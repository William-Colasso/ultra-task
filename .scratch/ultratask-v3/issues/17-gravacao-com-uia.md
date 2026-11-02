# 17: Gravação guarda o elemento UIA de cada clique

**O que construir:** Ao gravar, cada clique também registra o elemento UIA sob o cursor, com a mesma identificação do ticket 16. Esse elemento é resolvido fora da thread dos hooks, sem atrasar a gravação.

"Explodir em passos" transforma os cliques em elementos acionáveis (Invoke, Toggle, Select) em blocos "Elemento (UIA)", com o clique por coordenada como reserva. Os demais cliques continuam como Clique. A reprodução da gravação bruta não muda.

**Bloqueado por:** 16 (Blocos UI Automation).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Gravar cliques em botões de um app WinForms ou WPF e explodir gera blocos UIA que funcionam com a janela coberta.
- [ ] O hook nunca espera pela UIA, então a gravação não fica mais lenta. Registre no ticket o teste ou a medição que comprova isso.
- [ ] Scripts gravados antes deste ticket continuam válidos e tocam igual.
- [ ] A skill e o README cobrem o novo campo da gravação.
