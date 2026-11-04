//! Safe wrapper around the vendored blargg `snes_ntsc` 0.2.2 filter.
//!
//! Input and output are both 16-bit RGB565 (the library's compiled-in default,
//! see `vendor/snes_ntsc/snes_ntsc_config.h`). One low-res 256-wide field comes
//! out 602 wide.

use std::ffi::{c_int, c_long, c_void};
use std::os::raw::c_float;

const IN_CHUNK: u32 = 3;
const OUT_CHUNK: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy)]
struct SnesNtscSetup {
    hue: f64,
    saturation: f64,
    contrast: f64,
    brightness: f64,
    sharpness: f64,
    gamma: f64,
    resolution: f64,
    artifacts: f64,
    fringing: f64,
    bleed: f64,
    merge_fields: c_int,
    decoder_matrix: *const c_float,
    bsnes_colortbl: *const c_void,
}

impl SnesNtscSetup {
    const fn base() -> Self {
        SnesNtscSetup {
            hue: 0.0,
            saturation: 0.0,
            contrast: 0.0,
            brightness: 0.0,
            sharpness: 0.0,
            gamma: 0.0,
            resolution: 0.0,
            artifacts: 0.0,
            fringing: 0.0,
            bleed: 0.0,
            merge_fields: 1,
            decoder_matrix: std::ptr::null(),
            bsnes_colortbl: std::ptr::null(),
        }
    }
}

extern "C" {
    fn snes_ntsc_init(ntsc: *mut c_void, setup: *const SnesNtscSetup);
    fn snes_ntsc_blit(
        ntsc: *const c_void,
        input: *const u16,
        in_row_width: c_long,
        burst_phase: c_int,
        in_width: c_int,
        in_height: c_int,
        rgb_out: *mut c_void,
        out_pitch: c_long,
    );
    fn sixteen_snes_ntsc_sizeof() -> usize;
}

/// Video-signal presets. `Rf` is our tuning (see the vendor README); the rest
/// mirror the upstream presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Composite,
    SVideo,
    Rgb,
    Monochrome,
    Rf,
    /// Pixel nítido: sem artefatos, franjas nem vazamento de cor, só a
    /// resolução máxima do filtro.
    Sharp,
}

impl Preset {
    fn setup(self) -> SnesNtscSetup {
        let mut s = SnesNtscSetup::base();
        match self {
            Preset::Composite => {}
            Preset::SVideo => {
                s.sharpness = 0.2;
                s.resolution = 0.2;
                s.artifacts = -1.0;
                s.fringing = -1.0;
            }
            Preset::Rgb => {
                s.sharpness = 0.2;
                s.resolution = 0.7;
                s.artifacts = -1.0;
                s.fringing = -1.0;
                s.bleed = -1.0;
            }
            Preset::Monochrome => {
                s.saturation = -1.0;
                s.sharpness = 0.2;
                s.resolution = 0.2;
                s.artifacts = -0.2;
                s.fringing = -0.2;
                s.bleed = -1.0;
            }
            Preset::Sharp => {
                s.sharpness = 0.4;
                s.resolution = 1.0;
                s.artifacts = -1.0;
                s.fringing = -1.0;
                s.bleed = -1.0;
            }
            // Composite, but softer and noisier: the antenna/RF look.
            Preset::Rf => {
                s.resolution = -0.2;
                s.artifacts = 0.35;
                s.fringing = 0.35;
                s.bleed = 0.30;
                s.merge_fields = 0; // let the dot pattern crawl
            }
        }
        s
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Composite => "composite",
            Preset::SVideo => "s-video",
            Preset::Rgb => "rgb",
            Preset::Monochrome => "monochrome",
            Preset::Rf => "rf",
            Preset::Sharp => "sharp",
        }
    }
}

/// Os ajustes de imagem da TV (o menu OSD), somados por cima do preset:
/// cada um de -1 a 1, 0 = como o preset deixa.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Picture {
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub sharpness: f64,
}

pub struct NtscFilter {
    /// The ~8 MiB kernel table. `u64`-backed for 8-byte alignment.
    table: Box<[u64]>,
    preset: Preset,
    picture: Picture,
    phase: c_int,
    animate: bool,
    in_buf: Vec<u16>,
    out: Vec<u16>,
}

