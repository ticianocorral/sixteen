#!/usr/bin/env python3
# Gera o kit de artes de exemplo do pacote do app (mock): cartucho sem
# rótulo, cover, backcover e manual em PDF. Rodar de dentro desta pasta:
#   python3 gerar-mocks.py
# O cartucho base vem do zip de exemplo do repositório
# (../../exemplo-world-cup-2026.zip) — o rótulo é apagado e o poço do
# rótulo vira plástico liso, pronto pra colar uma arte nova por cima.

import zipfile
from io import BytesIO
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

AQUI = Path(__file__).resolve().parent
ZIP_EXEMPLO = AQUI.parent.parent / "exemplo-world-cup-2026.zip"
# Saída canônica: embutida no binário (exemplo.rs inclui estes arquivos) e
# copiada pro pacote de exemplo (exemplo-platformer-example.zip).
SAIDA = AQUI.parent.parent / "crates" / "app" / "assets"

# Paleta da marca (vermelho do wordmark SixteeN e os neutros do favicon)
VERMELHO = (231, 11, 23)
VERMELHO_ESCURO = (158, 8, 16)
PRETO = (26, 21, 18)
CARVAO = (33, 29, 24)
CARVAO_CLARO = (43, 38, 32)
CREME = (242, 237, 227)
CREME_MEIO = (214, 207, 193)
CINZA_TEXTO = (148, 141, 130)

FONTES = Path("/System/Library/Fonts/Supplemental")


def fonte(nome, tamanho):
    return ImageFont.truetype(str(FONTES / nome), tamanho)


def texto_espacado(draw, xy, texto, f, cor, espaco=0, ancora=None):
    """Texto com letter-spacing manual (pro visual de etiqueta)."""
    if espaco <= 0:
        draw.text(xy, texto, font=f, fill=cor, anchor=ancora)
        return
    larguras = [draw.textlength(ch, font=f) for ch in texto]
    total = sum(larguras) + espaco * (len(texto) - 1)
    x, y = xy
    if ancora in ("mm", "ma", "ms"):
        x -= total / 2
    elif ancora in ("rm", "ra", "rs"):
        x -= total
    for ch, largura in zip(texto, larguras):
        draw.text((x, y), ch, font=f, fill=cor, anchor="l" + (ancora[1] if ancora else "m"))
        x += largura + espaco


def fundo_listrado(w, h, base, listra, passo=26, angulo=0):
    """Fundo escuro com listras diagonais sutis (textura de caixa)."""
    im = Image.new("RGB", (w, h), base)
    d = ImageDraw.Draw(im)
    for i in range(-h, w + h, passo * 2):
        d.polygon(
            [(i, 0), (i + passo, 0), (i + passo + h, h), (i + h, h)],
            fill=listra,
        )
    return im.rotate(angulo, expand=False, fillcolor=base)


def chip(draw, xy, texto, f, fundo=VERMELHO, cor=(255, 255, 255), pad=(18, 10)):
    x, y = xy
    tw = draw.textlength(texto, font=f)
    th = f.size
    caixa = (x, y, x + tw + pad[0] * 2, y + th + pad[1] * 2)
    draw.rounded_rectangle(caixa, radius=6, fill=fundo)
    draw.text((x + pad[0], y + pad[1] - 2), texto, font=f, fill=cor)
    return caixa


def codigo_de_barras(im, xy, largura, altura, cor=(20, 17, 14), fundo=CREME):
    """Código de barras fake determinístico (visual, não escaneável)."""
    x0, y0 = xy
    d = ImageDraw.Draw(im)
    d.rectangle((x0, y0, x0 + largura, y0 + altura), fill=fundo)
    # barras na faixa de cima; dígitos ficam na faixa livre de baixo
    fim_barras = y0 + altura - 30
    rng = np.random.default_rng(2026)
    x = x0 + 8
    while x < x0 + largura - 10:
        espessura = int(rng.integers(2, 7))
        if rng.random() < 0.55:
            d.rectangle((x, y0 + 6, x + espessura, fim_barras), fill=cor)
        x += espessura + int(rng.integers(2, 6))
    d.text((x0 + largura / 2, y0 + altura - 15), "6 51016 00000 0",
           font=fonte("Arial.ttf", 20), fill=cor, anchor="mm")


# ---------------------------------------------------------------- cartucho
# Rótulo do cartucho do zip de exemplo, em coordenadas do PNG (1400×900).
ROTULO = (258, 4, 1146, 414)


