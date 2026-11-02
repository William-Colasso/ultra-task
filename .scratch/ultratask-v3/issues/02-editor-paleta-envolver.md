# 02: Editor: paleta de blocos, "Envolver em…" e recolher containers

**O que construir:**
- **Paleta.** Ao lado da árvore de blocos, uma paleta lista todos os tipos de bloco, agrupados pelas categorias e cores do ticket 01.
  - Arrastar um item da paleta para qualquer vaga da árvore insere ali um bloco novo.
  - Clicar num item o acrescenta no fim.
  - O antigo menu "➕ bloco" deixa de existir.
- **Envolver.** O menu de contexto (botão direito) de um cartão oferece "Envolver em Se / Repetir / Enquanto", que põe o bloco dentro de um container novo, no mesmo lugar.
- **Recolher.** Containers podem ser recolhidos e expandidos. Recolhidos, mostram quantos blocos têm dentro.

**Bloqueado por:** 01 (Editor: arrastar blocos para dentro e para fora de containers).

**Status:** ready-for-agent

Leia antes: `../CONVENCOES.md`.

- [ ] Todos os tipos de bloco aparecem na paleta, por categoria.
- [ ] Arrastar da paleta para uma vaga aninhada vazia insere o bloco novo nela.
- [ ] "Envolver em…" mantém a posição do bloco e o põe no corpo (ou no "então") do container novo. A operação tem teste unitário.
- [ ] Recolher e expandir só mudam a tela; o JSON do script não muda.
- [ ] O teste de glifos passa.
