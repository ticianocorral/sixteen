#!/usr/bin/env python3
"""Gera o pacote de exemplo de SNES do modo dev (crates/app/assets/example/).

Tudo é nosso — nenhuma arte, código ou dado de terceiros:
  roms/SixteeN Demo Cart.sfc        ROM sintética LoROM de 32 KB (65816 escrito
                                    aqui mesmo): tabuleiro com cores girando e
                                    um quadradinho que anda com o direcional
                                    (segurar B acelera as cores)
  assets/cover/…png  backcover/…png caixa (frente e verso)
  assets/cartridge/…png             cartucho cinza com o rótulo do jogo
  assets/logo/…png                  logo do jogo (fundo transparente)
  assets/manual/…pdf                manual de 5 páginas (cada página é uma
                                    imagem — o leitor do app só renderiza
                                    páginas-imagem)

Uso: python3 scripts/gen_example_assets.py
Precisa de Pillow e do cc65 (ca65/ld65: `brew install cc65`) — só para
regerar; os arquivos gerados são commitados e embutidos no binário.
"""
import colorsys
import math
import random
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "app" / "assets" / "example"
NAME = "SixteeN Demo Cart"
TITLE = "SIXTEEN DEMO CART"  # 21 bytes no header da ROM

FONT = "/System/Library/Fonts/HelveticaNeue.ttc"
MONO = "/System/Library/Fonts/Menlo.ttc"

# Paleta da marca (a mesma do kit de moldes do app)
RED = (231, 11, 23)
RED_DARK = (158, 8, 16)
CHAR = (33, 29, 24)
CHAR_LIGHT = (43, 38, 32)
CREAM = (242, 237, 227)
CREAM_MID = (214, 207, 193)
GREY_TXT = (148, 141, 130)


def font(size, bold=True, mono=False):
    if mono:
        return ImageFont.truetype(MONO, size, index=1 if bold else 0)
    return ImageFont.truetype(FONT, size, index=1 if bold else 0)


# ------------------------------------------------------------------ ROM -----

# O sprite do jogo: 16x16, cores 0 (transparente), 1 (vermelho), 2 (creme), 3 (carvão)
SPRITE = [
    "2222222222222222",
    "2111111111111112",
    "2111111111111112",
    "2113331111333112",
    "2113331111333112",
    "2113331111333112",
    "2111111111111112",
    "2111111111111112",
    "2111111111111112",
    "2113311111133112",
    "2111331111331112",
    "2111133333311112",
    "2111111111111112",
    "2111111111111112",
    "2111111111111112",
    "2222222222222222",
]


def bgr555(r, g, b):
    return (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)


def sprite_tiles():
    """4 tiles 4bpp (8x8) na ordem do hardware: 0, 1, 16, 17."""
    out = bytearray()
    for bx, by in ((0, 0), (1, 0), (0, 1), (1, 1)):
        low, high = bytearray(), bytearray()
        for row in range(8):
            line = SPRITE[by * 8 + row][bx * 8:bx * 8 + 8]
            p0 = p1 = 0
            for i, ch in enumerate(line):
                v = int(ch)
                p0 |= (v & 1) << (7 - i)
                p1 |= ((v >> 1) & 1) << (7 - i)
            low += bytes([p0, p1])
            high += bytes([0, 0])
        out += low + high
    return bytes(out)


def bg_tiles():
    """BG 2bpp: tile 0 vazio (vê o fundo girando), tile 1 sólido cor 1."""
    return bytes(16) + bytes([0xFF, 0x00] * 8)


def hue_tables():
    lo, hi = bytearray(), bytearray()
    for i in range(256):
        r, g, b = colorsys.hsv_to_rgb(i / 256, 0.55, 0.95)
        c = bgr555(int(r * 255), int(g * 255), int(b * 255))
        lo.append(c & 0xFF)
        hi.append(c >> 8)
    return bytes(lo), bytes(hi)


def palettes():
    bg = [0x0000, bgr555(24, 22, 52), 0x0000, 0x0000]
    spr = [0x0000, bgr555(*RED), bgr555(*CREAM), bgr555(*CHAR)]
    out = bytearray()
    for c in bg + spr:
        out += bytes([c & 0xFF, c >> 8])
    return bytes(out)


def db(label, data, per=16):
    lines = [f"{label}:"]
    for i in range(0, len(data), per):
        lines.append("    .byte " + ",".join(f"${b:02X}" for b in data[i:i + per]))
    return "\n".join(lines)