def cartucho_limpo():
    with zipfile.ZipFile(ZIP_EXEMPLO) as z:
        bruto = z.read("assets/cartridge/World Cup 2026.png")
    im = Image.open(BytesIO(bruto)).convert("RGBA")
    arr = np.array(im).astype(np.float64)

    x0, y0, x1, y1 = ROTULO
    a = arr[..., 3]

    # cor base por coluna, amostrada da faixa lisa logo abaixo do poço
    faixa = arr[y1 + 12 : y1 + 52, x0 : x1, :3]
    peso = (a[y1 + 12 : y1 + 52, x0 : x1, None] / 255.0)
    base = (faixa * peso).sum(axis=0) / np.maximum(peso.sum(axis=0), 1e-6)

    alt = y1 - y0
    # sombra suave no topo do poço (luz vem de cima) clareando pra baixo
    gradiente = np.linspace(0.93, 1.015, alt)[:, None, None]
    ruido = np.random.default_rng(16).normal(0, 2.2, (alt, x1 - x0, 1))
    preench = np.clip(base[None, :, :] * gradiente + ruido, 0, 255)

    # sombra interna na borda do poço (o rótulo fica rebaixo no shell)
    yy = np.arange(alt)[:, None]
    xx = np.arange(x1 - x0)[None, :]
    borda = np.minimum(
        np.minimum(yy, alt - 1 - yy), np.minimum(xx, (x1 - x0 - 1) - xx)
    ).astype(float)
    sombra = np.clip(1 - borda / 14.0, 0, 1) * 0.35
    # em cima a sombra é mais forte (borda superior do poço)
    sombra_topo = np.clip(1 - yy / 22.0, 0, 1) * 0.5
    fator = 1 - np.maximum(sombra, sombra_topo)[..., None]
    preench = np.clip(preench * fator, 0, 255)

    # só pinta onde o shell existe; pena de 6px nas bordas pra fundir
    regiao = a[y0:y1, x0:x1]
    alvo = (regiao > 10).astype(np.float64)
    pena = 6.0
    alvo = np.clip(
        np.minimum(
            np.minimum((yy + pena) / pena, (alt - 1 - yy + pena) / pena),
            np.minimum((xx + pena) / pena, ((x1 - x0 - 1) - xx + pena) / pena),
        ),
        0,
        1,
    )
    mescla = alvo[..., None]
    original = arr[y0:y1, x0:x1, :3]
    arr[y0:y1, x0:x1, :3] = original * (1 - mescla) + preench * mescla

    saida = Image.fromarray(np.uint8(arr.clip(0, 255)))
    saida.save(SAIDA / "exemplo-cartucho.png")
    return saida


# ------------------------------------------------------------------ cover
LARG_COVER, ALT_COVER = 1024, 777


