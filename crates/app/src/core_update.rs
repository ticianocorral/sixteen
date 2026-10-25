//! Download/update do core snes9x — uma ação da tela de configurações. O
//! core continua fora do pacote do app (licença non-commercial do snes9x,
//! ver `THIRD-PARTY-NOTICES.md`); o download vem do **asset da release do
//! próprio app no GitHub** e é aceito somente se o SHA-256 do binário
//! extraído bater com o hash embutido nesta release (`CORE_ASSETS`,
//! gerado por `packaging/prepare-core-asset.sh`) — a raiz de confiança é o
//! binário do app que o usuário instalou, não o canal de download. O
//! buildbot do libretro continua sendo a *origem* dos builds (o script de
//! release puxa de lá uma vez por release), só não é mais o canal direto:
//! é um nightly sem versão e sem checksum, e um `dlopen` de bytes não
//! verificados vindos dele seria execução de código arbitrário caso o
//! CDN fosse comprometido.

use std::io::Read;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

use sixteen_platform::tr;

/// Target triple desta build — vazio quando não há core homologado para a
/// combinação (e.g. Linux/arm64), o que desliga o download com o fallback
/// manual da tela de configurações.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const TARGET_TRIPLE: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
const TARGET_TRIPLE: &str = "";

/// Cores snes9x homologados para ESTA release: target triple → SHA-256 do
/// binário do core (não do zip — o que é `dlopen`ado é o binário).
/// Regenerar com `packaging/prepare-core-asset.sh` a cada release e subir
/// os zips gerados como assets da release no GitHub.
const CORE_ASSETS: &[(&str, &str)] = &[
    (
        "x86_64-unknown-linux-gnu",
        "9b2b6ea9d88f96c156590443ddd05ee56aaf4e495329a889c979921942717a31",
    ),
    (
        "aarch64-apple-darwin",
        "8ed333ac04544cc6ab67ceb445d095f7600bb17586d4887d99117fbd1ef776f6",
    ),
    (
        "x86_64-apple-darwin",
        "120e5f4c32e00c686a40bdd37e107682b0dd3c1eadf19dcd4ec91d39de1ffa8d",
    ),
    (
        "x86_64-pc-windows-msvc",
        "04828df677dec0e839583edda861bdc80656b06a32eef7e639bbdd0cd1ba0f05",
    ),
];

/// O SHA-256 esperado do core desta plataforma — `None` onde não há build
/// homologada.
fn core_expected_sha256() -> Option<&'static str> {
    CORE_ASSETS
        .iter()
        .find(|(t, _)| *t == TARGET_TRIPLE)
        .map(|(_, sha)| *sha)
}

/// The core file's name on this platform — what `sixteen` looks for in
/// `core/` at launch, and what a download is saved as.
pub fn core_file_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "snes9x_libretro.dylib"
    } else if cfg!(target_os = "windows") {
        "snes9x_libretro.dll"
    } else {
        "snes9x_libretro.so"
    }
}

/// A URL do core homologado para esta plataforma na release do GitHub do
/// app — `None` para uma combinação sem build (e.g. Linux/arm64), onde a
/// tela de configurações cai no "baixe manualmente".
pub fn core_download_url() -> Option<String> {
    core_expected_sha256()?;
    Some(format!(
        "https://github.com/ticianocorral/sixteen/releases/download/v{}/snes9x_libretro-{TARGET_TRIPLE}.zip",
        env!("CARGO_PKG_VERSION")
    ))
}

/// Progress/result reported back from the download thread.
pub enum CoreUpdateMsg {
    Progress { downloaded: u64, total: Option<u64> },
    Done,
    Failed(String),
}

/// The cabinet's nameplate text (plan revision: "mostrar versao do app e
/// versao do snes9x, onde esta o nome do app na tv") — the app's own
/// version on the first line and, if a core is installed, the core's
/// version on a second line below it (`draw_brand` splits on '\n'). Lives
/// here rather than in `sixteen` so the idle screen can rebuild it the
/// moment a setup-screen download finishes — the nameplate used to stay
/// without the snes9x line until the player left the screen.
/// `Core::load` only resolves symbols and reads that info (no `retro_
/// init`), so peeking at it here and dropping the `Core` right after is
/// cheap and side-effect-free.
pub fn nameplate_text(core_path: Option<&std::path::Path>) -> String {
    let app_version = env!("CARGO_PKG_VERSION");
    let core_version = core_path
        .and_then(|p| sixteen_emulation::Core::load(p).ok())
        .map(|c| c.system_version().to_string())
        .filter(|v| !v.is_empty());
    match core_version {
        Some(v) => format!("{} v{app_version}\nsnes9x {v}", sixteen_platform::BRAND),
        None => format!("{} v{app_version}", sixteen_platform::BRAND),
    }
}

/// The default core location, resolved the same way `sixteen` does at
/// startup (`--core`/`$SIXTEEN_CORE` aside) — used to refresh the
/// nameplate after an in-screen download.
pub fn default_core_path() -> Option<std::path::PathBuf> {
    let p = crate::dirs::core_dir().join(core_file_name());
    p.is_file().then_some(p)
}

