#!/usr/bin/env python3
"""Gera a tabela CRC32 → tempos do HowLongToBeat embutida no SixteeN
(hltb_data.txt).

Casa os títulos do HowLongToBeat (HLTB) com os CRC32 do No-Intro SNES — o
app faz o lookup pelo CRC32 que já sai do scan do cartucho (sem copier
header), sem rede e sem busca nenhuma em runtime (mesmo molde de
`tosec.rs`).

Fontes (arquivos locais, baixados à mão uma vez):

1. Dataset comunitário do HLTB — CSV de ~55 mil linhas com tempos
   (história principal / + extras / completista / todos) de
   https://github.com/johagan94/hltb-dataset-plugin
   (dataset/hltb_dataset_filtered.csv). Só entram as linhas cuja lista de
   plataformas contém "Super Nintendo" — o nome que o HLTB dá ao SNES,
   sem confundir com Nintendo 64/GameCube/Switch.
2. DAT No-Intro de SNES em formato clrmamepro (o mesmo que o app baixa no
   setup, do espelho libretro-database): blocos `game ( name "..." ... rom
   ( ... crc XXXXXXXX ... ) )`. O nome do jogo (com "(USA)"/"(En,Fr)"/
   "(Rev 1)" no fim) vira o título de casamento; o CRC é a chave.

Casamento: nome normalizado (minúsculas, só alfanuméricos, sufixos "(...)"
de AMBOS os lados removidos antes — "Donkey Kong Country 2 - Diddy's Kong
Quest (USA)" == "Donkey Kong Country 2: Diddy's Kong Quest", mesma regra do
`cheats.rs`). Sem casamento exato: um é prefixo do outro (título sem
subtítulo de um lado — "ActRaiser" vs "ActRaiser 2" NÃO casa: o piso de
proporção barra). Empate entre entradas HLTB: vence a mais votada
(`*_polled`).

Formato de saída — uma linha por CRC32 (hex maiúsculo), tempos em MINUTOS
inteiros, campo vazio = o HLTB não tem aquele tempo:

    CRC32\tmain[\textra[\tcompletista[\ttodos]]]

Uso:

    python3 scripts/gen_hltb_data.py hltb_dataset_filtered.csv nointro.dat \
        > crates/domain/src/hltb_data.txt
"""

from __future__ import annotations

import csv
import re
import sys
import unicodedata

# casamento por prefixo: o mais curto tem que cobrir quase tudo do longo
# (títulos sem subtítulo de um lado) — abaixo disso é outro jogo
PREFIXO_MIN = 0.75

GAME_BLOCK = re.compile(r"^game \(\s*$")
QUOTED = re.compile(r'^\s*(name|region)\s+"(.*)"\s*$')
ROM_CRC = re.compile(r"^\s*rom \( .*\bcrc ([0-9A-Fa-f]{8})\b.*\)\s*$")
PARENTESE = re.compile(r"\([^)]*\)")
# No-Intro inverte o artigo ("Addams Family, The"; também no MEIO do
# título: "Legend of Zelda, The - A Link to the Past"); o HLTB usa natural
# ("The Legend of Zelda: A Link to the Past") — reordena antes de
# normalizar, dos dois lados.
ARTIGO = re.compile(
    r"^([^,]+), (The|A|An|La|Le|Les|Der|Die|Das|El|Los|Las|O|Os|As)\b[,:]?\s*(.*)$",
    re.IGNORECASE,
)


def des_inverte_artigo(s: str) -> str:
    m = ARTIGO.match(s.strip())
    if not m:
        return s
    cabeca, artigo, resto = m.group(1), m.group(2), m.group(3)
    return f"{artigo} {cabeca} {resto}".strip()


def normaliza(s: str) -> str:
    s = des_inverte_artigo(s)
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = s.lower().replace("&", " and ")
    return "".join(c for c in s if c.isalnum())


def sem_sufixos(s: str) -> str:
    """O título de casamento: sufixos "(...)" fora — região, revisão,
    línguas, "(SNES)" de desambiguação do próprio HLTB."""
    return PARENTESE.sub(" ", s).strip()


def minutos(h: str) -> str:
    """HLTB vem em horas com 2 casas ("10.88") → minutos inteiros ("653")."""
    if not h:
        return ""
    return str(round(float(h) * 60))


