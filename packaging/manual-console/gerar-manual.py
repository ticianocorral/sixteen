#!/usr/bin/env python3
# Gera o "Manual do console" — o PDF que o botão de mesmo nome da tela
# inicial abre no tubo, em português e em inglês (o app escolhe pelo idioma
# da interface). Rodar de dentro desta pasta:
#   python3 gerar-manual.py
# O leitor de manual do app só mostra páginas com imagem embutida (o pdfium
# não vai no pacote), então cada página é desenhada aqui como bitmap e o PDF
# sai como uma imagem JPEG por página — igual ao manual do pacote de exemplo
# (packaging/exemplo-mock/gerar-mocks.py), de onde vêm a paleta e o grid.
# As capturas de tela vêm de site/assets/, exceto a da tela inicial (já com
# o botão "Manual do console"), tirada pelo exemplo console_manual_shot e
# guardada aqui ao lado.

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

AQUI = Path(__file__).resolve().parent
RAIZ = AQUI.parent.parent
CAPTURAS = RAIZ / "site" / "assets"
# Saída canônica: embutida no binário (idle.rs inclui estes arquivos).
SAIDA = RAIZ / "crates" / "app" / "assets"

# Paleta da marca (a mesma do manual de exemplo)
VERMELHO = (231, 11, 23)
PRETO = (26, 21, 18)
CARVAO = (33, 29, 24)
CREME = (242, 237, 227)
CREME_MEIO = (214, 207, 193)
CREME_ESCURO = (226, 219, 204)
CINZA_TEXTO = (148, 141, 130)
CORPO = (74, 67, 58)
ROXO = (124, 92, 196)

FONTES = Path("/System/Library/Fonts/Supplemental")

LARG_PG, ALT_PG = 1000, 1450
MARGEM = 60

# Idioma da edição sendo gerada ("pt" ou "en") — `main` passa pelos dois.
IDIOMA = "pt"


def t(pt, en):
    """O texto na língua da edição corrente."""
    return pt if IDIOMA == "pt" else en


def fonte(nome, tamanho):
    return ImageFont.truetype(str(FONTES / nome), tamanho)


def quebrar(d, texto, f, largura):
    """Quebra `texto` em linhas que cabem em `largura` px."""
    linhas = []
    for paragrafo in texto.split("\n"):
        atual = ""
        for palavra in paragrafo.split(" "):
            tentativa = f"{atual} {palavra}".strip()
            if d.textlength(tentativa, font=f) <= largura:
                atual = tentativa
            else:
                linhas.append(atual)
                atual = palavra
        linhas.append(atual)
    return linhas


def paragrafo(d, x, y, texto, largura, tam=26, cor=CORPO, entre=34):
    f = fonte("Arial.ttf", tam)
    for linha in quebrar(d, texto, f, largura):
        d.text((x, y), linha, font=f, fill=cor)
        y += entre
    return y


def _pagina():
    im = Image.new("RGB", (LARG_PG, ALT_PG), CREME)
    return im, ImageDraw.Draw(im)


def _cabecalho(d, titulo):
    d.rectangle((0, 0, LARG_PG, 118), fill=VERMELHO)
    d.text((MARGEM, 59), titulo, font=fonte("Arial Black.ttf", 50),
           fill=(255, 255, 255), anchor="lm")
    d.text((LARG_PG - MARGEM, 59), t("MANUAL DO CONSOLE", "CONSOLE MANUAL"),
           font=fonte("Arial Narrow Bold.ttf", 26), fill=(255, 255, 255), anchor="rm")
    return 168


def _rodape(d, pagina):
    d.line((MARGEM, ALT_PG - 92, LARG_PG - MARGEM, ALT_PG - 92), fill=CREME_MEIO, width=2)
    d.text((MARGEM, ALT_PG - 58), t("SixteeN — manual do console", "SixteeN — console manual"),
           font=fonte("Arial.ttf", 22), fill=CINZA_TEXTO)
    d.ellipse((LARG_PG - 108, ALT_PG - 84, LARG_PG - 52, ALT_PG - 28),
              outline=VERMELHO, width=3)
    d.text((LARG_PG - 80, ALT_PG - 56), str(pagina), font=fonte("Arial Bold.ttf", 26),
           fill=VERMELHO, anchor="mm")


