# 08: Windows 7 e 8.1 (melhor esforço)

**O que construir:** Builds para os alvos Win7 do Rust (`x86_64-win7-windows-msvc` e `i686-win7-windows-msvc`), que exigem toolchain nightly com `build-std`. Com eles, o UltraTask abre e toca scripts em Segundo plano e em Foco no Windows 7 SP1 e no 8.1.

Recursos que não existem nesses sistemas ficam desligados, com uma mensagem clara em vez de derrubar o app. Exemplos: o OCR do WinRT e a renderização por software do ticket 06, se ela depender do DirectX 12.

O README diz o que funciona em cada versão do Windows. Se uma dependência não suportar esses alvos, documente qual é e pare ali: este ticket é de melhor esforço.

**Bloqueado por:** 07 (Builds de 32 bits e ARM64 nativo).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Os dois builds Win7 compilam com um comando documentado.
- [ ] Numa VM com Windows 7 SP1 ou 8.1, o app abre, grava e toca em Segundo plano e em Foco. Se não houver VM, este passo fica com o usuário.
- [ ] Recursos ausentes mostram uma mensagem e não derrubam o app.
- [ ] O README tem uma tabela de recursos por versão do Windows.
