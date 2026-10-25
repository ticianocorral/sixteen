# Rascunho do PR (corpo, em inglês) — SixteeN

Título do PR: `Add io.github.ticianocorral.sixteen`
Base: `new-pr` (nunca `master`)

---

**App description**

SixteeN is a free, personal, non-commercial SNES emulator that presents
itself as a real console: instead of a bare game window, you see the
cabinet, the CRT tube with RF static, the game's side panel (logo label,
cartridge in the slot, Power/Reset switches) and a game shelf with 3D
boxes. Games are inserted as cartridges with animation, and the console
boots only when you flip Power. Includes save states, a pause notebook
with cheats and screenshot slots, a PDF reader for manuals and magazines
(the "Library" screen), RetroAchievements, an NTSC RF filter, per-game
play-time tracking, and an offline database built into the binary
(4,268 SNES titles: canonical name, year, publisher, ratings,
HowLongToBeat times).

Homepage: https://github.com/ticianocorral/sixteen

**Build**

- Built from source from the upstream git tag; fully offline. All cargo
  dependencies — including the SDL3/SDL_image/SDL_mixer C sources embedded
  in the `sdl3-src`, `sdl3-image-src` and `sdl3-mixer-src` crates, built
  via cmake by the `vendored-sdl` feature — are vendored through
  `cargo-sources.json`, generated from the committed `Cargo.lock`
  (238 crates). The toolchain comes from
  `org.freedesktop.Sdk.Extension.rust-stable`.
- The snes9x libretro core is **not** distributed with the app: it is
  downloaded at runtime, on a user-triggered action, like any libretro
  frontend's core downloader. The download comes from the project's own
  GitHub release and is accepted only after its SHA-256 matches the hash
  pinned inside the app binary (see `crates/app/src/core_update.rs`) —
  the runtime update channel is the same authenticated channel that
  ships the app itself. snes9x stays non-commercial, which is also why
  the core is never bundled with the package.
- The metainfo, desktop file and icons (256/512/1024) are installed by
  the build from the upstream source tree (not copied into this repo).
- The app's own self-updater is disabled inside the Flatpak sandbox
  (`FLATPAK_ID` detection): updates come from Flathub.

**Requested permissions**

- `--share=network` — RetroAchievements (opt-in login, achievements and
  badge art) and the user-triggered snes9x core download described above
  (first run, from the project's GitHub release, SHA-256 verified).
- `--device=dri` — GPU rendering (SDL3 renderer).
- `--device=all` — gamepads/joysticks via SDL's evdev/hidraw backend
  (flatpak has no `udev` device type; `all` is what grants access to the
  device nodes — the same pattern RetroArch uses on Flathub).
- No filesystem permissions beyond the sandbox's own `XDG_DATA_HOME`:
  the app reads and writes exclusively under
  `~/.var/app/io.github.ticianocorral.sixteen/data/SixteeN`
  (`roms/`, `saves/`, `core/`, `assets/`…). ROMs are copied into `roms/`
  from the file manager, like cart dumping into a real console.

**Extra notes**

- License: MIT (project), CC0-1.0 (metainfo). OARS 1.1: nothing
  objectionable. Third-party notices (snes9x non-commercial, SDL3 zlib,
  snes_ntsc LGPL with sources shipped, etc.) are installed under
  `/app/share/licenses/`.
- The user interface is currently Portuguese (Brazil) only; this is noted
  in the metainfo description.
- The desktop file and metainfo include English and pt-BR localisations.
