#!/usr/bin/env python3
"""Gera logo.inc: a logo do Cheat Genius (BG2, 2bpp) no espírito das caixas de
acessório de trapaça dos anos 90 — letras gordas em itálico, num arco leve,
com degradê amarelo -> vermelho, contorno escuro e sombra, e em cima o disco
de quatro cores do Genius (o brinquedo de memória), nas cores dos botões do
controle do Super Famicom. Arte nossa: as letras vêm da font8x8 (domínio público),
ampliadas e distorcidas aqui.

Saída:
  logo_tiles .. logo_tiles_end   tiles 2bpp, logo depois da fonte na VRAM
                                 (o tile 0 daqui é transparente)
  logo_maps  .. logo_maps_end    dois tilemaps 32x32 do BG2: o grande (tela
                                 de carga) e o pequeno (topo da lista)
  logo_pal   .. logo_pal_end     8 paletas de 4 cores para o BG2 (CGRAM 32+)

Índices de cor: 0 transparente, 1 contorno/sombra, 2 tom escuro, 3 tom claro.
O degradê vem da paleta por linha de tiles: cada linha tem o tom claro na
metade de cima e o escuro na de baixo.

Com `--preview ARQ.ppm` também grava uma prévia das duas telas do BG2.
"""
import re, sys, pathlib

here = pathlib.Path(__file__).parent
FONT_TILES = 98          # tiles da fonte (gen_font.py) antes dos da logo
PRIO = 0x2000            # BG2 com prioridade passa por cima do BG1

src = (here / "font8x8_basic.h").read_text()
rows = re.findall(r"\{\s*((?:0x[0-9A-Fa-f]{2},?\s*){8})\}", src)
glyphs = [[int(b, 16) for b in re.findall(r"0x([0-9A-Fa-f]{2})", r)] for r in rows]


def glyph(ch):
    """8x8 -> lista de linhas de bools (bit0 = pixel da esquerda)."""
    return [[bool(b >> x & 1) for x in range(8)] for b in glyphs[ord(ch)][:7]]


def wordmark(text, sx, rows_map, arch, slant, bold=0, space=None):
    """Máscara das letras: cada linha da font8x8 repetida conforme
    `rows_map` (quantas vezes cada uma das 7 linhas), cada coluna `sx` vezes,
    as letras num arco de altura `arch` e inclinadas 1px a cada `slant`."""
    gh = sum(rows_map)
    n = len(text)
    adv = 8 * sx - sx + bold    # a font8x8 tem uma coluna vazia à direita
    space = adv if space is None else space
    w = adv * n + gh // slant + 2 + bold
    h = gh + arch
    m = [[False] * w for _ in range(h)]
    pen = 0
    for i, ch in enumerate(text):
        if ch == " ":
            pen += space
            continue
        x0 = pen
        pen += adv
        t = (i + 0.5) / n * 2 - 1
        dy = round(arch * t * t)  # pontas mais baixas, meio mais alto
        g = glyph(ch)
        y = dy
        for r, rep in enumerate(rows_map):
            for _ in range(rep):
                shift = (gh - (y - dy)) // slant
                for c in range(8):
                    if g[r][c]:
                        for k in range(sx + bold):
                            m[y][x0 + c * sx + k + shift] = True
                y += 1
    used = max(x for row in m for x, v in enumerate(row) if v) + 1
    return [row[:used] for row in m]


def stylize(mask, pad_top):
    """Máscara -> índices: sombra 2px para baixo/direita e contorno de 1px
    (cor 1); dentro, tom claro na metade de cima de cada linha de tiles e
    escuro na de baixo."""
    h, w = len(mask), len(mask[0])
    H = h + pad_top + 4
    H = (H + 7) // 8 * 8
    W = w + 4
    img = [[0] * W for _ in range(H)]

    def at(y, x):
        y -= pad_top + 1
        x -= 1
        return 0 <= y < h and 0 <= x < w and mask[y][x]

    for y in range(H):
        for x in range(W):
            if at(y, x):
                img[y][x] = 3 if y % 8 < 4 else 2
            elif any(at(y + dy, x + dx) for dy in (-1, 0, 1) for dx in (-1, 0, 1)):
                img[y][x] = 1
            elif at(y - 2, x - 2) or at(y - 2, x - 1) or at(y - 1, x - 2):
                img[y][x] = 1
    return img


