# 07: Builds de 32 bits e ARM64 nativo

**O que construir:** Um comando documentado gera três executáveis de release: x64, x86 (32 bits) e ARM64. Os três herdam o que o ticket 06 entregou: sem runtime e sem GPU.

Nenhuma dependência pode exigir um toolchain C além do MSVC.

O README explica:
- quais alvos do Rust instalar;
- qual componente ARM64 do VS Build Tools instalar. Hoje a máquina só tem os toolsets x64 e x86, então instalar o ARM64 é passo do usuário ou exige a permissão dele.

**Bloqueado por:** 06 (Rodar em qualquer Windows 10/11 x64 sem instalar nada).

**Status:** needs-local-verification

Leia antes: `../CONVENCOES.md`.

- [ ] Os três exes compilam a partir de um único comando.
- [ ] O exe x86 roda num Windows de 64 bits (WoW64): grava, toca em Segundo plano e em Foco, e o OCR funciona.
- [ ] O exe ARM64 compila. Se houver uma máquina ARM64, ele roda nela; se não houver, o ticket anota que ficou sem verificar.
- [ ] Os testes passam também no alvo x86 (`cargo test --target i686-pc-windows-msvc`).

## Rastreabilidade

Feito num container Linux (alvos `*-windows-gnu`/`gnullvm` + Wine), ver `../AMBIENTE-LINUX.md`. Nenhum critério dá para marcar aqui: todos dependem do MSVC, de WoW64 real ou de uma máquina ARM64.

**Commits**
- (este commit) feat: builds de release x64, x86 e ARM64 com um comando

**Arquivos tocados**
- `.cargo/config.toml` (compartilhado): alias `cargo build-all` (os 3 alvos MSVC num só `cargo build --release`, que aceita vários `--target`); manifesto `asInvoker` embutido só no exe MSVC x86 (`/MANIFEST:EMBED /MANIFESTUAC:level='asInvoker'`).
- `README.md` (compartilhado): seção Requisitos cita x64/x86/ARM64; nova subseção "x64, x86 e ARM64 de uma vez" no Build (alvos do rustup, componente `Microsoft.VisualStudio.Component.VC.Tools.ARM64`, onde fica cada exe).
- Nenhuma mudança em `src/` nem no `Cargo.toml`.

**Testes**
- Nenhum teste novo: o ticket não tem lógica nova. A suíte inteira roda no alvo de 32 bits (abaixo).

**O que foi verificado aqui (sem marcar critério)**
- Sem C: `cargo tree -e normal,build -i cc --target <alvo>` não imprime nada para `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc`, `i686-pc-windows-gnu` e `aarch64-pc-windows-gnullvm` (nem `cmake`/`bindgen`/`pkg-config`).
- `cargo check --target i686-pc-windows-gnu` e `cargo check --target aarch64-pc-windows-gnullvm`: compilam sem warnings. O ARM64 não foi linkado (o container não tem o sysroot `aarch64-w64-mingw32` do llvm-mingw).
- `cargo build --target i686-pc-windows-gnu`: linka. `i686-w64-mingw32-objdump -p` lista as mesmas DLLs de sistema do exe x64 do ticket 06 (nada de libgcc/libwinpthread/vcruntime).
- Testes em 32 bits no Wine (com `wine32:i386` e `gcc-mingw-w64-i686` instalados e um `WINEPREFIX` próprio): `RUST_BACKTRACE=0 CARGO_TARGET_I686_PC_WINDOWS_GNU_RUNNER=wine cargo test --target i686-pc-windows-gnu` = 143 passaram, 0 falharam, 8 ignorados. Com `RUST_BACKTRACE=1` o processo cai no Wine ao imprimir o backtrace de um pânico esperado (`uia_rec::collector_never_blocks_the_hook`); reproduzido com um programa de 3 linhas (`catch_unwind(|| panic!())`), é do Wine WoW64 + i686-gnu, não do UltraTask.
- O alias expande certo: `cargo build-all -v` chama o rustc com os três `--target`; `cargo check -v --target i686-pc-windows-msvc` mostra `+crt-static` **e** os dois `link-arg` do manifesto (as tabelas `cfg` se somam), e `--target x86_64-pc-windows-msvc` só o `+crt-static`. O build em si para por falta da std MSVC no container.

**Decisões**
- Alias de cargo em vez de script `.ps1`: funciona em qualquer shell, sem política de execução do PowerShell, e o `cargo build` já aceita vários `--target` num comando só. Os exes ficam em `target\<alvo>\release\` (o `target\release\ultratask.exe` do `cargo build --release` continua igual).
- Manifesto só no x86: o ticket 06 deixou anotado que processos de 32 bits sem manifesto sofrem a virtualização de arquivos do UAC; com ela, o teste de escrita em `C:\Program Files (x86)\UltraTask` daria "aceita escrita" e os scripts iriam para `%LOCALAPPDATA%\VirtualStore`. O `link.exe` embute o manifesto sozinho (`/MANIFEST:EMBED`), sem `rc.exe` nem crate extra. x64 e ARM64 não sofrem essa virtualização, então ficam como estão.

### Pendente no Windows
1. **Componente ARM64.** No PowerShell como administrador: `& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" modify --installPath "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools" --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --passive` (ou pelo Visual Studio Installer, como no README). Depois `rustup target add i686-pc-windows-msvc aarch64-pc-windows-msvc`.
2. **Um comando, três exes** (critério 1 e 3). Na raiz do repo: `cargo build-all`. Esperado: termina sem erro e existem `target\x86_64-pc-windows-msvc\release\ultratask.exe`, `target\i686-pc-windows-msvc\release\ultratask.exe` e `target\aarch64-pc-windows-msvc\release\ultratask.exe`. No "Developer PowerShell for VS 2022": `dumpbin /headers <exe> | findstr machine` mostra `8664`, `14C` e `AA64`; `dumpbin /dependents <exe>` não lista `VCRUNTIME140.dll` nem `api-ms-win-crt-*`.
3. **Manifesto do x86.** `mt.exe -inputresource:target\i686-pc-windows-msvc\release\ultratask.exe;#1 -out:con` mostra `requestedExecutionLevel level="asInvoker"`. Se o link reclamar do `/MANIFESTUAC`, troque no `.cargo/config.toml` por `/MANIFESTUAC:NO` e reabra este ponto.
4. **Testes x86** (critério 4): `cargo test --target i686-pc-windows-msvc` e `cargo test --target i686-pc-windows-msvc -- --include-ignored`.
5. **WoW64** (critério 2): copiar o exe x86 para a Área de Trabalho, abrir, criar um script com o Bloco de Notas como alvo; gravar digitação; tocar em Segundo plano e em Foco (o texto aparece); um bloco "Ler texto (OCR)" numa região com texto lê o texto. Depois copiar o exe x86 para `C:\Program Files (x86)\UltraTask\` e abrir sem elevar: o log deve dizer que a pasta não aceita escrita e usar `%LOCALAPPDATA%\UltraTask`, e nada pode aparecer em `%LOCALAPPDATA%\VirtualStore\Program Files (x86)\UltraTask`.
6. **ARM64** (critério 3, parte de rodar): sem máquina ARM64 aqui, ficou sem verificar. Numa máquina com Windows 11 ARM: abrir o exe ARM64 e repetir o passo 5 (Gerenciador de Tarefas > Detalhes > coluna Arquitetura deve mostrar ARM64).
