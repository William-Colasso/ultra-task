# 07: Builds de 32 bits e ARM64 nativo

**O que construir:** Um comando documentado gera três executáveis de release: x64, x86 (32 bits) e ARM64. Os três herdam o que o ticket 06 entregou: sem runtime e sem GPU.

Nenhuma dependência pode exigir um toolchain C além do MSVC.

O README explica:
- quais alvos do Rust instalar;
- qual componente ARM64 do VS Build Tools instalar. Hoje a máquina só tem os toolsets x64 e x86, então instalar o ARM64 é passo do usuário ou exige a permissão dele.

**Bloqueado por:** 06 (Rodar em qualquer Windows 10/11 x64 sem instalar nada).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Os três exes compilam a partir de um único comando.
- [ ] O exe x86 roda num Windows de 64 bits (WoW64): grava, toca em Segundo plano e em Foco, e o OCR funciona.
- [ ] O exe ARM64 compila. Se houver uma máquina ARM64, ele roda nela; se não houver, o ticket anota que ficou sem verificar.
- [ ] Os testes passam também no alvo x86 (`cargo test --target i686-pc-windows-msvc`).
