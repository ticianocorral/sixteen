//! Portable app layout: every folder the app uses lives in one root — no
//! database. `roms/` (drop ROMs here), `core/` (the snes9x core), `assets/`
//! (local cover/logo art), `saves/`, `notes/`, plus `sixteen.cfg` and
//! `library.json` at the root.
//!
//! macOS special case: the `.app` on this platform ships in `/Applications`
//! (or wherever Finder drags it, often read-only-ish and not somewhere a
//! user expects an app to scribble folders into). So on macOS the root
//! isn't next to the executable at all — it's `~/Documents/SixteeN`,
//! created on first launch, same spirit as how a normal Mac app keeps its
//! user data.
//!
//! Linux special case: AppImage is a read-only container that extracts to a
//! temp directory. So on Linux the root is `~/.local/share/SixteeN`
//! (following XDG directory conventions), created on first launch. This
//! also supports regular Linux builds next to the executable.
//!
//! Windows keeps the simpler "next to the executable" portable layout, since
//! a `.exe` anywhere the user put it is already writable and exactly where
//! they'd look for `roms/` next to it.

use std::path::Path;
use std::path::PathBuf;

/// The folder the app treats as its root — see the module doc for the macOS
/// special case.
pub fn app_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        macos_root_for(&home)
    }
    #[cfg(target_os = "linux")]
    {
        linux_app_root()
    }
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    }
}

/// `~/Documents/SixteeN` — split out from `app_root` so it can be
/// unit-tested with a synthetic home directory.
#[cfg(target_os = "macos")]
fn macos_root_for(home: &Path) -> PathBuf {
    home.join("Documents").join("SixteeN")
}

/// XDG_DATA_HOME/.../SixteeN, falling back to ~/.local/share if unset.
#[cfg(target_os = "linux")]
fn linux_app_root() -> PathBuf {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    data_home.join("SixteeN")
}

pub fn roms_dir() -> PathBuf {
    app_root().join("roms")
}

/// Onde o download do update do app fica até ser aplicado no arranque
/// seguinte — na raiz do app (plan revision: "colocar o update na raiz das
/// pastas do app nao dentro dos saves"; saves guardam progresso de jogo).
pub fn update_dir() -> PathBuf {
    let p = app_root().join("update");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Igual a [`update_dir`], sem criar — o apply no arranque não deve ter
/// efeito colateral em disco.
pub fn update_dir_opt() -> Option<PathBuf> {
    Some(app_root().join("update"))
}

pub fn core_dir() -> PathBuf {
    app_root().join("core")
}

pub fn assets_dir() -> PathBuf {
    app_root().join("assets")
}

pub fn saves_dir() -> PathBuf {
    app_root().join("saves")
}

/// Tudo do RetroAchievements (cache por jogo, ids ganhos, sessões de
/// progresso, envios pendentes, badges) — plan revision: "dados do
/// retroachievements da pasta save devem ficar em uma pasta
/// 'retroachievements' na raiz - pasta save apenas são os saves dos jogos".
pub fn retroachievements_dir() -> PathBuf {
    app_root().join("retroachievements")
}

pub fn notes_dir() -> PathBuf {
    app_root().join("notes")
}

/// Os arquivos de configuração/identificação num lugar só — plan revision:
/// "criar pasta config e colocar o cfg, o dat, library e o hash".
pub fn config_dir() -> PathBuf {
    app_root().join("config")
}

pub fn config_path() -> PathBuf {
    config_dir().join("sixteen.cfg")
}

/// Play counts / added-at / last-played-at, keyed by ROM hash — the only
/// state that needs to survive between runs (everything else is recomputed
/// by scanning `roms/` fresh each launch). O hash cache do catálogo
/// (`hashcache.json`) vive ao lado, derivado deste path por
/// `with_file_name`.
pub fn library_path() -> PathBuf {
    config_dir().join("library.json")
}

/// A No-Intro DAT (XML) for canonical ROM titles — optional, supplied by
/// whoever runs the app (no direct download link exists on No-Intro's own
/// site to fetch it automatically).
pub fn nointro_dat_path() -> PathBuf {
    config_dir().join("nointro.dat")
}

/// The data root under the app's previous name ("SNES Xperience"), when
/// this platform ever had one — Windows always kept the exe-relative root,
/// so there's nothing to migrate there.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn legacy_app_root() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Some(home.join("Documents").join("SNES Xperience"))
    }
    #[cfg(target_os = "linux")]
    {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(data_home.join("SNES Xperience"))
    }
}

/// One-time migration for the app's rename (SNES Xperience → SixteeN): the
/// data root moves to the new name while the new one doesn't exist yet, and
/// `config/xperience.cfg` becomes `sixteen.cfg` inside whichever root we
/// end up in (Windows included — its root never moved, the cfg name did).
/// Best-effort and idempotent: a failed move logs and the app starts fresh
/// in the new location; both sides already in place means nothing happens.
/// Call once at startup, before anything reads [`app_root`] — the create-
/// dir-on-first-launch passes would otherwise block the folder rename.
pub fn migrate_renamed_root() {
    let root = app_root();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if !root.exists() {
        if let Some(legacy) = legacy_app_root() {
            if legacy.is_dir() {
                rename_dir_best_effort(&legacy, &root);
            }
        }
    }
    migrate_cfg_name(&config_dir());
    migrate_brand_overrides(&assets_dir(), &config_dir());
}

