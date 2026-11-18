//! Browser bridge for the local SNES Classic core: picture, controller, save states, PCM and dialogue keys.
//! HD art and the original Rust Traditional Chinese renderer share a single final picture.
use ssd_oracle::{joypad_from_bits, AudioSink, Machine, MemorySaveStore};
use ssd_catalogue::engine::{on_screen, screen_names};
use ssd_catalogue::translation::{render, Slot, Translation};
use ssd_hd::{Font as HdFont, HdOptions, Look, Renderer as HdRenderer};
use ssd_hd::translate::{ChineseFace, Overlay as TextOverlay};
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;
use wasm_bindgen::prelude::*;
mod live_bg;
use live_bg::LiveBackgrounds;

fn tracked_renderer(font: Option<Arc<HdFont>>, overlay: Option<Arc<TextOverlay>>) -> HdRenderer {
    let mut renderer = HdRenderer::new();
    renderer.set_layer_tracking(true);
    renderer.set_font(font);
    renderer.set_text_capture_only(true);
    renderer.set_native_translation(overlay);
    renderer
}

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
    rom: Vec<u8>,
    translation: Option<Translation>,
    native_overlay: Option<Arc<TextOverlay>>,
    native_page_hint: Option<usize>,
    native_painted: bool,
    text_font: Option<Arc<HdFont>>,
    menu_names: HashMap<String, String>,
    previous_page: Option<usize>,
    hd_enabled: bool,
    hd_renderer: HdRenderer,
    hd_pixels: Vec<u8>,
    hd_width: u32,
    hd_height: u32,
    hd_warning: String,
    live_backgrounds: LiveBackgrounds,
    hd_art_pixels: u32,
}