ASM = r"""
.p816
.a8
.i16

INIDISP  = $2100
OBSEL    = $2101
OAMADDL  = $2102
OAMADDH  = $2103
OAMDATA  = $2104
BGMODE   = $2105
BG1SC    = $2107
BG12NBA  = $210B
VMAINC   = $2115
VMADDL   = $2116
VMDATAL  = $2118
VMDATAH  = $2119
CGADD    = $2121
CGDATA   = $2122
TM       = $212C
NMITIMEN = $4200
RDNMI    = $4210
HVBJOY   = $4212
JOY1H    = $4219

px  = $10
py  = $11
hue = $12      ; $13 fica sempre 0 (indexa as tabelas com X de 16 bits)

.export Reset, NMI, Dummy

.segment "CODE"

Reset:
    sei
    clc
    xce
    rep #$10
    sep #$20
    .a8
    .i16
    ldx #$1FFF
    txs
    lda #$8F
    sta INIDISP          ; forced blank enquanto prepara tudo
    stz NMITIMEN
    stz $420B
    stz $420C
    stz $2130
    stz $2131
    stz $2133
    stz $210D
    stz $210D
    stz $210E
    stz $210E
    lda #$60
    sta OBSEL            ; sprites 16x16 / 32x32, tiles em VRAM $0000
    stz BGMODE           ; modo 0
    lda #$08
    sta BG1SC            ; mapa do BG1 em $0800
    lda #$01
    sta BG12NBA          ; tiles do BG1 em $1000
    lda #$11
    sta TM               ; BG1 + sprites

    ; paletas: BG em CGRAM 0, sprites em CGRAM 128
    stz CGADD
    ldx #0
    ldy #8
    jsr UploadCGRAM
    lda #128
    sta CGADD
    ldx #8
    ldy #8
    jsr UploadCGRAM

    ; VRAM: sprite (tiles 0,1 e 16,17), depois tiles do BG
    lda #$80
    sta VMAINC
    ldx #$0000
    stx VMADDL
    ldx #0
    ldy #64
    jsr UploadVRAM
    ldx #$0100
    stx VMADDL
    ldx #64
    ldy #64
    jsr UploadVRAM
    ldx #$1000
    stx VMADDL
    ldx #128
    ldy #32
    jsr UploadVRAM

    ; mapa 32x32 do BG1: tabuleiro (tile = (linha ^ coluna) & 1)
    ldx #$0800
    stx VMADDL
    ldy #0
@map:
    tya
    sta $00
    lsr
    lsr
    lsr
    lsr
    lsr
    eor $00
    and #$01
    sta VMDATAL
    stz VMDATAH
    iny
    cpy #1024
    bne @map

    ; OAM: 128 sprites escondidos (Y=$E0) + a tabela alta zerada
    stz OAMADDL
    stz OAMADDH
    ldx #128
@oam:
    stz OAMDATA
    lda #$E0
    sta OAMDATA
    stz OAMDATA
    stz OAMDATA
    dex
    bne @oam
    ldx #32
@oamh:
    stz OAMDATA
    dex
    bne @oamh

    lda #120
    sta px
    lda #104
    sta py
    stz hue
    stz hue+1

    lda #$81
    sta NMITIMEN         ; NMI + leitura automática do controle
    lda #$0F
    sta INIDISP          ; tela ligada
Main:
    wai
    bra Main

UploadCGRAM:             ; X = offset em PalData, Y = bytes
@l: lda PalData,x
    sta CGDATA
    inx
    dey
    bne @l
    rts

UploadVRAM:              ; X = offset em Tiles, Y = bytes (par)
@l: lda Tiles,x
    sta VMDATAL
    inx
    lda Tiles,x
    sta VMDATAH
    inx
    dey
    dey
    bne @l
    rts

NMI:
    rep #$30
    pha
    phx
    phy
    sep #$20
    rep #$10
    .a8
    .i16
    lda RDNMI
@wait:
    lda HVBJOY
    and #$01
    bne @wait            ; espera a leitura automática do controle
    lda JOY1H
    sta $00

    and #$08             ; cima
    beq @nu
    lda py
    cmp #$04
    bcc @nu
    sbc #$02
    sta py
@nu:
    lda $00
    and #$04             ; baixo
    beq @nd
    lda py
    cmp #208
    bcs @nd
    adc #$02
    sta py
@nd:
    lda $00
    and #$02             ; esquerda
    beq @nl
    lda px
    cmp #$04
    bcc @nl
    sbc #$02
    sta px
@nl:
    lda $00
    and #$01             ; direita
    beq @nr
    lda px
    cmp #240
    bcs @nr
    adc #$02
    sta px
@nr:
    lda $00
    and #$80             ; B acelera as cores
    beq @slow
    lda hue
    clc
    adc #$06
    bra @sh
@slow:
    lda hue
    clc
    adc #$01
@sh:
    sta hue

    stz CGADD            ; cor 0 = o fundo girando
    ldx hue
    lda ColLo,x
    sta CGDATA
    lda ColHi,x
    sta CGDATA

    stz OAMADDL          ; sprite 0 = o jogador
    stz OAMADDH
    lda px
    sta OAMDATA
    lda py
    sta OAMDATA
    stz OAMDATA
    lda #$30
    sta OAMDATA

    rep #$30
    ply
    plx
    pla
    rti

Dummy:
    rti

.segment "RODATA"
@@PAL@@
@@TILES@@
@@COLLO@@
@@COLHI@@
"""

