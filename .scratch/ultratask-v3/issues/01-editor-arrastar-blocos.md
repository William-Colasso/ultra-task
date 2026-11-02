# 01: Editor: arrastar blocos para dentro e para fora de containers

**O que construir:** Na aba Blocos, cada bloco vira um cartão com uma alça de arrastar e uma cor por categoria:
- **ação:** Clique, Mover, Tecla, Digitar, Rolar, gravação;
- **espera:** Esperar, Esperar até;
- **controle:** Se, Repetir, Enquanto, Chamar script, Parar;
- **variáveis:** Definir variável, Ler texto;
- **depuração:** Mostrar no log, Breakpoint.

Arrastar um cartão e soltá-lo entre outros cartões, ou numa vaga de container ("então"/"senão" de Se, corpo de Repetir/Enquanto), move o bloco para lá. As vagas aparecem mesmo vazias ("solte blocos aqui"), e um indicador mostra onde o bloco vai cair. Soltar um container dentro dele mesmo ou de um descendente é recusado.

Este ticket só move blocos que já existem; criar blocos arrastando é o ticket 02.

**Bloqueado por:** Nenhum (pode começar já).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Mover um bloco para frente ou para trás na mesma lista o deixa exatamente onde o indicador mostrou.
- [ ] Um bloco pode ir para o corpo vazio de um Repetir, para o "então" e o "senão" de um Se, e voltar ao nível de cima.
- [ ] Soltar um container nele mesmo ou num descendente é recusado: o indicador mostra que não pode, e a árvore não muda.
- [ ] Mover marca o script como alterado (💾 Salvar), e o JSON salvo guarda o novo aninhamento.
- [ ] 📍, Explodir, duplicar e excluir continuam funcionando em blocos aninhados.
- [ ] As cores funcionam nos temas claro e escuro.
- [ ] A operação de mover na árvore é uma função pura, fora da UI, com testes unitários para estes casos:
  - mesma lista, para frente e para trás;
  - para dentro de uma vaga filha;
  - para fora, até o pai;
  - recusa de soltar num descendente.