# O disco do Genius: quatro gomos (verde, vermelho, amarelo, azul — as
# cores dos botões do controle do Super Famicom / SNES europeu) num aro
# escuro, com o miolo redondo. O corte entre os gomos cai na borda dos
# tiles, então cada tile tem um gomo só e leva a paleta dele.
# Índices: 1 aro/corte/miolo, 2 cor do gomo, 3 brilho (borda de cima/esquerda).
def disc(size, ring, hub, gap, bevel):
    c = size / 2
    img = [[0] * size for _ in range(size)]

    def inside(y, x):
        if not (0 <= y < size and 0 <= x < size):
            return False
        dx, dy = x + 0.5 - c, y + 0.5 - c
        r = (dx * dx + dy * dy) ** 0.5
        return hub <= r <= c - ring and abs(dx) >= gap / 2 and abs(dy) >= gap / 2

    for y in range(size):
        for x in range(size):
            dx, dy = x + 0.5 - c, y + 0.5 - c
            if (dx * dx + dy * dy) ** 0.5 > c - 0.3:
                continue
            if not inside(y, x):
                img[y][x] = 1
            elif not inside(y - bevel, x - bevel) or not inside(y - bevel, x) \
                    or not inside(y, x - bevel):
                img[y][x] = 3
            else:
                img[y][x] = 2
    return img


# Paletas de cada gomo (na ordem em que acendem, sentido horário).
DISC_PALS = {"green": 0, "red": 5, "blue": 7, "yellow": 6}


def disc_pal(ty, tx, half):
    top, left = ty < half, tx < half
    return DISC_PALS[{(True, True): "green", (True, False): "red",
                      (False, False): "blue", (False, True): "yellow"}[(top, left)]]


tiles = [bytes(16)]               # 0: transparente
tile_ix = {tiles[0]: 0}


def tile_bytes(img, ty, tx):
    out = bytearray()
    for y in range(8):
        p0 = p1 = 0
        for x in range(8):
            v = img[ty * 8 + y][tx * 8 + x]
            p0 |= (v & 1) << (7 - x)
            p1 |= (v >> 1 & 1) << (7 - x)
        out += bytes([p0, p1])
    return bytes(out)


def place(tmap, img, col, row, pals):
    """Corta `img` em tiles e escreve no tilemap a partir de (col, row);
    `pals[i]` é a paleta da i-ésima linha de tiles, ou `pals(ty, tx)`."""
    pal = pals if callable(pals) else (lambda ty, tx: pals[ty])
    th, tw = len(img) // 8, len(img[0]) // 8
    for ty in range(th):
        for tx in range(tw):
            b = tile_bytes(img, ty, tx)
            if b not in tile_ix:
                tile_ix[b] = len(tiles)
                tiles.append(b)
            k = tile_ix[b]
            if k:
                tmap[(row + ty) * 32 + col + tx] = (FONT_TILES + k) | pal(ty, tx) << 10 | PRIO


def pad_w(img):
    w = (len(img[0]) + 7) // 8 * 8
    return [r + [0] * (w - len(r)) for r in img]


blank = FONT_TILES  # tile 0 da logo: transparente
big_map = [blank] * 1024
small_map = [blank] * 1024