LD_CFG = """MEMORY { ROM: start = $8000, size = $7FC0, fill = yes, fillval = $FF, file = %O; }
SEGMENTS { CODE: load = ROM, type = ro; RODATA: load = ROM, type = ro; }
"""


def build_rom():
    for tool in ("ca65", "ld65"):
        if not shutil.which(tool):
            raise SystemExit(f"{tool} não encontrado — brew install cc65")
    lo, hi = hue_tables()
    src = (
        ASM.replace("@@PAL@@", db("PalData", palettes()))
        .replace("@@TILES@@", db("Tiles", sprite_tiles() + bg_tiles()))
        .replace("@@COLLO@@", db("ColLo", lo))
        .replace("@@COLHI@@", db("ColHi", hi))
    )
    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        (td / "demo.s").write_text(src)
        (td / "demo.cfg").write_text(LD_CFG)
        subprocess.run(["ca65", "--cpu", "65816", "-o", "demo.o", "demo.s"], cwd=td, check=True)
        subprocess.run(
            ["ld65", "-C", "demo.cfg", "-o", "code.bin", "-Ln", "labels.txt", "demo.o"],
            cwd=td, check=True,
        )
        code = (td / "code.bin").read_bytes()
        labels = {}
        for line in (td / "labels.txt").read_text().splitlines():
            p = line.split()
            labels[p[2].lstrip(".")] = int(p[1], 16)
    assert len(code) == 0x7FC0
    reset, nmi, dummy = labels["Reset"], labels["NMI"], labels["Dummy"]

    header = bytearray(0x40)
    header[0x00:0x15] = TITLE.encode("ascii").ljust(21)
    header[0x15] = 0x20  # LoROM, SlowROM
    header[0x16] = 0x00  # só ROM
    header[0x17] = 0x05  # 32 KB
    header[0x18] = 0x00  # sem SRAM
    header[0x19] = 0x01  # América do Norte (NTSC)
    header[0x1A] = 0x33
    header[0x1B] = 0x00
    vec = {0x24: dummy, 0x26: dummy, 0x28: dummy, 0x2A: nmi, 0x2E: dummy,   # nativo
           0x34: dummy, 0x38: dummy, 0x3A: nmi, 0x3C: reset, 0x3E: dummy}  # emulação
    for off, addr in vec.items():
        header[off:off + 2] = addr.to_bytes(2, "little")
    rom = bytearray(code + bytes(header))
    # complemento + checksum somam sempre 0xFFFF (bytes: 0xFF+0xFF = 0x1FE)
    csum = (sum(rom) + 0x1FE) & 0xFFFF
    off = 0x7FC0
    rom[off + 0x1C:off + 0x1E] = (csum ^ 0xFFFF).to_bytes(2, "little")
    rom[off + 0x1E:off + 0x20] = csum.to_bytes(2, "little")
    return bytes(rom)


# ----------------------------------------------------------------- arte -----

def stripes(w, h, base=CHAR, alt=CHAR_LIGHT, step=26, thick=9):
    img = Image.new("RGB", (w, h), base)
    d = ImageDraw.Draw(img)
    for k in range(-h, w + h, step):
        d.polygon([(k, 0), (k + thick, 0), (k + thick + h, h), (k + h, h)], fill=alt)
    return img


