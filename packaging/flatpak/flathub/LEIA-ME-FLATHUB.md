# Flathub — kit de submissão do SixteeN

**Nada foi submetido.** Este diretório organiza tudo o que o Flathub exige
para que você revise e dê o ok. Referência:
https://docs.flathub.org/docs/for-app-authors/submission

## Arquivos preparados

| Arquivo | Papel |
| --- | --- |
| `io.github.ticianocorral.sixteen.yml` (aqui) | Manifest do PR — vai para a raiz do fork do `flathub/flathub` |
| `cargo-sources.json` (aqui) | 226 crates do crates.io com sha256 (gerado do `Cargo.lock`) — vai junto no PR |
| `gerar-cargo-sources.py` (aqui) | Regenera o JSON acima sempre que o `Cargo.lock` mudar |
| `../io.github.ticianocorral.sixteen.metainfo.xml` | Metainfo **upstream** (instalado pelo build; nunca se copia metainfo para o PR do Flathub) |
| `../io.github.ticianocorral.sixteen.desktop` | Desktop file upstream, inglês + pt-BR |
| `DESCRICAO-PR.md` (aqui) | Rascunho em inglês do corpo do PR |

O flatpak de publicação própria (`dev.ticianocorral.sixteen.yml`) ficou
intocado — os dois coexistem.

## Decisões tomadas (confirme)

1. **App ID `io.github.ticianocorral.sixteen`** — o ID atual
   (`dev.ticianocorral.sixteen`) pressupõe o domínio `ticianocorral.dev`,
   que **não existe** (verificado: DNS não resolve). O Flathub exige que o
   domínio do ID exista e esteja sob seu controle em HTTPS. O prefixo
   `io.github.` casa com o hosting real (`github.com/ticianocorral/sixteen`)
   e cumpre o mínimo de 4 componentes.
2. **Runtime `org.freedesktop.Platform 25.08`** — o Flathub exige a série
   mais recente; confirme com `flatpak remote-info flathub
   org.freedesktop.Platform` e ajuste o `runtime-version` do yml se houver
   série mais nova.
3. **Build 100% offline** — regra dura do Flathub. O curl do rustup e o
   cargo na rede do flatpak próprio viraram: extensão `rust-stable` do
   runtime + `cargo-sources.json`. O SDL3 vem embutido nos crates
   `sdl3-src`/`sdl3-image-src`/`sdl3-mixer-src` (crates.io), então o feature
   `vendored-sdl` funciona offline — o `Cargo.lock` não tem nenhuma
   dependência de git.
4. **Screenshots fixados na tag `v1.3.1`** (regra do Flathub: nunca apontar
   para branch). São as 3 capturas reais do `site/assets/`.
5. **Metainfo bilíngue** (inglês como base + pt-BR) com nota de que a UI
   está apenas em português — o Flathub exige inglês completo em UI,
   desktop file e metainfo; a nota na descrição é o caminho previsto para
   apps de região/idioma específico.

## Verificar ANTES de abrir o PR (em uma máquina Linux)

1. **Build local de verdade** (o PR já nasce mais forte):
   ```bash
   flatpak install -y flathub org.flatpak.Builder
   # dentro de uma pasta só com o yml e o cargo-sources.json:
   flatpak run --command=flatpak-builder org.flatpak.Builder \
     --user --install --force-clean builddir io.github.ticianocorral.sixteen.yml
   flatpak run io.github.ticianocorral.sixteen
   ```
2. **⚠️ O maior risco técnico: versão do Rust.** O workspace exige
   `rust-version = 1.98`; a extensão `rust-stable` do runtime pode trazer
   um rustc mais velho (a nota no flatpak próprio diz que a da série 24.08
   era 1.89). Cheque antes de tudo:
   ```bash
   flatpak run --command=bash org.freedesktop.Sdk//25.08 \
     -c '/usr/lib/sdk/rust-stable/bin/rustc --version'
   ```
   Se for mais velho que o código exige: (a) confirme se o código realmente
   precisa de 1.98 e abaixe o `rust-version`; ou (b) suba a série do
   runtime. Não há como burlar com rustup na rede — o Flathub proíbe.
3. **Lint** (erros E warnings são fatais na review):
   ```bash
   flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
     appstream ../io.github.ticianocorral.sixteen.metainfo.xml
   flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
     manifest io.github.ticianocorral.sixteen.yml
   ```
4. **Desktop file**: `desktop-file-validate ../io.github.ticianocorral.sixteen.desktop`.
5. **UI só em pt-BR** — decida conscientemente: traduzir a UI para inglês
   antes de submeter, ou assumir o risco de a review pedir. O metainfo e o
   desktop já estão bilíngues.
6. **`flathub.json`**: não é necessário — o build roda em x86_64 e aarch64
   (crates do SDL são portáveis). Só crie um se quiser limitar arquitetura.
7. **GitHub com 2FA ativo** — obrigatório para virar maintainer do repo do
   app; o convite expira em 1 semana.

## Passo a passo da submissão (depois do seu ok)

1. Fork de `flathub/flathub` **com "Copy the master branch only"
   DESMARCADO** (ou `gh repo fork --clone flathub/flathub`).
2. Na fork, crie uma branch a partir de `new-pr`.
3. Copie para a raiz **apenas** o `io.github.ticianocorral.sixteen.yml` e o
   `cargo-sources.json`. Commit e push.
4. Abra o PR com **base em `new-pr`** (não em `master`!) e título
   `Add io.github.ticianocorral.sixteen` — corpo em `DESCRICAO-PR.md` +
   template do Flathub.
5. Comente `bot, build` para disparar o build de teste (screenshots não
   aparecem nele — normal).
6. Responda todos os comentários dos reviewers; nunca feche o PR para
   refazer (nem em caso de troca de app ID); não dê merge de `master` na
   branch.
7. Aprovado → os reviewers fazem merge num repo novo da org Flathub e você
   é convidado como maintainer (aceitar em 1 semana). Publicação ~1–2h
   depois.

## Manutenção pós-publicação

- Release nova: atualize `tag` + `commit` no manifest (o `x-checker-data`
  faz o bot propor o PR sozinho), rode `gerar-cargo-sources.py` se o
  `Cargo.lock` mudou e adicione a release no metainfo (datas no passado,
  versões em ordem decrescente).
- Se as screenshots mudarem, troque a tag nas URLs das imagens.
