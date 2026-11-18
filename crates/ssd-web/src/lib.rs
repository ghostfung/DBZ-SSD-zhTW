//! First playable browser bridge: SNES Classic frames, no SDL window, no audio yet.
//! The HD renderer and zh-TW translation engine are intentionally not claimed here.
use ssd_oracle::{joypad_from_bits, Machine, MemorySaveStore, NullAudio};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WebGame {
    machine: Machine,
}

#[wasm_bindgen]
impl WebGame {
    /// Input is the user's local, legally obtained Japanese Rev 1 or Klepto 1.02 ROM.
    /// For Japanese Rev 1, supply the Klepto v1.02 .ips bytes as the second argument.
    /// Never transmits or embeds either file.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8], ips: &[u8]) -> Result<WebGame, JsValue> {
        let patch = (!ips.is_empty()).then_some(ips);
        let normalised = ssd_rom::normalize(rom, patch)
            .map_err(|err| JsValue::from_str(&format!("ROM 驗證失敗：{err}")))?;
        let machine = Machine::new(normalised.bytes, Box::new(MemorySaveStore::default()))
            .map_err(|err| JsValue::from_str(&format!("遊戲初始化失敗：{err}")))?;
        Ok(Self { machine })
    }

    /// The bit layout is given by `ssd_oracle::input::JOYPAD_BUTTONS`.
    pub fn tick(&mut self, joypad_bits: u16) -> Result<(), JsValue> {
        self.machine.run_frame(joypad_from_bits(joypad_bits), &mut NullAudio)
            .map_err(|err| JsValue::from_str(&format!("遊戲執行失敗：{err}")))
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

    /// Save states are ROM/core-version validated when loaded.
    pub fn save_state(&self) -> Vec<u8> {
        self.machine.save_state()
    }

    pub fn load_state(&mut self, state: &[u8]) -> Result<(), JsValue> {
        self.machine.load_state(state)
            .map_err(|err| JsValue::from_str(&format!("讀取存檔失敗：{err}")))
    }
}
