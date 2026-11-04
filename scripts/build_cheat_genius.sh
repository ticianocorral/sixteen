#!/usr/bin/env bash
# Monta a ROM do Cheat Genius (ca65 + ld65: `brew install cc65`) e grava
# crates/app/cheat_genius/cheat_genius.sfc, que o app embute com include_bytes!.
# O build normal do app NAO precisa do assembler: o .sfc fica versionado.
set -euo pipefail
cd "$(dirname "$0")/../crates/app/cheat_genius"
python3 gen_font.py
python3 gen_logo.py
ca65 --cpu 65816 -o /tmp/cheat_genius.o cheat_genius.s
ld65 -C snes.cfg -o cheat_genius.sfc /tmp/cheat_genius.o
python3 - <<'PY'
import pathlib
p = pathlib.Path("cheat_genius.sfc")
d = bytearray(p.read_bytes())
assert len(d) == 0x8000, len(d)
# checksum LoROM: soma de todos os bytes, complemento em $7FDC
d[0x7FDC:0x7FE0] = b"\xff\xff\x00\x00"
s = sum(d) & 0xFFFF
d[0x7FDC:0x7FE0] = ((s ^ 0xFFFF).to_bytes(2, "little")) + s.to_bytes(2, "little")
p.write_bytes(d)
print(f"cheat_genius.sfc: {len(d)} bytes, checksum {s:04X}")
PY
