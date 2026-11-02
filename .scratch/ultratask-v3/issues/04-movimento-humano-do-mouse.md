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

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] O gerador de trajetos é uma função pura, com semente. Os testes cobrem:
  - começa na posição atual e termina exatamente no alvo;
  - fica num envelope limitado ao redor da reta;
  - não é uma reta em distâncias longas;
  - a duração fica numa faixa coerente com a distância;
  - sementes diferentes dão trajetos diferentes.
- [ ] Teste do interpretador com o Io falso: com a opção ligada, um Clique gera vários movimentos antes do botão, e o último é o ponto do clique. Com a opção desligada, a sequência de eventos é a mesma de hoje.
- [ ] Parar durante um trajeto longo interrompe em até ~50 ms.
- [ ] O validador aceita o campo, a skill o documenta (com snippet validado por `--validate`) e o README o menciona.
- [ ] Verificação manual: no modo Foco, um arrasto no Paint deixa um traço curvo.