/// The commit suffix of a core's `library_version` — "1.63 fae2fea" →
/// "fae2fea" (lowercase). The libretro buildbot embeds the snes9x commit it
/// built from; a version string without a trailing hex chunk ("1.63",
/// "1.62.3") reports no commit.
pub fn commit_from_version(version: &str) -> Option<String> {
    let last = version.split_whitespace().next_back()?;
    let looks_like_sha =
        (7..=40).contains(&last.len()) && last.chars().all(|c| c.is_ascii_hexdigit());
    looks_like_sha.then(|| last.to_ascii_lowercase())
}

/// The commit the core at `core_path` was built from — `None` if it doesn't
/// load or doesn't report one. Like `nameplate_text`, this `Core::load`s
/// (no `retro_init`), so it's read-only and cheap; safe to call from a
/// background thread (`update_check` does).
pub fn core_commit(core_path: &std::path::Path) -> Option<String> {
    let core = sixteen_emulation::Core::load(core_path).ok()?;
    commit_from_version(core.system_version())
}

/// Download `url` and unzip the core into `dest_dir/core_file_name()`,
/// reporting progress on `tx`. Meant to run on a background thread (the app's
/// standard spawn + `mpsc` + per-frame `try_recv()` pattern) — a
/// `.call()`/full read can take a few seconds.
pub fn download_and_install(url: &str, dest_dir: &Path, tx: &Sender<CoreUpdateMsg>) {
    let msg = match try_download(url, dest_dir, tx) {
        Ok(()) => CoreUpdateMsg::Done,
        Err(e) => CoreUpdateMsg::Failed(e),
    };
    let _ = tx.send(msg);
}

/// Teto para o download (zip do core: ~1–5 MB na prática; 32 MB sobra) —
/// sem ele, um servidor/CDN comprometido poderia encher a RAM ou o disco.
const MAX_CORE_DOWNLOAD: usize = 32 * 1024 * 1024;

fn try_download(url: &str, dest_dir: &Path, tx: &Sender<CoreUpdateMsg>) -> Result<(), String> {
    let Some(expected_sha) = core_expected_sha256() else {
        return Err(tr!(
            "no verified core build for this platform",
            "sem build verificada do core para esta plataforma"
        )
        .to_string());
    };
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());

    let mut reader = resp.into_reader();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() > MAX_CORE_DOWNLOAD {
            return Err(tr!(
                "download larger than expected — aborted",
                "download maior que o esperado — abortado"
            )
            .to_string());
        }
        let _ = tx.send(CoreUpdateMsg::Progress {
            downloaded: bytes.len() as u64,
            total,
        });
    }

    let core_ext = Path::new(core_file_name())
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut index = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Ok(entry_name) = entry.name() else {
            continue;
        };
        let matches = entry_name
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(core_ext));
        if matches {
            index = Some(i);
            break;
        }
    }
    let index = index.ok_or_else(|| {
        tr!(
            format!("no .{core_ext} file inside the zip"),
            format!("nenhum arquivo .{core_ext} dentro do zip")
        )
    })?;
    let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    entry.read_to_end(&mut out).map_err(|e| e.to_string())?;

    // Nada é gravado (muito menos `dlopen`ado) sem o SHA-256 bater com o
    // hash homologado desta release.
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(&out));
    if digest != expected_sha {
        return Err(tr!(
            "core integrity check failed — download again",
            "verificação de integridade do core falhou — baixe de novo"
        )
        .to_string());
    }

    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    install_core_bytes(dest_dir, &out)?;
    Ok(())
}

/// Write the core into `dest_dir` under its final name via a temporary file
/// and a rename. The startup nameplate `Core::load`s (`dlopen`s) the
/// installed dylib, and macOS kills the process (SIGKILL, code-signature
/// revalidation — see `examples/core_refresh_probe.rs`) when a mapped dylib
/// gets overwritten in place; a rename swaps the directory entry and leaves
/// whatever inode is mapped untouched.
fn install_core_bytes(dest_dir: &Path, bytes: &[u8]) -> Result<(), String> {
    let dest = dest_dir.join(core_file_name());
    let tmp = dest_dir.join(format!("{}.new", core_file_name()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_replaces_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join("sixteen-core-update-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        install_core_bytes(&dir, b"antiga").unwrap();
        assert_eq!(
            std::fs::read(dir.join(core_file_name())).unwrap(),
            b"antiga"
        );
        install_core_bytes(&dir, b"nova").unwrap();
        assert_eq!(std::fs::read(dir.join(core_file_name())).unwrap(), b"nova");
        assert!(!dir.join(format!("{}.new", core_file_name())).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_from_version_extracts_the_build_commit() {
        assert_eq!(
            commit_from_version("1.63 fae2fea").as_deref(),
            Some("fae2fea")
        );
        assert_eq!(
            commit_from_version("Snes9x 1.63 FAE2FEA").as_deref(),
            Some("fae2fea")
        );
        assert_eq!(
            commit_from_version("1.63 0123456789abcdef0123456789abcdef0123abcd").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123abcd")
        );
        assert_eq!(commit_from_version("1.63"), None);
        assert_eq!(commit_from_version("1.62.3"), None);
        assert_eq!(commit_from_version(""), None);
    }
}
