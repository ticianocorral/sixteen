#!/usr/bin/env python3
"""Gera a tabela única de infos do SixteeN (infos_data.txt), por CRC32.

Une tudo que o painel mostra num lugar só — nome canônico, ano, editora,
região, nota (IGDB) e os tempos do HowLongToBeat — embutido no binário: o
usuário instala o app, solta a ROM e as infos estão lá, sem DAT e sem rede.

Fontes (arquivos locais em `scripts/dados/`):

1. `nointro_snes.dat` — o DAT No-Intro de SNES em clrmamepro (nome
   canônico + região por CRC32; o mesmo que o app baixa no setup).
2. `tosec_snes.txt` — ano e editora por CRC32 (a tabela TOSEC que já
   vinha embutida).
3. `hltb_snes.txt` — os tempos do HowLongToBeat já casados por CRC32
   (saída do `gen_hltb_data.py`).
4. `igdb_snes.json` — índice {nome normalizado: {nota, ano}} extraído da
   IGDB (plataforma 58, do espelho GameDB do LizardByte).

Casamento com a IGDB: nome normalizado (minúsculas, só alfanuméricos,
artigo reordenado, sufixos "(...)" de ambos os lados fora) — exato
primeiro, prefixo depois.

Formato de saída — `CRC32\\tnome\\tano\\teditora\\tregião\\tnota\\tmain\\textra`
(minutos), campo vazio = sem dado.

Uso:

    python3 scripts/gen_infos_data.py > crates/domain/src/infos_data.txt
"""

from __future__ import annotations

import json
import re
import sys
import unicodedata

ARTIGO = re.compile(
    r"^([^,]+), (The|A|An|La|Le|Les|Der|Die|Das|El|Los|Las|O|Os|As)\b[,:]?\s*(.*)$",
    re.IGNORECASE,
)
DAT_BLOCK = re.compile(r"^game \(\s*$")
DAT_QUOTED = re.compile(r'^\s*(name|region)\s+"(.*)"\s*$')
DAT_CRC = re.compile(r"^\s*rom \( .*\bcrc ([0-9A-Fa-f]{8})\b.*\)\s*$")
PARENTESE = re.compile(r"\([^)]*\)")


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
    return PARENTESE.sub(" ", s).strip()


def de_dat(caminho: str) -> dict[str, dict]:
    """crc → {nome canônico, região} do DAT No-Intro (clrmamepro)."""
    jogos: dict[str, dict] = {}
    em_jogo = False
    nome = ""
    regiao = ""
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            if DAT_BLOCK.match(linha):
                em_jogo, nome, regiao = True, "", ""
                continue
            if not em_jogo:
                continue
            if linha.startswith(")"):
                em_jogo = False
                continue
            m = DAT_QUOTED.match(linha)
            if m:
                campo, valor = m.group(1), m.group(2).strip()
                if campo == "name" and not nome:
                    nome = valor
                elif campo == "region" and not regiao:
                    regiao = valor
                continue
            m = DAT_CRC.match(linha)
            if m and nome:
                crc = m.group(1).upper()
                if crc not in jogos:
                    jogos[crc] = {"nome": nome, "regiao": regiao}
    return jogos


def de_tosec(caminho: str) -> dict[str, tuple[str, str]]:
    """crc → (ano, editora)."""
    jogos: dict[str, tuple[str, str]] = {}
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            if linha.startswith("#"):
                continue
            p = linha.split("\t")
            if len(p) >= 3:
                jogos[p[0]] = (p[1], p[2])
    return jogos


def de_hltb(caminho: str) -> dict[str, tuple[str, str]]:
    """crc → (principal, extras) em minutos."""
    jogos: dict[str, tuple[str, str]] = {}
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            if linha.startswith("#"):
                continue
            p = linha.split("\t")
            if len(p) >= 3:
                jogos[p[0]] = (p[1], p[2])
    return jogos



