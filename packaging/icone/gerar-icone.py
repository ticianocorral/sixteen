#!/usr/bin/env python3
# Gera o ícone do app — o cartucho de SNES que o próprio app desenha no slot
# (crates/app/assets/example-cartridge.png) com a etiqueta da marca: o
# wordmark "SixteeN" e a faixa "RETROCONSOLE SYSTEM", recortados da logo
# oficial — e exporta todos os formatos do pacote. Rodar de dentro desta pasta:
#   python3 gerar-icone.py
# Saídas: packaging/icon_1024.png, packaging/macos/AppIcon.icns (via
# iconutil, só no macOS), packaging/windows/AppIcon.ico e
# packaging/linux/sixteen{,-512,-1024}.png.

import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter

AQUI = Path(__file__).resolve().parent
PACOTE = AQUI.parent
LOGO = PACOTE.parent / "site" / "assets" / "console-logo.png"
CARTUCHO = PACOTE.parent / "crates" / "app" / "assets" / "example-cartridge.png"
# O poço da etiqueta na arte do cartucho, em px da arte (1400x900).
POCO = (264, 22, 1141, 405)

# Desenhado em 2048 e reduzido: as bordas saem limpas em qualquer tamanho.
S = 2048
K = S / 1024

VERMELHO = (231, 11, 23)
CARVAO = (26, 21, 18)
CARVAO_CLARO = (40, 34, 29)


def k(v):
    return round(v * K)


def recortar(semente):
    """O componente vermelho da logo que contém `semente`, como máscara
    (alfa da logo, com a borda antialiased preservada)."""
    im = Image.open(LOGO).convert("RGBA")
    arr = np.array(im)
    alfa = arr[:, :, 3]
    vermelho = (arr[:, :, 0] > 150) & (arr[:, :, 1] < 110) & (alfa > 100)

    def crescer(m):
        d = m.copy()
        d[1:] |= m[:-1]
        d[:-1] |= m[1:]
        d[:, 1:] |= m[:, :-1]
        d[:, :-1] |= m[:, 1:]
        return d

    x, y = semente
    g = np.zeros_like(vermelho)
    g[y, x] = True
    while True:
        n = g
        for _ in range(20):
            n = crescer(n) & vermelho
        if n.sum() == g.sum():
            break
        g = n
    borda = g
    for _ in range(3):
        borda = crescer(borda)
    ys, xs = np.where(g)
    mascara = Image.fromarray(np.where(borda, alfa, 0).astype("uint8"))
    return mascara.crop((xs.min() - 4, ys.min() - 4, xs.max() + 5, ys.max() + 5))


def recortar_marca():
    """O wordmark "SixteeN" da logo — tudo que é vermelho acima da faixa."""
    im = Image.open(LOGO).convert("RGBA")
    arr = np.array(im)
    alfa = arr[:, :, 3]
    vermelho = (arr[:, :, 0] > 150) & (arr[:, :, 1] < 110) & (alfa > 100)
    linhas = np.where(~vermelho.any(axis=1))[0]
    # A primeira linha sem vermelho depois das letras separa a marca da faixa.
    corte = linhas[linhas > 100][0]
    mascara = np.where(np.arange(arr.shape[0])[:, None] < corte, alfa, 0).astype("uint8")
    m = Image.fromarray(mascara)
    return m.crop(m.getbbox())


def pintar(mascara, cor, altura=None, largura=None):
    if altura:
        largura = round(mascara.width * altura / mascara.height)
    else:
        altura = round(mascara.height * largura / mascara.width)
    m = mascara.resize((largura, altura), Image.LANCZOS)
    solido = Image.new("RGBA", m.size, cor + (255,))
    solido.putalpha(m)
    return solido


def degrade(caixa, topo, base):
    """Retângulo com degradê vertical, do tamanho de `caixa`."""
    x0, y0, x1, y1 = caixa
    w, h = x1 - x0, y1 - y0
    col = np.linspace(0, 1, h)[:, None, None]
    a = np.array(topo, dtype=float)[None, None, :]
    b = np.array(base, dtype=float)[None, None, :]
    px = (a + (b - a) * col).repeat(w, axis=1).astype("uint8")
    return Image.fromarray(px).convert("RGBA")


def mascara_rr(caixa, raio):
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle(caixa, raio, fill=255)
    return m


