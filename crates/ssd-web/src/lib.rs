//! Browser bridge for the local SNES Classic core: picture, controller, save states, and PCM sound.
//! The HD renderer and zh-TW translation engine are separate future milestones.
use ssd_oracle::{joypad_from_bits, AudioSink, Machine, MemorySaveStore};
use wasm_bindgen::prelude::*;

/// Collect stereo floating-point samples in the precise order produced by the SNES core.
#[derive(Default)]
struct AudioFrame {
    samples: Vec<f32>,
}

impl AudioSink for AudioFrame {
    fn push(&mut self, left: f32, right: f32) {
        self.samples.push(left);
        self.samples.push(right);
    }
}

#[wasm_bindgen]
pub struct WebGame {
    machine: Machine,
    audio: AudioFrame,
}

#[wasm_bindgen]
impl WebGame {
    /// Input is the user's local Japanese Rev 1 or Klepto 1.02 ROM. ROM/IPS never leave the device.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8], ips: &[u8]) -> Result<WebGame, JsValue> {
        let patch = (!ips.is_empty()).then_some(ips);
        let normalised = ssd_rom::normalize(rom, patch)
            .map_err(|err| JsValue::from_str(&format!("ROM 驗證失敗：{err}")))?;
        let mut machine = Machine::new(normalised.bytes, Box::new(MemorySaveStore::default()))
            .map_err(|err| JsValue::from_str(&format!("遊戲初始化失敗：{err}")))?;
        machine.set_audio_frequency(48_000);
        Ok(Self { machine, audio: AudioFrame::default() })
    }

    /// AudioContext.sampleRate controls the emulator's PCM output rate.
    pub fn set_audio_rate(&mut self, hz: u32) -> Result<(), JsValue> {
        if !(8_000..=192_000).contains(&hz) {
            return Err(JsValue::from_str("音效取樣率不在支援範圍"));
        }
        self.machine.set_audio_frequency(u64::from(hz));
        self.audio.samples.clear();
        Ok(())
    }

    /// The bit layout is given by ssd_oracle::input::JOYPAD_BUTTONS.
    pub fn tick(&mut self, joypad_bits: u16) -> Result<(), JsValue> {
        self.machine.run_frame(joypad_from_bits(joypad_bits), &mut self.audio)
            .map_err(|err| JsValue::from_str(&format!("遊戲執行失敗：{err}")))
    }

    /// Interleaved stereo PCM, f32 in [-1, 1]: L0, R0, L1, R1...
    /// This drains the host-side samples but never changes the emulated sound chip's state.
    pub fn drain_audio(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio.samples)
    }

    pub fn width(&self) -> u32 {
        self.machine.frame().size.width
    }

    pub fn height(&self) -> u32 {
        self.machine.frame().size.height
    }

    /// RGBA8 pixels, copied across the wasm-to-JS boundary.
    pub fn frame_rgba(&self) -> Vec<u8> {
        bytemuck::cast_slice(self.machine.frame().pixels).to_vec()
    }

    pub fn frame_count(&self) -> u32 {
        self.machine.frame_count() as u32
    }

    pub fn save_state(&self) -> Vec<u8> {
        self.machine.save_state()
    }

    pub fn load_state(&mut self, state: &[u8]) -> Result<(), JsValue> {
        self.audio.samples.clear();
        self.machine.load_state(state)
            .map_err(|err| JsValue::from_str(&format!("讀取存檔失敗：{err}")))
    }
}