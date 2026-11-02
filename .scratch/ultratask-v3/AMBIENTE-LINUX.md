# Trabalhar nos tickets fora do Windows (container Linux)

Parte da rodada 3 foi feita num container Linux na nuvem. Este arquivo diz como
o código foi compilado e testado lá, e o que isso prova ou não prova.

## Como compilar e testar
- Alvo: `x86_64-pc-windows-gnu` (mingw-w64), em vez do `x86_64-pc-windows-msvc`.
- Os testes rodam no Wine:

  ```bash
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine WINEDEBUG=-all
  cargo test  --target x86_64-pc-windows-gnu
  cargo build --target x86_64-pc-windows-gnu
  LC_ALL=C.UTF-8 wine target/x86_64-pc-windows-gnu/debug/ultratask.exe --validate arquivo.json
  ```
- Sem `LC_ALL=C.UTF-8`, nomes de arquivo com acento chegam estragados ao exe no Wine.
- Para checar outros alvos sem linkar: `cargo check --target i686-pc-windows-gnu`.

## O que isso prova
- O código compila para Windows e os testes unitários passam (modelo, interpretador
  com Io falso, validador, glifos, lógica pura).

## O que isso NÃO prova (fica para a máquina Windows)
- Input real, captura de tela, OCR do WinRT, UI Automation, registro e qualquer
  teste `#[ignore]`: o Wine não reproduz o Windows com fidelidade
  (o `e2e_background_edit` falha no Wine, mesmo com Xvfb).
- O build MSVC (`cargo build --release` com `stable-x86_64-pc-windows-msvc`),
  `dumpbin`, e a aparência da UI.

Cada ticket feito aqui registra, na seção **Rastreabilidade** do próprio arquivo,
o que foi verificado no container e os passos que faltam no Windows.
Critérios que dependem do Windows ficam `[ ]`, e o status vira
`needs-local-verification` em vez de `done`.
