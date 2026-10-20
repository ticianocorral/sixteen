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
with cheats and screenshot slots, RetroAchievements, an NTSC RF filter,
per-game play-time tracking, and an offline database built into the binary
(4,268 SNES titles: canonical name, year, publisher, ratings,
HowLongToBeat times).

Homepage: https://github.com/ticianocorral/sixteen

**Build**

- Built from source from the upstream git tag; fully offline. All cargo
  dependencies — including the SDL3/SDL_image/SDL_mixer C sources embedded
  in the `sdl3-src`, `sdl3-image-src` and `sdl3-mixer-src` crates, built
  via cmake by the `vendored-sdl` feature — are vendored through
  `cargo-sources.json`, generated from the committed `Cargo.lock`
  (226 crates). The toolchain comes from
  `org.freedesktop.Sdk.Extension.rust-stable`.
- The snes9x libretro core is **not** distributed with the app: it is
  downloaded at runtime from the official libretro buildbot on a
  user-triggered action, like any libretro frontend's core downloader.
- The metainfo, desktop file and icon are installed by the build from the
  upstream source tree (not copied into this repo).

**Requested permissions**

- `--share=network` — downloading the snes9x libretro core (user-triggered,
  first run) and RetroAchievements data (opt-in).
- `--filesystem=home` — reading ROM files from any folder in the user's
  home; the app is a portable-style emulator and game libraries live
  anywhere in `~`. The app's own data (saves, notes, config) stays inside
  the sandbox (`XDG_DATA_HOME`).
- `--device=all` — gamepads/joysticks via SDL/hidapi.

**Extra notes**

- License: MIT (project), CC0-1.0 (metainfo). OARS 1.1: nothing
  objectionable.
- The user interface is currently Portuguese (Brazil) only; this is noted
  in the metainfo description.
- The desktop file and metainfo include English and pt-BR localisations.