fn rename_dir_best_effort(legacy: &Path, root: &Path) {
    if let Some(parent) = root.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::rename(legacy, root) {
        Ok(()) => log::info!("migrado: {} -> {}", legacy.display(), root.display()),
        Err(e) => log::warn!(
            "migrando a raiz do app ({} -> {}): {e}",
            legacy.display(),
            root.display()
        ),
    }
}

/// `xperience.cfg` → `sixteen.cfg` inside `config/` — same file, new name.
fn migrate_cfg_name(config: &Path) {
    let old = config.join("xperience.cfg");
    let new = config.join("sixteen.cfg");
    if !old.is_file() || new.exists() {
        return;
    }
    match std::fs::rename(&old, &new) {
        Ok(()) => log::info!("migrado: config/xperience.cfg -> config/sixteen.cfg"),
        Err(e) => log::warn!("migrando config/xperience.cfg: {e}"),
    }
}

/// One-shot cleanup of stale brand overrides: `assets/console.png` and
/// `assets/console-tag.png` dropped in the data root before the rename
/// carry the old wordmark, and the local-file-wins rule (deliberate — see
/// `console_art`) would keep it on screen forever, silently undoing the
/// rebrand. Renamed to `.bak`, never deleted (the override feature
/// stays); a marker in `config/` gates it so overrides the player
/// installs after the rebrand are left alone.
fn migrate_brand_overrides(assets: &Path, config: &Path) {
    let marker = config.join(".rebrand-sixteen");
    if marker.exists() {
        return;
    }
    for name in ["console.png", "console-tag.png"] {
        let src = assets.join(name);
        let dst = assets.join(format!("{name}.bak"));
        if !src.is_file() || dst.exists() {
            continue;
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => log::info!("migrado: assets/{name} -> {name}.bak (wordmark antigo)"),
            Err(e) => log::warn!("migrando assets/{name}: {e}"),
        }
    }
    let _ = std::fs::write(&marker, b"1");
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn update_dir_lives_at_the_app_root_not_saves() {
        // Plan revision: "colocar o update na raiz das pastas do app nao
        // dentro dos saves".
        let dir = super::update_dir_opt().expect("update dir");
        assert_eq!(dir, super::app_root().join("update"));
        assert!(!dir.starts_with(super::saves_dir()));
    }

    #[test]
    fn renamed_cfg_migrates_once() {
        let tmp = std::env::temp_dir().join(format!("sixteen-dirs-cfg-{}", std::process::id()));
        let config = tmp.join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("xperience.cfg"), "# t\n").unwrap();

        super::migrate_cfg_name(&config);
        assert!(config.join("sixteen.cfg").is_file());
        assert!(!config.join("xperience.cfg").exists());

        // Idempotente: rodar de novo com só o nome novo não explode nem
        // toca em nada.
        super::migrate_cfg_name(&config);
        assert!(config.join("sixteen.cfg").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stale_brand_overrides_are_renamed_once() {
        let tmp = std::env::temp_dir().join(format!("sixteen-dirs-brand-{}", std::process::id()));
        let assets = tmp.join("assets");
        let config = tmp.join("config");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(assets.join("console.png"), b"logo antiga").unwrap();

        super::migrate_brand_overrides(&assets, &config);
        assert!(!assets.join("console.png").exists());
        assert!(assets.join("console.png.bak").is_file());
        assert!(config.join(".rebrand-sixteen").exists());

        // One-shot: um override instalado DEPOIS do rebrand fica — o marker
        // impede a migração de tocar nele de novo.
        std::fs::write(assets.join("console.png"), b"logo nova do jogador").unwrap();
        super::migrate_brand_overrides(&assets, &config);
        assert!(assets.join("console.png").is_file());
        assert!(!assets.join("console.png.bak.bak").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_root_moves_when_the_new_one_is_still_missing() {
        let tmp = std::env::temp_dir().join(format!("sixteen-dirs-root-{}", std::process::id()));
        let legacy = tmp.join("SNES Xperience");
        let root = tmp.join("SixteeN");
        std::fs::create_dir_all(legacy.join("saves")).unwrap();
        std::fs::write(legacy.join("saves").join("game.srm"), b"x").unwrap();

        super::rename_dir_best_effort(&legacy, &root);
        assert!(root.join("saves").join("game.srm").is_file());
        assert!(!legacy.exists());

        // Raiz nova já existente nunca é sobrescrita — mas isso é decisão
        // do chamador (`migrate_renamed_root` só chama com a raiz ausente);
        // aqui cobrimos só o move em si, que é best-effort.
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_root_is_documents_sixteen() {
        assert_eq!(
            super::macos_root_for(Path::new("/Users/rex")),
            Path::new("/Users/rex/Documents/SixteeN")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_root_uses_xdg_data_home() {
        assert_eq!(
            super::linux_app_root(),
            Path::new(
                std::env::var("XDG_DATA_HOME")
                    .as_deref()
                    .unwrap_or(&format!(
                        "{}/.local/share",
                        std::env::var("HOME").unwrap_or_default()
                    ))
            )
            .join("SixteeN")
        );
    }
}
