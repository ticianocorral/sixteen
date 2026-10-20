# Design — Modo Locadora

> Gamificação opt-in do frontend: a locadora de bairro dos anos 90, dentro do
> SixteeN. Projeto pessoal, sem fins comerciais — todo o dinheiro é ficção.

## 1. Visão

O SixteeN já é nostalgia concreta: cartucho que desce e encaixa, Ejetar só
com o console desligado, TV fora do ar no CH 3. O Modo Locadora é o próximo
capítulo: o usuário vira freguês e funcionário de uma locadora de SNES.
Ele **trabalha** jogando o cartucho dele (o *cartucho em serviço*), fatura
grana fictícia por hora e decide se gasta em **locação** (testar vários
jogos) ou **junta e compra** um jogo para sempre — comprando destrava o que
a locadora esconde (contracapa, manual, caixa nova).

**Princípios:**

- **Opt-in por boot, saída fácil, zero castigo.** Quem só quer jogar joga no
  free play de sempre; quem entra na fantasia sai quando quiser sem perder
  nada. Carteira, tickets e troféus sempre persistem.
- **Uma moeda só.** "Reais" fictícios (símbolo configurável, default `R$`;
  nenhuma grana real, nenhuma compra de verdade — o projeto é sem fins
  comerciais).
- **Economia estável.** Todo jogo rende o mesmo por hora. Comprar é acesso +
  escolha do que trabalhar, nunca bola de neve.
- **A escassez é de verdade, mas nunca boba.** No modo, jogo não comprado
  precisa estar alugado para inserir. Saves nunca se perdem.

## 2. Tela inicial

A tela inicial (TV no CH 3) ganha nova hierarquia de botões, empilhados:

```
                 [ logo do app ]            ← como hoje
      ┌───────────────────────────┐
      │   ESTANTE DE GAMES        │ ← linha 2: free play (comportamento
      │      · FREE PLAY ·        │            de hoje, intocado)
      └───────────────────────────┘
      ┌───────────────────────────┐
      │   MODO LOCADORA           │ ← linha 3: nova porta
      └───────────────────────────┘
```

`MODO LOCADORA` exige **mínimo de 3 jogos** no acervo (`domain::catalog`).
Com menos, o botão responde: *"acervo insuficiente para abrir a locadora"*.

## 3. Tela inicial do modo

Ao entrar, a TV "sintoniza" a locadora: letreiro da locadora, balcão com a
caixa registradora (a carteira visível) e a prateleira com **todo o acervo**.
Cada jogo aparece num dos estados da §7. Da prateleira saem as portas:
**Hora avulsa**, **Ticket 48h**, **Loja** e **Devolver**.

## 4. Economia (uma moeda, cinco regras)

| Ação | Valor | Regra |
|---|---|---|
| **Salário do cartucho em serviço** | `+R$ 5,00/h` | Credita por minuto **proporcional**, só quando: console ligado **e** o cartucho em serviço está inserido. Acumulador por milissegundo jogado, sem deriva |
| **Hora avulsa** (qualquer jogo não comprado) | `R$ 8,00/h` | Roda só com console ligado; direito a **2 trocas de cartucho** durante a sessão |
| **Ticket de 48h corridas** | `R$ 12,00` | **1 jogo só**, levado "para casa": tempo **real**, vence mesmo desligado. Venceu → não insere até devolver; **+24h de atraso → devolução automática com multa de 30% do ganho do período** |
| **Compra na loja** | `R$ 60,00` | Permanente: caixa nova, contracapa, manual, e é elegível como *cartucho em serviço* |
| **Sorteio de boas-vindas** | grátis | Única vez: 3 jogos aleatórios do acervo → escolhe 1 → nasce **comprado de graça** e já entra *em serviço* |

- Carteira em **centavos** (`i64`), exibida como `R$ 12,34`.
- Comprar ≈ 5 tickets: junção de ~12h de trampo por jogo permanente.
- Hora avulsa custa **mais** que o salário: a freguesia líquida custa
  `−R$ 3/h`; o trampo rende `+R$ 5/h`.
