//! Artes de exemplo embutidas no binário e semeadas na pasta de dados —
//! plan revision: "quando instalar o app, na pasta que ele cria colocar os
//! assets como exemplo nas pastas correspondentes".
//!
//! Cada slot de arte (`assets/{cover,cartridge,backcover}` e
//! `assets/manual`) recebe um `exemplo.*`: os mocks gerados por
//! `packaging/exemplo-mock/gerar-mocks.py`, que servem de molde (tamanho e
//! formato de cada slot) pro jogador criar as artes dos seus jogos — e, no
//! caso do cartucho, um shell cinza sem rótulo pronto pra receber uma
//! label nova. Nunca sobrescreve: arquivo já existente é do jogador.
//! (`assets/logo/` fica de fora — sem arte o app já tem logo padrão
//! própria, e a semeadura ali seria só ruído.)

use std::path::Path;

const COVER_PNG: &[u8] = include_bytes!("../assets/exemplo-cover.png");
const CARTRIDGE_PNG: &[u8] = include_bytes!("../assets/exemplo-cartucho.png");
const BACKCOVER_PNG: &[u8] = include_bytes!("../assets/exemplo-backcover.png");
const MANUAL_PDF: &[u8] = include_bytes!("../assets/exemplo-manual.pdf");

/// Semeia os exemplos sob `assets/` (pastas criadas no arranque, antes
/// desta chamada). Best-effort: falha de escrita loga e segue.
pub fn seed(assets: &Path) {
    for (nome, bytes) in [
        ("cover/exemplo.png", COVER_PNG),
        ("cartridge/exemplo.png", CARTRIDGE_PNG),
        ("backcover/exemplo.png", BACKCOVER_PNG),
        ("manual/exemplo.pdf", MANUAL_PDF),
    ] {
        let alvo = assets.join(nome);
        if alvo.is_file() {
            continue;
        }
        if let Some(pai) = alvo.parent() {
            let _ = std::fs::create_dir_all(pai);
        }
        match std::fs::write(&alvo, bytes) {
            Ok(()) => log::info!("exemplo semeado: assets/{nome}"),
            Err(e) => log::warn!("semeando assets/{nome}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    fn assets_dir_de_teste(rotulo: &str) -> std::path::PathBuf {
        let tmp =
            std::env::temp_dir().join(format!("sixteen-exemplo-{rotulo}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    #[test]
    fn semeia_um_exemplo_em_cada_pasta_de_arte() {
        let assets = assets_dir_de_teste("semeia");
        super::seed(&assets);

        for nome in [
            "cover/exemplo.png",
            "cartridge/exemplo.png",
            "backcover/exemplo.png",
            "manual/exemplo.pdf",
        ] {
            let arquivo = assets.join(nome);
            assert!(arquivo.is_file(), "faltou {nome}");
            assert!(fs::metadata(&arquivo).unwrap().len() > 0, "{nome} vazio");
        }
        // A logo não é semeada — o app já tem logo padrão própria.
        assert!(!assets.join("logo/exemplo.png").exists());

        // Arquivo do jogador vence: conteúdo trocado não volta a ser o mock.
        fs::write(assets.join("cover/exemplo.png"), b"arte do jogador").unwrap();
        super::seed(&assets);
        assert_eq!(
            fs::read(assets.join("cover/exemplo.png")).unwrap(),
            b"arte do jogador"
        );

        let _ = fs::remove_dir_all(&assets);
    }

    #[test]
    fn semeia_mesmo_sem_as_pastas_de_arte() {
        // O arranque cria as pastas antes de semear, mas a função não deve
        // depender disso (pasta de dados apagada no meio, chamada avulsa).
        let assets = assets_dir_de_teste("sem-pastas");
        super::seed(&assets);
        assert!(assets.join("manual/exemplo.pdf").is_file());
        let _ = fs::remove_dir_all(&assets);
    }
}
