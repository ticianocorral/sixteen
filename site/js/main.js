// SixteeN — manual: interações mínimas.

// Ano do rodapé.
document.getElementById("ano").textContent = new Date().getFullYear();

// Revelação sutil ao rolar (figuras, cartões e cabeçalhos de seção).
const alvos = document.querySelectorAll(
  ".figura, .passo, .lateral, .aviso, .sumario-lista, .tabela"
);
alvos.forEach((el) => el.classList.add("reveal-in"));
const io = new IntersectionObserver(
  (entradas) => {
    for (const e of entradas) {
      if (e.isIntersecting) {
        e.target.classList.add("reveal-ok");
        io.unobserve(e.target);
      }
    }
  },
  { threshold: 0.15 }
);
alvos.forEach((el) => io.observe(el));

// Idioma: o <head> já escolheu antes do paint; aqui ficam o botão EN/PT e
// os textos alternativos das imagens.
function aplicarIdioma(l) {
  const raiz = document.documentElement;
  raiz.dataset.lang = l;
  raiz.lang = l === "pt" ? "pt-BR" : "en";
  document.title = l === "pt" ? "SixteeN — manual do proprietário" : "SixteeN — owner's manual";
  document.querySelectorAll("img[data-alt-en]").forEach((img) => {
    img.alt = img.dataset[l === "pt" ? "altPt" : "altEn"];
  });
  document.querySelectorAll("[data-seletor-para]").forEach(atualizarAltSeletor);
  const banner = document.getElementById("konami");
  if (banner && banner.classList.contains("konami-ok")) banner.textContent = konamiOk();
}
document.querySelectorAll("[data-set-lang]").forEach((b) => {
  b.addEventListener("click", () => {
    const l = b.dataset.setLang;
    try { localStorage.setItem("sixteen-lang", l); } catch (e) {}
    aplicarIdioma(l);
  });
});

// Seletores: um grupo de botões troca a imagem da figura irmã.
function atualizarAltSeletor(grupo) {
  const fig = document.querySelector(`[data-seletor="${grupo.dataset.seletorPara}"]`);
  const ativo = grupo.querySelector('[aria-pressed="true"]');
  if (!fig || !ativo) return;
  const titulo = grupo.dataset.seletorPara === "tv" ? "Contra III" : "SixteeN";
  const l = document.documentElement.dataset.lang;
  const rotulo = ativo.querySelector(`[data-l="${l}"]`) || ativo;
  fig.querySelector("[data-seletor-img]").alt = `${titulo} — ${rotulo.textContent.trim()}`;
}
document.querySelectorAll("[data-seletor-para]").forEach((grupo) => {
  const fig = document.querySelector(`[data-seletor="${grupo.dataset.seletorPara}"]`);
  if (!fig) return;
  const img = fig.querySelector("[data-seletor-img]");
  const botoes = grupo.querySelectorAll("button[data-src]");
  botoes.forEach((b) => {
    // pré-carrega no primeiro hover/foco para a troca ser instantânea
    const pre = () => { const i = new Image(); i.src = b.dataset.src; };
    b.addEventListener("pointerenter", pre, { once: true });
    b.addEventListener("focus", pre, { once: true });
    b.addEventListener("click", () => {
      botoes.forEach((o) => o.setAttribute("aria-pressed", String(o === b)));
      img.src = b.dataset.src;
      atualizarAltSeletor(grupo);
    });
  });
});

aplicarIdioma(document.documentElement.dataset.lang === "pt" ? "pt" : "en");

// Versão real da última release (o texto fixo no HTML é o fallback).
const selos = document.querySelectorAll("[data-release]");
if (selos.length) {
  fetch(
    "https://api.github.com/repos/ticianocorral/sixteen/releases/latest"
  )
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((r) => {
      const tag = String(r.tag_name || "").replace(/^v/, "");
      if (tag) selos.forEach((s) => (s.textContent = tag));
    })
    .catch(() => {}); // sem rede ou rate limit: fica o valor do HTML
}

// Código Konami no site também — claro que sim. O bannerzinho fica lá
// embaixo convidando; quem completa vê a mensagem virar.
const konami = [
  "ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown",
  "ArrowLeft", "ArrowRight", "ArrowLeft", "ArrowRight", "b", "a",
];
let passo = 0;
document.addEventListener("keydown", (ev) => {
  const ok = ev.key === konami[passo] || ev.key.toLowerCase() === konami[passo];
  passo = ok ? passo + 1 : ev.key === konami[0] ? 1 : 0;
  if (passo === konami.length) {
    passo = 0;
    const banner = document.getElementById("konami");
    if (banner) {
      banner.classList.add("konami-ok");
      banner.textContent = konamiOk();
    }
  }
});

function konamiOk() {
  return document.documentElement.dataset.lang === "pt"
    ? "código Konami aceito — no app, isso libera o modo dev ↑↑↓↓←→←→BA"
    : "Konami code accepted — in the app, this unlocks dev mode ↑↑↓↓←→←→BA";
}