def cover():
    im = fundo_listrado(LARG_COVER, ALT_COVER, PRETO, (31, 26, 22))
    d = ImageDraw.Draw(im)

    # moldura fina interna, ecoando o favicon
    d.rectangle((14, 14, LARG_COVER - 15, ALT_COVER - 15), outline=CREME_MEIO, width=2)

    f_chip = fonte("Arial Narrow Bold.ttf", 26)
    chip(d, (40, 38), "PACOTE DE EXEMPLO", f_chip)

    # título grande com sombra dura vermelha (fonte cabe na coluna esquerda)
    largura_titulo = 470
    tam = 160
    while True:
        f_titulo = fonte("Arial Black.ttf", tam)
        larguras = [d.textlength(p, font=f_titulo)
                    for p in ("JOGO", "EXEMPLO")]
        if max(larguras) <= largura_titulo or tam <= 80:
            break
        tam -= 4
    alt_linha = int(tam * 1.18)
    for dy, cor in ((10, VERMELHO), (0, CREME)):
        d.text((46, 250 + dy), "JOGO", font=f_titulo, fill=cor)
        d.text((46, 250 + alt_linha + dy), "EXEMPLO", font=f_titulo, fill=cor)
    y_regra = 250 + alt_linha * 2 + 24
    d.rectangle((52, y_regra, 52 + max(larguras), y_regra + 8), fill=VERMELHO)
    f_sub = fonte("Arial Narrow Bold.ttf", 30)
    d.text((52, y_regra + 28), "edição demonstração", font=f_sub, fill=CREME_MEIO)
    d.text((52, y_regra + 66), "troque esta capa pela sua arte",
           font=f_sub, fill=CINZA_TEXTO)

    # placeholder de captura, à direita
    px0, py0, px1, py1 = 640, 190, 984, 470
    d.rounded_rectangle((px0 - 6, py0 - 6, px1 + 6, py1 + 6), radius=14, fill=CARVAO)
    d.rounded_rectangle((px0, py0, px1, py1), radius=10, fill=(17, 14, 12))
    passo = 42
    for gx in range(px0 + passo, px1, passo):
        d.line((gx, py0 + 8, gx, py1 - 8), fill=(48, 42, 36), width=2)
    for gy in range(py0 + passo, py1, passo):
        d.line((px0 + 8, gy, px1 - 8, gy), fill=(48, 42, 36), width=2)
    f_ph = fonte("Arial Narrow Bold.ttf", 28)
    texto_espacado(
        d,
        ((px0 + px1) / 2, (py0 + py1) / 2 - 12),
        "SUA IMAGEM AQUI",
        f_ph,
        CREME,
        espaco=3,
        ancora="mm",
    )
    f_ph2 = fonte("Arial.ttf", 22)
    d.text(((px0 + px1) / 2, (py0 + py1) / 2 + 26), "captura do jogo (qualquer tamanho)",
           font=f_ph2, fill=CINZA_TEXTO, anchor="mm")
    # canto vermelho dobrado, estilo foto colada
    d.polygon((px1 - 34, py0, px1, py0, px1, py0 + 34), fill=VERMELHO)

    # selo redondo
    sx, sy, sr = 770, 600, 88
    d.ellipse((sx - sr, sy - sr, sx + sr, sy + sr), fill=CARVAO, outline=CREME_MEIO, width=3)
    d.ellipse((sx - sr + 12, sy - sr + 12, sx + sr - 12, sy + sr - 12),
              outline=VERMELHO, width=3)
    texto_espacado(d, (sx, sy - 22), "SIXTEEN", fonte("Arial Black.ttf", 30),
                   CREME, espaco=2, ancora="mm")
    texto_espacado(d, (sx, sy + 14), "EXEMPLO", fonte("Arial Narrow Bold.ttf", 24),
                   VERMELHO, espaco=4, ancora="mm")

    # faixa inferior, ecoando a etiqueta do console
    d.rectangle((0, ALT_COVER - 78, LARG_COVER, ALT_COVER), fill=VERMELHO)
    texto_espacado(
        d,
        (LARG_COVER / 2, ALT_COVER - 40),
        "RETROCONSOLE SYSTEM",
        fonte("Arial Black.ttf", 34),
        (255, 255, 255),
        espaco=10,
        ancora="mm",
    )

    im.convert("RGB").save(SAIDA / "exemplo-cover.png")
    return im


# -------------------------------------------------------------- backcover
LARG_BACK, ALT_BACK = 1024, 773