def de_hltb(caminho: str) -> dict[str, dict]:
    """Linhas do CSV com "Super Nintendo" na lista de plataformas, por nome
    normalizado; empate vence a entrada mais votada."""
    jogos: dict[str, dict] = {}
    with open(caminho, encoding="utf-8", newline="") as arq:
        for linha in csv.DictReader(arq):
            plats = [p.strip() for p in linha["platform"].split(",")]
            if "Super Nintendo" not in plats:
                continue
            votos = max(
                int(linha[c] or 0)
                for c in ("main_story_polled", "main_plus_sides_polled", "completionist_polled")
            )
            reg = normaliza(sem_sufixos(linha["name"]))
            if not reg:
                continue
            atual = jogos.get(reg)
            if atual is None or votos > atual["votos"]:
                jogos[reg] = {
                    "nome": linha["name"].strip(),
                    "main": minutos(linha["main_story"]),
                    "extra": minutos(linha["main_plus_sides"]),
                    "comp": minutos(linha["completionist"]),
                    "todos": minutos(linha["all_styles"]),
                    "votos": votos,
                }
    return jogos


def de_dat(caminho: str) -> dict[str, tuple[str, str]]:
    """CRC32 → (título de casamento, região) — blocos `game (` do
    clrmamepro; várias roms no bloco (raro em SNES) herdam o título."""
    jogos: dict[str, tuple[str, str]] = {}
    em_jogo = False
    titulo = ""
    regiao = ""
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            if GAME_BLOCK.match(linha):
                em_jogo, titulo, regiao = True, "", ""
                continue
            if not em_jogo:
                continue
            if linha.startswith(")"):
                em_jogo = False
                continue
            m = QUOTED.match(linha)
            if m:
                if m.group(1) == "name" and not titulo:
                    titulo = m.group(2).strip()
                elif m.group(1) == "region" and not regiao:
                    regiao = m.group(2).strip()
                continue
            m = ROM_CRC.match(linha)
            if m and titulo:
                crc = m.group(1).upper()
                if crc not in jogos:
                    jogos[crc] = (sem_sufixos(titulo), regiao or "outra")
    return jogos


def hltb_do(titulo: str, hltb: dict[str, dict]) -> dict | None:
    """Casamento exato primeiro, prefixo depois — o melhor dos candidatos."""
    reg = normaliza(titulo)
    if reg in hltb:
        return hltb[reg]
    if len(reg) < 4:
        return None
    melhor, nota = None, 0.0
    for cand, jogo in hltb.items():
        curto, longo = sorted((len(reg), len(cand)))
        if longo == curto:
            continue
        # prefixo nas duas direções (o início do nome define o jogo;
        # subtítulo de um lado só)
        if reg.startswith(cand) or cand.startswith(reg):
            if curto / longo >= PREFIXO_MIN and curto / longo > nota:
                melhor, nota = jogo, curto / longo
    return melhor


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    hltb = de_hltb(sys.argv[1])
    roms = de_dat(sys.argv[2])

    achados: dict[str, dict] = {}
    por_regiao: dict[str, int] = {}
    total_regiao: dict[str, int] = {}
    for crc, (titulo, regiao) in sorted(roms.items()):
        total_regiao[regiao] = total_regiao.get(regiao, 0) + 1
        jogo = hltb_do(titulo, hltb)
        if jogo:
            achados[crc] = jogo
            por_regiao[regiao] = por_regiao.get(regiao, 0) + 1

    usados = {j["nome"] for j in achados.values()}
    print("# gerado por scripts/gen_hltb_data.py — não editar à mão")
    print("# tempos do HowLongToBeat (minutos): main\\textra\\tcompletista\\ttodos")
    print(f"# {len(achados)} CRC32 de {len(roms)}, {len(usados)} títulos HLTB de {len(hltb)}")
    for regiao in ("USA", "Europe", "Japan", "World", "outra"):
        print(f"# {regiao}: {por_regiao.get(regiao, 0)}/{total_regiao.get(regiao, 0)}")
    for crc, jogo in sorted(achados.items()):
        if not (jogo["main"] or jogo["extra"] or jogo["comp"] or jogo["todos"]):
            continue
        print(f"{crc}\t{jogo['main']}\t{jogo['extra']}\t{jogo['comp']}\t{jogo['todos']}")

    # relatório de QA no stderr: os ocidentais que ficaram de fora
    sem = sorted(t for (c, (t, r)) in roms.items() if c not in achados and r in ("USA", "Europe"))
    print(f"\n-- sem casamento (USA/Europe): {len(sem)} de "
          f"{sum(1 for (_, (_, r)) in roms.items() if r in ('USA', 'Europe'))}",
          file=sys.stderr)
    for t in sem[:60]:
        print(f"   {t}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