impl NtscFilter {
    pub fn new(preset: Preset) -> Self {
        let bytes = unsafe { sixteen_snes_ntsc_sizeof() };
        let table = vec![0u64; bytes.div_ceil(8)].into_boxed_slice();
        let mut me = Self {
            table,
            preset,
            picture: Picture::default(),
            phase: 0,
            animate: preset == Preset::Rf,
            in_buf: Vec::new(),
            out: Vec::new(),
        };
        me.reinit();
        me
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    pub fn set_preset(&mut self, preset: Preset) {
        if preset != self.preset {
            self.preset = preset;
            self.animate = preset == Preset::Rf;
            self.reinit();
        }
    }

    pub fn picture(&self) -> Picture {
        self.picture
    }

    /// Troca os ajustes de imagem — refaz a tabela do filtro (alguns ms),
    /// então só quando o valor muda de fato.
    pub fn set_picture(&mut self, picture: Picture) {
        if picture != self.picture {
            self.picture = picture;
            self.reinit();
        }
    }

    fn reinit(&mut self) {
        let mut setup = self.preset.setup();
        let p = self.picture;
        let clamp = |v: f64| v.clamp(-1.0, 1.0);
        setup.brightness = clamp(setup.brightness + p.brightness);
        setup.contrast = clamp(setup.contrast + p.contrast);
        setup.saturation = clamp(setup.saturation + p.saturation);
        setup.sharpness = clamp(setup.sharpness + p.sharpness);
        unsafe { snes_ntsc_init(self.table.as_mut_ptr() as *mut c_void, &setup) };
    }

    /// Output width for a given low-res input width (e.g. 256 -> 602).
    pub fn output_width(in_width: u32) -> u32 {
        ((in_width.saturating_sub(1)) / IN_CHUNK + 1) * OUT_CHUNK
    }

    /// Filter one RGB565 frame. `src` is `src_h` rows of `src_w` pixels with a
    /// `src_pitch_bytes` byte stride. Returns the filtered RGB565 buffer, its
    /// width, and its height (height is unchanged).
    pub fn process(
        &mut self,
        src: &[u8],
        src_w: u32,
        src_h: u32,
        src_pitch_bytes: usize,
    ) -> (&[u16], u32, u32) {
        let (w, h) = (src_w as usize, src_h as usize);
        // Caminho direto: em little-endian, com o buffer alinhado a 2 bytes
        // e pitch par, o frame do core JÁ é um array de u16 — o blit lê dele
        // com o pitch real, sem remontar ~57k pixels em `in_buf` por frame.
        let direct = cfg!(target_endian = "little")
            && src.as_ptr().align_offset(2) == 0
            && src_pitch_bytes.is_multiple_of(2)
            && src_pitch_bytes >= w * 2
            && h > 0
            && src.len() >= (h - 1) * src_pitch_bytes + w * 2;
        let (in_ptr, in_row_width) = if direct {
            (src.as_ptr() as *const u16, src_pitch_bytes / 2)
        } else {
            self.in_buf.resize(w * h, 0);
            for row in 0..h {
                let off = row * src_pitch_bytes;
                for (i, dst) in self.in_buf[row * w..row * w + w].iter_mut().enumerate() {
                    let b = off + i * 2;
                    *dst = u16::from_le_bytes([src[b], src[b + 1]]);
                }
            }
            (self.in_buf.as_ptr(), w)
        };

        let out_w = Self::output_width(src_w) as usize;
        self.out.resize(out_w * h, 0);
        unsafe {
            snes_ntsc_blit(
                self.table.as_ptr() as *const c_void,
                in_ptr,
                in_row_width as c_long,
                self.phase,
                src_w as c_int,
                src_h as c_int,
                self.out.as_mut_ptr() as *mut c_void,
                (out_w * 2) as c_long,
            );
        }
        if self.animate {
            self.phase = (self.phase + 1) % 3;
        }
        (&self.out[..out_w * h], out_w as u32, src_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_and_copied_paths_match() {
        // Mesmo frame (com pitch maior que a linha) pelo caminho direto e
        // pelo de cópia (buffer desalinhado de propósito): saída idêntica.
        let (w, h, pitch) = (256usize, 8usize, 300usize * 2);
        let mut buf = vec![0u8; pitch * h + 1];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i * 31 % 251) as u8;
        }
        let mut a = NtscFilter::new(Preset::Rgb);
        let mut b = NtscFilter::new(Preset::Rgb);
        let aligned = if buf.as_ptr().align_offset(2) == 0 {
            0
        } else {
            1
        };
        let out_a = a
            .process(&buf[aligned..], w as u32, h as u32, pitch)
            .0
            .to_vec();
        let mut shifted = vec![0u8; pitch * h + 1];
        let off = 1 - aligned;
        shifted[off..off + pitch * h].copy_from_slice(&buf[aligned..aligned + pitch * h]);
        let out_b = b
            .process(&shifted[off..], w as u32, h as u32, pitch)
            .0
            .to_vec();
        assert_eq!(out_a, out_b);
    }

    #[test]
    fn brightness_really_brightens_and_is_fast() {
        // Cinza médio: o ajuste de brilho do menu da TV tem que mudar a
        // saída de verdade, e refazer a tabela não pode travar o quadro.
        let (w, h) = (256u32, 4u32);
        let gray = 0x8410u16.to_le_bytes();
        let buf: Vec<u8> = (0..w * h).flat_map(|_| gray).collect();
        let luma = |f: &mut NtscFilter| {
            let (out, ow, _) = f.process(&buf, w, h, w as usize * 2);
            let px = out[(ow * 2 + ow / 2) as usize];
            ((px >> 11) & 31) as u32 + ((px >> 5) & 63) as u32 / 2 + (px & 31) as u32
        };
        let mut f = NtscFilter::new(Preset::Rgb);
        let base = luma(&mut f);
        let t = std::time::Instant::now();
        f.set_picture(Picture {
            brightness: 0.4,
            ..Picture::default()
        });
        let took = t.elapsed();
        assert!(luma(&mut f) > base + 4, "brilho não clareou");
        assert!(took.as_millis() < 250, "reinit levou {took:?}");
    }

    #[test]
    fn output_width_matches_upstream() {
        assert_eq!(NtscFilter::output_width(256), 602);
    }

    #[test]
    fn filters_a_flat_frame_to_602() {
        let mut f = NtscFilter::new(Preset::Rf);
        let src = vec![0u8; 256 * 224 * 2];
        let (out, w, h) = f.process(&src, 256, 224, 256 * 2);
        assert_eq!((w, h), (602, 224));
        assert_eq!(out.len(), 602 * 224);
    }
}