def captura(im, d, nome, y, legenda):
    """Captura de tela emoldurada, na largura útil da página."""
    larg = LARG_PG - MARGEM * 2
    origem = AQUI / nome if (AQUI / nome).is_file() else CAPTURAS / nome
    img = Image.open(origem).convert("RGB")
    alt = round(img.height * larg / img.width)
    img = img.resize((larg, alt), Image.LANCZOS)
    d.rectangle((MARGEM - 6, y - 6, MARGEM + larg + 6, y + alt + 6), fill=PRETO)
    im.paste(img, (MARGEM, y))
    y += alt + 18
    d.text((MARGEM, y), legenda, font=fonte("Arial Narrow Bold.ttf", 22), fill=CINZA_TEXTO)
    return y + 44


def itens(d, y, lista, larg_rotulo=250, tam=25):
    """Lista "rótulo — explicação" em duas colunas."""
    f_rot = fonte("Arial Bold.ttf", tam)
    larg = LARG_PG - MARGEM * 2 - larg_rotulo
    for rotulo, texto in lista:
        d.rectangle((MARGEM, y + 8, MARGEM + 10, y + 18), fill=VERMELHO)
        d.text((MARGEM + 22, y), rotulo, font=f_rot, fill=PRETO)
        y = paragrafo(d, MARGEM + larg_rotulo, y, texto, larg, tam=tam, entre=tam + 8) + 14
    return y


def secao(d, y, titulo):
    d.text((MARGEM, y), titulo, font=fonte("Arial Black.ttf", 32), fill=PRETO)
    return y + 54


def nota(d, y, texto):
    f = fonte("Arial Narrow Bold.ttf", 26)
    linhas = quebrar(d, texto, f, LARG_PG - MARGEM * 2 - 60)
    alt = 30 + len(linhas) * 34
    d.rounded_rectangle((MARGEM, y, LARG_PG - MARGEM, y + alt), radius=12, fill=CREME_ESCURO)
    for i, linha in enumerate(linhas):
        d.text((MARGEM + 30, y + 16 + i * 34), linha, font=f, fill=PRETO)
    return y + alt + 24


