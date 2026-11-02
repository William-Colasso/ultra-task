# 04: Movimento humano do mouse

**O que construir:** Uma opção por script, "Movimento humano do mouse", na aba Config e no campo `human_mouse` do JSON.

Com ela ligada, o ponteiro deixa de "teletransportar" em todo deslocamento feito por Mover, Clique, Rolar e arrastos (Down → Mover → Up). Ele vai da posição atual até o alvo por uma curva aleatória:
- com um pequeno ruído no caminho;
- acelerando no começo e freando no fim;
- com duração que cresce com a distância (lei de Fitts);
- às vezes passando um pouco do ponto e corrigindo.

O clique cai exatamente no alvo, mais o ± px do próprio bloco. Blocos de gravação tocam como foram gravados.

A opção vale nos dois modos:
- **Foco:** o cursor real faz o trajeto.
- **Segundo plano:** a janela recebe os movimentos intermediários. A posição atual é a última que a execução enviou; no começo, é o centro da área cliente.

A velocidade do script escala a duração do trajeto. Parar e pausar funcionam no meio do caminho.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [x] O gerador de trajetos é uma função pura, com semente. Os testes cobrem:
  - começa na posição atual e termina exatamente no alvo;
  - fica num envelope limitado ao redor da reta;
  - não é uma reta em distâncias longas;
  - a duração fica numa faixa coerente com a distância;
  - sementes diferentes dão trajetos diferentes.
- [x] Teste do interpretador com o Io falso: com a opção ligada, um Clique gera vários movimentos antes do botão, e o último é o ponto do clique. Com a opção desligada, a sequência de eventos é a mesma de hoje.
- [x] Parar durante um trajeto longo interrompe em até ~50 ms.
- [x] O validador aceita o campo, a skill o documenta (com snippet validado por `--validate`) e o README o menciona.
- [ ] Verificação manual: no modo Foco, um arrasto no Paint deixa um traço curvo.

## Rastreabilidade

Feito no container Linux (alvo `x86_64-pc-windows-gnu` + Wine), veja `../AMBIENTE-LINUX.md`.

**Commits**
- `b0e6a8b` feat: movimento humano do mouse por script
- (este registro) docs: rastreabilidade do ticket 04

**Arquivos tocados**
- `src/humanmouse.rs` (novo): gerador de trajetos puro, com semente.
- `src/runner.rs`: campo `Interp::human_mouse` (igual ao `trace`, setado depois do `new`), `Interp::pos` (última posição enviada), métodos `Interp::send` e `Interp::move_to`; `Move`, `Click` e `Scroll` passam por eles; novo método do trait `Io::pointer` **com implementação padrão** (os `Io` falsos de outros tickets não precisam mudar); `WinIo::pointer`; `spawn` liga `it.human_mouse = script.human_mouse`; testes novos.
- `src/model.rs`: campo `Script::human_mouse: bool` (padrão `false`).
- `src/win.rs`: `pub fn cursor_client(h)` (GetCursorPos + ScreenToClient; sem feature nova do `windows`).
- `src/ui.rs`: linha "Mouse" na aba Config, logo antes de "Debug".
- `src/main.rs`: `mod humanmouse;`.
- `README.md`, `skills/ultratask-scripts/SKILL.md`: campo documentado, com snippet.

**Testes adicionados**
- `humanmouse::tests::starts_near_origin_and_ends_exactly_on_target`
- `humanmouse::tests::stays_inside_envelope_around_the_line`
- `humanmouse::tests::long_moves_are_not_straight`
- `humanmouse::tests::duration_follows_distance`
- `humanmouse::tests::seeds_change_the_path`
- `humanmouse::tests::some_paths_overshoot_and_correct`
- `humanmouse::tests::accelerates_then_brakes`
- `runner::tests::human_mouse_glides_before_click` (Clique, arrasto Down → Mover → Up, Rolar)
- `runner::tests::human_mouse_off_keeps_old_sequence`
- `runner::tests::stop_interrupts_human_path_fast`