def camada(caixa, raio, topo, base):
    """Uma peça de plástico: degradê recortado num retângulo arredondado."""
    c = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    c.paste(degrade(caixa, topo, base), caixa[:2])
    c.putalpha(ImageChops.multiply(c.split()[3], mascara_rr(caixa, raio)))
    return c


def sombra(mascara, desloc, raio, opacidade):
    s = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    s.putalpha(mascara.point(lambda v: int(v * opacidade)))
    s = s.filter(ImageFilter.GaussianBlur(raio))
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.alpha_composite(s, (0, desloc))
    return out


def icone():
    marca = recortar_marca()
    faixa = recortar((30, 800))

    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    # O cartucho do slot, centrado, com a sombra no "chão" do Dock.
    cart = Image.open(CARTUCHO).convert("RGBA")
    esc = k(960) / cart.width
    cart = cart.resize((k(960), round(cart.height * esc)), Image.LANCZOS)
    cx0, cy0 = (S - cart.width) // 2, (S - cart.height) // 2
    silhueta = Image.new("L", (S, S), 0)
    silhueta.paste(cart.split()[3], (cx0, cy0))
    img.alpha_composite(sombra(silhueta, k(20), k(24), 0.55))
    img.alpha_composite(cart, (cx0, cy0))

    # A etiqueta no poço: carvão com listras diagonais finas, o wordmark e
    # a faixa da logo.
    etiqueta = (
        cx0 + round(POCO[0] * esc),
        cy0 + round(POCO[1] * esc),
        cx0 + round(POCO[2] * esc),
        cy0 + round(POCO[3] * esc),
    )
    et = camada(etiqueta, k(6), CARVAO_CLARO, CARVAO)
    listras = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    dl = ImageDraw.Draw(listras)
    for i in range(-S, S, k(22)):
        dl.line((i, S, i + S, 0), fill=(255, 255, 255, 9), width=k(7))
    listras.putalpha(ImageChops.multiply(listras.split()[3], et.split()[3]))
    et.alpha_composite(listras)
    img.alpha_composite(et)

    margem = k(16)
    larg_et = etiqueta[2] - etiqueta[0]
    # Faixa e nome na mesma largura, como na logo oficial.
    barra = pintar(faixa, VERMELHO, largura=round(larg_et * 0.9))
    barra_y = etiqueta[3] - margem - barra.height
    img.alpha_composite(barra, ((etiqueta[0] + etiqueta[2] - barra.width) // 2, barra_y))
    # O wordmark inteiro, o maior que couber acima da faixa, centrado.
    alt_max = barra_y - etiqueta[1] - margem * 2
    nome = pintar(marca, VERMELHO, largura=round(larg_et * 0.9))
    if nome.height > alt_max:
        nome = pintar(marca, VERMELHO, altura=alt_max)
    topo = etiqueta[1] + (barra_y - etiqueta[1] - nome.height) // 2
    img.alpha_composite(nome, ((etiqueta[0] + etiqueta[2] - nome.width) // 2, topo))
    # O reflexo do papel brilhante no alto da etiqueta.
    brilho = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(brilho).polygon(
        [(etiqueta[0], etiqueta[1]), (etiqueta[2], etiqueta[1]),
         (etiqueta[2], etiqueta[1] + k(30)), (etiqueta[0], etiqueta[1] + k(110))],
        fill=(255, 255, 255, 14))
    brilho.putalpha(ImageChops.multiply(brilho.split()[3], et.split()[3]))
    img.alpha_composite(brilho)

    return img.resize((1024, 1024), Image.LANCZOS)


def main():
    ic = icone()
    ic.save(PACOTE / "icon_1024.png")
    for tam, nome in ((256, "sixteen.png"), (512, "sixteen-512.png"), (1024, "sixteen-1024.png")):
        ic.resize((tam, tam), Image.LANCZOS).save(PACOTE / "linux" / nome)
    ic.save(PACOTE / "windows" / "AppIcon.ico",
            sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as tmp:
            iconset = Path(tmp) / "AppIcon.iconset"
            iconset.mkdir()
            for base in (16, 32, 128, 256, 512):
                ic.resize((base, base), Image.LANCZOS).save(iconset / f"icon_{base}x{base}.png")
                ic.resize((base * 2, base * 2), Image.LANCZOS).save(
                    iconset / f"icon_{base}x{base}@2x.png")
            subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o",
                            str(PACOTE / "macos" / "AppIcon.icns")], check=True)
    else:
        print("iconutil não encontrado — AppIcon.icns não foi gerado")
    print("ícones ok")


if __name__ == "__main__":
    main()