def igdb_api_atualiza_indice(caminho_indice: str, platform_id: int) -> dict:
    """Com credenciais da IGDB/Twitch no ambiente (`IGDB_CLIENT_ID` e
    `IGDB_CLIENT_SECRET` — gratuitas em dev.twitch.tv), baixa TODOS os jogos
    da plataforma direto da API oficial e atualiza o índice — cobertura
    completa, inclusive os jogos que o espelho do GameDB não tem. Sem
    credenciais, usa o índice embarcado como está."""
    import os
    import urllib.request

    idx = json.load(open(caminho_indice, encoding="utf-8"))
    client_id = os.environ.get("IGDB_CLIENT_ID")
    client_secret = os.environ.get("IGDB_CLIENT_SECRET")
    if not (client_id and client_secret):
        return idx

    req = urllib.request.Request(
        "https://id.twitch.tv/oauth2/token?client_id={}&client_secret={}"
        "&grant_type=client_credentials".format(client_id, client_secret),
        data=b"",
    )
    token = json.load(urllib.request.urlopen(req))["access_token"]

    offset = 0
    while True:
        pedido = (
            "fields name,total_rating,rating,aggregated_rating,"
            "first_release_date; where platforms = ({}); limit 500; offset {};"
        ).format(platform_id, offset)
        req = urllib.request.Request(
            "https://api.igdb.com/v4/games",
            data=pedido.encode(),
            headers={
                "Client-ID": client_id,
                "Authorization": "Bearer " + token,
                "Accept": "application/json",
            },
        )
        lote = json.load(urllib.request.urlopen(req))
        if not lote:
            break
        for g in lote:
            nota = g.get("total_rating") or g.get("rating") or g.get("aggregated_rating")
            key = normaliza(g.get("name") or "")
            if key and key not in idx:
                idx[key] = {"nota": round(nota) if nota else None, "ano": None}
        offset += 500
        print(f"  igdb api: {offset} jogos baixados...", file=sys.stderr)

    json.dump(idx, open(caminho_indice, "w"))
    print(f"  índice atualizado pela API: {len(idx)} jogos", file=sys.stderr)
    return idx

def nota_wikidata(score: str) -> int | None:
    """Normaliza os formatos do Wikidata ("93%", "8.2/10", "34/40",
    "4,5/5") para uma nota 0-100."""
    s = score.strip().replace(",", ".")
    m = re.match(r"^([\d.]+)\s*%$", s)
    if m:
        v = float(m.group(1))
        return round(v) if 0 <= v <= 100 else None
    m = re.match(r"^([\d.]+)\s*/\s*([\d.]+)$", s)
    if m:
        a, b = float(m.group(1)), float(m.group(2))
        if b > 0:
            v = round(a / b * 100)
            return v if 0 <= v <= 100 else None
    return None


def de_wikidata(caminho: str) -> dict[str, int]:
    """nome normalizado → nota (0-100) das críticas do Wikidata (P444 —
    Metacritic, GameRankings, Famitsu etc.). Falha com graça: sem o
    arquivo, sem notas de crítica."""
    try:
        d = json.load(open(caminho, encoding="utf-8"))
    except Exception:
        return {}
    notas: dict[str, int] = {}
    for b in d.get("results", {}).get("bindings", []):
        label = b["gameLabel"]["value"]
        if re.fullmatch(r"Q\d+", label):
            continue  # sem label em inglês — não casável por nome
        nota = nota_wikidata(b["score"]["value"])
        if nota is None:
            continue
        key = normaliza(label)
        if key and key not in notas:
            notas[key] = nota
    return notas


def nota_igdb(titulo: str, igdb: dict) -> str:
    reg = normaliza(titulo)
    if reg in igdb:
        return str(igdb[reg]["nota"] or "")
    if len(reg) < 4:
        return ""
    for cand, info in igdb.items():
        curto, longo = sorted((len(reg), len(cand)))
        if longo == curto:
            continue
        if (reg.startswith(cand) or cand.startswith(reg)) and curto / longo >= 0.75:
            return str(info["nota"] or "")
    return ""


def main() -> int:
    igdb = igdb_api_atualiza_indice("scripts/dados/igdb_snes.json", 58)
    dat = de_dat("scripts/dados/nointro_snes.dat")
    wikidata = de_wikidata("scripts/dados/wikidata_snes.json")
    tosec = de_tosec("scripts/dados/tosec_snes.txt")
    hltb = de_hltb("scripts/dados/hltb_snes.txt")

    linhas = []
    for crc in sorted(dat):
        info = dat[crc]
        nome = info["nome"]
        regiao = info["regiao"] or "outra"
        # o casamento com a IGDB usa o nome sem os sufixos "(...)"
        nota = nota_igdb(sem_sufixos(des_inverte_artigo(nome)), igdb)
        # a crítica do Wikidata cobre o que a IGDB não tem
        critica = ""
        if not nota:
            reg = normaliza(sem_sufixos(des_inverte_artigo(nome)))
            critica = str(wikidata[reg]) if reg in wikidata else ""
        ano, editora = tosec.get(crc, ("", ""))
        main, extra = hltb.get(crc, ("", ""))
        if not (nome or ano or editora or nota or critica or main or extra):
            continue
        linhas.append(
            f"{crc}\t{nome}\t{ano}\t{editora}\t{regiao}\t{nota}\t{critica}\t{main}\t{extra}"
        )

    print("# gerado por scripts/gen_infos_data.py — não editar à mão")
    print("# infos unificadas por CRC32: nome\\tano\\teditora\\tregião\\tnota (IGDB)\\tHLTB main\\tHLTB extra (minutos)")
    print(f"# {len(linhas)} CRC32 de {len(dat)}", file=sys.stderr)
    for linha in linhas:
        print(linha)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