- **Corte de energia da carteira:** na hora avulsa, zerando o caixa com o
  console ligado o CRT "apaga no meio da partida" e abre o pedido: *devolver*
  (encerra a sessão) ou *trocar de cartucho* (consome uma das 2 trocas).
  Saves ficam intactos — devolveu, alugou de novo, continua de onde parou.
- Todos os valores são **editáveis no cfg** também (`config/config.cfg`).

## 5. Cartucho em serviço

- **1 jogo por vez** é o *em serviço* (o pagador).
- A coleção é composta por: o sorteado de boas-vindas + tudo comprado na loja.
- Qualquer jogo da coleção pode ser titulado *em serviço*, na prateleira.
- Só o cartucho em serviço **inserido e com console ligado** paga.

## 6. Ciclos de locação

### Hora avulsa (o turno no balcão)
Escolhe 1 jogo do acervo (não comprado), paga conceito de hora (debita
proporcional a cada minuto com console ligado). Durante a sessão pode
**trocar o cartucho 2 vezes** (ejeta, devolve o atual à prateleira, escolhe
outro). Encerra com *Devolver* — os 3 jogos da sessão voltam à prateleira
com etiqueta gasta de novo.

### Ticket de 48h (levando para casa)
Escolhe 1 jogo, paga `R$ 12,00`, recebe **prazo real** (agora + 48h de
relógio corrido). O adesivo de prazo fica no cartucho. Venceu: o cartucho
não insere mais até *Devolver*.

**Devolução tardia (multa):** passado o prazo, o adesivo vira
"ATRASADO". O jogador pode devolver manualmente nesse estado — ou a
locadora **devolve automaticamente 24h depois do prazo**, levando o
cartucho de volta. Nos dois casos a multa é **30% de tudo que a carteira
ganhou durante o período do ticket** (na prática, o salário do cartucho
em serviço nos dias locados — o ticket acumula esse ganho em
`earned_cents`). A carteira nunca fica negativa: desconta o que houver.
Devolução tardia **não conta** para o cartão de fidelidade e rende o
troféu "Devolução Atrasada".

## 7. Física dos cartuchos e caixas

Desenhado por cima da arte do usuário (`assets/{cover,logo,cartridge,backcover}/`),
com overlays procedurais — **sem arte nova por jogo**:

**📼 Cartucho de locadora (não comprado):** label desbotada/amarelada, cantos
rasgados, remendo de fita adesiva amarelada e a faixa clássica
**"PROPRIEDADE DA LOCADORA — NÃO DOBRAR"** com carimbo de número do
exemplar. A caixa acompanha o desgaste. **Sem manual, sem contracapa**: no
lugar da contracapa, a caixa aberta mostra a **ficha de catálogo** (fichinha
datilografada: nome, ano de fábrica, gênero, "carimbado por").

**⏰ Cartucho em ticket:** por cima, o **adesivo branco de prazo**
datilografado ("DEVOLVER ATÉ 18/10"). Vencido: adesivo carimbado
`DEVOLVIDO` até a devolução.

**✨ Cartucho comprado (label nova):** na compra, a cerimônia — paga no
balcão (gaveta abre, grana some), o cartucho gasto desce da prateleira, a
etiqueta velha **descasca** (peel), entra a label nova lisa com a arte da
capa. Dele para sempre: caixa nova, **contracapa** com arte completa e
**manual** browseável (scans do usuário em `assets/manual/<jogo>/`, folheados
como livro; segue o padrão de arrasta-png da casa).

O jogo identifica-se pelo **sha1 da ROM** — a mesma chave que o
`library.json` (`RomRow.sha1`, sobrevive a renomes de arquivo); *títulos*
são resolvidos na exibição via `CatalogEntry::title()`.

## 8. Primeira visita (onboarding, uma vez só)

