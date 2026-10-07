# SixteeN — Manual Técnico

*Compatível com a versão 1.1.x do app. Última atualização deste manual: 01/10/2026.*

Este manual explica **como o sistema funciona por dentro**, em linguagem
acessível para quem não vive de código: a arquitetura, o fluxo de dados,
os arquivos que o app cria e mexe, os mecanismos de atualização e — como
este projeto é testado por QA — **as ferramentas de diagnóstico e teste**.

Se você quer apenas usar o app, leia o
[Manual do Usuário](manual-do-usuario.md). Para o histórico de mudanças,
veja o [`CHANGELOG.md`](../CHANGELOG.md) e as notas de implementação em
`docs/fase-0.md` … `docs/fase-4.md`.

---

## Sumário

1. [Visão geral](#1-visão-geral)
2. [A analogia que explica tudo](#2-a-analogia-que-explica-tudo)
3. [Organização do código](#3-organização-do-código)
4. [O ciclo de vida do app](#4-o-ciclo-de-vida-do-app)
5. [Como uma ROM vira jogo](#5-como-uma-rom-vira-jogo)
6. [A emulação e o core libretro](#6-a-emulação-e-o-core-libretro)
7. [A apresentação: gabinete, CRT e som](#7-a-apresentação-gabinete-crt-e-som)
8. [RetroAchievements por dentro](#8-retroachievements-por-dentro)
9. [Persistência: cada arquivo do app](#9-persistência-cada-arquivo-do-app)
10. [Configuração (`sixteen.cfg`)](#10-configuração-sixteencfg)
11. [Atualizações: os três mecanismos](#11-atualizações-os-três-mecanismos)
12. [Compilar e rodar em desenvolvimento](#12-compilar-e-rodar-em-desenvolvimento)
13. [Testes e ferramentas de QA](#13-testes-e-ferramentas-de-qa)
14. [CI/CD e releases](#14-cicd-e-releases)
15. [Scripts auxiliares](#15-scripts-auxiliares)
16. [Decisões de projeto e armadilhas conhecidas](#16-decisões-de-projeto-e-armadilhas-conhecidas)

---

## 1. Visão geral

O SixteeN é um **emulador de Super Nintendo** cuja interface é um
console inteiro desenhado na tela (gabinete, TV de tubo com filtro NTSC,
painel de chaves, estante de cartuchos). É escrito em **Rust**, roda em
macOS, Windows e Linux, e se distribui como app de mesa (DMG, zip,
AppImage).

Princípios de projeto que explicam quase todas as decisões:

1. **Portátil e sem banco de dados.** Nada de instalação ou registro no
   sistema: o app vive em uma pasta (arquivos planos: JSON, TOML, TXT,
   PNG). Migrar = copiar a pasta.
2. **Sem raspagem online de metadados.** Ano/editora dos jogos vêm de uma
   tabela embutida no binário (derivada do DAT TOSEC) e, opcionalmente, de
   um arquivo DAT No-Intro local. A única rede usada por padrão é a do
   RetroAchievements (se você logar) e a das atualizações.
3. **O app não distribui conteúdo de jogo.** A ROM é do usuário; o core
   (snes9x) é baixado pelo próprio usuário pela interface. Por isso os
   pacotes de release não incluem nem um nem outro.
4. **Só mouse e gamepad** para operar; teclado existe para jogar e para
   digitar texto. A interface é um desenho próprio, sem toolkit — todo
   botão é um retângulo com teste de clique feito à mão.

---

## 2. A analogia que explica tudo

O sistema foi desenhado imitando um console real, e o código acompanha a
metáfora:

| No console de verdade | No SixteeN | Onde vive no código |
|---|---|---|
| A TV de tubo + o móvel | A janela desenhada: CRT, painel, estante | `crates/platform` (SDL3 desenha tudo) |
| A placa-mãe que roda o jogo | O **core** `snes9x` (um "chip" que o app carrega em tempo real) | `crates/emulation` |
| O cartucho | O arquivo de ROM e sua identidade (hash, nome, capa) | `crates/domain` |
| O placar/oficial de conquistas | O serviço RetroAchievements | `crates/ra` + `crates/app/src/ra.rs` |
| A pessoa ligando o console | A lógica do app: telas, menus, fluxos | `crates/app` |

Dois detalhes que valem ouro para entender o resto:

- O app **não sabe emular SNES**. Ele carrega em tempo de execução
  (via `dlopen` — a mesma mecânica de "encaixar um chip") um arquivo
  `snes9x_libretro.dylib/.dll/.so` que faz o trabalho pesado. Esse padrão
  se chama **libretro**: um contrato de funções que qualquer emulador pode
  implementar e qualquer frontend pode usar.
- Tudo que o usuário vê **não é um vídeo**: cada frame do jogo é
  processado (filtro NTSC + deformação de tubo) e redesenhado, e a
  interface inteira (botões, estante, livro) é desenhada por código,
  quadro a quadro.

---

## 3. Organização do código

É um **workspace Cargo** com 6 *crates* (as "caixas" de código Rust; pense
nelas como módulos/bibliotecas). A versão é única para todo o workspace —
hoje **1.1.5** — e as dependências apontam **só para baixo**:

```
                sixteen-app   (binários + lógica de aplicação)
               /   |    |    \
              /    |    |     \
     emulation  platform domain  ra
          \        |      |     /
           \       |      |    /
            (ntsc — filtro NTSC, crate folha usado pela platform/app)
```

| Crate | Nome do pacote | Responsabilidade | Depende de |
|---|---|---|---|
| `crates/app` | `sixteen-app` | Os 3 binários, as telas (idle, estante, configurações), o laço de jogo (`runner`), atualizações, cola com RA | todos abaixo |
| `crates/domain` | `sixteen-domain` | Identificar ROMs (hashes), catálogo da estante, nomes No-Intro/TOSEC, banco de cheats | nada do app |
| `crates/emulation` | `sixteen-emulation` | Carregar o core libretro em runtime e conversar com ele (frames, áudio, saves, cheats) | nada (nem SDL) |
| `crates/platform` | `sixteen-platform` | SDL3: janela, desenho do gabinete/CRT, áudio, mouse/teclado/controle | `ntsc` |
| `crates/ra` | `sixteen-ra` | Wrapper seguro do motor de conquistas rcheevos (C, vendorizado) | nada |
| `crates/ntsc` | `sixteen-ntsc` | O filtro de TV antiga (blargg `snes_ntsc` vendorizado) | nada |

**Três binários** saem de `crates/app/src/bin/`:

- **`sixteen`** — o app completo (o que o usuário abre).
- **`emu-run`** — emulador puro para desenvolvimento: roda uma ROM solta
  sem estante nem catálogo (`emu-run --core <arquivo> --rom <jogo.sfc>`).
- **`selector`** — a estante sozinha, para testá-la isolada; ao escolher
  um jogo, imprime o caminho da ROM no terminal e sai.

**Nota de build:** mesmo em desenvolvimento o projeto compila com
otimização (`opt-level = 2/3` no `Cargo.toml` do workspace), porque o laço
de emulação é inutilizável sem otimização — não precisa de `--release`
para testar jogando. O SDL3 vem do sistema (ex.: `brew install sdl3`) ou
é compilado junto com a feature `vendored-sdl` (exige CMake), que é o que
o CI usa.

---

## 4. O ciclo de vida do app

O `sixteen` é uma **máquina de estados** simples de três telas:

```
abertura ──► IDLE (tela inicial) ──"Estante de games"──► ESTANTE
                ▲   │                                      │
                │   └── "Configurações" ──► CONFIGURAÇÕES  │ escolheu jogo
                │                                          ▼
                └──────── EJETAR ──────────────── JOGO (console ligável)
```

No arranque, antes de qualquer janela, o app:

1. Migra dados de layouts antigos (pastas mudaram de lugar entre versões;
   as migrações são idempotentes — rodam sempre e não fazem nada se já
   estiverem feitas).
2. Aplica um **pacote de atualização pendente** na pasta `update/`, se
   houver (rede de segurança da atualização automática).
3. Cria as pastas do app se faltam, carrega `config/sixteen.cfg`, abre
   o catálogo (varre `roms/`), abre a janela única de 1280×800 e dispara
   em uma *thread* paralela a checagem de atualizações.

Cada tela é uma função que roda seu próprio laço de frames e devolve um
enum de "para onde ir" (`IdleExit`, `Pick`, `GameExit`). Trocar de tela =
trocar de função.

**A janela é uma só.** Todas as telas — jogo, estante, configurações,
livro de pausa, modais — são desenhadas na mesma janela, compostas pelo
"tubo CRT" (as bordas curvas e o brilho são deformação de imagem, não
elementos de janela do sistema operacional). O app abre em tela cheia por
padrão.

---

## 5. Como uma ROM vira jogo

Do arquivo no disco até o primeiro frame, este é o pipeline completo:

```
roms/Jogo (USA).sfc
   │  1. VARREDURA (ao abrir o app ou clicar "atualizar")
   │     caminha por roms/ (aceita subpastas; .zip é aberto e a ROM de
   │     dentro é usada transparentemente)
   ▼
hashes CRC32 + SHA1 (sempre DESCARTANDO o cabeçalho de 512 bytes
   │  que algumas copiadoras adicionam — detectado pelo tamanho % 1024)
   │  (o resultado é lembrado em config/hashcache.json, chaveado por
   │   caminho + tamanho + data — um ROM inalterado não é re-hasheado)
   ▼
   │  2. CATÁLOGO (config/library.json, chaveado por SHA1)
   │     guarda por jogo: quando foi adicionado, última vez jogado,
   │     contagem de partidas, favorito e título customizado
   ▼
   │  3. NOME DE EXIBIÇÃO — nesta ordem de prioridade:
   │       título customizado  →  nome do DAT No-Intro (por CRC32)
   │       →  nome interno do cabeçalho SNES  →  nome do arquivo
   │     (ano/editora: DAT No-Intro → tabela TOSEC embutida no binário)
   ▼
   │  4. ESTANTE mostra capa (assets/cover/<nome do arquivo>.png) e
   │     identifica o jogo no RetroAchievements (hash MD5 próprio — §8)
   ▼
   │  5. JOGO: bytes da ROM vão para o core (retro_load_game)
   │     + SRAM anterior é reinjetada (saves/<nome>/sram.srm)
   │     + cheats do banco embutido são aplicados (os ligados)
   ▼
frames de vídeo (RGB) ──► filtro NTSC ──► deformação CRT ──► TV
soma de áudio (S16)   ──► fila de áudio SDL ──► caixa de som
```

Pontos finos que costumam gerar dúvida em teste:

- **A chave de tudo que é "por jogo" (saves, notas, cheats) é o nome do
  arquivo da ROM sem extensão** — não o título bonito do catálogo. É por
  isso que o botão "Renomear ROMs" move saves/notas/artes junto com o
  arquivo: sem isso, o jogo perderia a memória associada.
- **O catálogo e o cache de hash sobrevivem a renomeações** porque são
  chaveados por SHA1/CRC32 (conteúdo), não por nome.
- Um `.zip` de ROM nunca é descompactado no disco: a ROM é extraída em
  memória a cada uso (o cache de hash evita refazer isso quando o arquivo
  não mudou).
- Arquivos ilegíveis ou ROM muito pequena não derrubam a varredura: são
  registrados no log e pulados.

---

## 6. A emulação e o core libretro

### O contrato libretro

Libretro é uma interface padronizada entre emuladores ("cores") e
frontends. O core snes9x é uma biblioteca comum (`.dylib` no macOS,
`.dll` no Windows, `.so` no Linux) que exporta funções com nome fixo.
O crate `emulation` carrega esse arquivo em runtime com `libloading`
(que por baixo usa `dlopen`/`LoadLibrary`) e resolve **24 símbolos** — os
mais importantes:

| Função do core | O que faz |
|---|---|
| `retro_init` / `retro_deinit` | liga/desliga o "chip" |
| `retro_load_game` / `retro_unload_game` | coloca/tira a ROM |
| `retro_run` | avança **um quadro** do jogo |
| `retro_reset` | botão Reset |
| `retro_serialize` / `retro_unserialize` | save state (a "foto" do instante) |
| `retro_get_memory_data` | acesso direto à memória: SRAM (save de bateria) e RAM do sistema |
| `retro_cheat_set` | aplica um cheat (Game Genie etc.) |
| `retro_set_video_refresh` / `retro_set_audio_sample_batch` / `retro_set_input_state` | registra os *callbacks* por onde o core **entrega** vídeo/áudio e **pergunta** o estado dos controles |

A conversa é invertida: o app registra funções de retorno (*callbacks*) e,
a cada `retro_run`, o core chama de volta — "aqui está o frame", "aqui
está o áudio", "o jogador 1 está apertando A?". É por isso que o app
consegue desenhar o frame com o próprio filtro NTSC: ele recebe o frame
cru e faz o que quiser com ele. (O filtro interno do snes9x fica
explicitamente desligado via a opção `snes9x_blargg = disabled`.)

### O laço de um quadro (o coração do app, em `runner.rs`)

Enquanto o console está **ligado**, a cada volta do laço:

1. Lê mouse/teclado/controles (SDL) e preenche a matriz de botões das 2
   portas de controle.
2. Chama `core.run()` → o core devolve frame + áudio.
3. O áudio entra numa fila com limite (~0,15 s) que **paca o laço** — é o
   áudio que define a velocidade do jogo, não um timer.
4. A conquistas são avaliadas **contra a memória RAM real do SNES**
   (lida sem cópia do core) — antes de qualquer frame especulativo do
   run-ahead, para não registrar eventos de quadros que nunca "existiram".
5. O frame passa pelo filtro NTSC e vai para o tubo.
6. A cada ~10 s (600 quadros), a SRAM é gravada em disco, se mudou.

**Run-ahead:** com valor N, o app salva um state, roda N quadros à frente
(descartando o que produzem) só para *ler* os botões com antecedência e
reduzir latência; depois rebobina e apresenta o quadro "de agora". Por
isso é desligado no modo hardcore (seria borda no princípio do desafio).

**Power/Reset/Eject no app:**

- **Power off** salva a SRAM, congela o cronômetro de sessão e suspende o
  core na memória. **Power on** retoma do mesmo ponto sem recarregar
  nada (enquanto o processo viver).
- **Reset** chama `retro_reset` e rearma os contadores de conquistas
  (progresso já ganho permanece).
- **Eject** só é aceito desligado (no desenho, o botão "trava"); salva o
  progresso RA e devolve o controle para a tela inicial.

---

## 7. A apresentação: gabinete, CRT e som

Tudo é desenhado pelo crate `platform` sobre o SDL3 (GPU), num único
arquivo de desenho — `cabinet.rs` (o "gabinete"). Não há toolkit de
interface: cada botão é um retângulo desenhado que se registra numa lista
de áreas clicáveis (*hit-testing*) refeita a cada frame. Por isso os
mesmos botões existem com ids (`PanelButton::Power`,
`ShelfButton::Settings`…) e são as "handles" dos testes de interface.

Componentes visuais que valem conhecer:

- **Tubo CRT:** o frame do jogo (256×224 → 602 px de largura pelo filtro
  NTSC) é aplicado numa **malha 3D curvada** (warp), com vinheta e
  scanlines; a estática "fora do ar" é um ruído gerado em tempo real. O
  preset de filtro é o **RF** — um ajuste próprio do app sobre o blargg
  NTSC ("composite mais suave e ruidoso: o visual de antena").
- **Fonte:** um bitmap embutido (Noto Sans Mono, 9×20) — por isso os
  textos têm aquele aspecto de terminal de vídeo.
- **Estante:** desenhada como "painel flat" dentro do tubo; tiles de capa
  260×195 (paisagem). Sem arte nenhuma, vira lista numerada estilo
  multicart.
- **Livro de pausa / modais:** desenhos próprios sobrepostos à TV; os
  modais compartilham um componente único (grade de linhas com scroll e
  busca).
- **Áudio:** dois caminhos — o áudio **do jogo** (fila S16 na taxa que o
  core declarou) e **foleys do console** (WAVs curtos embutidos no
  binário: inserir, ejetar, power on/off, reset, conquista). Sem
  dispositivo de áudio, há bipes sintetizados como fallback.
- **Entrada:** teclado mapeia só a porta 1 de jogo (padrão: setas, Z=B,
  X=A, A=Y, S=X, Q=L, W=R, Enter=Start, Shift dir.=Select); controles
  USB/Bluetooth ocupam as 2 portas, com hotplug. Todo o resto da interface
  é mouse + gamepad (A confirma, B volta). Digitação por teclado só
  existe nos campos de texto (anotações, busca, configuração de RA). O
  **código Konami** (↑↑↓↓←→←→BA) é escutado apenas na tela inicial e liga
  o modo dev para a sessão.

---

## 8. RetroAchievements por dentro

O RA tem duas metades: **rede** (em `crates/app/src/ra.rs`) e **motor**
(vendor C, em `crates/ra`).

### Identificação do jogo

O RA não usa o nome do arquivo: usa um **hash MD5 da ROM** (descartando
cabeçalho de copiadora, quando o tamanho indica que existe). O app pede
ao serviço, "qual jogo tem este hash?", e guarda a resposta em
`retroachievements/ra-cache/<hash>.json`. Um hash que o serviço não
conhece é cacheado como **arquivo vazio** — para não perguntar de novo a
todo momento.

### O motor de conquistas (rcheevos)

Cada conquista tem uma expressão de lógica (ex.: `0xH0042=10` — "o byte
da memória 0x0042 vale 10"). O crate `ra` compila o rcheevos (vendorizado,
um subconjunto sem Lua/discos) e expõe uma sessão segura em Rust:

- Ao abrir um jogo, as conquistas conhecidas são **ativadas** no motor.
- A cada quadro, o app entrega a **RAM real do SNES** ao motor (`tick`);
  quando uma lógica completa, o motor dispara um evento com o id da
  conquista.
- O estado da sessão (contadores parciais de cada lógica) pode ser
  serializado — é o arquivo `.rap` por jogo; **Reset** do console rearma
  os contadores, mas o que já foi ganho permanece.
- Testes de integração reais (`crates/ra/tests/runtime.rs`) disparam uma
  conquista sintética escrevendo na memória.

### Login e envio

- **Senha** (Configurações → conquistas) faz login na rota Connect e
  devolve um **token de sessão** — é ele que autoriza o envio de
  conquistas. **A senha nunca é gravada em disco**; só o token (no
  `sixteen.cfg`).
- O **token da web API** (do site, aba Settings → Keys) serve só para
  leitura de metadados e para o botão "Testar login". É opcional.
- Ao desbloquear, o app: grava o evento **localmente primeiro**
  (`ra-earned/<hash>.json` — para nunca perder/duplicar), toca o som,
  mostra o OSD no queixo por 6 s, e envia ao servidor numa thread à parte
  com 3 tentativas. Esgotadas as tentativas, o evento vai para
  `ra-pending.jsonl`.
- **Hardcore** é um flag da sessão: bloqueia cheats, save states,
  loadstates e run-ahead, e marca o envio como hardcore (`h=1`). Padrão:
  ligado.

---

## 9. Persistência: cada arquivo do app

Raiz por sistema (definida em `crates/app/src/dirs.rs`):

| SO | Raiz |
|---|---|
| macOS | `~/Documents/SixteeN` |
| Windows | pasta do executável |
| Linux | `$XDG_DATA_HOME/SixteeN` (padrão `~/.local/share/…`) |

```
<raiz>/
├── roms/                      ROMs do usuário (.sfc .smc .fig .swc .bs .st .bin, .zip)
│                              (subpastas são varridas)
├── core/
│   └── snes9x_libretro.*      o core (dylib/dll/so), baixado pela UI
├── assets/                    arte POR JOGO — nome do arquivo = nome da ROM sem extensão
│   ├── cover/<rom>.png|jpg    capa (paisagem)
│   ├── logo/<rom>.*           logo do painel
│   ├── cartridge/<rom>.*      cartucho do slot
│   ├── backcover/<rom>.*      contracapa
│   ├── manual/<rom>.pdf       manual escaneado (leitor da estante)
│   ├── console.png            (opcional) troca o logo do console
│   └── console-tag.png        (opcional) troca o wordmark do slot
├── saves/<nome da ROM>/       um diretório por jogo
│   ├── sram.srm               SRAM de bateria (o save do próprio jogo)
│   ├── 0.state … 9.state      save states (bytes crus do core)
│   ├── cheats.txt             cheats ligados (um "true/false" por linha,
│   │                          na ordem do banco embutido)
│   └── playtime.txt           segundos totais com o console ligado (texto)
├── notes/<nome da ROM>/
│   ├── 01.png … 15.png        álbum de prints
│   ├── 01.txt … 15.txt        notas de texto (máx. 240 caracteres cada)
│   └── slots.json             metadados: fixado? legenda do print?
├── retroachievements/
│   ├── ra-cache/<md5>.json    identificação do jogo (verbatim do serviço;
│   │                          arquivo vazio = jogo desconhecido)
│   ├── ra-cache/earned/<id>.json  conquistas ganhas segundo o servidor
│   ├── ra-cache/badges/*.png  badges das conquistas
│   ├── ra-earned/<md5>.json   conquistas ganhas localmente {"id": hardcore?}
│   ├── ra-progress/<md5>.rap  estado da sessão rcheevos (contadores)
│   └── ra-pending.jsonl       unlocks que não conseguiram ser enviados
├── config/
│   ├── sixteen.cfg          preferências (TOML — ver §10)
│   ├── nointro.dat            DAT No-Intro (XML Logiqx ou clrmamepro)
│   ├── library.json           catálogo persistido (por SHA1: favorito,
│   │                          playtime agregado, título custom, datas)
│   └── hashcache.json         cache de hashes (por caminho+tamanho+mtime)
└── update/                    pacote de atualização baixado, pendente de aplicar
```

Regras gerais de tolerância: **qualquer arquivo de dados corrompido ou
ausente volta ao estado vazio sem erro** (o app nunca derruba o usuário
por causa de um sidecar). `library.json` e `hashcache.json` são
reconstruídos varrendo `roms/`.

**Formatação de tempo:** `playtime.txt` guarda segundos crus (texto); a
estante formata ("nunca", "menos de 1min", "37min", "2h 05min"). O
cronômetro "sessão" do painel conta só o tempo com o console ligado e é
somado ao total ao sair do jogo.

**Manual em PDF:** o leitor da estante não renderiza PDF vetorial — ele
extrai **a maior imagem embutida de cada página** do PDF (é como manuais
escaneados vêm). Página sem imagem embutida não aparece. (`manual.rs`,
usando `lopdf` + `image`.)

---

## 10. Configuração (`sixteen.cfg`)

Arquivo TOML de texto em `config/sixteen.cfg`. A tela de Configurações
grava nele a cada mudança; nada de "Aplicar". Chaves:

| Chave | Padrão | O que é |
|---|---|---|
| `runahead` | 1 | quadros de antecipação (0–4 na UI; hardcore força 0) |
| `fullscreen` | `true` | abrir em tela cheia |
| `check_updates_on_start` | `true` | checar atualizações ao abrir |
| `hiss_on_static` | `false` | chiado de fundo na estática |
| `ra_user` | — | usuário RetroAchievements |
| `ra_token` | — | web API key (opcional; metadados/teste de login) |
| `ra_connect` | — | token de sessão Connect (gravado ao logar por senha) |
| `ra_hardcore` | `true` | modo hardcore |
| `[keyboard]` / `[gamepad]` | — | remapeamento dos botões de jogar (ação → tecla SDL / botão SDL) |

Ordem de resolução do arquivo: flag `--config` → variável de ambiente
`SIXTEEN_CONFIG` → `config/sixteen.cfg` (criado com padrões se
faltando). Remapeamentos antigos de botões de console (que não são mais
remapeáveis) são ignorados com aviso no log.

---

## 11. Atualizações: os três mecanismos

Todos seguem o mesmo princípio: **nunca atrapalhar o usuário** — checagem
silenciosa em thread, resultado vira uma setinha verde no queixo da tela
inicial, e falha de rede é silenciosa.

### 1. O próprio app (`update_check.rs`)
Consulta `api.github.com/repos/ticianocorral/sixteen/releases/latest`,
compara versões por inteiro (`1.2.10 > 1.2.9`) e baixa o asset do
plataforma para `update/`. A aplicação acontece **na tela de update**
(botão vira "reiniciar agora") ou, como rede de segurança, **no próximo
arranque**. Mecânica por plataforma:

- **macOS:** monta o DMG (`hdiutil`) e copia o `.app` por cima do atual
  (`ditto`). O processo vivo continua rodando do inode antigo.
- **Linux/AppImage:** escreve um `<app>.new` e entra por **rename**
  atômico (sobrescrever in-place um AppImage montado em FUSE corrompia a
  leitura — corrigido na 1.1.4).
- **Windows:** sem auto-update; a tela orienta a baixar do release.

### 2. O núcleo snes9x (`core_update.rs`)
Baixa do buildbot oficial do libretro (nightly por plataforma) e instala
via `.new` + **rename atômico** — sobrescrever o `.dylib` em uso derruba
o app no macOS (SIGKILL na revalidação de assinatura; corrigido na 1.1.3
e reproduzível com o exemplo `core_refresh_probe`).

**Como o app sabe que há núcleo novo:** o próprio binário do core reporta
sua versão ("snes9x 1.63 fae2fea" — versão upstream + hash de commit). O
app compara esse hash com o HEAD do repositório `libretro/snes9x`. Como o
buildbot recompila *nightly* mesmo sem mudança no snes9x, a seta só acende
quando existe **commit novo de verdade** (comportamento da 1.1.3).

### 3. O DAT No-Intro (`dat_update.rs`)
Baixa o DAT do mirror oficial do libretro-database, valida o cabeçalho
(clrmamepro ou XML) e instala com `.part` + rename em
`config/nointro.dat`.

### Bônus: renomear ROMs (`rom_rename.rs`)
Ação manual (Configurações → sistema) que casa cada ROM pelo **CRC32** no
DAT e renomeia o arquivo para o nome canônico No-Intro, **movendo junto**
`saves/<antigo>/`, `notes/<antigo>/` e as artes em `assets/`. Falhas
individuais são puladas com aviso; ROM já canônica vira "nothing to do".

---

## 12. Compilar e rodar em desenvolvimento

Pré-requisitos: Rust estável (`rust-toolchain.toml` fixa a versão) e SDL3
do sistema (`brew install sdl3` no macOS; no Linux, os pacotes de dev do
SDL3) — ou CMake para usar `vendored-sdl`.

```bash
# o app completo
cargo run --bin sixteen

# com overrides pontuais (todas opcionais)
cargo run --bin sixteen -- \
  --core ~/caminho/snes9x_libretro.dylib \
  --config ~/outro.cfg --save-dir DIR --system-dir DIR --notes-dir DIR \
  --order name|shelf --runahead 0-4

# emulador puro, sem estante (bom para isolar problemas de emulação)
cargo run --bin emu-run -- --core <core> --rom <jogo.sfc> \
  [--runahead N] [--shot tela.bmp --shot-frame 180]

# só a estante (imprime a ROM escolhida e sai; --frames N roda headless)
cargo run --bin selector -- [--order name] [--shot tela.bmp]

# sondar um core sem ROM (nome/versão/extensões)
cargo run -p sixteen-emulation --example probe -- <core>
```

Variáveis de ambiente: `SIXTEEN_CORE` (caminho do core),
`SIXTEEN_CONFIG` (caminho do cfg), `SIXTEEN_EXEMPLO_ZIP_URL`
(sobrepõe a URL do zip de exemplo do modo dev), `RUST_LOG` (nível de log),
`SIXTEEN_TRACE=1` (ver §13).

No macOS, `scripts/install-macos.sh` compila e instala em
`/Applications/SixteeN.app` (assinatura ad-hoc) — o caminho rápido
"compilar e testar no meu Mac".

---

## 13. Testes e ferramentas de QA

### Suíte de testes

```bash
# exatamente o que o CI roda
cargo test --workspace --features sixteen-platform/vendored-sdl

# testes de rede reais (marcados #[ignore], rodam só sob pedido)
cargo test -p sixteen-app --lib -- --ignored
```

Cobrem: identificação de ROM (cabeçalho, hashes), catálogo (ordens,
títulos), DAT (XML e clrmamepro), cheats (inclui **regressões de
segurança**: códigos com placeholder ou acima de 96 caracteres são
excluídos na geração porque estouravam um buffer no core), config
(roundtrip), remapeamento, RA (hash de copiadora, parse de lógica,
medalhas), atualizações (comparação de versões, instalação atômica) e o
motor rcheevos (integração real com memória sintética).

### Capturas de tela automatizadas (sem clicar em nada)

Vários bins/examples desenham telas reais e gravam **BMP**:

```bash
cargo run --bin sixteen -- --debug-settings main --shot tela.bmp
cargo run --bin sixteen -- --debug-settings controls --shot tela.bmp
cargo run --bin sixteen -- --debug-idle-shot --shot tela.bmp
cargo run --bin emu-run -- --core <core> --rom <rom> \
    --shot-frame 300 --shot tela.bmp          # frame 300 do jogo
cargo run --bin emu-run -- --debug-shot-modal save --shot modal.bmp
cargo run --bin emu-run -- --debug-shot-pause --shot pausa.bmp
```

### Diagnóstico ao vivo

- **Logs:** saem no **stderr** (não há arquivo de log). Nível padrão
  `info`; para o detalhe todo, rode com `RUST_LOG=debug`. Para capturar:
  `…/sixteen 2> log.txt`. O log registra: carregamento de config,
  contagem de ROMs, versão do core, identificação de cada ROM (crc32,
  sha1, mapper), SRAM salva/carregada, cheats aplicados, sessão RA e
  envios, updates aplicados, migrações.
- **Performance:** `SIXTEEN_TRACE=1` emite avisos quando uma fase do
  frame passa de ~2 ms (`poll`, `present`, atraso de pacing) — é o jeito
  rápido de localizar travadas de frame.
- **Sintomas clássicos e onde olhar:**
  - "atualizei o núcleo e a versão não muda" → o texto exibido é a
    versão **do binário**; entre builds sem commit novo upstream a string
    é igual de propósito (§11.2). Log `info` mostra o que carregou.
  - "a estante não mostra meu jogo" → o jogo pulou na varredura (log de
    aviso na varredura: ilegível, pequeno demais, ou zip sem ROM dentro).
  - "perdi save ao renomear ROM" → conferir se a pasta `saves/<nome
    antigo>/` foi movida (o renomear só move quando o CRC32 casa no DAT).
  - conquistas não disparam → conferir no log se a sessão RA abriu e se
    o jogo foi identificado (`ra-cache/<hash>.json` vazio = desconhecido).

### Exemplos (`examples/`) úteis para reproduzir bugs

| Exemplo | Serve para |
|---|---|
| `core_refresh_probe` (app) | reproduzir o diagnóstico de "sobrescrever dylib em uso" (histórico da 1.1.3) |
| `fav_deck_repro` (app) | reproduzir o layout da estante em 1280×800 com favoritos (histórico da 1.1.2), com cliques sintéticos |
| `strip_click_repro` (app) | repro das setas de rolagem das faixas |
| `ra_osd_mock` (app) | gerar a notificação de conquista em BMP (usado nas mockups de `docs/mocks/`) |
| `setup_shot`, `devmode_shot` (app) | telas de setup e do modo dev |
| `manual_dump` (app) | despejar as páginas de um PDF como PNG (diagnóstico do leitor de manuais) |
| `state_check` (emulation) | sanidade de save state + SRAM: roda 600 quadros diretos vs salvar no 300 e carregar, compara os frames |
| `cart_scene` (platform) | frames da animação do cartucho |

---

## 14. CI/CD e releases

- **`ci.yml`** — a cada push/PR, matriz nos 3 sistemas: `cargo fmt
  --check`, `cargo clippy -D warnings`, `cargo build` e `cargo test`
  (sempre com `vendored-sdl`).
- **`release.yml`** — disparado por **tag `vX.Y.Z`**: compila com
  `vendored-sdl` nos 3 sistemas e produz `SixteeN-<v>-macos.dmg`,
  `…-windows-x86_64.zip` e `…-linux-x86_64.AppImage`; um job final cria o
  GitHub Release (`--generate-notes`) e anexa os arquivos. **O core e
  ROMs nunca entram nos pacotes** (licença não-comercial do snes9x) — o
  README e os avisos legais explicam isso ao usuário.
- **`deploy-pages.yml`** — push em `main` que toque `site/**` publica a
  pasta `site/` no GitHub Pages.

Versionamento: **SemVer**, versão única no workspace (`[workspace.package]`
em `Cargo.toml`), cada release gera tag + entrada no `CHANGELOG.md`
(formato Keep a Changelog, em pt-BR).

---

## 15. Scripts auxiliares

Em `scripts/`, Python/bash de manutenção — não fazem parte do app:

| Script | O que faz |
|---|---|
| `gen_tosec_data.py` | gera `tosec_data.txt` (TSV CRC32 → ano/editora) a partir do DAT TOSEC; vira tabela embutida no binário |
| `gen_cheats_data.py` | gera `cheats_data.txt` (~2,7 MB) do libretro-database; **exclui** códigos com placeholder (`X`/`?`) e códigos > 96 caracteres (regressão de segurança do `retro_cheat_set`) |
| `sync_art_with_roms.py` | mantém `assets/*/` consistentes com `roms/` (arte sem ROM vai para `assets/<tipo>-extra/` e volta se a ROM retornar) |
| `rename_art_to_nointro.py` | renomeia pastas de arte para o padrão No-Intro casando títulos normalizados |
| `install-macos.sh` | compila e instala o app em `/Applications` no macOS |

---

## 16. Decisões de projeto e armadilhas conhecidas

Coisas que pareceriam bugs mas são decisões (ou foram bugs reais, já com
regressão coberta) — leitura recomendada antes de abrir uma ocorrência:

1. **Instalação sempre atômica (`.new` + rename).** Tudo que sobrescreve
   arquivo em uso — core, AppImage, app do macOS — entra por rename.
   Sobrescrever biblioteca/binário mapeado derruba o processo (macOS
   SIGKILL) ou corrompe a leitura (AppImage/FUSE).
2. **Ejetar é privilégio do console desligado.** O clique com console
   ligado é propositalmente ignorado (com um "clunk" de trava).
3. **Saves, notas e cheats são chaveados pelo nome do arquivo da ROM.**
   Renomear a ROM por fora do app "desconecta" o progresso. O botão de
   renomear existe exatamente para mover tudo junto.
4. **Cheats do banco são pré-filtrados.** Códigos com valor variável ou
   muito longos não existem no banco (risco de estourar buffer no core);
   um jogo pode legítimamente não ter cheats.
5. **A seta de update do núcleo não acende a cada nightly.** Desde a
   1.1.3 a comparação é por commit do snes9x, não por data do build.
6. **"Unsupported Game Version"** — a identificação RA marca jogos fora
   do registro oficial; o botão Conquistas da estante aparece desativado
   com o motivo, não sumido.
7. **A senha do RA nunca toca o disco** — o cfg guarda só o token Connect.
   Apagar `ra_connect` do cfg desloga a conta.
8. **O modo dev (Konami) é efêmero**: vale só para a sessão, nada é
   persistido.
9. **Estante em tela baixa** (Steam Deck 1280×800): a faixa "jogados
   recentemente" cede lugar a "favoritos" para os tiles grandes
   sobreviverem (1.1.2).
10. **Mensagem "ao lado do executável" na estante vazia** descreve a
    convenção do Windows/Linux; no macOS a pasta real é
    `~/Documents/SixteeN` — diferença conhecida de texto.
11. **Um jogo por vez.** Só existe um cartucho no slot: ejetar é o caminho
    para trocar de jogo (e ejetar exige desligar).
12. **Sem logs em arquivo, por projeto.** Tudo vai ao stderr; para
    capturar, rode o binário pelo terminal com `2> arquivo.log`.

---

*Referências cruzadas: [Manual do Usuário](manual-do-usuario.md) ·
[CHANGELOG](../CHANGELOG.md) · [Avisos de terceiros](../THIRD-PARTY-NOTICES.md) ·
Plano de implementação: `docs/plano-emulador-moldura.md` e `docs/fase-0.md` … `docs/fase-4.md`.*