def spaced(d, xy, text, f, fill, spacing=0, anchor="l"):
    w = sum(d.textlength(c, font=f) for c in text) + spacing * (len(text) - 1)
    x, y = xy
    if anchor == "m":
        x -= w / 2
    elif anchor == "r":
        x -= w
    for c in text:
        d.text((x, y), c, font=f, fill=fill)
        x += d.textlength(c, font=f) + spacing


def title_block(d, x, y, lines, size, fill=CREAM, shadow=RED, off=5):
    f = font(size)
    for i, ln in enumerate(lines):
        yy = y + i * int(size * 1.0)
        d.text((x + off, yy + off), ln, font=f, fill=shadow)
        d.text((x, yy), ln, font=f, fill=fill)


def sprite_art(d, x, y, scale):
    pal = {"1": RED, "2": CREAM, "3": CHAR}
    for ry, row in enumerate(SPRITE):
        for rx, ch in enumerate(row):
            if ch in pal:
                d.rectangle([x + rx * scale, y + ry * scale,
                             x + (rx + 1) * scale - 1, y + (ry + 1) * scale - 1], fill=pal[ch])


def checker(d, box, cell, a, b):
    x0, y0, x1, y1 = box
    for j, y in enumerate(range(y0, y1, cell)):
        for i, x in enumerate(range(x0, x1, cell)):
            d.rectangle([x, y, min(x + cell, x1) - 1, min(y + cell, y1) - 1],
                        fill=a if (i + j) % 2 == 0 else b)


def make_cover(path, w=1024, h=777):
    img = stripes(w, h)
    d = ImageDraw.Draw(img)
    d.rectangle([14, 14, w - 15, h - 15], outline=CREAM_MID, width=2)
    d.rounded_rectangle([40, 38, 330, 86], radius=8, fill=RED)
    spaced(d, (60, 47), "DEMO CART", font(30), CREAM, 2)
    title_block(d, 48, 225, ["SIXTEEN", "DEMO CART"], 84)
    d.rectangle([52, 450, 560, 456], fill=RED)
    d.text((52, 484), "a sample game for the SixteeN shelf", font=font(30, False), fill=CREAM_MID)
    d.text((52, 528), "move the square, change the colors", font=font(26, False), fill=GREY_TXT)
    # a "tela" do jogo
    box = (640, 190, 985, 470)
    d.rounded_rectangle([box[0] - 8, box[1] - 8, box[2] + 8, box[3] + 8], radius=18, fill=(22, 19, 16))
    checker(d, box, 28, (108, 205, 170), (24, 22, 52))
    sprite_art(d, 780, 300, 5)
    d.rectangle([0, h - 78, w, h], fill=RED)
    spaced(d, (w / 2, h - 60), "R E T R O C O N S O L E   S Y S T E M", font(32), CREAM, 3, "m")
    img.save(path)


