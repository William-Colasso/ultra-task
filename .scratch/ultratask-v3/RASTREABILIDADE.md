# Rastreabilidade da rodada 3

Uma linha por ticket. O detalhe (commits, testes, verificação e pendências) fica na
seção **Rastreabilidade** de cada arquivo em `issues/`.

Status possíveis: `ready-for-agent`, `in-progress`, `needs-local-verification`
(feito e testado no container; faltam passos no Windows), `done`.

| Ticket | Status | Commits | Pendente no Windows |
|---|---|---|---|
| [01-editor-arrastar-blocos](issues/01-editor-arrastar-blocos.md) | needs-local-verification | eb5d77c, 284f8ee | 📍 em bloco aninhado; cores nos temas claro/escuro |
| [02-editor-paleta-envolver](issues/02-editor-paleta-envolver.md) | needs-local-verification | 3e38132, 92ebe03 | Build release MSVC e aparência |
| [03-cola-de-comandos-e-manual](issues/03-cola-de-comandos-e-manual.md) | needs-local-verification | aedb358, f53e8a4 | Botão 📖 Manual da barra, escolha de sugestão com o mouse, aparência |
| [04-movimento-humano-do-mouse](issues/04-movimento-humano-do-mouse.md) | needs-local-verification | b0e6a8b, 575d1c9 | Arrasto no Paint (Foco) deixa traço curvo |
| [05-exportar-exe](issues/05-exportar-exe.md) | needs-local-verification | bd4f49d, 1b64d1b, a91c303 | Duplo clique; dois .exe em paralelo com o app aberto; Parar na bandeja e atalho de pânico |
| [06-roda-em-qualquer-windows](issues/06-roda-em-qualquer-windows.md) | needs-local-verification | ae46e5b, 9083a17, a91c303 | `dumpbin /dependents` do exe MSVC; VM/Sandbox sem GPU; Windows sem OCR; pasta sem escrita em Program Files |
| [07-builds-32-bits-e-arm64](issues/07-builds-32-bits-e-arm64.md) | ready-for-agent | | |
| [08-windows-7-e-8](issues/08-windows-7-e-8.md) | ready-for-agent | | |
| [09-integracoes-rest](issues/09-integracoes-rest.md) | needs-local-verification | 905c4d8, 9fca086 | Aba Integrações (criar/editar/remover) e painel Execuções na tela; HTTPS real (`https_real_viacep`) |
| [10-integracao-arquivo](issues/10-integracao-arquivo.md) | ready-for-agent | | |
| [11-integracao-sqlite](issues/11-integracao-sqlite.md) | ready-for-agent | | |
| [12-integracao-odbc](issues/12-integracao-odbc.md) | ready-for-agent | | |
| [13-integracao-comando-e-clipboard](issues/13-integracao-comando-e-clipboard.md) | ready-for-agent | | |
| [14-segundo-plano-modificadores](issues/14-segundo-plano-modificadores.md) | needs-local-verification | 029b940, 6a669f9, 951c509 | Ctrl+S 20/20 no Notepad coberto; Ctrl real nunca pressionado; alvo travado; E2E antigo |
| [15-captura-wgc](issues/15-captura-wgc.md) | needs-local-verification | b873562, 8efcc8e, 951c509 | Pixel/OCR no Chrome e jogo DirectX cobertos; coberta ≈ visível (teste ignorado) |
| [16-blocos-uia](issues/16-blocos-uia.md) | needs-local-verification | e19e993, b293069, 303b138 | 3 alvos cobertos (WinForms/Notepad, WPF, Electron) com UIA real; erro/clique de reserva reais; `real_uia_on_own_window` |
| [17-gravacao-com-uia](issues/17-gravacao-com-uia.md) | needs-local-verification | 9d698e2, bf089d8, 303b138 | Gravar em WinForms/WPF, explodir e tocar com a janela coberta |
| [18-modo-cdp](issues/18-modo-cdp.md) | needs-local-verification | 0104381, 753855b, 2bf4b68 | 3 instâncias sobrepostas sem mudar cursor/janela ativa; pixel com janela coberta; porta 127.0.0.1 e perfil descartável no Windows; .exe exportado com `cdp` |
