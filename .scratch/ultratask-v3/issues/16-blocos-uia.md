# 16: Blocos UI Automation: agir em elementos sem mouse nem teclado (tracer)

**O que construir:** Um novo bloco, "Elemento (UIA)", age direto num controle da janela-alvo pela UI Automation do Windows, sem input nenhum. Por isso roda em paralelo de verdade e com a janela coberta. Vale inclusive para apps que ignoram o Segundo plano: Chromium 126 ou mais novo, Electron com acessibilidade ligada, egui via AccessKit, WPF, WinForms, UWP e Qt.

- **Captura.** O elemento é capturado com 📍: clicar no controle grava a identificação dele (AutomationId, Name, ControlType, ClassName) e o ponto clicado.
- **Ações:**
  - acionar (Invoke);
  - alternar (Toggle);
  - definir valor (aceita `{…}`);
  - selecionar;
  - expandir/recolher;
  - ler o valor ou o nome para uma variável.
- **Elemento não encontrado.** Se o elemento não aparecer dentro de um timeout, o bloco para com erro. Uma opção do bloco troca isso por clicar no ponto capturado, como o Clique faz hoje, com aviso no log.
- **Condição.** Uma nova condição, "elemento", testa se ele existe, se o nome ou o valor contém um texto, e se está marcado.

Restrições:
- **Threads.** Chamadas UIA nunca rodam na thread da UI do UltraTask nem na thread dos hooks, porque um hook lento trava o input do sistema inteiro.
- **Timeout.** Invoke pode bloquear em alguns apps, então use timeout.
- **Apps elevados.** Elementos de apps elevados exigem o UltraTask elevado; o banner atual já cobre esse caso.
- **Biblioteca.** O crate `uiautomation` 0.25 usa a mesma versão do `windows` que o projeto. A alternativa é usar o COM direto.

Detalhes na pesquisa: `../pesquisa-input-paralelo.md`, R1.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Três alvos, cobertos por outras janelas, são acionados enquanto o usuário digita em outro app:
  - o Bloco de Notas ou um app WinForms;
  - um app WPF;
  - um Electron com `--force-renderer-accessibility`, ou o próprio UltraTask.

  O efeito é conferido lendo o valor pela UIA. O cursor e a janela ativa não mudam.
- [ ] Elemento não encontrado gera erro claro no log, ou o clique de reserva quando a opção estiver ligada.
- [ ] A escolha do elemento a partir da identificação gravada é lógica pura, com testes unitários sobre uma árvore falsa.
- [ ] O validador, a skill (com snippets validados) e o README cobrem o bloco e a condição.