def capa():
    im = Image.new("RGB", (LARG_PG, ALT_PG), PRETO)
    d = ImageDraw.Draw(im)
    logo = Image.open(CAPTURAS / "console-logo.png").convert("RGBA")
    logo.thumbnail((760, 400))
    im.paste(logo, ((LARG_PG - logo.width) // 2, 260), logo)
    d.rectangle((0, 760, LARG_PG, 776), fill=VERMELHO)
    d.text((LARG_PG / 2, 900), t("MANUAL DO", "CONSOLE"), font=fonte("Arial Black.ttf", 92),
           fill=CREME, anchor="mm")
    d.text((LARG_PG / 2, 1000), t("CONSOLE", "MANUAL"), font=fonte("Arial Black.ttf", 92),
           fill=VERMELHO, anchor="mm")
    d.text((LARG_PG / 2, 1110),
           t("instruções de uso — leia antes de ligar", "instructions — read before powering on"),
           font=fonte("Arial.ttf", 30), fill=CREME_MEIO, anchor="mm")
    d.rectangle((0, ALT_PG - 120, LARG_PG, ALT_PG), fill=CARVAO)
    d.text((LARG_PG / 2, ALT_PG - 60),
           t("projeto pessoal, sem fins comerciais", "a personal, non-commercial project"),
           font=fonte("Arial Narrow Bold.ttf", 28), fill=CINZA_TEXTO, anchor="mm")
    return im


def comece():
    im, d = _pagina()
    y = _cabecalho(d, t("COMECE POR AQUI", "START HERE"))
    y = paragrafo(d, MARGEM, y, t(
        "O SixteeN é um console de 16 bits inteiro desenhado na tela: a TV de tubo, o "
        "painel com o slot do cartucho e as chaves de verdade. Tudo funciona com mouse "
        "ou controle — o teclado só entra para escrever anotações.",
        "SixteeN is a whole 16-bit console drawn on screen: the tube TV, the panel with "
        "the cartridge slot and real switches. Everything works with a mouse or a "
        "controller — the keyboard is only needed to write notes."),
        LARG_PG - MARGEM * 2) + 30
    passos = [
        (t("Abra o app", "Open the app"), t(
            "na primeira vez ele cria a pasta do console — no macOS, "
            "~/Documents/SNES SixteeN; no Windows e no Linux, ao lado do executável.",
            "the first time, it creates the console's folder — on macOS, "
            "~/Documents/SNES SixteeN; on Windows and Linux, next to the executable.")),
        (t("Baixe o núcleo", "Get the core"), t(
            "sem o núcleo snes9x, a TV mostra a tela de instalação com um botão para "
            "baixá-lo (e outro para o nointro.dat, opcional).",
            "without the snes9x core, the TV shows the setup screen with a button to "
            "download it (and another for nointro.dat, optional).")),
        (t("Coloque suas ROMs", "Add your ROMs"), t(
            "arquivos .sfc/.smc (ou .zip) na pasta roms/. Cada um vira uma caixa na "
            "estante de games.",
            ".sfc/.smc (or .zip) files in the roms/ folder. Each one becomes a box on "
            "the game shelf.")),
        (t("Ligue a TV", "Turn the TV on"), t(
            "no botão POWER da TV. Com ela acesa, clique em \"Estante de games\", "
            "escolha um jogo e ligue o console no POWER do painel.",
            "with the TV's POWER button. Once it's on, click \"Game shelf\", pick a "
            "game and switch the console on with the panel's POWER.")),
    ]
    larg = LARG_PG - MARGEM * 2 - 100
    for i, (titulo, corpo) in enumerate(passos, 1):
        d.ellipse((MARGEM, y, MARGEM + 72, y + 72), fill=VERMELHO)
        d.text((MARGEM + 36, y + 36), str(i), font=fonte("Arial Black.ttf", 40),
               fill=(255, 255, 255), anchor="mm")
        d.text((MARGEM + 100, y + 2), titulo, font=fonte("Arial Black.ttf", 34), fill=PRETO)
        fim = paragrafo(d, MARGEM + 100, y + 52, corpo, larg)
        y = max(fim, y + 80) + 36
    y = secao(d, y + 10, t("As pastas do console", "The console's folders"))
    y = itens(d, y, [
        ("roms/", t("seus jogos", "your games")),
        ("core/", t("o núcleo snes9x (baixado pelo app)", "the snes9x core (downloaded by the app)")),
        ("assets/", t("capas, logos, cartuchos, contracapas e manuais",
                      "covers, logos, cartridges, back covers and manuals")),
        ("saves/", t("jogos salvos e savestates", "game saves and savestates")),
        ("notes/", t("anotações e capturas de tela", "notes and screenshots")),
        ("config/", t("o sixteen.cfg, o nointro.dat e o catálogo",
                      "sixteen.cfg, nointro.dat and the catalog")),
    ], larg_rotulo=200)
    _rodape(d, 2)
    return im


def tela_inicial():
    im, d = _pagina()
    y = _cabecalho(d, t("A TELA INICIAL", "THE HOME SCREEN"))
    y = captura(im, d, "inicial.jpg", y, t("TV fora do ar no canal 3, e o painel ao lado",
                                           "the TV off the air on channel 3, with the panel beside it"))
    y = paragrafo(d, MARGEM, y, t(
        "É a casa do console: o app abre aqui, e é para cá que você volta ao ejetar um "
        "cartucho.",
        "The console's home: the app opens here, and you come back here when you eject "
        "a cartridge."), LARG_PG - MARGEM * 2) + 20
    y = secao(d, y, t("No painel", "On the panel"))
    y = itens(d, y, [
        (t("Estante de games", "Game shelf"), t(
            "abre a estante para escolher um jogo (precisa da TV ligada).",
            "opens the shelf to pick a game (the TV must be on).")),
        (t("Manual do console", "Console manual"), t(
            "abre este manual no tubo. Use < página e página > para folhear, e voltar "
            "para fechar.",
            "opens this manual on the tube. Use < page and page > to flip through, and "
            "back to close it.")),
        (t("Configurações", "Settings"), t(
            "idioma, vídeo, controles, núcleo e conquistas.",
            "language, video, controls, core and achievements.")),
    ], larg_rotulo=280)
    y = secao(d, y + 10, t("Na TV", "On the TV"))
    itens(d, y, [
        ("POWER", t("liga e desliga o tubo, com o aquecimento e o colapso do CRT.",
                    "turns the tube on and off, with the CRT warm-up and collapse.")),
        ("VOL- / VOL+", t("volume do som do jogo, com as barrinhas na tela.",
                          "game sound volume, with the on-screen bars.")),
        ("CH", t("alterna entre o canal 3 (o do console) e o 4.",
                 "switches between channel 3 (the console's) and 4.")),
    ], larg_rotulo=280)
    _rodape(d, 3)
    return im


def estante():
    im, d = _pagina()
    y = _cabecalho(d, t("A ESTANTE DE GAMES", "THE GAME SHELF"))
    y = captura(im, d, "estante.jpg", y, t(
        "as caixas em pé na prateleira, e o jogo focado no painel",
        "boxes standing on the shelf, and the focused game on the panel"))
    y = itens(d, y, [
        (t("Escolher", "Pick"), t(
            "clique numa caixa ou ande com o direcional; confirmar insere o cartucho no "
            "slot do painel.",
            "click a box or move with the d-pad; confirming inserts the cartridge into "
            "the panel's slot.")),
        (t("Buscar", "Search"), t("a busca filtra a estante pelo nome do jogo.",
                                  "search filters the shelf by game name.")),
        (t("Favoritos", "Favorites"), t(
            "Favoritar coloca o jogo na faixa de favoritos, acima da estante.",
            "Add favorite puts the game in the favorites strip, above the shelf.")),
        (t("Recentes", "Recent"), t(
            "os jogados recentemente e o histórico, com o tempo de jogo.",
            "recently played games and the history, with play time.")),
    ], larg_rotulo=200)
    y = secao(d, y + 6, t("O painel do jogo", "The game panel"))
    y = itens(d, y, [
        (t("informações", "information"), t("ano, editora e nota do jogo.",
                                            "the game's year, publisher and rating.")),
        (t("capa traseira", "back cover"), t("a contracapa — clique para ampliar no tubo.",
                                             "the back cover — click to zoom in on the tube.")),
        ("manual", t("o manual do jogo em PDF, aberto no tubo.",
                     "the game's PDF manual, opened on the tube.")),
        (t("cartucho", "cartridge"), t("a arte do cartucho, ampliada.",
                                       "the cartridge art, zoomed in.")),
        (t("Conquistas", "Achievements"), t(
            "a lista do RetroAchievements, quando o jogo tem conquistas.",
            "the RetroAchievements list, when the game has achievements.")),
    ], larg_rotulo=200)
    _rodape(d, 4)
    return im


def jogando():
    im, d = _pagina()
    y = _cabecalho(d, t("JOGANDO", "PLAYING"))
    y = captura(im, d, "screenshot.jpg", y, t(
        "o cartucho assentado no slot, e as chaves do console",
        "the cartridge seated in the slot, and the console's switches"))
    y = paragrafo(d, MARGEM, y, t(
        "Como no aparelho de verdade, o cartucho desce até o slot e o console só liga "
        "quando você aciona o POWER.",
        "Just like the real hardware, the cartridge slides into the slot and the console "
        "only turns on when you flip POWER."), LARG_PG - MARGEM * 2) + 20
    y = itens(d, y, [
        ("POWER", t("liga e desliga o console. Ao religar, o jogo continua de onde parou.",
                    "turns the console on and off. Turn it back on and the game resumes "
                    "where it left off.")),
        ("RESET", t("reinicia o jogo, como o botão do console.",
                    "restarts the game, like the console's button.")),
        (t("EJETAR", "EJECT"), t(
            "solta o cartucho — só com o console desligado — e volta para a tela inicial.",
            "releases the cartridge — only with the console off — and goes back to the "
            "home screen.")),
        (t("sessão", "session"), t(
            "o rodapé do painel conta o tempo de jogo, só com o console ligado.",
            "the panel's footer counts play time, only while the console is on.")),
    ], larg_rotulo=200)
    nota(d, y + 10, t(
        "O jogo é salvo na pasta saves/. Os savestates ficam nos modais Salvar e "
        "Carregar, com espaços próprios para cada jogo.",
        "Game saves go to the saves/ folder. Savestates live in the Save and Load "
        "dialogs, with their own slots for each game."))
    _rodape(d, 5)
    return im


def controles():
    im, d = _pagina()
    y = _cabecalho(d, t("CONTROLES", "CONTROLS"))
    y = paragrafo(d, MARGEM, y, t(
        "Um controle USB ou Bluetooth funciona assim que é conectado. O teclado também "
        "joga. Os dois podem ser remapeados em Configurações → controles.",
        "A USB or Bluetooth controller works as soon as it's connected. The keyboard "
        "plays too. Both can be remapped in Settings → controls."),
        LARG_PG - MARGEM * 2) + 30
    cols = (MARGEM, MARGEM + 300, MARGEM + 560)
    d.rectangle((MARGEM - 10, y - 10, LARG_PG - MARGEM + 10, y + 46), fill=PRETO)
    for x, rot in zip(cols, (t("botão do SNES", "SNES button"), t("teclado", "keyboard"),
                             t("controle", "controller"))):
        d.text((x + 10, y + 18), rot, font=fonte("Arial Bold.ttf", 26), fill=CREME, anchor="lm")
    y += 64
    linhas = [
        (t("Direcional", "D-pad"), t("setas", "arrows"), t("direcional", "d-pad")),
        ("B", "Z", t("botão de baixo", "bottom button")),
        ("A", "X", t("botão da direita", "right button")),
        ("Y", "A", t("botão da esquerda", "left button")),
        ("X", "S", t("botão de cima", "top button")),
        ("L / R", "Q / W", t("ombros", "shoulders")),
        ("START", "Enter", "start"),
        ("SELECT", t("Shift direito", "Right Shift"), "back / select"),
    ]
    f = fonte("Arial.ttf", 26)
    for i, linha in enumerate(linhas):
        if i % 2 == 0:
            d.rectangle((MARGEM - 10, y - 8, LARG_PG - MARGEM + 10, y + 40), fill=CREME_ESCURO)
        for j, (x, cel) in enumerate(zip(cols, linha)):
            d.text((x + 10, y + 16), cel, font=fonte("Arial Bold.ttf", 26) if j == 0 else f,
                   fill=PRETO if j == 0 else CORPO, anchor="lm")
        y += 48
    y = secao(d, y + 40, t("Nos menus", "In menus"))
    itens(d, y, [
        (t("Confirmar", "Confirm"), t("A no controle, Enter no teclado — ou um clique.",
                                      "A on the controller, Enter on the keyboard — or a click.")),
        (t("Voltar", "Back"), t(
            "B no controle, Esc no teclado. Na tela inicial, Voltar fecha o app.",
            "B on the controller, Esc on the keyboard. On the home screen, Back quits "
            "the app.")),
        (t("Navegar", "Navigate"), t("o direcional anda pela estante e pelas listas.",
                                     "the d-pad moves around the shelf and lists.")),
    ], larg_rotulo=200)
    _rodape(d, 6)
    return im


def anotacoes():
    im, d = _pagina()
    y = _cabecalho(d, t("ANOTAÇÕES", "NOTES"))
    y = captura(im, d, "anotacoes.jpg", y, t("o livro de duas páginas, com o jogo pausado",
                                              "the two-page notebook, with the game paused"))
    y = paragrafo(d, MARGEM, y, t(
        "O botão de anotações pausa o jogo e abre um livro de duas páginas, guardado "
        "por jogo.",
        "The notes button pauses the game and opens a two-page notebook, kept per game."),
        LARG_PG - MARGEM * 2) + 20
    itens(d, y, [
        ("Cheats", t(
            "códigos selecionados para cada jogo, ligados por checklist. Os ativos "
            "aparecem no painel.",
            "hand-picked codes for each game, toggled from a checklist. The active ones "
            "show on the panel.")),
        (t("Nota", "Note"), t("texto livre — aqui o teclado entra em cena.",
                              "free text — this is where the keyboard comes in.")),
        (t("Capturas", "Screenshots"), t(
            "15 espaços para prints da tela. Dê nome e fixe os que quiser manter: um "
            "print fixado não é sobrescrito.",
            "15 slots for screenshots. Name and pin the ones you want to keep: a pinned "
            "screenshot is never overwritten.")),
        (t("Voltar ao jogo", "Back to the game"), t("fecha o livro e despausa.",
                                                    "closes the notebook and unpauses.")),
    ], larg_rotulo=240)
    _rodape(d, 7)
    return im


def artes():
    im, d = _pagina()
    y = _cabecalho(d, t("ARTES E MANUAIS", "ART AND MANUALS"))
    y = paragrafo(d, MARGEM, y, t(
        "O console não busca nada na internet: as artes dos jogos vêm de imagens suas. "
        "Dê ao arquivo o mesmo nome da ROM, sem a extensão.",
        "The console doesn't scrape anything online: game art comes from your own "
        "images. Give the file the same name as the ROM, without the extension."),
        LARG_PG - MARGEM * 2) + 20
    y = nota(d, y, t("Super Jogo (USA).sfc  →  assets/cover/Super Jogo (USA).png",
                     "Super Game (USA).sfc  →  assets/cover/Super Game (USA).png"))
    y = itens(d, y, [
        ("assets/cover/", t(
            "a capa da caixa, na estante (2D ou caixa 3D com fundo transparente).",
            "the box cover, on the shelf (2D, or a 3D box with a transparent background).")),
        ("assets/logo/", t("a logo do jogo, no topo do painel.",
                           "the game's logo, at the top of the panel.")),
        ("assets/cartridge/", t("o cartucho que desce no slot.",
                                "the cartridge that slides into the slot.")),
        ("assets/backcover/", t("a contracapa, ampliável no tubo.",
                                "the back cover, zoomable on the tube.")),
        ("assets/manual/", t("o manual em PDF (Super Jogo (USA).pdf).",
                             "the PDF manual (Super Game (USA).pdf).")),
    ], larg_rotulo=280)
    y = secao(d, y + 10, t("Bom saber", "Good to know"))
    itens(d, y, [
        (t("Exemplos", "Examples"), t(
            "cada pasta traz um example.* com o tamanho e o formato certos — use como "
            "modelo.",
            "each folder comes with an example.* in the right size and shape — use it "
            "as a template.")),
        (t("Manuais", "Manuals"), t(
            "o leitor do tubo mostra PDFs escaneados ou feitos de imagens, como este. "
            "PDFs só de texto aparecem como página não disponível.",
            "the tube's reader shows scanned or image-based PDFs, like this one. "
            "Text-only PDFs show up as page not available.")),
        (t("Nomes", "Names"), t(
            "ROMs com nome fora do padrão podem ser renomeadas para o padrão No-Intro "
            "nas configurações — saves, notas e artes vão junto.",
            "ROMs with non-standard names can be renamed to No-Intro names in settings — "
            "saves, notes and art move along.")),
    ], larg_rotulo=200)
    _rodape(d, 8)
    return im


def configuracoes():
    im, d = _pagina()
    y = _cabecalho(d, t("CONFIGURAÇÕES", "SETTINGS"))
    y = paragrafo(d, MARGEM, y, t(
        "Os ajustes valem na hora e ficam gravados no config/sixteen.cfg. A tela é "
        "dividida em seções:",
        "Changes apply right away and are saved to config/sixteen.cfg. The screen is "
        "split into sections:"), LARG_PG - MARGEM * 2) + 24
    grupos = [
        (t("jogo", "game"), [(t("Controles", "Controls"), t(
            "a configuração dos controles do jogo.", "the game's control setup."))]),
        (t("vídeo", "video"), [
            (t("Tela cheia", "Fullscreen"), t(
                "ligada por padrão, na resolução nativa do monitor.",
                "on by default, at the monitor's native resolution.")),
            (t("Chiado", "Static hiss"), t("o som da estática com a TV fora do ar.",
                                           "the static's sound with the TV off the air.")),
            (t("Estante", "Shelf"), t(
                "móvel de madeira ou expositor de metal da locadora.",
                "wooden bookcase or metal rental-store rack.")),
        ]),
        (t("sistema", "system"), [
            (t("Atualizações", "Updates"), t("procura versões novas ao abrir o app.",
                                             "checks for new versions on launch.")),
            (t("Idioma", "Language"), t("português ou inglês.", "English or Portuguese.")),
            (t("Núcleo", "Core"), t("baixa ou atualiza o snes9x.",
                                    "downloads or updates snes9x.")),
            ("No-Intro", t("renomeia as ROMs para os nomes No-Intro.",
                           "renames ROMs to their No-Intro names.")),
        ]),
        (t("conquistas", "achievements"), [
            (t("Conta", "Account"), t(
                "usuário, senha e chave da web API do RetroAchievements. Com a conta "
                "conectada, o queixo da TV mostra RA ATIVADO.",
                "RetroAchievements username, password and web API key. With the account "
                "connected, the TV's chin shows RA ENABLED.")),
        ]),
        (t("controles", "controls"), [
            (t("Remapear", "Remap"), t(
                "clique numa ação e aperte a tecla ou o botão novo; Esc cancela.",
                "click an action and press the new key or button; Esc cancels.")),
        ]),
    ]
    for nome, lista in grupos:
        d.rounded_rectangle((MARGEM, y, MARGEM + 210, y + 40), radius=8, fill=ROXO)
        d.text((MARGEM + 105, y + 20), nome, font=fonte("Arial Bold.ttf", 24),
               fill=(255, 255, 255), anchor="mm")
        y = itens(d, y + 56, lista, larg_rotulo=220, tam=24) + 10
    _rodape(d, 9)
    return im


def ajuda():
    im, d = _pagina()
    y = _cabecalho(d, t("PRECISA DE AJUDA?", "NEED HELP?"))
    duvidas = [
        (t("A estante não abre", "The shelf won't open"), t(
            "a estante mora no tubo: ligue a TV no botão POWER dela. Sem o núcleo snes9x "
            "instalado, a estante e as configurações ficam travadas — baixe o núcleo "
            "pelo botão do painel.",
            "the shelf lives on the tube: turn the TV on with its POWER button. Without "
            "the snes9x core installed, the shelf and settings are locked — download the "
            "core with the panel's button.")),
        (t("O jogo não aparece", "A game doesn't show up"), t(
            "confira se a ROM está em roms/ (.sfc, .smc ou .zip).",
            "check that the ROM is in roms/ (.sfc, .smc or .zip).")),
        (t("A arte não aparece", "The art doesn't show up"), t(
            "o nome do arquivo tem que ser igual ao da ROM, sem extensão. "
            "Super Jogo.png ↔ Super Jogo.sfc.",
            "the file name must match the ROM's, without the extension. "
            "Super Game.png ↔ Super Game.sfc.")),
        (t("O cartucho não sai", "The cartridge won't eject"), t(
            "Ejetar só funciona com o console desligado: desligue no POWER do painel "
            "primeiro.",
            "Eject only works with the console off: switch it off with the panel's POWER "
            "first.")),
        (t("O controle não responde", "The controller doesn't respond"), t(
            "reconecte e confira o mapeamento em Configurações → controles. Controles "
            "genéricos às vezes trocam os botões de lugar.",
            "reconnect it and check the mapping in Settings → controls. Generic "
            "controllers sometimes swap buttons around.")),
        (t("Há uma versão nova", "There's a new version"), t(
            "uma seta verde acende ao lado da versão, no queixo da TV. Clique nela para "
            "ver as novidades e atualizar.",
            "a green arrow lights up next to the version, on the TV's chin. Click it to "
            "see what's new and update.")),
    ]
    larg = LARG_PG - MARGEM * 2
    for pergunta, resposta in duvidas:
        d.text((MARGEM, y), pergunta, font=fonte("Arial Black.ttf", 30), fill=PRETO)
        y = paragrafo(d, MARGEM, y + 46, resposta, larg) + 30
    d.rectangle((0, ALT_PG - 230, LARG_PG, ALT_PG - 120), fill=PRETO)
    d.text((LARG_PG / 2, ALT_PG - 175), t("bom jogo!", "have fun!"),
           font=fonte("Arial Black.ttf", 46), fill=CREME, anchor="mm")
    _rodape(d, 10)
    return im


def main():
    global IDIOMA
    for IDIOMA in ("pt", "en"):
        paginas = [capa(), comece(), tela_inicial(), estante(), jogando(),
                   controles(), anotacoes(), artes(), configuracoes(), ajuda()]
        destino = SAIDA / f"console-manual-{IDIOMA}.pdf"
        paginas[0].save(destino, save_all=True, append_images=paginas[1:],
                        resolution=150.0, quality=88,
                        title=t("SixteeN — Manual do console", "SixteeN — Console manual"))
        print(f"{destino.name} ok ({destino.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
