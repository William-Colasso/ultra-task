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
| [07-builds-32-bits-e-arm64](issues/07-builds-32-bits-e-arm64.md) | needs-local-verification | 3463e69, 0763fc5, 533beca, d7f1047 | `cargo build-all` real; `dumpbin`/manifesto; `cargo test --target i686-pc-windows-msvc`; WoW64; máquina ARM64 |
| [08-windows-7-e-8](issues/08-windows-7-e-8.md) | needs-local-verification (Win7 bloqueado) | 409711f, 533beca, d7f1047 | `cargo +nightly build-win7`; VM 8.1 (abrir, gravar, tocar, mensagens); confirmar recusa no Win7 (`windows-core` importa `combase.dll`) |
| [09-integracoes-rest](issues/09-integracoes-rest.md) | needs-local-verification | 905c4d8, 9fca086 | Aba Integrações (criar/editar/remover) e painel Execuções na tela; HTTPS real (`https_real_viacep`) |
| [10-integracao-arquivo](issues/10-integracao-arquivo.md) | needs-local-verification | f8627fc, 8c7bc2b, 26d083f | Build MSVC/release; aparência da aba; erro de arquivo somente leitura (teste ignorado no Wine) |
| [11-integracao-sqlite](issues/11-integracao-sqlite.md) | needs-local-verification | 563429e, 1318cd0, 26d083f | Build MSVC/release (SQLite `bundled` compilado com cl.exe); aparência da aba |
| [12-integracao-odbc](issues/12-integracao-odbc.md) | needs-local-verification | f9e249b, 364f9af, 26d083f | Teste real com Access/LocalDB via `ULTRATASK_ODBC_TESTE`; build MSVC/release |
| [13-integracao-comando-e-clipboard](issues/13-integracao-comando-e-clipboard.md) | needs-local-verification | 874eb55, cdb82e3, 26d083f | Ida e volta na área de transferência (teste ignorado); build MSVC/release |
| [14-segundo-plano-modificadores](issues/14-segundo-plano-modificadores.md) | needs-local-verification | 029b940, 6a669f9, 951c509 | Ctrl+S 20/20 no Notepad coberto; Ctrl real nunca pressionado; alvo travado; E2E antigo |
| [15-captura-wgc](issues/15-captura-wgc.md) | needs-local-verification | b873562, 8efcc8e, 951c509 | Pixel/OCR no Chrome e jogo DirectX cobertos; coberta ≈ visível (teste ignorado) |
| [16-blocos-uia](issues/16-blocos-uia.md) | needs-local-verification | e19e993, b293069, 303b138 | 3 alvos cobertos (WinForms/Notepad, WPF, Electron) com UIA real; erro/clique de reserva reais; `real_uia_on_own_window` |
| [17-gravacao-com-uia](issues/17-gravacao-com-uia.md) | needs-local-verification | 9d698e2, bf089d8, 303b138 | Gravar em WinForms/WPF, explodir e tocar com a janela coberta |
| [18-modo-cdp](issues/18-modo-cdp.md) | needs-local-verification | 0104381, 753855b, 2bf4b68 | 3 instâncias sobrepostas sem mudar cursor/janela ativa; pixel com janela coberta; porta 127.0.0.1 e perfil descartável no Windows; .exe exportado com `cdp` |

## Como continuar no Windows

Tudo acima foi feito num container Linux: compilado para `x86_64-pc-windows-gnu` e testado no Wine
(166 testes passando, 11 ignorados). Veja `AMBIENTE-LINUX.md`. Ordem sugerida na máquina Windows:

1. `git pull` do branch `main-vqbyri`.
2. `cargo test` e `cargo build --release` com o toolchain MSVC. É a primeira vez que o código é compilado com MSVC. Atenção ao SQLite `bundled` (usa `cl.exe`) e ao `.cargo/config.toml` (CRT estático, manifesto x86).
3. `cargo test -- --include-ignored`. Rode os testes ignorados de cada ticket, conforme a seção *Pendente no Windows* de cada um.
4. Faça os roteiros manuais por ticket, na ordem da tabela, e troque o status para `done` quando os critérios fecharem.

Decisões que mudaram o contrato ou que vale revisar:
- **18:** o modo CDP liga com o campo opcional `Script.cdp`, e não com uma variante `Mode::Cdp`.
- **12:** a string de conexão do ODBC não aceita `{…}`. As chaves são sintaxe do ODBC, então a senha fica em texto puro no JSON e no .exe exportado.
- **10/11/13:** os caminhos relativos partem da pasta de dados (`compat::data_dir()`), não necessariamente da pasta do exe.
- **06:** sem GPU, o app se relança com `--render-software` e desenha com wgpu/DX12 (WARP).
- **05:** o atalho de pânico do .exe exportado é lido com `GetAsyncKeyState`, para não brigar com o app aberto.
- **08:** o Windows 7 fica bloqueado pela `windows-core` 0.62. O 8.1 deve funcionar, mas ainda não foi testado.