def backcover():
    im = fundo_listrado(LARG_BACK, ALT_BACK, PRETO, (31, 26, 22))
    d = ImageDraw.Draw(im)

    f_head = fonte("Arial Black.ttf", 40)
    for dy, cor in ((4, VERMELHO), (0, CREME)):
        d.text((LARG_BACK / 2, 64 + dy), "COLOQUE SEU JOGO NA ESTANTE",
               font=f_head, fill=cor, anchor="mm")
    d.rectangle((60, 108, LARG_BACK - 60, 112), fill=VERMELHO)

    # coluna esquerda: texto + ficha técnica
    f_corpo = fonte("Arial.ttf", 24)
    f_negrito = fonte("Arial Bold.ttf", 24)
    paragrafos = [
        "Este pacote é um exemplo: uma ROM de",
        "demonstração e as artes que o SixteeN",
        "procura nas pastas do app.",
    ]
    y = 150
    for linha in paragrafos:
        d.text((60, y), linha, font=f_corpo, fill=CREME_MEIO)
        y += 32
    y = 254
    itens = [
        ("assets/cover/", "capa — tile da estante"),
        ("assets/logo/", "logo — topo do painel"),
        ("assets/cartridge/", "cartucho — aba Cartucho"),
        ("assets/backcover/", "contracapa — aba Capa traseira"),
        ("assets/manual/", "manual — PDF aberto no tubo"),
    ]
    for pasta, descricao in itens:
        d.rectangle((60, y + 7, 72, y + 19), fill=VERMELHO)
        d.text((84, y), pasta, font=f_negrito, fill=CREME)
        d.text((84, y + 28), descricao, font=f_corpo, fill=CINZA_TEXTO)
        y += 56

    # ficha técnica compacta, em duas colunas
    ficha = [("JOGADORES", "1–2"), ("BACKUP", "16 Megabit"),
             ("TIPO", "Demonstração"), ("IDIOMA", "Português")]
    f_ficha = fonte("Arial Bold.ttf", 18)
    for i, (rotulo, valor) in enumerate(ficha):
        fx = 60 + (i % 2) * 314
        fy = 566 + (i // 2) * 48
        d.rectangle((fx, fy, fx + 300, fy + 44), outline=CREME_MEIO, width=2)
        d.rectangle((fx, fy, fx + 134, fy + 44), fill=CARVAO)
        d.text((fx + 12, fy + 22), rotulo, font=f_ficha, fill=CREME, anchor="lm")
        d.text((fx + 146, fy + 22), valor, font=fonte("Arial.ttf", 22),
               fill=CREME_MEIO, anchor="lm")

    # coluna direita: 4 capturas placeholders
    capturas = [(570, 150), (779, 150), (570, 330), (779, 330)]
    f_leg = fonte("Arial Narrow Bold.ttf", 22)
    for i, (cx, cy) in enumerate(capturas):
        d.rounded_rectangle((cx, cy, cx + 195, cy + 150), radius=8, fill=(17, 14, 12),
                            outline=CARVAO_CLARO, width=2)
        d.text((cx + 97, cy + 66), "▢", font=fonte("Arial Unicode.ttf", 44),
               fill=(70, 63, 55), anchor="mm")
        d.text((cx + 97, cy + 112), "CAPTURA DE TELA", font=f_leg,
               fill=CINZA_TEXTO, anchor="mm")
        if i == 1:
            d.polygon((cx + 195 - 26, cy, cx + 195, cy, cx + 195, cy + 26),
                      fill=VERMELHO)

    # base: código de barras e miudinho
    codigo_de_barras(im, (700, 620), 264, 92)
    d.text((60, 682), "Pacote de exemplo do SixteeN — copie e adapte.",
           font=fonte("Arial Bold.ttf", 22), fill=CREME_MEIO)

    d.rectangle((0, ALT_BACK - 44, LARG_BACK, ALT_BACK), fill=CARVAO)
    texto_espacado(d, (LARG_BACK / 2, ALT_BACK - 22),
                   "SIXTEEN  •  RETROCONSOLE SYSTEM",
                   fonte("Arial Narrow Bold.ttf", 24), CINZA_TEXTO,
                   espaco=4, ancora="mm")

    im.convert("RGB").save(SAIDA / "exemplo-backcover.png")
    return im


# ----------------------------------------------------------------- manual
LARG_PG, ALT_PG = 1000, 1450


def _pagina(base):
    im = Image.new("RGB", (LARG_PG, ALT_PG), base)
    return im, ImageDraw.Draw(im)


def _cabecalho(d, titulo, escuro=False):
    d.rectangle((0, 0, LARG_PG, 118), fill=VERMELHO if not escuro else CARVAO)
    f = fonte("Arial Black.ttf", 52)
    cor = (255, 255, 255) if not escuro else CREME
    d.text((60, 59), titulo, font=f, fill=cor, anchor="lm")
    texto_espacado(d, (LARG_PG - 60, 59), "JOGO EXEMPLO",
                   fonte("Arial Narrow Bold.ttf", 28),
                   (255, 255, 255) if not escuro else CINZA_TEXTO,
                   espaco=3, ancora="rm")
    return 170


def _rodape(d, pagina, escuro=False):
    cor = CINZA_TEXTO if not escuro else (110, 102, 92)
    d.line((60, ALT_PG - 92, LARG_PG - 60, ALT_PG - 92), fill=CREME_MEIO, width=2)
    d.text((60, ALT_PG - 58), "SixteeN — pacote de exemplo",
           font=fonte("Arial.ttf", 22), fill=cor)
    d.ellipse((LARG_PG - 108, ALT_PG - 84, LARG_PG - 52, ALT_PG - 28),
              outline=VERMELHO, width=3)
    d.text((LARG_PG - 80, ALT_PG - 56), str(pagina), font=fonte("Arial Bold.ttf", 26),
           fill=VERMELHO, anchor="mm")


def manual(cap):
    paginas = []

    # 1 — capa: cartucho limpo sobre creme
    im, d = _pagina(CREME)
    d.rectangle((0, 0, LARG_PG, 300), fill=PRETO)
    d.rectangle((0, 300, LARG_PG, 316), fill=VERMELHO)
    d.text((LARG_PG / 2, 130), "SixteeN", font=fonte("Arial Black.ttf", 110),
           fill=CREME, anchor="mm")
    texto_espacado(d, (LARG_PG / 2, 230), "RETROCONSOLE SYSTEM",
                   fonte("Arial Narrow Bold.ttf", 40), CINZA_TEXTO,
                   espaco=12, ancora="mm")
    d.text((LARG_PG / 2, 420), "MANUAL DE", font=fonte("Arial Black.ttf", 84),
           fill=PRETO, anchor="mm")
    d.text((LARG_PG / 2, 510), "INSTRUÇÕES", font=fonte("Arial Black.ttf", 84),
           fill=VERMELHO, anchor="mm")
    # cartucho sem rótulo como ilustração da capa
    mini = cap.copy()
    mini.thumbnail((640, 640))
    sombra = Image.new("RGBA", mini.size, (0, 0, 0, 0))
    masc = Image.new("L", mini.size, 0)
    masc.paste(mini.split()[3], (0, 0))
    sombra.putalpha(masc.point(lambda v: v // 3))
    im.paste(Image.new("RGB", mini.size, (26, 21, 18)),
             (LARG_PG // 2 - mini.width // 2 + 14, 654), sombra)
    im.paste(mini, (LARG_PG // 2 - mini.width // 2, 640), mini)
    d.rectangle((0, ALT_PG - 120, LARG_PG, ALT_PG), fill=PRETO)
    d.text((LARG_PG / 2, ALT_PG - 60), "edição demonstração — substitua este manual pelo seu",
           font=fonte("Arial Narrow Bold.ttf", 28), fill=CREME_MEIO, anchor="mm")
    paginas.append(im)

    # 2 — comece por aqui
    im, d = _pagina(CREME)
    y = _cabecalho(d, "COMECE POR AQUI")
    passos = [
        ("Extraia o pacote", "junte as pastas roms/ e assets/ deste zip à pasta\ndo SixteeN (no macOS: ~/Documents/SixteeN)."),
        ("Coloque suas ROMs", "arquivos .sfc/.smc (ou .zip) dentro de roms/ —\ncada um aparece na estante ao abrir o app."),
        ("Dê arte aos jogos", "solte <nome-da-rom>.png em assets/{cover,logo,\ncartridge,backcover} — os tamanhos deste kit servem."),
        ("Instale o núcleo", "sem snes9x instalado, o próprio app oferece o\ndownload na primeira abertura."),
    ]
    for i, (titulo, corpo) in enumerate(passos, 1):
        d.ellipse((60, y, 132, y + 72), fill=VERMELHO)
        d.text((96, y + 36), str(i), font=fonte("Arial Black.ttf", 40),
               fill=(255, 255, 255), anchor="mm")
        d.text((160, y + 2), titulo, font=fonte("Arial Black.ttf", 34), fill=PRETO)
        for j, linha in enumerate(corpo.split("\n")):
            d.text((160, y + 52 + j * 30), linha, font=fonte("Arial.ttf", 26),
                   fill=(74, 67, 58))
        y += 172
    d.rounded_rectangle((60, y + 6, LARG_PG - 60, y + 96), radius=12, fill=(226, 219, 204))
    d.text((LARG_PG / 2, y + 51), "este manual abre no tubo: aba Manual na estante",
           font=fonte("Arial Narrow Bold.ttf", 28), fill=PRETO, anchor="mm")
    _rodape(d, 2)
    paginas.append(im)

    # 3 — controles
    im, d = _pagina(PRETO)
    y = _cabecalho(d, "CONTROLES", escuro=True)
    # corpo do controle
    cx, cy = LARG_PG / 2, y + 300
    d.rounded_rectangle((cx - 380, cy - 170, cx + 380, cy + 170), radius=90,
                        fill=(168, 163, 172), outline=(120, 115, 124), width=4)
    # d-pad
    dx, dy = cx - 200, cy
    d.rounded_rectangle((dx - 26, dy - 88, dx + 26, dy + 88), radius=14, fill=(40, 36, 32))
    d.rounded_rectangle((dx - 88, dy - 26, dx + 88, dy + 26), radius=14, fill=(40, 36, 32))
    # botões XYAB em losango
    bx, by = cx + 200, cy
    for ox, oy, letra, cor in ((0, -66, "X", VERMELHO), (-66, 0, "Y", VERMELHO),
                               (66, 0, "A", (58, 53, 48)), (0, 66, "B", (58, 53, 48))):
        d.ellipse((bx + ox - 44, by + oy - 44, bx + ox + 44, by + oy + 44),
                  fill=cor, outline=(20, 17, 14), width=3)
        d.text((bx + ox, by + oy + 2), letra, font=fonte("Arial Black.ttf", 34),
               fill=CREME, anchor="mm")
    # start/select
    for ox, rotulo in ((-70, "SELECT"), (70, "START")):
        d.rounded_rectangle((cx + ox - 52, cy + 108, cx + ox + 52, cy + 136),
                            radius=14, fill=(40, 36, 32))
        d.text((cx + ox, cy + 158), rotulo, font=fonte("Arial Narrow Bold.ttf", 20),
               fill=CINZA_TEXTO, anchor="mm")
    tabela = [
        ("Direcional", "mover"), ("B", "correr / voltar"),
        ("A", "pular / confirmar"), ("Y", "ação"), ("X", "menu"),
        ("L / R", "ombros"), ("START", "pausar"), ("SELECT", "alternar"),
    ]
    y += 540
    meio = 4
    for i, (tecla, acao) in enumerate(tabela):
        col = i // meio
        linha = i % meio
        tx = 110 + col * 460
        ty = y + linha * 52
        d.text((tx, ty), tecla, font=fonte("Arial Bold.ttf", 28), fill=CREME)
        d.text((tx + 180, ty), acao, font=fonte("Arial.ttf", 28), fill=CINZA_TEXTO)
    _rodape(d, 3, escuro=True)
    paginas.append(im)

    # 4 — anotações
    im, d = _pagina(CREME)
    _cabecalho(d, "ANOTAÇÕES")
    d.line((120, 190, 120, ALT_PG - 130), fill=VERMELHO, width=3)
    y = 230
    while y < ALT_PG - 140:
        d.line((150, y, LARG_PG - 80, y), fill=(198, 190, 175), width=2)
        y += 74
    _rodape(d, 4)
    paginas.append(im)

    # 5 — verso
    im, d = _pagina(CREME)
    y = _cabecalho(d, "PRECISA DE AJUDA?")
    duvidas = [
        ("O jogo não aparece na estante", "confira se a ROM está em roms/ e clique em\nAtualizar; .zip também vale."),
        ("A arte não aparece no painel", "o nome do arquivo tem que ser igual ao da ROM\n(sem extensão): Jogo Exemplo.png ↔ Jogo Exemplo.sfc."),
        ("O manual não abre", "o leitor mostra páginas com imagem embutida\n(scan ou PDF de imagens) — como este."),
    ]
    for pergunta, resposta in duvidas:
        d.text((60, y), pergunta, font=fonte("Arial Black.ttf", 30), fill=PRETO)
        for j, linha in enumerate(resposta.split("\n")):
            d.text((60, y + 44 + j * 30), linha, font=fonte("Arial.ttf", 26),
                   fill=(74, 67, 58))
        y += 150
    codigo_de_barras(im, (60, ALT_PG - 260), 300, 110)
    d.text((400, ALT_PG - 240), "Pacote de exemplo do SixteeN.",
           font=fonte("Arial.ttf", 24), fill=(74, 67, 58))
    d.text((400, ALT_PG - 208), "Imagens e textos livres — use como modelo.",
           font=fonte("Arial.ttf", 24), fill=(74, 67, 58))
    _rodape(d, 5)
    paginas.append(im)

    primeira = paginas[0].convert("RGB")
    primeira.save(SAIDA / "exemplo-manual.pdf", save_all=True,
                  append_images=[p.convert("RGB") for p in paginas[1:]],
                  resolution=150.0, title="Jogo Exemplo — Manual")
    return paginas


def main():
    cap = cartucho_limpo()
    print("exemplo-cartucho.png ok")
    cover()
    print("exemplo-cover.png ok")
    backcover()
    print("exemplo-backcover.png ok")
    manual(cap)
    print("exemplo-manual.pdf ok")


if __name__ == "__main__":
    main()
