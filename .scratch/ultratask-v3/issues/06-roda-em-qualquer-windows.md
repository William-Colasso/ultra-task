# 06: Rodar em qualquer Windows 10/11 x64 sem instalar nada

**O que construir:** O exe de release roda numa instalação limpa do Windows 10 ou 11 de 64 bits, copiado sozinho, sem instalar nada:
- **Sem runtime.** Não depende do runtime do Visual C++.
- **Sem GPU.** A janela abre em máquinas sem aceleração de vídeo (VM, RDP, Windows Sandbox sem vGPU), com renderização por software. O OpenGL continua sendo o caminho normal, porque os tickets 07 e 08 dependem dele.
- **Sem OCR.** Se não houver um idioma com OCR instalado, os blocos e condições de OCR falham com uma mensagem que diz como instalar, em vez de travar.
- **Pasta sem escrita.** Se a pasta do exe não aceitar escrita (ex.: Arquivos de Programas), o app avisa e guarda scripts e ajustes em `%LOCALAPPDATA%\UltraTask`.

O event loop do winit não pode ser recriado no mesmo processo. Por isso, se abrir com OpenGL falhar, a troca de renderização precisa relançar o exe ou ser decidida antes de abrir a janela.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] O exe só importa DLLs do sistema (confira com `dumpbin /dependents` ou equivalente).
- [ ] O app abre, grava e toca um script no Windows Sandbox com a vGPU desligada, ou numa VM sem 3D. Se não houver como testar, peça esse teste ao usuário.
- [ ] Sem OCR disponível, aparece a mensagem e o app não entra em pânico.
- [ ] Numa pasta sem escrita, o app avisa e usa o local alternativo. Numa pasta normal, o comportamento é igual ao de hoje.
- [ ] O README lista os requisitos: Windows 10/11 e nada para instalar.
