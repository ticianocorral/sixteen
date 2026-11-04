#!/usr/bin/env python3
"""Gera font.inc (tiles SNES 2bpp) a partir de font8x8_basic.h (dominio publico).

Tile = 16 bytes: por linha, plano0 (glifo) e plano1 (fundo = ~glifo), de modo
que o pixel do glifo vale 1 e o do fundo vale 2 (a barra de selecao usa a cor 2).
Tiles 0..95 = ASCII 0x20..0x7F; 96 = bloco cheio (cor 1); 97 = bloco de fundo (cor 2).
"""
import re, sys, pathlib

here = pathlib.Path(__file__).parent
src = (here / "font8x8_basic.h").read_text()
rows = re.findall(r"\{\s*((?:0x[0-9A-Fa-f]{2},?\s*){8})\}", src)
assert len(rows) == 128, len(rows)
glyphs = [[int(b, 16) for b in re.findall(r"0x([0-9A-Fa-f]{2})", r)] for r in rows]

def rev8(b):
    return int(f"{b:08b}"[::-1], 2)

out = ["; gerado por gen_font.py - nao editar\n", "font_tiles:\n"]
for c in range(0x20, 0x80):
    g = glyphs[c]
    data = []
    for r in range(8):
        p0 = rev8(g[r])  # font8x8: bit0 = pixel da esquerda; SNES: bit7
        data += [p0, p0 ^ 0xFF]
    out.append("  .byte " + ",".join(f"${b:02X}" for b in data) + f"  ; {chr(c)!r}\n")
out.append("  .byte " + ",".join(["$FF,$00"] * 8) + "  ; solid\n")
out.append("  .byte " + ",".join(["$00,$FF"] * 8) + "  ; shell\n")
out.append("font_tiles_end:\n")
(here / "font.inc").write_text("".join(out))
