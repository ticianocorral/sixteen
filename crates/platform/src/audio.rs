//! Push-model audio: the frontend hands over the S16 stereo the core produced
//! each frame and SDL's stream does the resampling to the device rate.

use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::AudioSubsystem;

use crate::PlatformError;

pub struct AudioOut {
    /// `None` = saída muda ([`AudioOut::muted`]): sem dispositivo de áudio o
    /// jogo ainda roda, só sem som.
    stream: Option<AudioStreamOwner>,
    /// Source rate the core declared; kept for diagnostics.
    pub source_rate: u32,
}

impl AudioOut {
    pub(crate) fn new(audio: &AudioSubsystem, sample_rate: u32) -> Result<Self, PlatformError> {
        let device = audio.default_playback_device();
        let spec = AudioSpec::new(Some(sample_rate as i32), Some(2), Some(AudioFormat::S16LE));
        let stream = device
            .open_device_stream(Some(&spec))
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        stream
            .resume()
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        Ok(Self {
            stream: Some(stream),
            source_rate: sample_rate,
        })
    }

    /// Uma saída que engole tudo — o caminho de quando o dispositivo de
    /// áudio não abre (PipeWire caído, saída HDMI/Bluetooth que sumiu).
    pub fn muted(sample_rate: u32) -> Self {
        Self {
            stream: None,
            source_rate: sample_rate,
        }
    }

    /// Interleaved L/R. Non-blocking; SDL buffers and resamples.
    pub fn queue(&self, interleaved_stereo: &[i16]) {
        if interleaved_stereo.is_empty() {
            return;
        }
        if let Some(s) = &self.stream {
            let _ = s.put_data_i16(interleaved_stereo);
        }
    }

    /// Stereo sample-frames still waiting to play. Used to pace the loop so the
    /// buffer neither starves nor grows without bound.
    pub fn queued_frames(&self) -> usize {
        self.stream
            .as_ref()
            .and_then(|s| s.queued_bytes().ok())
            .map(|b| (b.max(0) as usize) / 4)
            .unwrap_or(0)
    }

    pub fn clear(&self) {
        if let Some(s) = &self.stream {
            let _ = s.clear();
        }
    }
}

/// O microfone (mono, S16): a cena do sopro lê o que chegou a cada frame
/// para sentir o sopro de verdade (Steam Deck, ROG Ally, notebook). Aberto
/// só enquanto a cena dura — fechar é soltar o valor.
pub struct AudioIn {
    stream: AudioStreamOwner,
    pub source_rate: u32,
    buf: Vec<i16>,
}

impl AudioIn {
    pub(crate) fn new(audio: &AudioSubsystem, sample_rate: u32) -> Result<Self, PlatformError> {
        let device = audio.default_recording_device();
        let spec = AudioSpec::new(Some(sample_rate as i32), Some(1), Some(AudioFormat::S16LE));
        let stream = device
            .open_device_stream(Some(&spec))
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        stream
            .resume()
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        Ok(Self {
            stream,
            source_rate: sample_rate,
            buf: Vec::new(),
        })
    }

    /// Tudo o que o microfone gravou desde a última leitura (mono).
    pub fn read(&mut self) -> &[i16] {
        let avail = self
            .stream
            .available_bytes()
            .map(|b| b.max(0) as usize / 2)
            .unwrap_or(0);
        self.buf.resize(avail, 0);
        let n = if avail == 0 {
            0
        } else {
            self.stream.read_i16_samples(&mut self.buf).unwrap_or(0)
        };
        self.buf.truncate(n);
        &self.buf
    }
}
