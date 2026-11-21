//! Browser bridge for the local SNES Classic core: picture, controller, save states, PCM and dialogue keys.
//! HD art and the original Rust Traditional Chinese renderer share a single final picture.
use ssd_oracle::{joypad_from_bits, AudioSink, Machine, MemorySaveStore};
use ssd_catalogue::engine::{on_screen, screen_names, story_pages_near, OnScreen};
use ssd_catalogue::translation::{render, Slot, Translation};
use ssd_hd::{Font as HdFont, HdOptions, Look, Renderer as HdRenderer};
use ssd_hd::translate::{ChineseFace, Overlay as TextOverlay};
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;
use wasm_bindgen::prelude::*;
mod live_bg;
mod live_fighters;
mod live_specials;
mod live_maps;
mod live_cards;
mod live_mode7;
mod live_portraits;
mod hd_profile;
mod hd_luma;
mod cheat_bridge;
use ssd_menus::cheats::Cheats;
use hd_profile::Profile;
use live_portraits::LivePortraits;
use live_bg::LiveBackgrounds;
use live_fighters::LiveFighters;
use live_specials::LiveSpecials;
use live_maps::LiveMaps;
use live_cards::LiveCards;
use live_mode7::LiveMode7;

/// Keep every page ever observed at a frame boundary briefly: the message
/// engine sometimes points to the next string while the last page remains
/// visible. Only the PPU glyph matcher may restore a prior visible page.
fn remember_page(history: &mut Vec<(u32, OnScreen)>, page: &OnScreen, frame: u32) {
    history.retain(|(seen, old)| frame.saturating_sub(*seen) <= 150
        && !(old.table == page.table && old.string == page.string && old.page == page.page));
    history.push((frame, page.clone()));
    if history.len() > 12 { history.remove(0); }
}

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

/// Enlarges only the *original* 1x SNES RGBA pixels. This is not the HD
/// PPU renderer: each source pixel becomes one exact 2x2 block so the pixel
/// art stays untouched, while Chinese glyphs can be rasterised at 2x.
fn nearest_rgba_2x(source: &[u8], width: usize, height: usize, out: &mut Vec<u8>) {
    let source_stride = width * 4;
    if source.len() != source_stride * height {
        out.clear();
        return;
    }
    let out_stride = source_stride * 2;
    out.resize(out_stride * height * 2, 0);
    for (y, row) in source.chunks_exact(source_stride).enumerate() {
        let offset = 2 * y * out_stride;
        for (x, rgba) in row.chunks_exact(4).enumerate() {
            let at = offset + 8 * x;
            out[at..at + 4].copy_from_slice(rgba);
            out[at + 4..at + 8].copy_from_slice(rgba);
        }
        out.copy_within(offset..offset + out_stride, offset + out_stride);
    }
}

#[cfg(test)]
mod classic_zh_pixel_tests {
    use super::nearest_rgba_2x;

    #[test]
    fn upscaled_original_pixels_keep_exact_rgba_and_two_dimensional_edges() {
        let source = [1, 2, 3, 255, 5, 6, 7, 250, 9, 10, 11, 240, 13, 14, 15, 230];
        let mut output = Vec::new();
        nearest_rgba_2x(&source, 2, 2, &mut output);
        assert_eq!(output.len(), 4 * 4 * 4);
        for y in 0..4 {
            for x in 0..4 {
                let from = (y / 2 * 2 + x / 2) * 4;
                let at = (y * 4 + x) * 4;
                assert_eq!(&output[at..at + 4], &source[from..from + 4]);
            }
        }
        nearest_rgba_2x(&[], 2, 2, &mut output);
        assert!(output.is_empty());
    }
}

