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
| **Ticket de 48h corridas** | `R$ 12,00` | **1 jogo só**, levado "para casa": tempo **real**, vence mesmo desligado. Venceu → o cartucho não insere até devolver |
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
relógio corrido). O adesivo de prazo fica no cartucho. Venceu: no próximo
boot (ou em tempo real, se o app estiver aberto) o cartucho é marcado como
vencido — não insere até *Devolver*. **Sem multa na v1.**

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

O jogo identifica-se pelo **mesmo identificador** de saves/anotações/arte
(nome canônico No-Intro, ou nome de arquivo na falta de DAT — `domain::catalog`).

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
  "owned": ["<game-id>", ...],
  "service_game": "<game-id>" | null,
  "active_ticket": { "game": "<game-id>", "deadline_unix": 0 } | null,
  "hourly": { "game": "<game-id>", "swaps_used": 0, "active": false } | null,
  "trophies": [{ "id": "primeiro-turno", "unlocked_unix": 0 }],
  "stats": { "ms_served": 0, "rentals": 0, "swaps": 0, "purchases": 0,
             "days_rented": 0, "blackout_saves": 0, "ms_by_game": {} }
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

Multa por atraso, venda de jogos da coleção, troféus RetroAchievements,
empréstimo entre usuários, grana real, nome editável da locadora
(na v1 é um nome fixo só para a fantasia).

## 13. Testes (seguindo `cargo test` da casa)

Regras de dinheiro (creditor por minuto sem deriva), limite de 2 trocas,
ticket vencido recusa inserção e libera após devolução, corte de luz
preserva progresso da carteira, sorteio único, troféus disparam uma vez,
persistência sobrevive a boot sem acervo suficiente.

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