def make_backcover(path, w=1024, h=773):
    img = stripes(w, h)
    d = ImageDraw.Draw(img)
    d.rectangle([14, 14, w - 15, h - 15], outline=CREAM_MID, width=2)
    spaced(d, (w / 2, 44), "SIXTEEN DEMO CART", font(50), CREAM, 3, "m")
    d.rectangle([60, 120, w - 60, 126], fill=RED)
    lines = [
        ("A SAMPLE GAME FOR THE SIXTEEN SHELF", True),
        ("", False),
        ("This is not a real game. It is a tiny synthetic", False),
        ("ROM that ships with the app's dev mode, so the", False),
        ("shelf, the cover, the cartridge, the back cover", False),
        ("and the manual reader have something to show.", False),
    ]
    y = 160
    for t, bold in lines:
        d.text((60, y), t, font=font(26, bold), fill=CREAM if bold else CREAM_MID)
        y += 36
    # capturas
    for i in range(3):
        x = 60 + i * 300
        d.rounded_rectangle([x - 4, 400, x + 276, 576], radius=10, fill=(22, 19, 16))
        checker(d, (x, 404, x + 272, 572), 24, (108, 205, 170), (24, 22, 52))
        sprite_art(d, x + 100 + i * 30, 450 + (i % 2) * 20, 4)
    # ficha técnica
    cells = [("PLAYERS", "1"), ("TYPE", "Demo"), ("BACKUP", "None"), ("LANGUAGE", "English")]
    for i, (k, v) in enumerate(cells):
        x = 60 + (i % 2) * 300
        y = 610 + (i // 2) * 52
        d.rectangle([x, y, x + 130, y + 42], fill=CHAR_LIGHT)
        d.text((x + 10, y + 10), k, font=font(17), fill=CREAM)
        d.rectangle([x + 130, y, x + 280, y + 42], outline=CREAM_MID, width=2)
        d.text((x + 144, y + 8), v, font=font(22, False), fill=CREAM)
    # código de barras falso
    d.rectangle([700, 610, 965, 720], fill=CREAM)
    rnd = random.Random(4)
    x = 716
    while x < 950:
        bw = rnd.choice([2, 3, 5])
        d.rectangle([x, 622, x + bw - 1, 690], fill=(10, 10, 10))
        x += bw + rnd.choice([2, 3, 4])
    d.text((716, 694), "SIXTEEN-DEMO-0001", font=font(16, mono=True), fill=(10, 10, 10))
    d.rectangle([0, h - 50, w, h], fill=(26, 22, 18))
    spaced(d, (w / 2, h - 40), "SIXTEEN  •  RETROCONSOLE SYSTEM", font(22), GREY_TXT, 4, "m")
    img.save(path)


def make_label(w=885, h=400):
    img = Image.new("RGB", (w, h), CHAR)
    img.paste(stripes(w, h, CHAR, CHAR_LIGHT, 22, 8), (0, 0))
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, w - 1, 64], fill=RED)
    spaced(d, (24, 14), "SIXTEEN", font(36), CREAM, 5)
    spaced(d, (w - 24, 20), "DEMO CART", font(26), CREAM, 3, "r")
    title_block(d, 36, 100, ["DEMO", "CART"], 110, off=5)
    box = (520, 100, 845, 330)
    d.rounded_rectangle([box[0] - 6, box[1] - 6, box[2] + 6, box[3] + 6], radius=12, fill=(22, 19, 16))
    checker(d, box, 26, (108, 205, 170), (24, 22, 52))
    sprite_art(d, 640, 190, 4)
    d.rectangle([0, h - 40, w, h], fill=RED_DARK)
    spaced(d, (w / 2, h - 32), "SAMPLE GAME  •  NOT FOR SALE", font(20), CREAM, 3, "m")
    d.rectangle([0, 0, w - 1, h - 1], outline=(10, 10, 10), width=2)
    return img


def vgrad(w, h, top, bottom):
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return img


