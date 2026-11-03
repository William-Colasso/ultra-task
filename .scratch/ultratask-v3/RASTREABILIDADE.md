# Rastreabilidade da rodada 3

Uma linha por ticket. O detalhe (commits, testes, verificação e pendências) fica na
seção **Rastreabilidade** de cada arquivo em `issues/`.

Status possíveis: `ready-for-agent`, `in-progress`, `needs-local-verification`
(feito e testado no container; faltam passos no Windows), `done`.

| Ticket | Status | Commits | Pendente no Windows |
|---|---|---|---|
| [01-editor-arrastar-blocos](issues/01-editor-arrastar-blocos.md) | needs-local-verification | eb5d77c, 284f8ee | 📍, Explodir, duplicar e excluir em bloco aninhado |
| [02-editor-paleta-envolver](issues/02-editor-paleta-envolver.md) | done | 3e38132, 92ebe03 | — |
| [03-cola-de-comandos-e-manual](issues/03-cola-de-comandos-e-manual.md) | needs-local-verification | aedb358, f53e8a4 | Escolha de sugestão da cola com o mouse |
| [04-movimento-humano-do-mouse](issues/04-movimento-humano-do-mouse.md) | needs-local-verification | b0e6a8b, 575d1c9 | Arrasto no Paint (Foco) deixa traço curvo |
| [05-exportar-exe](issues/05-exportar-exe.md) | needs-local-verification | bd4f49d, 1b64d1b, a91c303 | Duplo clique; dois .exe em paralelo com o app aberto; Parar na bandeja e atalho de pânico |
| [06-roda-em-qualquer-windows](issues/06-roda-em-qualquer-windows.md) | needs-local-verification | ae46e5b, 9083a17, a91c303 | VM/Sandbox sem GPU; Windows sem OCR; pasta sem escrita em Program Files |
| [07-builds-32-bits-e-arm64](issues/07-builds-32-bits-e-arm64.md) | needs-local-verification | 3463e69, 0763fc5, 533beca, d7f1047 | Gravar e tocar em modo Foco com o exe x86; máquina ARM64 |
| [08-windows-7-e-8](issues/08-windows-7-e-8.md) | needs-local-verification (Win7 bloqueado) | 409711f, 533beca, d7f1047 | VM 8.1 (abrir, gravar, tocar, mensagens) |
| [09-integracoes-rest](issues/09-integracoes-rest.md) | needs-local-verification | 905c4d8, 9fca086 | Criar/editar/remover/salvar integração e ver o resultado no painel Execuções |
| [10-integracao-arquivo](issues/10-integracao-arquivo.md) | needs-local-verification | f8627fc, 8c7bc2b, 26d083f | Aparência da aba |
| [11-integracao-sqlite](issues/11-integracao-sqlite.md) | needs-local-verification | 563429e, 1318cd0, 26d083f | Aparência da aba |
| [12-integracao-odbc](issues/12-integracao-odbc.md) | needs-local-verification | f9e249b, 364f9af, 26d083f | Teste real com Access/LocalDB via `ULTRATASK_ODBC_TESTE` |
| [13-integracao-comando-e-clipboard](issues/13-integracao-comando-e-clipboard.md) | done | 874eb55, cdb82e3, 26d083f | — |
| [14-segundo-plano-modificadores](issues/14-segundo-plano-modificadores.md) | needs-local-verification | 029b940, 6a669f9, 951c509 | Roteiro Ctrl+S 20/20 no Bloco de Notas coberto |
| [15-captura-wgc](issues/15-captura-wgc.md) | needs-local-verification | b873562, 8efcc8e, 951c509 | Pixel/OCR no Chrome e jogo DirectX cobertos |
| [16-blocos-uia](issues/16-blocos-uia.md) | needs-local-verification | e19e993, b293069, 303b138 | 3 alvos cobertos (WinForms/Notepad, WPF, Electron) com UIA real; erro/clique de reserva reais |
| [17-gravacao-com-uia](issues/17-gravacao-com-uia.md) | needs-local-verification | 9d698e2, bf089d8, 303b138 | Gravar em WinForms/WPF, explodir e tocar com a janela coberta |
| [18-modo-cdp](issues/18-modo-cdp.md) | needs-local-verification | 0104381, 753855b, 2bf4b68 | 3 instâncias sobrepostas sem mudar cursor/janela ativa; pixel com janela coberta; porta 127.0.0.1 e perfil descartável no Windows; .exe exportado com `cdp` |

## Verificação no Windows (2026-11-03)

Em 2026-11-03, numa máquina Windows 11 com o toolchain `stable-x86_64-pc-windows-msvc` (branch `main-vqbyri` @ 1903fe5):
- `cargo test`: 166 ok, 0 falhas, 11 ignorados;
- `cargo build --release`: ok (SQLite `bundled` compilado com `cl.exe`); exe de 15,9 MB que só importa DLLs do sistema;
- `cargo test -- --ignored --skip cdp_real_browser --skip real_database_round_trip`: 9 ok, 0 falhas.

Os 9 testes ignorados que passaram: `e2e_ctrl_s_forged`, `e2e_hung_target_does_not_freeze`, `clipboard_round_trip`, `read_only_file_is_permission_error`, `https_real_viacep`, `real_uia_on_own_window`, `e2e_wgc_covered_matches_visible`, `e2e_background_edit` e `registry_roundtrip`. Ficaram de fora `cdp_real_browser` (precisa de Chromium com porta de depuração) e `real_database_round_trip` (precisa de um banco ODBC).

Verificação visual (mesmo dia): pelo exe de release aberto, com captura da janela e UI Automation, foram conferidos os cartões nos dois temas, a paleta, recolher/expandir, o Manual (pela barra e pelo "?") e a aba Integrações. Isso também mostrou que o egui expõe todos os controles pela UIA (AccessKit). Os tickets 01, 02, 03 e 09 foram atualizados.

Builds extras (mesmo dia):
- `cargo build-all` gerou os exes x64, x86 e ARM64, e `cargo +nightly build-win7` gerou os dois de Win7/8.1.
- O x86 passou nos testes sob WoW64 (166 + 9).
- Os 5 exes estão anexados ao release v1.1.0.
- Detalhes nos tickets 07 e 08.

Os passos 1 a 3 abaixo já foram feitos; falta o 4 (roteiros manuais).

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
