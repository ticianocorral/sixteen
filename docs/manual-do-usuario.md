# SixteeN — Manual do Usuário

*Compatível com a versão 1.6.x do app. Última atualização deste manual: 31/10/2026.*

Bem-vindo! Este manual explica, passo a passo e sem termos técnicos, como
usar o SixteeN: um console de Super Nintendo inteiro desenhado na
tela do seu computador — com a TV de tubo, o painel de chaves e a estante
de cartuchos. Se você já usou um SNES de verdade, vai se sentir em casa.

---

## Sumário

1. [O que é o SixteeN](#1-o-que-é-o-sixteen)
2. [Instalar o app](#2-instalar-o-app)
3. [Primeira execução](#3-primeira-execução)
4. [Colocando seus jogos](#4-colocando-seus-jogos)
5. [A tela inicial](#5-a-tela-inicial)
6. [A estante de jogos](#6-a-estante-de-jogos)
7. [Jogando](#7-jogando)
8. [Salvando seu progresso](#8-salvando-seu-progresso)
9. [Anotações: o livro de pausa](#9-anotações-o-livro-de-pausa)
10. [Cheats (códigos de trapaça)](#10-cheats-códigos-de-trapaça)
11. [Conquistas (RetroAchievements)](#11-conquistas-retroachievements)
12. [Configurações](#12-configurações)
13. [Atualizações](#13-atualizações)
14. [Curiosidade: o modo Dev](#14-curiosidade-o-modo-dev)
15. [Onde ficam seus arquivos](#15-onde-ficam-seus-arquivos)
16. [Problemas e soluções](#16-problemas-e-soluções)
17. [Jogar só com o controle](#17-jogar-só-com-o-controle)

---

## 1. O que é o SixteeN

É um **emulador**: um programa que faz seu computador se comportar como um
Super Nintendo. A diferença para os emuladores comuns é que aqui o app
inteiro é desenhado como um console de verdade:

- Uma **TV de tubo** (com aquele visual de TV antiga, "chiando") onde o
  jogo aparece.
- Um **painel lateral** com as chaves **POWER** e **RESET**, o botão
  **EJETAR** e o slot onde o cartucho do jogo entra com animação e som.
- Uma **estante de jogos** com capas, busca, favoritos e histórico.

**Importante:** o app **não vem com jogos**. Ele é só o console — os jogos
(arquivos chamados de *ROMs*) você coloca você mesmo, igual aos cartuchos
de antigamente.

Funciona com **mouse e controle (gamepad)** para quase tudo. O teclado é
usado só para jogar e para digitar textos (anotações e buscas).

---

## 2. Instalar o app

Os pacotes prontos ficam na aba
[Releases](https://github.com/ticianocorral/sixteen/releases) do
projeto no GitHub. Baixe o arquivo do seu sistema:

| Sistema | Arquivo | Como instalar |
|---|---|---|
| macOS | `SixteeN-…-macos.dmg` | Abra o .dmg e arraste o **SixteeN** para a pasta Aplicativos |
| Windows | `SixteeN-…-windows-x86_64.zip` | Descompacte o zip numa pasta de sua escolha e execute `sixteen.exe` |
| Linux | `SixteeN-…-linux-x86_64.AppImage` | Marque o arquivo como executável e dê dois cliques nele |

Não precisa instalar mais nada: o app já traz consigo tudo o que precisa
para abrir e desenhar a tela.

> **O app se atualiza sozinho** — a partir da versão baixada, as próximas
> atualizações aparecem dentro do próprio app (veja a
> [seção 13](#13-atualizações)).

---

## 3. Primeira execução

Na primeira vez que abrir, o app **cria sozinho as pastas** onde tudo será
guardado (jogos, saves, configurações):

- **macOS:** dentro de **Documentos**, na pasta `SixteeN`
- **Windows:** na mesma pasta onde o `sixteen.exe` está
- **Linux:** em `~/.local/share/SixteeN`

Você vai receber as boas-vindas na própria TV: a tela
**"bem-vindo ao sixteen"** avisa que faltam dois downloads — é só
clicar nos botões ali mesmo:

1. **"baixar núcleo snes9x"** — o *núcleo* é o "cérebro" que roda os
   jogos. **É obrigatório.** O app baixa e instala sozinho.
2. **"baixar nointro.dat"** — um catálogo de nomes oficiais dos jogos
   (com ano e editora). **É opcional**, mas deixa a estante bonita e com
   os nomes certos.

Os dois botões viram **"instalado"** em verde quando terminam. Aí é só
clicar em **"continuar"**.

> Sem internet? Dá para fazer depois: o download do núcleo também fica no
> menu **Configurações → sistema**, e o arquivo do núcleo pode ser
> colocado à mão na pasta `core/` (veja [seção 16](#16-problemas-e-soluções)).

---

## 4. Colocando seus jogos

Os jogos são arquivos de ROM do Super Nintendo. Para adicioná-los:

1. Abra a pasta do app (a mesma que o app criou na primeira execução —
   no macOS fica em **Documentos/SixteeN**).
2. Copie suas ROMs para dentro da subpasta **`roms/`**.
3. Volte ao app e clique em **"atualizar"** na estante (ou feche e abra a
   estante de novo).

**Formatos aceitos:** `.sfc`, `.smc`, `.fig`, `.swc`, `.bs`, `.st`,
`.bin` e `.zip` (com a ROM dentro). Se o jogo estiver zipado, não precisa
descompactar.

**A estante aceita subpastas:** pode organizar por gênero, ano, ou do
jeito que quiser dentro de `roms/` — o app encontra tudo.

### Deixando a estante bonita (opcional)

Cada jogo pode ganhar arte sua. Dentro de `assets/` há pastas para isso —
basta nomear a imagem **exatamente igual ao arquivo da ROM** (sem a
extensão):

| Pasta | O que mostra |
|---|---|
| `assets/cover/` | Capa na estante (desenhada deitada, estilo paisagem) |
| `assets/logo/` | Logo do jogo no painel lateral |
| `assets/cartridge/` | O cartucho que entra no slot do console |
| `assets/backcover/` | A contracapa (versão de trás da caixa) |
| `assets/manual/` | O manual do jogo em PDF, para folhear na TV |

Exemplo: para o jogo salvo como `roms/Super Mario World (USA).sfc`, a capa
deve se chamar `assets/cover/Super Mario World (USA).png` (ou `.jpg`).

Sem arte o app funciona normalmente: o jogo aparece na estante como um
cartucho cinza genérico, com uma etiqueta de papel colada e o nome e o ano
escritos à caneta. Cada jogo tem sempre a mesma letra, a mesma caneta e o
mesmo durex.

---

## 5. A tela inicial

Esta é a "sala do console": a TV fora do ar (aquela estática com o selo
verde **CH 3**) e o painel sem cartucho. Nela você encontra:

- **"Estante de games"** — o botão principal, na altura do slot. Abre a
  estante para escolher um jogo (é o equivalente a pegar um cartucho na
  estante).
- **"Manual do console"** — logo acima de Configurações: abre no tubo o
  manual do próprio SixteeN (10 páginas, no idioma da interface). As setas
  ou os botões **< página**/**página >** folheiam; **voltar** fecha.
- **"Configurações"** — no pé do painel.
- Os **controles da TV** no queixo: **POWER** liga e desliga o tubo,
  **VOL –/+** muda o volume e o **seletor de canal** — o botão giratório
  com o 3 e o 4 impressos — troca o canal com um clique. O console sai no
  canal 3; no **canal 4** a TV pega só a borda do sinal, como no canal
  vizinho do modulador de verdade — imagem fraca e com neve, cor sumindo,
  a imagem rolando de vez em quando e o som abafado. Dá para jogar assim,
  se quiser a nostalgia completa.
- **MENU** abre o menu de imagem da TV, em letras verdes: **brilho**,
  **contraste**, **cor** e **nitidez**. Cada toque no MENU passa para o
  próximo item (depois do último, fecha); com o menu aberto, **VOL –/+**
  ajustam o item em vez do volume. Os ajustes mexem no filtro da imagem do
  jogo de verdade e ficam salvos. O menu fecha sozinho depois de alguns
  segundos parado. (A tecla MENU fica à esquerda do POWER e só aparece quando
  a janela tem espaço no queixo.)
- O vidro estala de leve algumas vezes nos primeiros minutos depois de
  ligar a TV, enquanto esquenta.
- O vidro da TV tem um brilho de luz: uma mancha difusa no alto à
  esquerda, uma faixa diagonal e um aro claro na borda, acompanhando a
  curva do tubo (ligada ou desligada).
- O **nome do console** no queixo da TV, com a versão do app e do núcleo.
  **Setas verdes** acendem ao lado quando existe atualização do app ou do
  núcleo — clique nelas para atualizar (veja a [seção 13](#13-atualizações)).
- O **X** no canto fecha o app; o **–** minimiza.

> Com um controle conectado: aperte **A** para abrir a estante. O botão
> **B** na tela inicial encerra o app.

---

## 6. A estante de jogos

A estante aparece **dentro da TV**, como uma gaveta de cartuchos. De cima
para baixo:

- **Favoritos** — os jogos que você marcou com estrela (o painel da
  direita tem o botão **"Favoritar"**; a estrela aparece no canto da capa).
- **Jogados recentemente** — os últimos 8 jogos que você rodou.
- **Todos os jogos** — a coleção completa, em ordem alfabética.

### Navegação

| Ação | Mouse | Controle |
|---|---|---|
| Escolher jogo | Clique para selecionar; clique de novo para abrir | Direcionais para andar, **A** para abrir |
| Voltar à tela inicial | Botão **"Voltar"** | **B** |
| Passar página | Setinhas das faixas | LB / RB |
| Rolar a lista | Roda do mouse | Direcionais |

### Busca

Clique na caixa **"buscar..."** no alto da estante e digite (aqui o
teclado é usado). A busca filtra enquanto você digita e ignora maiúsculas
e minúsculas. Clicar com o **botão direito** na caixa cola o texto que
estiver copiado. Para limpar o filtro, apague o texto e aperte Enter.

### O painel de detalhes

Ao selecionar um jogo, o painel da direita mostra as informações dele em
**abas**:

- **informações** — ano de lançamento, editora, desenvolvedor, gênero,
  tamanho do cartucho ("32 megas"), tempo total que você jogou, e o andar
  das conquistas (se você usa o RetroAchievements).
- **capa traseira** e **cartucho** — clique na imagem para ver em tela
  cheia.
- **manual** — se você colocou um PDF do manual em `assets/manual/`, dá
  para **folhear o manual na TV**: tem botão de avançar e voltar página.

### Histórico

O botão **"histórico"** mostra o ranking dos seus jogos **mais jogados**,
com o tempo de cada um. O tempo só conta com o console **ligado** — igual
a um SNES de verdade, ficar com o app aberto não soma tempo.

### Sugestão da noite (opcional)

Sem saber o que jogar? Ligue **Configurações → estante → Sugestão da
noite** e aparece o botão **"sugestão"** no cabeçalho da estante (à
esquerda do "álbum"). Ele sorteia um jogo com a ROM na biblioteca,
dando preferência aos que você **nunca abriu** e aos que estão **parados
há mais tempo**; jogo com pouco tempo de jogo também ganha uma ajuda. O
cartão mostra a capa, o título e o motivo ("nunca jogado", "faz 3 meses
que você não joga", "só 12 min jogados"). **A** joga, **X** sorteia outra
(nunca repete a anterior na hora) e **B** fecha — o mouse usa os botões
do cartão. Desligada (o padrão), o botão não aparece.

### Álbum de prints

O botão **"álbum"** (ou **Y** no controle, direto na estante) abre o
**álbum de prints**: todas as capturas de tela que você guardou com o botão
**Print** do painel, de **todos os jogos**, como fotos de polaroide numa
página de couro — com a legenda que você escreveu e o nome do jogo embaixo.

- **direcional** — move entre as fotos; **A** abre a foto grande.
- **L / R** — troca o filtro: todos os jogos, ou só um jogo de cada vez.
- Na foto grande, **esquerda/direita** (ou **L/R**) passam as fotos e
  **B** volta para a grade; **B** na grade volta para a estante.
- Com mouse ou toque: clique numa foto para abrir, use as setas **<** **>**
  na tela (foto grande e filtro), e clique fora da foto para fechar.

Sem nenhum print ainda, o álbum mostra uma dica: tire prints durante o
jogo com o botão **Print** do painel. O álbum só mostra as fotos; para
apagar ou trocar uma, use o livro de pausa (seção 9).

> **Estante vazia?** Se aparecer "nenhuma rom encontrada", é porque a
> pasta `roms/` está vazia (ou o app está apontando para outra pasta —
> veja a [seção 15](#15-onde-ficam-seus-arquivos)).

---

## 7. Jogando

O ritual é o de um console de verdade:

1. Na estante, clique no jogo — **o cartucho entra no slot** com
   animação e o clique de encaixe (se o jogo tiver arte de cartucho).
2. A TV fica fora do ar: o console está **desligado**.
3. Clique na chave **POWER** — o console liga e o jogo aparece na TV.
4. Para **pausar e mexer nas anotações**, clique em **"Anotações"** na
   lista de comandos (veja a [seção 9](#9-anotações-o-livro-de-pausa)).
5. Para sair: desligue com **POWER** e clique em **EJETAR** — o cartucho
   sai e você volta à tela inicial.

### As chaves do painel

| Chave | O que faz |
|---|---|
| **POWER** | Liga e desliga. **Desligar não perde o lugar**: religar o console retoma o jogo exatamente de onde parou (enquanto o app estiver aberto). |
| **RESET** | Reinicia o jogo do começo (a chave volta sozinha, como no SNES real). |
| **EJETAR** | Tira o cartucho e volta à tela inicial. **Só funciona com o console desligado** — com ele ligado, o botão "trava" de propósito, para você não perder progresso. |

No rodapé do painel há o cronômetro **"sessão"**: é o tempo que você está
jogando *agora*. Ele soma no tempo total do jogo quando você sai.

### Controles do jogo

**Teclado (padrão):**

| Tecla | Botão do SNES |
|---|---|
| Setas | Direcional (d-pad) |
| **Z** | B |
| **X** | A |
| **A** | Y |
| **S** | X |
| **Q** | L |
| **W** | R |
| **Enter** | Start |
| **Shift direito** | Select |

**Controle (gamepad):** os botões seguem o padrão moderno — direcional,
**Sul** = B, **Leste** = A, **Oeste** = Y, **Norte** = X, LB = L, RB = R,
**Back** = Select e **Start** = Start. Se o seu controle tiver nomes
diferentes, dá para remapear em **Configurações → controles**.

Dá para conectar até **2 controles** — o teclado é sempre o jogador 1.

**Menu do jogo só com o controle (Steam Deck inclusive):** o jogo usa todos
os botões, então o menu abre com um acorde — **segure Select + Start por
cerca de 1 segundo**. O jogo congela (e não recebe o Select/Start) e aparece
o **Menu do jogo**: Continuar, Salvar estado, Carregar estado, Print,
Anotações, Conquistas (com a conta RetroAchievements ativa), Reset,
Desligar (ou Ligar), Ejetar e Sair do app. Uma dica ("segure Select+Start =
menu") aparece na TV logo que o jogo começa.

Com um menu ou um seletor de slots aberto: **direcional** move o destaque
(←/→ trocam de coluna na lista de 15 prints), **A** escolhe, **B** cancela e
**LB/RB** saltam de página nas listas longas (ex.: conquistas). Nenhum
botão do Steam, grip traseiro ou trackpad é necessário.

### Mau contato: soprar o cartucho

Opcional e desligado por padrão — ligue em **Configurações → jogo → Mau
contato do cartucho** (*raro* ≈ 1 encaixe em 15, *anos 90* ≈ 1 em 5).

Com ele ligado, às vezes o cartucho entra mal encaixado e o primeiro
**POWER** não sobe o jogo: tela preta com zumbido, lixo de tiles colorido,
a abertura travando com um "bzzzt" em loop, ou o jogo rodando com os
gráficos embaralhados (sprites e cenários cheios de lixo) até travar. Para
resolver, faça o ritual:

1. Desligue no **POWER** e clique em **EJETAR**.
2. O cartucho sai do slot e vem para a frente da TV, com os contatos
   virados para você.
3. **Segure o clique** (ou **Y**/**B** no controle) para soprar: a poeira
   sai e os pinos passam de foscos a dourados. Fique de olho na barra de
   **fôlego** — soprar até o fim deixa você sem ar e **embaça os pinos**,
   e pino embaçado encaixa pior. Sopros curtos funcionam melhor.
4. Clique em **Inserir de novo** (ou **A**/**Start**) e ligue.

**Soprando de verdade:** ligue **Configurações → jogo → Soprar no
microfone** e sopre no microfone do aparelho — no Steam Deck e no ROG Ally
o microfone fica na própria carcaça, então é só soprar no portátil. Uma
barrinha **MIC** mostra o que ele está ouvindo; o clique e o Y/B continuam
valendo. O microfone só é aberto durante a cena do sopro e nada é gravado.
No macOS o sistema pede permissão na primeira vez; sem microfone (ou sem
permissão), a cena funciona só com o clique.

Desligar e ligar de novo, sem ejetar, às vezes também resolve. Se
preferir desistir, **Guardar na estante** (ou **Select**) tira o cartucho
como o EJETAR de sempre.

Para não irritar: a falha só acontece até o jogo subir uma vez — depois
de um boot bom, desligar e religar com POWER nunca falha —, nunca há mais
de **duas falhas seguidas**, e o seu save não é tocado: durante a falha o
jogo não conta tempo, não salva e não ganha conquistas.

---

## 8. Salvando seu progresso

O SixteeN cuida disso de dois jeitos:

### 1. O save do próprio jogo (automático)

Igual ao cartucho real: quando o jogo tem memória de bateria (a maioria
dos RPGs e jogos com "Save" no menu), o app grava essa memória
**sozinho**, a cada ~10 segundos e sempre que você desliga, ejeta ou
fecha o app. Você não precisa fazer nada — é o "Salvar" de dentro do jogo
de sempre.

### 2. Save states (a "foto" do momento)

Uma foto completa do jogo em qualquer instante — até no meio de um pulo.
No painel, na lista de **"comandos"**:

- **"Salvar"** abre a lista de slots (0 a 9). Clique num slot para gravar.
- **"Carregar"** abre a mesma lista; slots vazios aparecem apagados.

Cada jogo tem seus próprios 10 slots. Os arquivos ficam na pasta
`saves/` (veja a [seção 15](#15-onde-ficam-seus-arquivos)).

> ⚠️ **Com o modo hardcore das conquistas ligado**, save states e cheats
> ficam bloqueados (aparece "MODO HARDCORE — savestates bloqueados"). Veja
> a [seção 11](#11-conquistas-retroachievements).

---

## 9. Anotações: o livro de pausa

Clique em **"Anotações"** nos comandos: o jogo pausa e abre um **livro de
duas páginas** por cima da TV.

**Página esquerda — anotações de texto.** São 15 slots (01 a 15) para
dicas suas, senhas de fase, o que quiser:

- **"Escrever"** (ou "Editar") abre a digitação — o teclado entra em ação;
  Enter salva, Esc cancela. Cada anotação cabe em 240 caracteres.
- **"Fixar"** destaca uma anotação: o que está fixado aparece no painel
  do console enquanto você joga, em "notas fixadas" — o print grande, numa
  moldura com o nome embaixo, e o texto inteiro num papel amarelo. Se o
  painel não tiver espaço para tudo, a foto encolhe e o texto termina em
  "...".
- **"Apagar"** limpa o slot (um slot fixado não apaga — desfixe primeiro).

**Página direita — álbum de prints.** Também são 15 slots, agora de
capturas de tela do jogo:

- **"Fixar"** deixa o print visível no painel durante o jogo.
- **"Nomear print"** dá uma legenda de até 40 letras.

**Como capturar um print:** clique em **"Printscreen"** nos comandos. O
app pergunta **"Onde salvar o print?"** — escolha o slot, dê um nome se
quiser, e pronto. Se os 15 slots já estiverem ocupados, o botão avisa
("Printscreen: sem espaço").

Para voltar ao jogo, clique em **"Continuar"**.

**No controle:** abra o livro pelo Menu do jogo (Select + Start →
"Anotações"). **←/→** passam os prints, **LB/RB** passam as anotações de
texto, **X** fixa o print, **Y** nomeia, **Start** escreve a anotação,
**Select** fixa a anotação e **B** volta ao jogo. Ao escrever, **Start**
salva e **B** cancela.

> Os prints são capturas da imagem de verdade do jogo — ficam salvos como
> PNG na pasta `notes/` e podem ser usados fora do app também.

---

## 10. Cheats (códigos de trapaça)

Os cheats são escolhidos no **Cheat Genius**, uma tela própria, como um
adaptador dos anos 90 no slot do console. Ao apertar **POWER** (e de novo a
cada **RESET**), o Cheat Genius carrega e lista os códigos do jogo:

| Botão | Faz |
|---|---|
| ↑ ↓ (segure para correr) · L/R | Move pela lista · pula uma página |
| **A** | Liga/desliga o código |
| **START** | Joga com os códigos marcados |
| **B** | Cancela: joga **sem nenhum** (o que estava salvo continua salvo) |
| **Y** | Digita um código novo (não precisa estar no banco) |
| **X** | Marca um código seu para apagar (de novo desmarca) |

No teclado de códigos aceita Game Genie (`C24F-74D4`), RAM (`7E036999`) e
vários juntos com `+`. Código inválido é recusado na hora. Seus códigos
ficam guardados por jogo e voltam no topo da lista. Para mudar os cheats
durante o jogo, aperte **RESET** — como no aparelho de verdade.

Os cheats ligados ficam contados no painel ("2 cheats ativados") e a
escolha fica guardada: na próxima vez que jogar, os mesmos cheats voltam
marcados.

No **hardcore** das conquistas não há cheats: o Cheat Genius não aparece e
o jogo liga direto.

Os cheats vêm de um banco embutido no app (curado a partir do projeto
libretro) — não precisa baixar nada. Ainda assim, **não existe cheat para
todo jogo**: se a lista vier vazia, é que não há códigos para aquele
título — dá para digitar os seus com **Y**.

---

## 11. Conquistas (RetroAchievements)

O SixteeN se conecta ao [RetroAchievements](https://retroachievements.org)
— um serviço gratuito que cria **conquistas para jogos retrô** ("termine a
fase sem perder vida", "achou o segredo X"...), com pontos e medalhas.

### Para ativar

1. Crie uma conta grátis no site retroachievements.org.
2. No app, abra **Configurações → conquistas**.
3. Preencha **Usuário** e **Senha** (a mesma senha do site). Clicar fora
   do campo conecta a conta — aparece **"conta conectada — conquistas
   liberadas"**.
4. Opcional: **"Testar login"** confere a conta e mostra seus pontos.

A partir daí, ao jogar um jogo que tem conquistas no serviço:

- Desbloqueou algo? Um aviso desliza no queixo da TV: **"CONQUISTA
  DESBLOQUEADA"**, com o nome, os pontos e o selo da conquista, e um
  sininho toca.
- O badge **"RA ATIVADO"** fica fixo no queixo, junto com o modo
  (**HARDCORE** ou **SOFTCORE**).
- O painel da estante mostra o progresso ("conquistas 3 de 42 (7%)") e o
  comando **"Conquistas"** abre a lista completa do jogo, com as que você
  já ganhou marcadas.

### Hardcore x Softcore

- **Hardcore (padrão):** o jogo valendo — **sem cheats, sem save states,
  sem run-ahead** (o recurso que "adianta" quadros). Se tentar, o app
  bloqueia e avisa. Conquistas em hardcore valem mais "status" no site.
- **Softcore:** o modo relaxado — pode usar tudo, e as conquistas contam
  como softcore.

Troque em **Configurações → conquistas → Modo hardcore**.

> Um jogo pode não ter conquistas no serviço, ou ter a versão marcada
> como incompatível — nesse caso o botão "Conquistas" da estante aparece
> desativado com o motivo. Nada está quebrado.

---

## 12. Configurações

Abra pelo botão **"Configurações"** (na tela inicial ou na estante). Há
seis seções na coluna; tudo vale na hora e é salvo ao sair. **Voltar** sai
das configurações de qualquer seção.

### controles
- O **controle de Super Nintendo** aparece desenhado na TV: o botão da
  linha escolhida fica marcado e o que você aperta acende.
- A primeira linha, **Mapear**, escolhe o que configurar: **teclado**,
  **controle 1** ou **controle 2** (setas esquerda/direita ou clique). Cada
  um tem o seu mapa — o controle 2 não precisa ser igual ao 1.
- Clique num botão da lista e **aperte a tecla (no teclado) ou o botão (no
  controle escolhido)** — Esc ou um clique cancelam. Os botões aparecem com
  o nome impresso no seu controle (Xbox, PlayStation ou Nintendo).
- Se você usar uma tecla ou botão que já era de outra ação, **as duas
  trocam** e um aviso embaixo do controle conta o que mudou.
- **Configurar tudo em sequência** — pede um botão de cada vez, de Cima até
  Select.
- **Testar botões** — tudo o que você aperta só acende no desenho. Sai com
  Esc, um clique ou **Start + Select** juntos.
- **Restaurar padrão (deste aparelho)** — aperte duas vezes para voltar ao
  mapa de fábrica do aparelho escolhido.
- Os botões do **console** (Power, Reset, Ejetar...) e da **interface** não
  mudam — só os botões de jogar. Brilho, contraste, cor e nitidez ficam no
  botão **MENU** da TV.

### jogo
- **Mau contato do cartucho** — *desligado* (padrão), *raro* ou *anos 90*:
  a chance de o primeiro POWER não subir o jogo até você soprar o cartucho
  (veja [Mau contato](#mau-contato-soprar-o-cartucho)).
- **Soprar no microfone** — *desligado* (padrão) ou *ligado*: na cena do
  sopro, soprar no microfone de verdade limpa os contatos (Steam Deck, ROG
  Ally, notebook).

### imagem e som
- **Tela cheia** — liga/desliga o modo tela cheia (padrão: ligada).
- **Som do jogo** — *3 em 1 estéreo* (padrão) ou *alto-falante da TV*:
  pequeno, mono e abafado, como o som saía da própria TV.
- **Chiado da TV fora do ar** — o barulhinho de estática quando o console
  está desligado. Nostálgicos: ligado. Padrão: desligado.
- **Chiado do flyback (~15 kHz)** — o apito agudo de TV de tubo ligada.
  Tem quem nem escute e tem quem não aguente: padrão desligado.
- **Burn-in do tubo** — horas no mesmo jogo marcam o HUD de leve no tubo,
  e a marca some com o tempo jogando outra coisa. Padrão: desligado.
- **Visual da tela** — como o jogo aparece na TV (esquerda/direita troca,
  vale na hora): *TV de antena (RF)* (padrão: imagem mole, com chuvisco e
  cores vazando), *Composite*, *S-Video* (mais limpo, scanlines leves),
  *RGB / PVM* (monitor profissional: scanlines e máscara de pontos),
  *Trinitron* (grade de apertura, cores vivas), *LCD nítido* (pixels
  quadrados com grade, vidro plano) e *Preto e branco*. Brilho, contraste,
  cor e nitidez continuam no botão MENU da TV e valem por cima de qualquer
  visual. Em janelas pequenas as scanlines somem aos poucos, para não virar
  moiré.
- **Linhas e máscara** — escolhe as scanlines e a máscara à parte do
  visual, para misturar: por exemplo a *TV de antena (RF)* com as
  *scanlines RGB / PVM*. Opções: *do visual* (padrão, usa as do próprio
  visual), *nenhuma*, *scanlines leves*, *scanlines RGB / PVM*, *grade de
  apertura* e *grade de LCD*.

### estante
- **Tema** — o móvel da estante: o móvel de madeira, o expositor de metal
  da locadora, o rack de som preto (vidro fumê e frisos prateados), o
  quarto anos 90 (papel de parede Memphis e móvel laqueado) ou o fliperama
  (carpete fluorescente e neon).
- **Sugestão da noite** — liga/desliga o botão "sugestão" no cabeçalho da
  estante (desligado por padrão; veja a [seção 6](#6-a-estante-de-jogos)).
- **Renomear ROMs para o padrão No-Intro** — deixa os nomes dos arquivos
  na `roms/` no formato oficial dos catálogos ("Super Mario World (USA)").
  Os saves, anotações e artes de cada jogo **vão junto** com a renomeação.
  Precisa do `nointro.dat` instalado.

### conquistas
- Conta do RetroAchievements, teste de login e o seletor de **modo
  hardcore** (detalhes na [seção 11](#11-conquistas-retroachievements)).

### sistema
- **Núcleo: baixar / atualizar** — instala ou atualiza o "cérebro" snes9x.
  Aparece há quanto tempo o atual foi instalado.
- **Verificar atualizações ao abrir** — o app procura novidades quando
  abre (setas verdes na tela inicial). Pode desligar.
- **Idioma** — português ou inglês, vale na hora.

---

## 13. Atualizações

O app cuida das próprias atualizações — nunca aparece janela nenhuma no
meio do jogo:

- **Setas verdes** acendem no queixo da TV (na tela inicial) quando há
  atualização do **app** ou do **núcleo**.
- **Seta do app:** abre uma tela com as novidades da versão e o botão
  **"atualizar"**. O app baixa, instala e o botão vira
  **"reiniciar agora"** — clique para reabrir já atualizado.
- **Seta do núcleo:** baixa e instala a versão nova do snes9x na hora,
  com o progresso aparecendo no painel.
- Se a atualização foi baixada mas o app fechou antes de aplicar, ela é
  aplicada **na próxima vez que abrir**.
- No Windows a atualização automática do app não está disponível — a
  tela avisa para baixar o release novo no GitHub.

O catálogo de nomes (`nointro.dat`) é opcional: se ele não estiver
instalado (ou for apagado), a tela de boas-vindas
**"bem-vindo ao sixteen"** reaparece na tela inicial com o botão
**"baixar nointro.dat"** para instalá-lo de novo.

---

## 14. Curiosidade: o modo Dev

Existe um menu secreto para quem está testando o app. Na **tela
inicial**, digite o **código Konami** no teclado (ou no direcional do
controle):

> **↑ ↑ ↓ ↓ ← → ← → B A**

Um botão **"Dev"** aparece acima de "Configurações". Dentro dele há a
ação de **instalar um jogo de exemplo** (com artes), útil para ver o
app funcionando numa instalação recém-feita — depois de instalar, clique
em **"atualizar"** na estante.

O modo vale só até fechar o app; nada fica marcado.

---

## 15. Onde ficam seus arquivos

Tudo fica junto, numa pasta só — **para fazer backup, é copiar essa
pasta**:

| Sistema | Pasta |
|---|---|
| macOS | `Documentos/SixteeN` |
| Windows | a pasta onde o `sixteen.exe` está |
| Linux | `~/.local/share/SixteeN` |

Dentro dela:

| Pasta | O que guarda |
|---|---|
| `roms/` | **Seus jogos** — coloque aqui |
| `core/` | O núcleo snes9x (baixado pelo app) |
| `assets/` | Capas, logos, cartuchos, contracapas e manuais dos jogos |
| `saves/` | Progresso: memória dos jogos, save states, cheats ligados, tempo jogado |
| `notes/` | Anotações de texto e os prints de cada jogo |
| `retroachievements/` | Dados das conquistas (progresso, badges, cache) |
| `config/` | Preferências do app (`sixteen.cfg`), catálogo de nomes e cache |
| `update/` | Pacote de atualização baixado (some após aplicar) |

**Para passar o acervo para outro computador:** copie a pasta inteira
(jogos incluídos) para o mesmo lugar no outro computador. Tudo vem junto —
saves, notas, artes e preferências.

---

## 16. Problemas e soluções

| Mensagem / sintoma | O que significa e o que fazer |
|---|---|
| **"Para jogar é necessário baixar o núcleo snes9x"** | O "cérebro" dos jogos não está instalado. Clique no botão de baixar (na tela inicial ou em Configurações → sistema). |
| **"núcleo não encontrado"** ao abrir um jogo | O arquivo do núcleo sumiu de `core/`. Baixe de novo pelo app. |
| **"nenhuma rom encontrada"** | A pasta `roms/` está vazia. Coloque suas ROMs lá e clique em "atualizar" na estante. Confira se está na pasta certa (veja a seção 15). |
| **"MODO HARDCORE — savestates/cheats bloqueados"** | Você está jogando valendo conquistas hardcore. Desligue o modo hardcore em Configurações → conquistas se quiser usar esses recursos. |
| **"Conquistas (versão sem suporte)"** | Esse jogo/versão não tem conquistas aceitas no RetroAchievements. Não há o que fazer — o resto funciona normal. |
| **"usuário ou token inválidos"** nas configurações | Confira o usuário e a senha no site retroachievements.org (é a mesma conta). |
| **EJETAR não faz nada** | O console está ligado. Desligue com POWER primeiro — é uma proteção de propósito. |
| **O jogo não aparece com arte/capa** | Os arquivos de arte precisam ter exatamente o mesmo nome da ROM (sem extensão) e estar nas pastas certas de `assets/`. |
| **A atualização baixou mas o app não mudou** | Feche e abra o app: o pacote pendente em `update/` é aplicado na abertura. |
| **Sem som na TV fora do ar** | Normal: o chiado da estática é opcional — ligue em Configurações → imagem e som. |
| **O jogo vai "lento" ou instável** | Diminua o Run-ahead em Configurações → jogo (ou ponha 0). |

**Última ferramenta:** com o app fechado, você pode apagar (ou renomear)
subpastas específicas para "zerar" uma parte sem perder o resto — por
exemplo, apagar `config/sixteen.cfg` volta todas as preferências ao
padrão; apagar `saves/Nome do Jogo/` apaga o progresso daquele jogo.

---

## 17. Jogar só com o controle

Tudo fora do jogo funciona só com um controle — mouse e teclado são
opcionais. O mapa abaixo usa os nomes do controle Xbox (no Steam Deck: **A**
embaixo, **B** à direita, **X** à esquerda, **Y** em cima; **LB/RB** são os
ombros e **LT/RT** os gatilhos). Nenhum comando exige o botão Steam/Guide,
o menu rápido (…), os botões traseiros (L4/R4/L5/R5), os trackpads nem os
cliques de analógico (L3/R3) — então o Deck funciona do jeito que vem.

**Tela inicial**

| Botão | O que faz |
|---|---|
| Direcional | Anda pelos botões do painel (Estante, Manual do console, Configurações), pelas setas verdes de update e pelo **POWER** da TV (o foco aparece como um anel lavanda) |
| **A** | Ativa o que está em foco. Em "Estante de games" com a TV desligada, liga a TV primeiro |
| **Start** | Abre as Configurações |
| **X** | Baixa o núcleo, quando ele falta |
| **B** | Pergunta "Sair do SixteeN?" — **A** confirma, **B** volta |

Na **primeira execução**: **Cima/Baixo** escolhem entre baixar o núcleo,
baixar o DAT e "bora jogar"; **A** ativa, **X** baixa o que falta, **Start**
continua. Na tela de **update do app**, **A** atualiza (ou reinicia) e **B**
volta.

**Estante**

| Botão | O que faz |
|---|---|
| Direcional | Anda pela grade e pelos favoritos |
| **A** | Joga o jogo em foco |
| **B** | Volta à tela inicial |
| **X** | Favorita / desfavorita o jogo |
| **Y** | Abre o álbum de prints |
| **Select** | Abre o histórico |
| **Start** | Abre as Configurações |
| **LB / RB** | Página acima / abaixo |
| **LT / RT** | Aba anterior / próxima do painel (informações, capa traseira, cartucho, manual) |
| **Cima** na fileira de cima | Foca o cabeçalho: sugestão (se ligada), álbum, atualizar, histórico e a caixa de busca (**Esquerda/Direita** escolhem, **A** abre — a busca abre o teclado na tela —, **Baixo** ou **B** voltam aos jogos) |
| **Direita** na última coluna | Foca o painel lateral: favoritar, conquistas, capa traseira, cartucho, manual, configurações, voltar (**Cima/Baixo** escolhem, **A** abre, **Esquerda** ou **B** voltam aos jogos) |

No cartão da **sugestão da noite**: **A** joga, **X** (ou **Y**) sorteia
outra e **B** fecha.

Uma legenda no pé da tela lembra os botões da zona em que você está. Dentro
da capa ampliada, do manual (**LB/RB** viram a página) e da lista de
conquistas, **B** fecha.

**Configurações**

| Botão | O que faz |
|---|---|
| **LB / RB** (ou **LT / RT**) | Seção anterior / próxima — controles, jogo, imagem e som, estante, conquistas, sistema |
| **Cima / Baixo** | Escolhe a linha |
| **Esquerda / Direita** | Ajusta a linha (liga/desliga, opções) |
| **A** | Muda / executa / abre a linha. Nos campos das conquistas (usuário, chave, senha) abre o teclado na tela |
| **B** | Sai das configurações |

Na seção **controles**, ao escolher um botão a mapear: aperte o botão
desejado no controle; **segurar B por ~1 segundo** cancela (a dica mostra o
progresso) e, sem apertar nada por 5 segundos, a captura desiste sozinha. B
também pode ser mapeado — é só dar um toque rápido. O botão Guide/Steam
nunca é mapeado.

**Teclado na tela**

Aparece sozinho quando você abre um campo de texto com o controle (ou
aperta qualquer botão do controle com o campo aberto). Digitar no teclado de
verdade — ou no teclado do Steam (**Steam + X**) — continua funcionando e
esconde a grade.

| Botão | O que faz |
|---|---|
| Direcional | Move pela grade (dá a volta nas bordas) |
| **A** | Digita a tecla (ou aciona **Aa**, **#@!**, espaço, apagar, **OK**) |
| **X** | Apaga o caractere antes do cursor |
| **Y** | Alterna minúsculas → maiúscula (vale uma letra) → símbolos e acentos |
| **LB / RB** | Move o cursor dentro do texto |
| **Start** | Confirma (o mesmo que **OK**) |
| **B** | Cancela e descarta o texto |

Senhas aparecem como `*`. Letras, números e `@ . _ - : / +` estão na grade;
os acentos ficam na camada de símbolos.

---

*Divirta-se! Este app é um projeto pessoal, sem fins comerciais. O
emulador não distribui jogos nem o núcleo — cada um coloca os seus.*
