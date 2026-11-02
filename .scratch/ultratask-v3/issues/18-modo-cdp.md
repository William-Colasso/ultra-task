# 18: Modo CDP para Chromium, Electron e WebView2

**O que construir:** Um terceiro modo de execução, "Navegador (CDP)", para apps Chromium, Electron e WebView2, que ignoram os cliques do Segundo plano. O modo é mais uma implementação da interface de I/O do interpretador, ao lado do Segundo plano e do Foco.

**Como abre e conecta.** O script ganha o comando que abre o app. Ao tocar, o UltraTask:
1. lança o app com depuração remota:
   - `--remote-debugging-port`;
   - um `--user-data-dir` próprio por instância;
   - as flags que impedem o Chromium de congelar janelas cobertas.

   Para WebView2, a porta vai pela variável de ambiente `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`.
2. conecta pelo Chrome DevTools Protocol, só em 127.0.0.1;
3. escolhe a página cujo título casa com a regra da janela-alvo.

**Como age.**
- Clique, Mover, Rolar, Tecla e Digitar viram eventos do protocolo, sem tocar no mouse nem no teclado reais. Tecla aceita modificadores, então Ctrl+S funciona.
- As coordenadas dos blocos continuam na área cliente. O modo converte para o viewport, descontando a barra do navegador e o devicePixelRatio.
- Pixel e OCR usam a captura de tela do próprio protocolo.

Várias instâncias rodam ao mesmo tempo, cada uma com a sua porta e o seu perfil.

Detalhes na pesquisa: `../pesquisa-input-paralelo.md`, R2.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Três instâncias sobrepostas de um app Electron (ou do Chrome) recebem cliques e textos ao mesmo tempo. O resultado é conferido no DOM, e o cursor e a janela ativa não mudam.
- [ ] Ctrl+S, ou outro atalho com modificador, funciona.
- [ ] Uma condição de pixel funciona com a janela coberta.
- [ ] A porta só escuta em 127.0.0.1 e o perfil é descartável. O README explica o risco de deixar a porta aberta.
- [ ] A conexão é sem TLS, só localhost, e a dependência de WebSocket compila nos alvos dos tickets 07 e 08 sem toolchain C além do MSVC.
- [ ] O validador, a skill (com snippets validados) e o README cobrem o modo.