**Como cada critério marcado foi verificado**
- Gerador: os 7 testes de `humanmouse` acima, em centenas de sementes e vários pares origem/alvo (inclui distância zero e curta). `cargo test --target x86_64-pc-windows-gnu`: 28 passaram, 2 ignorados (os que já eram `#[ignore]`).
- Interpretador com Io falso: `human_mouse_glides_before_click` confere ≥ 10 movimentos antes do botão, todos `Move`, o último igual ao ponto do clique, e o trajeto espalhado no tempo; `human_mouse_off_keeps_old_sequence` confere a sequência exata com tempos (`Move`, `Move`, botão ⬇, botão ⬆ +30 ms, roda +60 ms), igual à de antes.
- Parar: `stop_interrupts_human_path_fast` usa o Io falso dormindo de verdade, velocidade 0,05 e um trajeto de ~3300 px (vários segundos); outra thread pede parada aos 300 ms; o teste mede o tempo entre o pedido e o retorno (≤ 50 ms) e confere que nenhum botão foi enviado. Rodado 6 vezes no Wine sem falha.
- Validador/skill/README: `cargo build --target x86_64-pc-windows-gnu` e `LC_ALL=C.UTF-8 wine target/x86_64-pc-windows-gnu/debug/ultratask.exe --validate "Traço no Paint.json" "Login diário.json"` (os dois blocos JSON da skill que têm `human_mouse`, salvos com o nome derivado de `name`): `OK` nos dois. Sem `LC_ALL=C.UTF-8`, o Wine estraga os acentos do nome do arquivo e o validador acusa nome errado.

**Decisões de projeto**
- Curva: Bézier cúbica num arco de um lado só (desvio de 25% a 100% de `min(22% da distância, 180 px)`), por isso nunca vira reta em distâncias ≥ 40 px; abaixo disso vai reto. Ruído perpendicular de até 1,5 px, nulo nas pontas.
- Perfil de tempo: jerk mínimo (10t³ − 15t⁴ + 6t⁵), que acelera no começo e freia no fim.
- Duração: Fitts `80 + 110·log2(1 + d/12)` ms, ±15% aleatório. 1000 px dá ~810 ms.
- Passar do ponto: 30% de chance a partir de 150 px; passa 3% a 6% da distância (teto 25 px), um pouco de lado, e corrige com um trecho reto curto.
- Um ponto a cada 10 ms (`STEP_MS`), dividido pela velocidade do script. A cada ponto, o interpretador checa pausa (desloca a linha do tempo, como no `Recording`) e parada (o `sleep_until` acorda a cada ≤ 20 ms).
- Ponto de partida (`Io::pointer`): no Foco, a posição real do cursor (então funciona mesmo que o usuário tenha mexido); no Segundo plano, a última posição enviada pela execução e, no começo, o centro da área cliente.
- O ± px do Clique é sorteado antes do trajeto; o trajeto termina exatamente nesse ponto.
- `Scroll` com a opção desligada continua sem `Move` antes da roda (comportamento de hoje). `Recording` não muda.
- A semente de cada trajeto vem do `fastrand` global; o gerador continua puro.
- `Interp::new` **não** mudou de assinatura.

### Pendente no Windows
- [ ] **Verificação manual no Paint (modo Foco).**
  1. `cargo build --release` (MSVC) e abra `target\release\ultratask.exe`.
  2. Abra o Paint (`mspaint`), com o pincel selecionado e uma tela em branco.
  3. No UltraTask, importe/crie o script da skill (`skills/ultratask-scripts/SKILL.md`, exemplo "Traço no Paint"), ou crie à mão: janela-alvo = Paint, modo **Foco**, **Config → Mouse → Movimento humano do mouse** ligado, blocos Clique *Down* (200, 300), Mover (700, 450), Clique *Up* (700, 450). Ajuste as coordenadas para caírem na tela de desenho.
  4. Toque. Esperado: o cursor anda até (200, 300) por uma curva, aperta, e o traço até (700, 450) sai **curvo** (não reto), terminando no ponto exato.
  5. Desligue a opção e toque de novo: o traço sai reto (salto direto).
  6. Ligue de novo, ponha velocidade 0,2, toque e aperte ⏹ (ou `Ctrl+Shift+Alt+X`) no meio do traço: o cursor para na hora e o botão é solto.
- Opcional: repetir no modo Segundo plano num app Win32 que desenhe com WM_MOUSEMOVE (o Paint moderno pode ignorar PostMessage) e conferir que o cursor real não se mexe.
- `cargo build --release` com MSVC e `cargo test` no Windows.