1. CH 3 → clica `MODO LOCADORA` → TV sintoniza a locadora, carteira `R$ 0,00`.
2. **Sorteio de boas-vindas:** 3 cartuchos no spotlight → escolhe 1 →
   cerimônia de label nova (vê o que comprar te dá um dia) → nasce *em serviço*.
3. Power o console ligado, joga; cartão de ponto tiquetaca `+R$0,08/min` (500/60).
4. Pausou? Ejetou → o balcão: hora avulsa, ticket 48h, loja ou devolver.
5. Primeiros troféus pipocam ("Bem-vindo à ficha", "Primeiro turno"...).

Da segunda visita em diante: entra direto, com carteira, coleção e tickets
persistidos. O sorteio é único na vida.

## 9. Troféus (v1 = troféus da locadora)

Parede de troféus na locadora: mini-sprites no estilo da casa, cada um num
pedestal com cartão (título, jogo, data). Gatilhos determinísticos, locais:

- **Bem-vindo à ficha** — sorteou e escolheu o primeiro jogo
- **Primeiro turno** — 1h acumulada no cartucho em serviço
- **Vizinho do acervo** — primeira hora avulsa
- **Troca-troca** — primeira troca de cartucho
- **Prazo cumprido** — devolveu um ticket dentro do prazo
- **Colegão** — 5 jogos na coleção
- **Lojista** — 10 jogos comprados
- **Freguês fiel** — locou em 7 dias distintos
- **Sobrevivente da estática** — voltou ao trampo depois do tubo apagar
- **Devolução Atrasada** — devolveu (ou foi devolvido automaticamente)
  depois do prazo. Não conta para o cartão de fidelidade. *"Todo mundo
  devolve tarde uma vez."*

V2 (fora do escopo): espelhar conquistas RetroAchievements como troféus.

## 10. Persistência

`config/locadora.json` (lado do `library.json`, mesmo padrão de escrita
atômica da casa):

```jsonc
{
  "version": 1,
  "wallet_cents": 0,
  "lifetime_earned_cents": 0, "lifetime_spent_cents": 0,
  "intro_done": false,
  "owned": ["<sha1>", ...],
  "service_game": "<sha1>" | null,
  "active_ticket": { "game": "<sha1>", "deadline_unix": 0, "earned_cents": 0 } | null,
  "hourly": { "game": "<sha1>", "swaps_used": 0, "active": false } | null,
  "trophies": [{ "id": "primeiro-turno", "unlocked_unix": 0 }],
  "stats": { "ms_served": 0, "rentals": 0, "swaps": 0, "purchases": 0,
             "days_rented": 0, "blackout_saves": 0, "ms_by_game": {},
             "returns_late": 0, "on_time_streak": 0 },
  "rentals_per_game": { "<sha1>": 3 },
}
```

## 11. Integração técnica (tentativa, refinada no plano)

- **Nova crate `crates/locadora`**: domínio puro e testável — carteira,
  contratos de locação, regras de troca, prazos, troféus. Sem SDL. Depende de
  `sixteen-domain` (identidade de jogo) e nada mais da UI.
- **`app`** (config.rs, shelf.rs, console_art.rs, runner.rs): ligação —
  tela inicial nova, prateleira da locadora, estados visuais (§7), leitura de
  tempo de jogo (o app já mede ms por jogo com console ligado) alimentando o
  creditor de minutos.
- **`settings.rs` / config**: preços, símbolo da moeda, mínimo de jogos.

## 12. Fora do escopo da v1

Multa simples virou **devolução tardia com multa de 30% (§6)**. Fora do
escopo da v1: venda de jogos da coleção, troféus RetroAchievements,
empréstimo entre usuários, grana real, nome editável da locadora
(na v1 é um nome fixo só para a fantasia), reembolso de jogo apagado
(a locadora não paga por jogo que sumiu da estante).

## 13. Testes (seguindo `cargo test` da casa)

