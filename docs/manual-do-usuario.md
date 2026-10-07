# SixteeN — Manual do Usuário

*Compatível com a versão 1.1.x do app. Última atualização deste manual: 01/10/2026.*

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

Sem arte nenhuma o app funciona normalmente: a estante mostra uma lista
numerada com os nomes dos jogos.

---

## 5. A tela inicial

Esta é a "sala do console": a TV fora do ar (aquela estática com o selo
verde **CH 3**) e o painel sem cartucho. Nela você encontra:

- **"Estante de games"** — o botão principal, na altura do slot. Abre a
  estante para escolher um jogo (é o equivalente a pegar um cartucho na
  estante).
- **"Configurações"** — no pé do painel.
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
  do console enquanto você joga.
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

> Os prints são capturas da imagem de verdade do jogo — ficam salvos como
> PNG na pasta `notes/` e podem ser usados fora do app também.

---

## 10. Cheats (códigos de trapaça)

Se o jogo tiver códigos conhecidos (Game Genie, Pro Action Replay etc.),
o comando **"Cheats"** aparece na lista. Ele abre uma janela com os cheats
do jogo, prontos para ligar e desligar:

- Clique na linha para **ligar/desligar** na hora — a caixinha
  `[x]`/`[ ]` mostra o estado.
- Use **"Buscar..."** para achar um cheat pelo nome e os filtros
  **Todos / Ligados / Desligados** para encurtar a lista.
- Os cheats ligados ficam contados no painel ("2 cheats ativados").
- A escolha fica guardada: na próxima vez que jogar, os mesmos cheats
  voltam ligados.

Os cheats vêm de um banco embutido no app (curado a partir do projeto
libretro) — não precisa baixar nada. Ainda assim, **não existe cheat para
todo jogo**: se o comando não aparecer, é que não há códigos para aquele
título.

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
cinco seções na coluna; tudo é salvo na hora.

### jogo
- **Run-ahead** — um recurso avançado que reduz a demora entre você
  apertar e o jogo reagir ("latência de entrada"). Valores maiores = menos
  atraso, mas pede mais do computador. Se algum jogo apresentar
  instabilidade, volte para 0 ou 1. Fica desligado no modo hardcore.

### vídeo
- **Tela cheia** — liga/desliga o modo tela cheia (padrão: ligada).
- **Chiado da TV fora do ar** — o barulhinho de estática quando o console
  está desligado. Nostálgicos: ligado. Padrão: desligado.

### sistema
- **Núcleo: baixar / atualizar** — instala ou atualiza o "cérebro" snes9x.
  Aparece há quanto tempo o atual foi instalado.
- **Verificar atualizações ao abrir** — o app procura novidades quando
  abre (setas verdes na tela inicial). Pode desligar.
- **Renomear ROMs para o padrão No-Intro** — deixa os nomes dos arquivos
  na `roms/` no formato oficial dos catálogos ("Super Mario World (USA)").
  Os saves, anotações e artes de cada jogo **vão junto** com a renomeação.
  Precisa do `nointro.dat` instalado.

### conquistas
- Conta do RetroAchievements, teste de login e o seletor de **modo
  hardcore** (detalhes na [seção 11](#11-conquistas-retroachievements)).

### controles
- Lista de cada botão do SNES e a tecla/botão de controle que faz aquela
  ação. Clique na linha e **aperte a tecla ou o botão que você quer** —
  ele é capturado na hora (Esc cancela). Se você atribuir uma tecla já em
  uso, ela muda de lugar (fica vazia na ação antiga).
- Os botões do **console** (Power, Reset, Ejetar...) e da **interface**
  não mudam — só os botões de jogar.

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
| **Sem som na TV fora do ar** | Normal: o chiado da estática é opcional — ligue em Configurações → vídeo. |
| **O jogo vai "lento" ou instável** | Diminua o Run-ahead em Configurações → jogo (ou ponha 0). |

**Última ferramenta:** com o app fechado, você pode apagar (ou renomear)
subpastas específicas para "zerar" uma parte sem perder o resto — por
exemplo, apagar `config/sixteen.cfg` volta todas as preferências ao
padrão; apagar `saves/Nome do Jogo/` apaga o progresso daquele jogo.

---

*Divirta-se! Este app é um projeto pessoal, sem fins comerciais. O
emulador não distribui jogos nem o núcleo — cada um coloca os seus.*