def make_cartridge(path):
    """Cartucho de SNES desenhado do zero (1400x900, fundo transparente)."""
    W, H = 1400, 900
    S = 2  # supersampling
    base = Image.new("RGBA", (W * S, H * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(base)

    def r(box, **kw):
        d.rectangle([v * S for v in box], **kw)

    def rr(box, rad, **kw):
        d.rounded_rectangle([v * S for v in box], radius=rad * S, **kw)

    shell, shell_hi, shell_lo = (128, 125, 134), (150, 147, 156), (96, 93, 102)
    # asas laterais com as nervuras
    for side_x0, side_x1 in ((0, 235), (1165, 1400)):
        n = 6
        top, bottom = 40, 870
        step = (bottom - top) / n
        for k in range(n):
            y0 = top + k * step + 4
            y1 = top + (k + 1) * step - 4
            rr((side_x0 + 4, y0, side_x1 - 4, y1), 10, fill=shell)
            r((side_x0 + 8, y0 + 3, side_x1 - 8, y0 + 9), fill=shell_hi)
            r((side_x0 + 8, y1 - 9, side_x1 - 8, y1 - 3), fill=shell_lo)
    # corpo central
    rr((215, 0, 1185, 900), 22, fill=shell)
    r((222, 4, 1178, 10), fill=shell_hi)
    # poço do rótulo
    rr((250, 8, 1155, 420), 10, fill=shell_lo)
    # recesso do conector
    rr((270, 548, 1130, 900), 14, fill=shell_lo)
    r((275, 556, 1125, 900), fill=(104, 101, 110))
    for x in (575, 825):
        r((x - 3, 556, x + 3, 745), fill=(78, 75, 84))
        r((x + 3, 556, x + 6, 745), fill=shell_hi)
    # parafusos
    for cx, cy in ((130, 790), (1270, 790)):
        d.ellipse([(cx - 24) * S, (cy - 24) * S, (cx + 24) * S, (cy + 24) * S], fill=(80, 76, 84))
        d.ellipse([(cx - 18) * S, (cy - 18) * S, (cx + 18) * S, (cy + 18) * S], fill=(190, 150, 70))
        d.ellipse([(cx - 10) * S, (cy - 10) * S, (cx + 10) * S, (cy + 10) * S], fill=(120, 90, 40))
        d.line([(cx - 9) * S, (cy - 9) * S, (cx + 9) * S, (cy + 9) * S], fill=(60, 45, 20), width=3 * S)
    base = base.resize((W, H), Image.LANCZOS)
    # grão do plástico
    rnd = random.Random(3)
    px = base.load()
    for _ in range(90000):
        x, y = rnd.randrange(W), rnd.randrange(H)
        if px[x, y][3] > 0:
            v = rnd.randint(-6, 6)
            px[x, y] = tuple(max(0, min(255, c + v)) for c in px[x, y][:3]) + (px[x, y][3],)
    # o rótulo no poço (sombra por baixo, como um adesivo colado)
    label = make_label(885, 400)
    shadow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rectangle([260, 14, 1150, 424], fill=(0, 0, 0, 90))
    base = Image.alpha_composite(base, shadow.filter(ImageFilter.GaussianBlur(5)))
    base.paste(label, (263, 12))
    base.save(path)


def make_logo(path):
    img = Image.new("RGBA", (860, 260), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    f = font(150)
    for dx, dy in ((7, 7),):
        d.text((26 + dx, 8 + dy), "SIXTEEN", font=f, fill=RED_DARK + (255,))
    d.text((26, 8), "SIXTEEN", font=f, fill=CREAM + (255,))
    spaced(d, (430, 190), "D E M O   C A R T", font(52), RED + (255,), 0, "m")
    img.save(path)


PAGES = [
    ("SIXTEEN DEMO CART", ["INSTRUCTION BOOKLET", "", "SIXTEEN-DEMO-0001", "SAMPLE - NOT A REAL GAME"]),
    ("WELCOME", [
        "This booklet is a sample manual.",
        "It exists so the manual reader on the",
        "game shelf has pages to show.",
        "",
        "Open it from the shelf, then use the",
        "arrows to flip pages.",
    ]),
    ("CONTROLS", [
        "D-PAD ........ Move the square",
        "B ............ Hold to spin the colors",
        "",
        "The console panel has the rest: reset,",
        "save states and the pause notebook for",
        "notes and screenshots of any game.",
    ]),
    ("SAVING", [
        "This cartridge has no battery backup.",
        "Use the save-state slots in the console",
        "panel to keep your place anywhere.",
        "",
        "States live in the app's saves/ folder,",
        "one set per game.",
    ]),
    ("ABOUT", [
        "SixteeN is a personal, non-commercial",
        "project. This sample cartridge, its art",
        "and this manual were generated for the",
        "app's dev mode.",
        "",
        "Delete them any time from the roms/",
        "and assets/ folders.",
    ]),
]


def make_manual(path):
    pages = []
    for i, (title, lines) in enumerate(PAGES):
        img = vgrad(600, 800, (246, 242, 233), (226, 220, 206))
        d = ImageDraw.Draw(img)
        d.rectangle([0, 0, 600, 90], fill=CHAR)
        d.rectangle([0, 90, 600, 98], fill=RED)
        d.text((30, 28), "SIXTEEN DEMO CART", font=font(26), fill=CREAM)
        d.text((30, 135), title, font=font(44), fill=CHAR)
        d.line([30, 200, 570, 200], fill=RED, width=4)
        y = 235
        for ln in lines:
            d.text((30, y), ln, font=font(24, False), fill=(50, 46, 40))
            y += 38
        sprite_art(d, 470, 620, 6)
        d.text((540, 762), str(i + 1), font=font(22, False), fill=(110, 104, 92))
        pages.append(img)
    pages[0].save(path, "PDF", save_all=True, append_images=pages[1:], resolution=100.0)


def main():
    for sub in ("roms", "assets/cover", "assets/backcover", "assets/cartridge", "assets/logo", "assets/manual"):
        (OUT / sub).mkdir(parents=True, exist_ok=True)
    (OUT / "roms" / f"{NAME}.sfc").write_bytes(build_rom())
    make_cover(OUT / "assets/cover" / f"{NAME}.png")
    make_backcover(OUT / "assets/backcover" / f"{NAME}.png")
    make_cartridge(OUT / "assets/cartridge" / f"{NAME}.png")
    make_logo(OUT / "assets/logo" / f"{NAME}.png")
    make_manual(OUT / "assets/manual" / f"{NAME}.pdf")
    for p in sorted(OUT.rglob("*")):
        if p.is_file():
            print(f"{p.relative_to(OUT)}  {p.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
