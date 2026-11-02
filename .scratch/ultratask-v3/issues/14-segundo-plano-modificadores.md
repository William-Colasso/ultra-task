# 14: Segundo plano: Ctrl, Alt e Shift confiáveis

**O que construir:** Hoje, no modo Segundo plano, combinações como Ctrl+S falham. O app-alvo consulta o estado real do teclado (GetKeyState), e as teclas postadas não o alteram.

Com este ticket, enquanto a execução mantiver um modificador pressionado, cada tecla enviada à janela-alvo leva esse estado de modificadores falsificado só para a thread do alvo. A técnica é a do pywinauto: AttachThreadInput + SetKeyboardState, com o estado restaurado logo depois. O teclado real do usuário e a janela ativa não são afetados.

Cuidados levantados pela pesquisa (`../pesquisa-input-paralelo.md`, R3a):
- a thread da execução precisa ter uma fila de mensagens;
- o attach zera o estado das teclas;
- um attach numa thread travada congela as duas. Pule janelas que não respondem, como faz o ControlSend do AutoHotkey;
- GetAsyncKeyState e RawInput continuam vendo o hardware. Isso é limitação conhecida, não bug.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Ctrl+S, enviado em Segundo plano para o Bloco de Notas (ou um app WinForms) coberto por outra janela, abre o diálogo Salvar em 20 de 20 tentativas.
- [ ] Durante o teste, o Ctrl real nunca aparece pressionado para o resto do sistema (GetAsyncKeyState), e a janela ativa do usuário não muda.
- [ ] Uma janela-alvo travada não congela a execução: a combinação falha com um aviso no log.
- [ ] Teclas sem modificador seguem o caminho de hoje, e o teste E2E existente continua verde.
- [ ] O README tira esse item das limitações conhecidas, ou explica o que ainda não funciona.
