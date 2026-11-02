# 15: Pixel e OCR em segundo plano com Windows Graphics Capture

**O que construir:** No modo Segundo plano, as condições de pixel e OCR capturam a janela com PrintWindow, já com PW_RENDERFULLCONTENT. Em janelas DirectX ou DirectComposition encobertas, como jogos em janela e alguns Chromium, a imagem pode vir preta.

Com este ticket, quando a captura vier toda preta (ou de uma cor só) e a janela não estiver minimizada, a execução passa a usar o Windows Graphics Capture (CreateForWindow) para aquela janela até o fim.
- Janela minimizada continua sem captura: a condição dá falso com aviso, como hoje.
- No Windows 10 pode aparecer a borda amarela de captura. Onde o sistema permitir, desligue-a.

Detalhes na pesquisa: `../pesquisa-input-paralelo.md`, R3b.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Com uma janela do Chrome e um jogo (ou amostra) DirectX em janela, ambos cobertos por outra janela, uma condição de pixel e um OCR leem o valor certo em Segundo plano.
- [ ] A imagem capturada com a janela coberta é praticamente igual à capturada com ela visível. O teste fica ignorado por padrão, porque mexe em janelas reais.
- [ ] Apps comuns continuam no PrintWindow, sem custo extra.
- [ ] Se houver dependência nova, ela compila nos alvos dos tickets 07 e 08 sem toolchain C além do MSVC. Onde o WGC não existir, fica o PrintWindow.
- [ ] O README atualiza a limitação "jogos DirectX aparecem pretos".
