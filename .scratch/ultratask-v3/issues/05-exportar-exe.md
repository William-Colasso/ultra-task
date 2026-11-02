# 05: Exportar script como .exe que só executa

**O que construir:** Um botão "📦 Exportar .exe" na lista de scripts abre "Salvar como" e grava um executável. Esse .exe é uma cópia do próprio UltraTask e leva embutidos:
- o script;
- todos os scripts que ele chama com Chamar script, em cadeia;
- os ajustes de reprodução: atalho de pânico, retomada após input físico e escala do OCR.

Se o ticket 09 já estiver na `main`, as integrações do script vão junto.

Ao abrir o .exe exportado:
- nenhuma janela de edição aparece;
- ele acha a janela-alvo pela regra do script. Se ela não existir, espera que apareça, e o ícone da bandeja mostra "Aguardando janela…";
- toca com o modo e as repetições do script;
- a bandeja oferece "Parar", e o atalho de pânico funciona;
- ao terminar, sai com código 0 (ok), 1 (erro) ou 2 (parado);
- se foi aberto num terminal, mostra o log ali.

O .exe exportado não lê nem grava os ajustes nem a pasta de scripts do app. Ele ignora o gatilho e o atalho do script, e não conflita com o UltraTask aberto nem com outros .exe exportados.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Ao exportar e abrir o .exe com duplo clique, a macro roda na janela-alvo e o processo sai sozinho.
- [ ] A exportação é recusada, com uma mensagem que diz o motivo, quando falta um script chamado ou quando o validador reprova o script.
- [ ] Um teste unitário cobre:
  - a ida e volta do conteúdo embutido;
  - um exe sem nada embutido, que abre o app normal.
- [ ] Dois .exe exportados em Segundo plano rodam ao mesmo tempo, com o UltraTask aberto.
- [ ] "Parar" na bandeja e o atalho de pânico interrompem a execução e soltam teclas e botões presos.
- [ ] O README explica a exportação e avisa que antivírus podem estranhar executáveis de macro.