#[wasm_bindgen]
impl WebGame {
    /// Input is the user's local Japanese Rev 1 or Klepto 1.02 ROM. ROM/IPS never leave the device.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8], ips: &[u8]) -> Result<WebGame, JsValue> {
        let patch = (!ips.is_empty()).then_some(ips);
        let normalised = ssd_rom::normalize(rom, patch)
            .map_err(|err| JsValue::from_str(&format!("ROM 驗證失敗：{err}")))?;
        let rom = normalised.bytes;
        let mut machine = Machine::new(rom.clone(), Box::new(MemorySaveStore::default()))
            .map_err(|err| JsValue::from_str(&format!("遊戲初始化失敗：{err}")))?;
        machine.set_audio_frequency(48_000);
        let text_font = HdFont::standard(&rom).ok().map(Arc::new);
        let hd_renderer = tracked_renderer(text_font.clone(), None);
        Ok(Self { machine, audio: AudioFrame::default(), rom, translation: None,
            native_overlay: None, native_page_hint: None, native_painted: false,
            text_font, menu_names: HashMap::new(), previous_page: None,
            hd_enabled: false, hd_renderer, hd_pixels: Vec::new(),
            hd_width: 0, hd_height: 0, hd_warning: String::new(),
            live_backgrounds: LiveBackgrounds::default(), hd_art_pixels: 0 })
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
        // The ROM's WRAM describes the page at the START of the rendered frame.
        // Resolving the page after run_frame causes one-frame English flashes.
        if self.hd_enabled && self.native_overlay.is_some() {
            let screen = on_screen(&self.rom, self.machine.wram(), self.native_page_hint);
            if let Some(ref page) = screen {
                self.native_page_hint = Some(page.page);
            }
            self.hd_renderer.set_translation_screen(screen);
        }
        self.machine.run_frame(joypad_from_bits(joypad_bits), &mut self.audio)
            .map_err(|err| JsValue::from_str(&format!("遊戲執行失敗：{err}")))?;
        self.native_painted = false;
        if self.hd_enabled {
            if let Some(log) = self.machine.frame_log() {
                match self.hd_renderer.render_hd(log, HdOptions { scale: 2, look: Look::Crisp }) {
                    Ok(picture) => {
                        self.hd_width = picture.width;
                        self.hd_height = picture.height;
                        self.hd_pixels.clear();
                        self.hd_pixels.extend_from_slice(&picture.rgba);
                        self.live_backgrounds.refresh(log);
                        self.hd_art_pixels = self.live_backgrounds.composite(
                            log, self.hd_renderer.layer_ids(), &mut self.hd_pixels,
                            self.hd_width as usize, self.hd_height as usize,
                        );
                        // The original native renderer owns every Chinese glyph.
                        // It executes AFTER HD backgrounds but before JS displays
                        // the frame, so English and Chinese cannot coexist.
                        self.native_painted = self.hd_renderer.paint_native_translation(
                            log, &mut self.hd_pixels,
                            self.hd_width as usize, self.hd_height as usize,
                        );
                        self.hd_warning.clear();
                    }
                    Err(error) => {
                        // Keep the game playable even when this frame cannot render in HD.
                        self.hd_pixels.clear();
                        self.hd_art_pixels = 0;
                        self.hd_warning = error.to_string();
                    }
                }
            } else {
                self.hd_pixels.clear();
                self.hd_art_pixels = 0;
                self.hd_warning = "等待完整 PPU 畫面紀錄".to_owned();
            }
        }
        Ok(())
    }

    /// Interleaved stereo PCM, f32 in [-1, 1]: L0, R0, L1, R1...
    /// This drains the host-side samples but never changes the emulated sound chip's state.
    pub fn drain_audio(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio.samples)
    }

    /// Load the original RC1 zh-TW translation TSV in memory without altering the ROM.
    pub fn load_translation(&mut self, tsv: &str) -> Result<u32, JsValue> {
        let table = Translation::parse(tsv)
            .map_err(|err| JsValue::from_str(&format!("繁體中文 TSV 無法讀取：{err}")))?;
        let count = table.len() as u32;
        self.menu_names = screen_names(&self.rom, &table);
        // Bundle the author's OFL-licensed font in WASM; ROM, IPS, HD ZIP and
        // fonts are never uploaded from the player's device.
        const FONT_BYTES: &[u8] =
            include_bytes!("../../../translations/zh-tw/fonts/LXGWWenKaiTC-Regular.ttf");
        let face = ChineseFace::new(FONT_BYTES.to_vec())
            .map_err(|error| JsValue::from_str(&format!("中文字型無法載入：{error}")))?;
        self.native_overlay = Some(Arc::new(
            TextOverlay::new(Arc::new(table.clone()), Arc::new(face))
                .with_names(self.menu_names.clone()),
        ));
        self.translation = Some(table);
        self.native_page_hint = None;
        self.native_painted = false;
        self.hd_renderer.set_native_translation(self.native_overlay.clone());
        self.previous_page = None;
        Ok(count)
    }

    /// The original project's menu and name translations, indexed by the English words on screen.
    pub fn menu_translations(&self) -> String {
        serde_json::to_string(&self.menu_names).unwrap_or_else(|_| "{}".to_owned())
    }

    /// The glyph positions from the actual PPU frame (without any ROM modification).
    pub fn visible_text_cells(&self) -> String {
        serde_json::to_string(&self.hd_renderer.observed_text_cells())
            .unwrap_or_else(|_| "[]".to_owned())
    }

    /// A direct lookup for an already-translated, token-free alert.
    pub fn translation_for_key(&self, key: &str) -> String {
        self.translation.as_ref().and_then(|table| table.text(key))
            .map(|entry| render(entry, |_| String::new()))
            .unwrap_or_default()
    }

    /// True only when the original Rust translator painted the *current*
    /// Enhanced picture; the browser must not add heuristic text overlays.
    pub fn native_translation_active(&self) -> bool {
        self.hd_enabled && self.native_painted && self.hd_active()
    }

    /// Returns the original translator's keyed dialogue as JSON for testing.
    /// This does not redraw the SNES picture: the companion browser UI displays
    /// a non-destructive Chinese subtitle until the HD overlay is ported.
    pub fn current_dialogue(&mut self) -> String {
        let Some(table) = self.translation.as_ref() else { return String::new() };
        let Some(page) = on_screen(&self.rom, self.machine.wram(), self.previous_page) else {
            return String::new();
        };
        self.previous_page = Some(page.page);
        let Some(text) = table.text(&page.key) else { return String::new() };
        let plain = |entry: &str| render(entry, |_| String::new()).replace('\n', "");
        let translated = render(text, |slot| match slot {
            Slot::Name(n) => page.names.get(usize::from(n).wrapping_sub(1))
                .and_then(Option::as_ref)
                .map(|name| table.text(&name.key).map_or_else(|| name.english.clone(), plain))
                .unwrap_or_default(),
            Slot::Number(n) => page.numbers.get(usize::from(n).wrapping_sub(1))
                .cloned().unwrap_or_default(),
            Slot::Phrase(n) => table.text(&format!("phrase:{n}"))
                .map(plain).unwrap_or_default(),
        });
        let (column, row) = page.base_cell();
        json!({
            "key": page.key,
            "text": translated,
            "progress": page.progress,
            "column": column,
            "row": row,
            "page": page.page,
            "state": page.state,
            "stopped": page.stopped,
            "portrait": page.portrait
        }).to_string()
    }

    /// Receive only the verified player's on-device background index (no media upload).
    pub fn set_background_index(&mut self, json: &str) -> Result<u32, JsValue> {
        let count = self.live_backgrounds.load_catalog(json)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(count as u32)
    }

    /// Matches use the live PPU registers and VRAM content hash.
    pub fn background_matches(&self) -> String {
        self.live_backgrounds.matches_json()
    }

    /// Accept a single locally decoded PNG/WebP of the matched battle background.
    pub fn set_background_art(&mut self, index: usize, width: u32, height: u32,
        rgba: &[u8]) -> Result<(), JsValue>
    {
        self.live_backgrounds.add_picture(index, width as usize, height as usize, rgba)
            .map_err(|e| JsValue::from_str(&e))
    }

    pub fn hd_art_pixels(&self) -> u32 { self.hd_art_pixels }

    /// Use upstream Enhanced 2x renderer; backgrounds are supplied by the player.
    /// Unsupported frames fall back to Classic without affecting emulator state.
    pub fn set_hd_mode(&mut self, enabled: bool) {
        if self.hd_enabled == enabled { return; }
        self.hd_enabled = enabled;
        self.machine.set_line_log(enabled);
        self.hd_renderer = tracked_renderer(self.text_font.clone(), self.native_overlay.clone());
        self.native_painted = false;
        self.native_page_hint = None;
        self.hd_art_pixels = 0;
        self.hd_pixels.clear();
        self.hd_width = 0;
        self.hd_height = 0;
        self.hd_warning.clear();
    }

    pub fn hd_mode(&self) -> bool { self.hd_enabled }

    pub fn hd_active(&self) -> bool { self.hd_enabled && !self.hd_pixels.is_empty() }

    pub fn hd_warning(&self) -> String { self.hd_warning.clone() }

    pub fn width(&self) -> u32 {
        if self.hd_active() { return self.hd_width; }
        self.machine.frame().size.width
    }

    pub fn height(&self) -> u32 {
        if self.hd_active() { return self.hd_height; }
        self.machine.frame().size.height
    }

    /// RGBA8 pixels, copied across the wasm-to-JS boundary.
    pub fn frame_rgba(&self) -> Vec<u8> {
        if self.hd_active() { return self.hd_pixels.clone(); }
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
        self.previous_page = None;
        self.hd_pixels.clear();
        self.hd_renderer = tracked_renderer(self.text_font.clone(), self.native_overlay.clone());
        self.native_painted = false;
        self.native_page_hint = None;
        self.hd_art_pixels = 0;
        self.machine.load_state(state)
            .map_err(|err| JsValue::from_str(&format!("讀取存檔失敗：{err}")))
    }
}