# Tela de carga: o disco (48px) nas linhas 1-6, nome nas linhas 8-11.
big_disc = disc(48, 3, 9, 2, 2)
place(big_map, big_disc, 13, 1, lambda ty, tx: disc_pal(ty, tx, 3))
big = pad_w(stylize(wordmark("CHEAT GENIUS", 2, [3] * 7, 5, 4, bold=1, space=8), 0))
assert len(big) // 8 == 4, len(big)
place(big_map, big, (32 - len(big[0]) // 8) // 2, 8, [1, 2, 3, 4])

# Topo da lista: disquinho (16px) e o nome pequeno, nas linhas 0-1.
small = pad_w(stylize(wordmark("CHEAT GENIUS", 1, [2, 1, 2, 1, 2, 1, 2], 0, 4, bold=1, space=5), 1))
assert len(small) <= 16, len(small)
sw = len(small[0]) // 8
c0 = (32 - (sw + 3)) // 2
place(small_map, disc(16, 1, 3, 2, 1), c0, 0, lambda ty, tx: disc_pal(ty, tx, 1))
place(small_map, small, c0 + 3, 0, [1, 3])


def bgr(r, g, b):
    return b << 10 | g << 5 | r


OUT = bgr(3, 3, 13)                     # contorno azul-noite
SH = [bgr(*c) for c in [(31, 31, 22), (31, 29, 9), (31, 26, 3), (31, 22, 0),
                        (31, 17, 0), (30, 12, 0), (28, 7, 0), (23, 3, 2)]]
RIM = bgr(7, 7, 9)                      # aro e miolo do disco
# Cada gomo: (cor, brilho) apagado e aceso.
GLOW = {
    "green":  [(5, 16, 6), (8, 21, 9), (10, 30, 12), (22, 31, 22)],
    "red":    [(18, 4, 4), (23, 7, 7), (31, 9, 8), (31, 21, 20)],
    "blue":   [(4, 10, 21), (7, 14, 26), (9, 19, 31), (21, 27, 31)],
    "yellow": [(21, 17, 2), (26, 22, 5), (31, 28, 6), (31, 31, 20)],
}
G = {k: [bgr(*c) for c in v] for k, v in GLOW.items()}
pals = [[0, OUT, 0, 0] for _ in range(8)]
pals[1] = [0, OUT, SH[1], SH[0]]                          # 1-4 nome grande
pals[2] = [0, OUT, SH[3], SH[2]]                          # (o pequeno usa 1 e 3)
pals[3] = [0, OUT, SH[5], SH[4]]
pals[4] = [0, OUT, SH[7], SH[6]]
for k, p in DISC_PALS.items():                            # 0, 5, 6, 7 gomos
    pals[p] = [0, RIM, G[k][0], G[k][1]]

assert FONT_TILES + len(tiles) <= 1024

out = ["; gerado por gen_logo.py - nao editar\n", "logo_tiles:\n"]
for t in tiles:
    out.append("  .byte " + ",".join(f"${b:02X}" for b in t) + "\n")
out.append("logo_tiles_end:\n\nlogo_maps:\n")
for m in (big_map, small_map):
    for i in range(0, 1024, 16):
        out.append("  .word " + ",".join(f"${v:04X}" for v in m[i:i + 16]) + "\n")
out.append("logo_maps_end:\n\nlogo_pal:\n")
for p in pals:
    out.append("  .word " + ",".join(f"${v:04X}" for v in p) + "\n")
out.append("logo_pal_end:\n\n")
# Para o NMI acender um gomo de cada vez: endereço CGRAM da cor 2 de cada
# gomo e, por gomo, 4 palavras: (cor, brilho) apagado e (cor, brilho) aceso.
order = ["green", "red", "blue", "yellow"]
out.append("disc_cg:\n  .byte " + ",".join(str(32 + DISC_PALS[k] * 4 + 2) for k in order) + "\n")
out.append("disc_cols:\n")
for k in order:
    out.append("  .word " + ",".join(f"${v:04X}" for v in G[k]) + "\n")
(here / "logo.inc").write_text("".join(out))
print(f"logo.inc: {len(tiles)} tiles")

if "--preview" in sys.argv:
    path = sys.argv[sys.argv.index("--preview") + 1]

    def rgb(v):
        return ((v & 31) * 8, (v >> 5 & 31) * 8, (v >> 10 & 31) * 8)

    W, H = 256, 224 * 2 + 8
    px = [[(0, 32, 40)] * W for _ in range(H)]
    for s, m in enumerate((big_map, small_map)):
        for ty in range(28):
            for tx in range(32):
                e = m[ty * 32 + tx]
                k, pal = (e & 0x3FF) - FONT_TILES, e >> 10 & 7
                t = tiles[k]
                for y in range(8):
                    for x in range(8):
                        v = (t[y * 2] >> (7 - x) & 1) | (t[y * 2 + 1] >> (7 - x) & 1) << 1
                        if v:
                            px[s * 232 + ty * 8 + y][tx * 8 + x] = rgb(pals[pal][v])
    with open(path, "wb") as f:
        f.write(f"P6 {W} {H} 255\n".encode())
        for r in px:
            for c in r:
                f.write(bytes(c))