#[wasm_bindgen]
pub struct WebGame {
    machine: Machine,
    audio: AudioFrame,
    cheats: Cheats,
    rom: Vec<u8>,
    translation: Option<Translation>,
    native_overlay: Option<Arc<TextOverlay>>,
    native_page_hint: Option<usize>,
    native_painted: bool,
    native_page_key: Option<String>,
    native_late_page: bool,
    recent_pages: Vec<(u32, OnScreen)>,
    native_visible_fallback: bool,
    native_visible_score: usize,
    native_untranslated_visible: Option<String>,
    story_candidates: Vec<OnScreen>,
    story_candidates_for: Option<(u8, u16, u16)>,
    native_expected_english: String,
    native_story_text_pending: bool,
    native_story_ascii_cells: usize,
    text_font: Option<Arc<HdFont>>,
    menu_names: HashMap<String, String>,
    previous_page: Option<usize>,
    hd_enabled: bool,
    classic_zh_enabled: bool,
    classic_zh_pixels: Vec<u8>,
    hd_renderer: HdRenderer,
    hd_pixels: Vec<u8>,
    hd_width: u32,
    hd_height: u32,
    hd_warning: String,
    live_backgrounds: LiveBackgrounds,
    hd_art_pixels: u32,
    live_fighters: LiveFighters,
    hd_fighter_pixels: u32,
    hd_fighter_enabled: bool,
    live_specials: LiveSpecials,
    hd_special_pixels: u32,
    hd_special_enabled: bool,
    live_maps: LiveMaps,
    hd_map_pixels: u32,
    hd_map_enabled: bool,
    live_cards:LiveCards,
    hd_card_enabled:bool,
    hd_card_pixels:u32,
    live_mode7:LiveMode7,
    hd_mode7_enabled:bool,
    hd_mode7_pixels:u32,
    live_portraits: LivePortraits,
    hd_portrait_pixels: u32,
    hd_portrait_enabled: bool,
    hd_profile: Profile,
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
        Ok(Self { machine, audio: AudioFrame::default(), cheats: Cheats::neutral(), rom, translation: None,
            native_overlay: None, native_page_hint: None, native_painted: false,
            native_page_key: None, native_late_page: false,
            recent_pages: Vec::new(), native_visible_fallback: false,
            native_visible_score: 0, native_untranslated_visible: None,
            story_candidates: Vec::new(), story_candidates_for: None,
            native_expected_english: String::new(),
            native_story_text_pending: false, native_story_ascii_cells: 0,
            text_font, menu_names: HashMap::new(), previous_page: None,
            hd_enabled: false, classic_zh_enabled: false,
            classic_zh_pixels: Vec::new(), hd_renderer, hd_pixels: Vec::new(),
            hd_width: 0, hd_height: 0, hd_warning: String::new(),
            live_backgrounds: LiveBackgrounds::default(), hd_art_pixels: 0,
            live_fighters: LiveFighters::default(), hd_fighter_pixels: 0,
            hd_fighter_enabled: true,
            live_specials: LiveSpecials::default(),
            hd_special_pixels: 0, hd_special_enabled: true,
            live_maps: LiveMaps::default(), hd_map_pixels:0, hd_map_enabled:true,
            live_cards:LiveCards::default(),hd_card_enabled:true,hd_card_pixels:0,
            live_mode7:LiveMode7::default(),hd_mode7_enabled:true,hd_mode7_pixels:0,
            live_portraits: LivePortraits::default(),
            hd_portrait_pixels: 0, hd_portrait_enabled: true,
            hd_profile: Profile::default() })
    }

    /// Detect stale Safari JS/WASM mixes; contains no user or ROM information.
    pub fn browser_build_tag(&self) -> String { "dbz-hd-tw-pwa-v51".into() }

    /// AudioContext.sampleRate controls the emulator's PCM output rate.
    pub fn set_audio_rate(&mut self, hz: u32) -> Result<(), JsValue> {
        if !(8_000..=192_000).contains(&hz) {
            return Err(JsValue::from_str("音效取樣率不在支援範圍"));
        }
        self.machine.set_audio_frequency(u64::from(hz));
        self.audio.samples.clear();
        Ok(())
    }

    /// Query the first-phase gameplay cheats. Never exposes ROM, RAM or save data.
    pub fn cheat_state(&self) -> String { cheat_bridge::status(self.cheats) }

    /// Apply only the upstream's whitelisted gameplay settings. Changes are
    /// limited to the running session and are not embedded in exported states.
    pub fn set_cheat_option(&mut self, key: &str, value: &str) -> Result<String, JsValue> {
        let next = cheat_bridge::choose(self.cheats, key, value)
            .map_err(|err| JsValue::from_str(&err))?;
        cheat_bridge::configure(&mut self.machine, self.cheats, next)
            .map_err(|err| JsValue::from_str(&err))?;
        self.cheats = next;
        Ok(cheat_bridge::status(self.cheats))
    }

    /// The bit layout is given by ssd_oracle::input::JOYPAD_BUTTONS.
    pub fn tick(&mut self, joypad_bits: u16) -> Result<(), JsValue> {
        // The previous-frame WRAM is the primary source of the page that
        // started this PPU frame (WideRun semantics). Keep short-lived page
        // candidates so we can confirm what actually remained visible later.
        // NMI prepares the PPU frame from the field state at its start.
        // Do not read the next map/camera after run_frame(), which can already
        // be on another scene during transitions.
        let field_state=if self.hd_enabled && self.hd_map_enabled && self.live_maps.has_entries() {
            ssd_hd::world::FieldState::from_wram(self.machine.wram())
        } else {None};
        let mut started = None;
        if (self.hd_enabled || self.classic_zh_enabled) && self.native_overlay.is_some() {
            started = on_screen(&self.rom, self.machine.wram(), self.native_page_hint);
            if let Some(ref page) = started {
                self.native_page_hint = Some(page.page);
                remember_page(&mut self.recent_pages, page,
                    self.machine.frame_count() as u32);
            }
        }
        // Sample one frame in eight: timings are local-only, never sent.
        let profile_on=self.hd_enabled && self.machine.frame_count()%8==0;
        let mut profile_clock=if profile_on{hd_profile::now()}else{0.0};
        cheat_bridge::before_frame(&mut self.machine, self.cheats)
            .map_err(|err| JsValue::from_str(&format!("作弊設定執行失敗：{err}")))?;
        self.machine.run_frame(joypad_from_bits(joypad_bits), &mut self.audio)
            .map_err(|err| JsValue::from_str(&format!("遊戲執行失敗：{err}")))?;
        self.hd_profile.mark(hd_profile::EMU,profile_on,&mut profile_clock);
        self.native_late_page = false;
        self.native_visible_fallback = false;
        self.native_visible_score = 0;
        self.native_untranslated_visible = None;
        let mut selected = started;
        if (self.hd_enabled || self.classic_zh_enabled) && self.native_overlay.is_some() && selected.is_none() {
            // A page that began during this emulated frame is a candidate,
            // never proof that it is already visible.
            let ended = on_screen(&self.rom, self.machine.wram(), self.native_page_hint);
            if let Some(ref page) = ended {
                remember_page(&mut self.recent_pages, page,
                    self.machine.frame_count() as u32);
                self.native_page_hint = Some(page.page);
            }
            selected = ended.filter(|page| self.translation.as_ref()
                .is_some_and(|table| table.text(&page.key).is_some()));
            self.native_late_page = selected.is_some();
        }
        self.native_page_key = selected.as_ref().map(|page| page.key.clone());
        self.native_painted = false;
        self.classic_zh_pixels.clear();
        self.native_story_text_pending = false;
        self.native_story_ascii_cells = 0;
        self.hd_fighter_pixels = 0;
        self.hd_special_pixels = 0;
        self.hd_map_pixels = 0;
        self.hd_card_pixels = 0;
        self.hd_mode7_pixels = 0;
        self.hd_portrait_pixels = 0;
        if self.hd_enabled {
            if let Some(log) = self.machine.frame_log() {
                let rendered=self.hd_renderer.render_hd(log, HdOptions {
                    scale: 2, look: Look::Crisp
                });
                self.hd_profile.mark(hd_profile::PPU,profile_on,&mut profile_clock);
                match rendered {
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
                        self.hd_profile.mark(hd_profile::BG,profile_on,&mut profile_clock);
                        // Field map art is bound by the ROM map layout and
                        // independently checked against live PPU BG2 tilemap.
                        if self.hd_map_enabled && self.live_maps.has_entries() {
                            self.live_maps.refresh(&self.rom,field_state,log);
                            self.hd_map_pixels=self.live_maps.composite(
                                log,self.hd_renderer.layer_ids(),&mut self.hd_pixels,
                                self.hd_width as usize,self.hd_height as usize);
                        }
                        self.hd_profile.mark(hd_profile::MAP,profile_on,&mut profile_clock);
                        // Mode7 art is keyed to an exact VRAM plane and
                        // original palette, and sampled with the SNES HDMA
                        // matrix per scanline and subpixel. No fixed scale.
                        if self.hd_mode7_enabled && self.live_mode7.has_entries() {
                            // Always verify VRAM, palette and HDMA on this
                            // exact frame. Reuse the prior validated plane
                            // on camera-only movement; revalidate every 4
                            // frames to discover newly visible art tiles.
                            if self.live_mode7.needs_attention(log) ||
                                (self.live_mode7.is_active() &&
                                 self.machine.frame_count()%4==0) {
                                self.live_mode7.refresh(log);
                            }
                            self.hd_mode7_pixels=self.live_mode7.composite(
                                log,self.hd_renderer.layer_ids(),&mut self.hd_pixels,
                                self.hd_width as usize,self.hd_height as usize);
                        }
                        self.hd_profile.mark(hd_profile::MODE7,profile_on,&mut profile_clock);
                        // Reuse the desktop renderer's two-pass strategy:
                        // hide only validated fighter OAM sprites, reconstruct
                        // the genuine PPU pixels beneath them and reapply HD
                        // background art. Then draw the transparent HD pose.
                        // No guessed rectangles or solid colour masking.
                        self.live_fighters.refresh(log);
                        self.live_specials.refresh(log);
                        let fighters=if self.hd_fighter_enabled {
                            self.live_fighters.ready_oam_bits()
                        } else { 0 };
                        // A fighter already owns every one of its OAM
                        // pieces. Never allow a class3 sprite or class2
                        // effect to claim one of those pieces again.
                        self.live_specials.exclude_oam(fighters);
                        let specials=if self.hd_special_enabled {
                            self.live_specials.ready_oam_bits()
                        } else { 0 };
                        let hidden=fighters|specials;
                        if hidden != 0 {
                            let original_oam=self.hd_renderer.oam_ids().to_vec();
                            let original_layers=self.hd_renderer.layer_ids().to_vec();
                            if let Some((mut below,below_layers))=
                                self.hd_renderer.render_fighter_beneath(log,hidden,
                                    HdOptions {scale:2,look:Look::Crisp})
                            {
                                self.live_backgrounds.composite(
                                    log,&below_layers,&mut below.rgba,
                                    self.hd_width as usize,self.hd_height as usize,
                                );
                                // The underpass must also reproduce the HD
                                // ground and Mode 7 below a transparent pose.
                                if self.hd_map_enabled {
                                    self.live_maps.composite(log,&below_layers,
                                        &mut below.rgba,self.hd_width as usize,
                                        self.hd_height as usize);
                                }
                                if self.hd_mode7_enabled {
                                    self.live_mode7.composite(log,&below_layers,
                                        &mut below.rgba,self.hd_width as usize,
                                        self.hd_height as usize);
                                }
                                if self.hd_fighter_enabled {
                                    self.hd_fighter_pixels=self.live_fighters.composite(
                                        &original_oam,&original_layers,&below.rgba,
                                        &mut self.hd_pixels,
                                        self.hd_width as usize,self.hd_height as usize,
                                    );
                                }
                                if self.hd_special_enabled {
                                    self.hd_special_pixels=self.live_specials.composite(
                                        &original_oam,&original_layers,&below.rgba,
                                        &mut self.hd_pixels,
                                        self.hd_width as usize,self.hd_height as usize,
                                    );
                                }
                            }
                        }
                        self.hd_profile.mark(hd_profile::FIGHTER,profile_on,&mut profile_clock);
                        // Class1 faces: compare real winning 4bpp PPU source
                        // tile hashes against the player's ROM streams. Only
                        // verified local HD artwork may replace those pixels.
                        if self.live_portraits.has_entries() {
                            let cadence=if self.live_portraits.needs_attention() {1} else {2};
                            if self.machine.frame_count() % cadence == 0 {
                                self.live_portraits.refresh(
                                    log, self.hd_renderer.portrait_pixels());
                            }
                            if self.hd_portrait_enabled {
                                self.hd_portrait_pixels=self.live_portraits.composite(
                                    log,self.hd_renderer.portrait_pixels(),
                                    &mut self.hd_pixels,
                                    self.hd_width as usize,self.hd_height as usize);
                            }
                        }
                        self.hd_profile.mark(hd_profile::PORTRAIT,profile_on,&mut profile_clock);
                        // Card/UI 2bpp art uses a unique ROM tile hash and
                        // the PPU's winning BG tile, not a guessed card box.
                        if self.hd_card_enabled && self.live_cards.has_entries() {
                            // Keep the already tested deterministic, full
                            // visible tile selection; no 140 tile cutoff.
                            self.live_cards.refresh(log,self.hd_renderer.portrait_pixels());
                            self.hd_card_pixels=self.live_cards.composite(
                                log,self.hd_renderer.portrait_pixels(),&mut self.hd_pixels,
                                self.hd_width as usize,self.hd_height as usize);
                        }
                        self.hd_profile.mark(hd_profile::CARDS,profile_on,&mut profile_clock);
                        // Compare the page identified from WRAM with the actual
                        // PPU glyphs. When it does NOT match, a recent earlier
                        // page may still be visible behind the advancing engine.
                        // This uses the upstream character/position matcher,
                        // not background colour, OCR or hard-coded coordinates.
                        let direct = selected.as_ref()
                            .and_then(|page| self.hd_renderer.visible_translation_score(page));
                        let mut best_score = direct.unwrap_or(0);
                        let mut best_translation = None;
                        let mut best_untranslated = None;
                        if direct.is_none() {
                            let now = self.machine.frame_count() as u32;
                            let reference = selected.as_ref().or_else(|| {
                                self.recent_pages.last().map(|(_, page)| page)
                            });
                            // Parse nearby ROM pages of BOTH narration and
                            // portrait dialogue. Prior code restricted this
                            // fallback to table 3 and missed str2:0x002
                            // ("I'd offer to help") while WRAM already
                            // indicated its later "Don't die" page.
                            if let Some(page) = reference.filter(|page|
                                matches!(page.table, 2 | 3))
                            {
                                let location = (page.table, page.string, page.base);
                                if self.story_candidates_for != Some(location) {
                                    self.story_candidates =
                                        story_pages_near(&self.rom, page, 2);
                                    self.story_candidates_for = Some(location);
                                }
                            }
                            let recent = self.recent_pages.iter()
                                .rev()
                                .filter(|(seen, _)| now.saturating_sub(*seen) <= 150)
                                .map(|(_, page)| page);
                            let nearby = self.story_candidates.iter();
                            for page in recent.chain(nearby) {
                                if selected.as_ref().is_some_and(|current|
                                    current.key == page.key) { continue; }
                                if let Some(score) = self.hd_renderer.visible_translation_score(page)
                                    && score > best_score
                                {
                                    best_score = score;
                                    if self.translation.as_ref().is_some_and(|table|
                                        table.text(&page.key).is_some())
                                    {
                                        best_translation = Some(page.clone());
                                        best_untranslated = None;
                                    } else {
                                        best_translation = None;
                                        best_untranslated = Some(page.key.clone());
                                    }
                                }
                            }
                            if let Some(page) = best_translation {
                                selected = Some(page);
                                self.native_visible_fallback = true;
                            } else if best_untranslated.is_some() {
                                // Never paint the wrong Chinese text over a
                                // positively identified, untranslated page.
                                selected = None;
                            }
                            self.native_untranslated_visible = best_untranslated;
                        }
                        self.native_visible_score = best_score;
                        self.native_expected_english = selected.as_ref().map(|page| {
                            page.english.iter().map(|line| {
                                line.iter().map(|ch| ch.unwrap_or('·')).collect::<String>()
                            }).collect::<Vec<_>>().join("|")
                        }).unwrap_or_default();
                        self.native_page_key = selected.as_ref().map(|page| page.key.clone());
                        // Determine whether the actual PPU has started writing
                        // story letters. The preview should not expose these
                        // letters before the native Chinese glyph pass is ready.
                        // Portrait dialogue uses table 2; narration uses 3.
                        // Never use an ancient page to classify a later menu
                        // as dialogue. A recent page is eligible briefly after
                        // the message engine stops naming it.
                        let now = self.machine.frame_count() as u32;
                        let text_page = selected.as_ref()
                            .filter(|page| matches!(page.table, 2 | 3))
                            .or_else(|| self.recent_pages.iter().rev()
                                .find(|(seen, page)| {
                                    now.saturating_sub(*seen) <= 18
                                        && matches!(page.table, 2 | 3)
                                })
                                .map(|(_, page)| page));
                        self.native_story_ascii_cells = text_page
                            .map_or(0, |page| self.hd_renderer.story_ascii_cells(page));
                        self.hd_renderer.set_translation_screen(selected.clone());
                        // The original native renderer owns every Chinese glyph.
                        // It runs AFTER HD art. This boolean means the pass ran,
                        // not necessarily that translated pixels changed.
                        self.native_painted = self.hd_renderer.paint_native_translation(
                            log, &mut self.hd_pixels,
                            self.hd_width as usize, self.hd_height as usize,
                        );
                        // This is a signal for the browser presentation gate;
                        // never pause the emulator or suppress audio. A zero
                        // glyph-sample count is not independently proof of
                        // English, hence the required PPU story-text check.
                        self.native_story_text_pending = self.native_painted
                            && self.native_story_ascii_cells > 0
                            && self.hd_renderer.native_changed_samples() == 0;
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
        if self.classic_zh_enabled && !self.hd_enabled {
            // Keep every Classic Chinese frame at one consistent output size,
            // even if the PPU text log is momentarily unavailable.
            // The artwork is always the original SNES frame, just nearest-
            // neighbour duplicated; no 2x HD PPU pass or art compositing.
            let (width, height) = {
                let frame = self.machine.frame();
                let width = frame.size.width as usize;
                let height = frame.size.height as usize;
                nearest_rgba_2x(
                    bytemuck::cast_slice(frame.pixels), width, height,
                    &mut self.classic_zh_pixels,
                );
                (width * 2, height * 2)
            };
            if let Some(log) = self.machine.frame_log() {
                if self.hd_renderer.capture_classic_text(log) {
                    // If the engine named its next page before that page
                    // visibly arrived, prefer recently confirmed real glyphs.
                    let mut best = selected.as_ref()
                        .and_then(|page| self.hd_renderer.visible_translation_score(page))
                        .unwrap_or(0);
                    if best == 0 {
                        for (seen, page) in self.recent_pages.iter().rev() {
                            if (self.machine.frame_count() as u32).saturating_sub(*seen) > 18 {
                                break;
                            }
                            if let Some(score) = self.hd_renderer.visible_translation_score(page)
                                && score > best
                            {
                                best = score;
                                selected = Some(page.clone());
                                self.native_visible_fallback = true;
                            }
                        }
                    }
                    self.native_visible_score = best;
                    self.native_page_key = selected.as_ref().map(|page| page.key.clone());
                    self.hd_renderer.set_translation_screen(selected);
                    // Only the Chinese glyphs get 2x rasterisation. Source
                    // art remains the original, without HD sprite/background.
                    self.native_painted = self.hd_renderer.paint_classic_translation(
                        log, &mut self.classic_zh_pixels, width, height,
                    );
                }
            }
        }
        self.hd_profile.mark(hd_profile::TEXT,profile_on,&mut profile_clock);
        self.hd_profile.finish(profile_on);
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
        self.native_page_key = None;
        self.native_late_page = false;
        self.recent_pages.clear();
        self.native_visible_fallback = false;
        self.native_visible_score = 0;
        self.native_untranslated_visible = None;
        self.story_candidates.clear();
        self.story_candidates_for = None;
        self.native_expected_english.clear();
        self.native_painted = false;
        self.native_story_text_pending = false;
        self.native_story_ascii_cells = 0;
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
        self.native_painted && (self.hd_active()
            || (self.classic_zh_enabled && !self.classic_zh_pixels.is_empty()))
    }

    /// Whether a story dialogue frame contains untranslated PPU letters
    /// that should be deferred briefly before Safari presents it.
    pub fn story_translation_pending(&self) -> bool {
        self.hd_enabled && self.native_story_text_pending && self.hd_active()
    }

    /// A compact, local-only diagnostic for rare missed English pages.
    /// Contains no ROM/IPS/HD bytes and never sends data to a server.
    pub fn translation_diagnostics(&self) -> String {
        let cells = self.hd_renderer.observed_text_cells();
        // Cell capture is the ROM's *displayed* ASCII, not an OCR guess.
        let letters: Vec<_> = cells.into_iter()
            .filter(|(x, y, ch, _, _)| {
                *x >= 0 && *x < 256 && *y >= 0 && *y < 240 &&
                    ch.chars().any(|c| c.is_ascii_alphabetic())
            })
            .take(96)
            .map(|(x,y,ch,_,_)| json!([x,y,ch]))
            .collect();
        let end = on_screen(&self.rom, self.machine.wram(), self.native_page_hint);
        let end_key = end.as_ref().map(|screen| screen.key.clone());
        json!({
            "frame": self.machine.frame_count(),
            "hd_active": self.hd_active(),
            "translator_ran": self.native_painted,
            "changed_glyph_samples": self.hd_renderer.native_changed_samples(),
            "story_translation_pending": self.story_translation_pending(),
            "story_ascii_cells": self.native_story_ascii_cells,
            "visible_page_fallback": self.native_visible_fallback,
            "visible_page_score": self.native_visible_score,
            "untranslated_visible_key": self.native_untranslated_visible,
            "selected_expected_english": self.native_expected_english,
            "recent_page_keys": self.recent_pages.iter()
                .map(|(_,page)| page.key.as_str()).collect::<Vec<_>>(),
            "start_key": self.native_page_key,
            "end_key": end_key,
            "start_has_translation": self.native_page_key.as_ref()
                .is_some_and(|key| self.translation.as_ref()
                    .is_some_and(|table| table.text(key).is_some())),
            "end_has_translation": end.as_ref()
                .is_some_and(|page| self.translation.as_ref()
                    .is_some_and(|table| table.text(&page.key).is_some())),
            "late_page": self.native_late_page,
            "glyphs": letters
        }).to_string()
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

    /// Only the player's verified local class2 index; never HD bytes or ROM
    /// data sent over a network. Exact fighter palette and hash are required.
    pub fn set_fighter_index(&mut self, json: &str) -> Result<u32, JsValue> {
        self.live_fighters.load_catalog(json)
            .map(|count| count as u32)
            .map_err(|err| JsValue::from_str(&err))
    }

    pub fn fighter_matches(&self) -> String {
        self.live_fighters.matches_json()
    }

    /// Read-only counts explaining why a given stance has no local HD art.
    pub fn fighter_probe(&self) -> String {
        self.live_fighters.probe_json()
    }

    pub fn set_fighter_art(&mut self, index: usize, width: u32,
        height: u32, rgba: &[u8]) -> Result<(), JsValue> {
        self.live_fighters.add_picture(index,width as usize,height as usize,rgba)
            .map_err(|err|JsValue::from_str(&err))
    }

    /// Safely decoded, official enlarged class2 pose; never a guessed overlay.
    pub fn set_fighter_variant(&mut self,index:usize,width:u32,height:u32,rgba:&[u8])
        ->Result<(),JsValue>
    {
        self.live_fighters.add_variant(index,width as usize,height as usize,rgba)
            .map_err(|err|JsValue::from_str(&err))
    }

    /// SNES pixels with confirmed visible fighter OAM pixels replaced.
    pub fn hd_fighter_pixels(&self) -> u32 { self.hd_fighter_pixels }

    /// A/B compare exact same battle frame with background HD left on.
    pub fn set_fighter_hd_enabled(&mut self, enabled: bool) {
        self.hd_fighter_enabled=enabled;
    }

    /// Only verified class3 sprites and class2 effects, never a named
    /// character or a guessed screen rectangle.
    pub fn set_special_index(&mut self, json:&str)->Result<u32,JsValue>{
        self.live_specials.load_catalog(json)
            .map(|n|n as u32).map_err(|e|JsValue::from_str(&e))
    }
    pub fn special_matches(&self)->String{self.live_specials.matches_json()}
    pub fn special_probe(&self)->String{self.live_specials.probe_json()}
    pub fn set_special_art(&mut self,index:usize,width:u32,height:u32,rgba:&[u8])
        ->Result<(),JsValue>{
        self.live_specials.add_picture(index,width as usize,height as usize,rgba)
            .map_err(|e|JsValue::from_str(&e))
    }
    pub fn hd_special_pixels(&self)->u32{self.hd_special_pixels}
    pub fn set_special_hd_enabled(&mut self,enabled:bool){self.hd_special_enabled=enabled;}

    pub fn set_map_index(&mut self,json:&str)->Result<u32,JsValue>{
        self.live_maps.load_catalog(json)
            .map(|n|n as u32).map_err(|e|JsValue::from_str(&e))
    }
    pub fn map_matches(&self)->String{self.live_maps.matches_json()}
    pub fn map_probe(&self)->String{self.live_maps.probe_json()}
    pub fn set_map_tile(&mut self,index:usize,x:usize,y:usize,width:u32,height:u32,rgba:&[u8])
        ->Result<(),JsValue>{
        self.live_maps.add_picture(index,x,y,width as usize,height as usize,rgba)
            .map_err(|e|JsValue::from_str(&e))
    }
    pub fn hd_map_pixels(&self)->u32{self.hd_map_pixels}
    pub fn set_map_hd_enabled(&mut self,enabled:bool){self.hd_map_enabled=enabled;}

    pub fn set_mode7_index(&mut self,json:&str)->Result<u32,JsValue>{
        self.live_mode7.load_catalog(json)
            .map(|n|n as u32).map_err(|e|JsValue::from_str(&e))
    }
    pub fn mode7_matches(&self)->String{self.live_mode7.matches_json()}
    pub fn mode7_probe(&self)->String{self.live_mode7.probe_json()}
    pub fn set_mode7_tile(&mut self,index:usize,x:usize,y:usize,width:u32,height:u32,
        rgba:&[u8])->Result<(),JsValue>{
        self.live_mode7.add_tile(index,x,y,width as usize,height as usize,rgba)
            .map_err(|e|JsValue::from_str(&e))
    }
    pub fn hd_mode7_pixels(&self)->u32{self.hd_mode7_pixels}
    pub fn set_mode7_hd_enabled(&mut self,on:bool){self.hd_mode7_enabled=on;}

    /// Source-keyed official card and UI tile art, layers blended through
    /// the ROM's PPU palettes (base and roles).
    pub fn set_card_index(&mut self,json:&str)->Result<u32,JsValue>{
        let count=self.live_cards.load_catalog(json,&self.rom)
            .map_err(|e|JsValue::from_str(&e))?;
        self.hd_renderer.set_portrait_tracking(
            self.live_cards.has_entries()||self.live_portraits.has_entries());
        Ok(count as u32)
    }
    pub fn card_matches(&self)->String{self.live_cards.matches_json()}
    pub fn card_probe(&self)->String{self.live_cards.probe_json()}
    pub fn set_card_art(&mut self,index:usize,layer:u8,width:u32,height:u32,
        rgba:&[u8])->Result<(),JsValue>{
        self.live_cards.add_art(index,layer,width as usize,height as usize,rgba)
            .map_err(|e|JsValue::from_str(&e))
    }
    /// Bitmask of visible HD art still awaiting local ZIP image decoding.
    /// This prevents six expensive JS/WASM JSON roundtrips on *every* frame
    /// while preserving same-frame requests when a new match appears.
    /// Average and maximum sampled phase times since the last read. A
    /// read resets counters, so each iPhone screenshot reflects the current
    /// scene rather than an old battle stall.
    pub fn hd_profile_report(&mut self)->String {
        self.hd_profile.take_json()
    }
    pub fn hd_pending_mask(&self)->u32 {
        if !self.hd_enabled {return 0;}
        u32::from(self.hd_fighter_enabled && self.live_fighters.has_pending_art()) |
        (u32::from(self.hd_portrait_enabled && self.live_portraits.has_pending_art()) << 1) |
        (u32::from(self.hd_special_enabled && self.live_specials.has_pending_art()) << 2) |
        (u32::from(self.hd_map_enabled && self.live_maps.has_pending_art()) << 3) |
        (u32::from(self.hd_card_enabled && self.live_cards.has_pending_art()) << 4) |
        (u32::from(self.hd_mode7_enabled && self.live_mode7.has_pending_art()) << 5) |
        (u32::from(self.live_backgrounds.has_pending_art()) << 6)
    }
    pub fn hd_background_pending(&self)->bool {
        self.hd_enabled && self.live_backgrounds.has_pending_art()
    }
    pub fn hd_card_pixels(&self)->u32{self.hd_card_pixels}
    pub fn set_card_hd_enabled(&mut self,enabled:bool){self.hd_card_enabled=enabled;}

    /// Load validated class1 metadata; the palette-index source pixels are
    /// reconstructed from the user's own ROM, never from remote services.
    pub fn set_portrait_index(&mut self, json: &str) -> Result<u32, JsValue> {
        let n=self.live_portraits.load_catalog(json,&self.rom)
            .map_err(|e|JsValue::from_str(&e))?;
        self.hd_renderer.set_portrait_tracking(n>0||self.live_cards.has_entries());
        Ok(n as u32)
    }

    pub fn portrait_matches(&self)->String {self.live_portraits.matches_json()}
    pub fn portrait_probe(&self)->String {self.live_portraits.probe_json()}
    pub fn set_portrait_art(&mut self,index:usize,width:u32,height:u32,rgba:&[u8])
        ->Result<(),JsValue> {
        self.live_portraits.add_picture(index,width as usize,height as usize,rgba)
            .map_err(|e|JsValue::from_str(&e))
    }
    pub fn hd_portrait_pixels(&self)->u32 {self.hd_portrait_pixels}
    pub fn set_portrait_hd_enabled(&mut self,enabled:bool) {
        self.hd_portrait_enabled=enabled;
    }

    /// Render real PPU pixels at 1x, with Traditional Chinese drawn by the
    /// existing glyph-matched translator. No HD artwork is required.
    pub fn set_classic_zh_mode(&mut self, enabled: bool) {
        if self.classic_zh_enabled == enabled { return; }
        self.classic_zh_enabled = enabled;
        self.machine.set_line_log(self.hd_enabled || enabled);
        self.hd_renderer = tracked_renderer(self.text_font.clone(), self.native_overlay.clone());
        self.hd_renderer.set_layer_tracking(self.hd_enabled);
        self.hd_renderer.set_portrait_tracking(
            self.hd_enabled && (self.live_portraits.has_entries() || self.live_cards.has_entries()),
        );
        self.native_painted = false;
        self.native_page_hint = None;
        self.recent_pages.clear();
        self.story_candidates.clear();
        self.story_candidates_for = None;
        self.classic_zh_pixels.clear();
    }

    pub fn classic_zh_mode(&self) -> bool { self.classic_zh_enabled }

    /// Use upstream Enhanced 2x renderer; backgrounds are supplied by the player.
    /// Unsupported frames fall back to Classic without affecting emulator state.
    pub fn set_hd_mode(&mut self, enabled: bool) {
        if self.hd_enabled == enabled { return; }
        self.hd_enabled = enabled;
        self.machine.set_line_log(enabled || self.classic_zh_enabled);
        self.hd_renderer = tracked_renderer(self.text_font.clone(), self.native_overlay.clone());
        self.hd_renderer.set_portrait_tracking(enabled && (self.live_portraits.has_entries()||self.live_cards.has_entries()));
        self.native_painted = false;
        self.native_page_hint = None;
        self.native_page_key = None;
        self.native_late_page = false;
        self.recent_pages.clear();
        self.native_visible_fallback = false;
        self.native_visible_score = 0;
        self.native_untranslated_visible = None;
        self.story_candidates.clear();
        self.story_candidates_for = None;
        self.native_expected_english.clear();
        self.native_story_text_pending = false;
        self.native_story_ascii_cells = 0;
        self.hd_art_pixels = 0;
        self.hd_fighter_pixels = 0;
        self.hd_special_pixels = 0;
        self.hd_map_pixels = 0;
        self.hd_card_pixels = 0;
        self.hd_mode7_pixels = 0;
        self.hd_portrait_pixels = 0;
        self.hd_pixels.clear();
        self.classic_zh_pixels.clear();
        self.hd_width = 0;
        self.hd_height = 0;
        self.hd_warning.clear();
    }

    pub fn hd_mode(&self) -> bool { self.hd_enabled }

    pub fn hd_active(&self) -> bool { self.hd_enabled && !self.hd_pixels.is_empty() }

    pub fn hd_warning(&self) -> String { self.hd_warning.clone() }

    pub fn width(&self) -> u32 {
        if self.hd_active() { return self.hd_width; }
        if self.classic_zh_enabled && !self.classic_zh_pixels.is_empty() {
            return self.machine.frame().size.width * 2;
        }
        self.machine.frame().size.width
    }

    pub fn height(&self) -> u32 {
        if self.hd_active() { return self.hd_height; }
        if self.classic_zh_enabled && !self.classic_zh_pixels.is_empty() {
            return self.machine.frame().size.height * 2;
        }
        self.machine.frame().size.height
    }

    /// RGBA8 pixels, copied across the wasm-to-JS boundary.
    pub fn frame_rgba(&self) -> Vec<u8> {
        if self.hd_active() { return self.hd_pixels.clone(); }
        if self.classic_zh_enabled && !self.classic_zh_pixels.is_empty() {
            return self.classic_zh_pixels.clone();
        }
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
        self.hd_renderer.set_portrait_tracking(self.live_portraits.has_entries()||self.live_cards.has_entries());
        self.native_painted = false;
        self.native_page_hint = None;
        self.native_page_key = None;
        self.native_late_page = false;
        self.recent_pages.clear();
        self.native_visible_fallback = false;
        self.native_visible_score = 0;
        self.native_untranslated_visible = None;
        self.story_candidates.clear();
        self.story_candidates_for = None;
        self.native_expected_english.clear();
        self.native_story_text_pending = false;
        self.native_story_ascii_cells = 0;
        self.hd_art_pixels = 0;
        self.hd_fighter_pixels = 0;
        self.hd_special_pixels = 0;
        self.hd_map_pixels = 0;
        self.hd_card_pixels = 0;
        self.hd_mode7_pixels = 0;
        self.hd_portrait_pixels = 0;
        self.machine.load_state(state)
            .map_err(|err| JsValue::from_str(&format!("讀取存檔失敗：{err}")))?;
        // Existing .sav files remain byte-for-byte compatible. Never inherit
        // a stale cheat block or an unknown script-battle gate from a state.
        self.cheats = Cheats::neutral();
        cheat_bridge::clear_after_state_load(&mut self.machine)
            .map_err(|err| JsValue::from_str(&format!("重設作弊設定失敗：{err}")))?;
        Ok(())
    }
}