Regras de dinheiro (creditor por minuto sem deriva), limite de 2 trocas,
ticket vencido recusa inserção e libera após devolução, corte de luz
preserva progresso da carteira, sorteio único, troféus disparam uma vez,
persistência sobrevive a boot sem acervo suficiente. **Reconciliação de
acervo:** jogo removido sai da coleção e encerra seu ticket/sessão/em
serviço (sem reembolso); carteira, troféus e globais persistem.
**Devolução tardia:** automática em prazo+24h (ou manual atrasada) com
multa de 30% do `earned_cents` do ticket, carteira nunca negativa.

## 14. Registro de decisões (conversa com o autor)

1. Moeda **100% fictícia** (R$ default, símbolo no cfg).
2. **Jogar o em serviço PAGA**; gasta-se locando ou comprando.
3. **1 pagador por vez**; comprado pode ser eleito; nunca snowball.
4. **Todo o acervo é locável** por créditos; a loja é o caminho da propriedade
   (custa mais, é permanente).
5. Trocas (2×) pertencem à **hora avulsa**; ticket 48h é **1 jogo**, tempo real.
6. Desgaste também na **caixa**; locadora esconde **manual e contracapa** —
   comprar destrava (incentivo de compra).
7. Tela inicial: estante = **FREE PLAY** na segunda linha; **MODO LOCADORA**
   abaixo. Mínimo de 3 jogos (proposto, configurável).
8. **Apagou o jogo, zerou o progresso dele**: sai da coleção, ticket/sessão/
   em serviço encerram, sem reembolso — a carteira e os troféus ficam.
   Reconciliação roda no boot e ao abrir o balcão (§16).
9. **Devolução tardia**: +24h do prazo → devolução automática com multa de
   **30% do ganho do período do ticket** (`earned_cents`); devolução
   manual atrasada paga a mesma multa. Carteira nunca negativa.
10. **Extras aprovados da v1** (§17): cicatriz da estante, salário dobrado
    no fim de semana, cartão de fidelidade, cartaz da semana, troféu
    "Devolução Atrasada".

## 17. Extras da v1 (aprovados)

- **🩹 Cicatriz da estante:** `rentals_per_game` por sha1; a intensidade do
  overlay de desgaste (§7) escala com o contador. O exemplar mais alugado
  vira a lenda da prateleira.
- **📈 Salário dobrado no fim de semana:** sábado/domingo o cartucho em
  serviço paga `10,00/h` (dia do relógio do sistema; liga/desliga no cfg).
- **🎟️ Cartão de fidelidade:** 5 devoluções no prazo (`on_time_streak` = 5)
  rendem 1 cupom de locação grátis (avulsa ou ticket). Devolução atrasada
  zera a sequência. Carimbos datilografados no balcão.
- **🖼️ Cartaz da semana:** todo domingo, seed pelo número da semana escolhe
  1 jogo em destaque — locação pela metade (avulsa `4,00/h`, ticket
  `6,00`), com cartaz desenhado na vitrine. Determinístico pela data.
- **😅 Troféu "Devolução Atrasada":** ver §6/§9.

## 16. Regra do acervo — jogo removido, progresso zerado

Quando ROMs são apagadas de `roms/` enquanto o modo existe, a locadora
faz a **reconciliação** no boot e nas entradas do balcão:

- O jogo que sumiu **sai da coleção** (`owned`): sem ROM, sem caixa —
  a cópia também era física. **Sem reembolso.**
- **Cartucho em serviço** removido → cargo vago (elege outro quando quiser).
- **Ticket ativo** de jogo removido → devolvido *sem* contar prazo cumprido
  e *sem* devolver grana (não foi devolução, foi sumiço).
- **Hora avulsa** cujo cartucho remane → sessão encerrada.
- **Carteira, troféus e contadores globais persistem** — grana do freguês
  não é do jogo. Troféus já conquistados são para sempre; os contados
  (Colegão/Lojista) apenas deixam de ser re-audiáveis.
- **Borda honesta:** reconciliação acontece em boot/balcão; sessão em curso
  com o jogo carregado termina normal, e o estado fecha no próximo bilhete.
