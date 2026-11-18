//! Local HD battle backgrounds for the iPhone Web bridge.
//! Matches the player's verified bg/index.json against the live PPU FrameLog.
//! Artwork is supplied by the browser after local ZIP extraction; neither ROM nor art is uploaded.
use serde::Deserialize;
use ssd_hd::bgart::{layer_key, layer_size, palette_key};
use ssd_oracle::FrameLog;
use std::collections::HashMap;

const MAX_LAYERS: usize = 4096;
const MAX_ART_BYTES: usize = 24 * 1024 * 1024;
const WIDTH: usize = 256;
const SCALE: usize = 2;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layer {
    id: String,
    key: String,
    bg: u8,
    mode: u8,
    map_base: u16,
    char_base: u16,
    size: u8,
    rows: [u16; 2],
    #[serde(default)]
    fill: bool,
    art: String,
    #[serde(default)]
    available: bool,
    #[serde(default)]
    palette_key: String,
}

struct Picture {
    width: usize,
    rgba: Vec<u8>,
}

/// A maximum of two live battle-background layers; all bytes are from a player-selected ZIP.
#[derive(Default)]
pub struct LiveBackgrounds {
    layers: Vec<Layer>,
    active: Vec<usize>,
    pictures: HashMap<usize, Picture>,
    ticks: u32,
}

impl LiveBackgrounds {
    pub fn load_catalog(&mut self, json: &str) -> Result<usize, String> {
        let layers: Vec<Layer> = serde_json::from_str(json)
            .map_err(|e| format!("背景索引解析失敗：{e}"))?;
        if layers.len() > MAX_LAYERS {
            return Err("HD 背景索引數量異常".to_string());
        }
        for layer in &layers {
            if layer.id.len() > 160 || layer.art.len() > 240 || layer.bg > 3
                || layer.mode > 7 || layer.size > 3 || layer.rows[0] >= layer.rows[1]
                || layer.rows[1] > if layer.size & 2 != 0 { 64 } else { 32 }
                || layer.key.is_empty() || layer.key.len() > 16
                || u64::from_str_radix(&layer.key, 16).is_err()
                || (!layer.palette_key.is_empty()
                    && u64::from_str_radix(&layer.palette_key, 16).is_err())
            {
                return Err(format!("HD 背景索引資料有誤：{}", layer.id));
            }
        }
        self.layers = layers;
        self.active.clear();
        self.pictures.clear();
        self.ticks = 0;
        Ok(self.layers.len())
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Check expensive VRAM content hashes once per 12 simulation frames, not every 60 Hz frame.
    pub fn refresh(&mut self, log: &FrameLog) {
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % 12 != 1 {
            return;
        }
        if !log.complete || !log.vram_writes.is_empty() {
            self.active.clear();
            self.pictures.clear();
            return;
        }
        let mut hashes: HashMap<(u8, u8, u16, u16, u8, [u16; 2]), u64> = HashMap::new();
        let mut chosen: Vec<(usize, u8)> = Vec::new();
        for (index, entry) in self.layers.iter().enumerate() {
            if !entry.available || !matches!(entry.mode, 0 | 1) {
                continue;
            }
            let bg = usize::from(entry.bg);
            let shown = log.entries.iter().find(|line| {
                let r = &line.regs;
                !r.forced_blanking && r.bg_mode == entry.mode
                    && r.main_bg[bg] && !r.bg_large_tiles[bg]
                    && r.bg_map_base[bg] == entry.map_base
                    && r.bg_char_base[bg] == entry.char_base
                    && r.bg_screen_size[bg] == entry.size
            });
            let Some(shown) = shown else { continue };
            let planes = if entry.mode == 0 || bg >= 2 { 2 } else { 4 };
            let class = (entry.bg, entry.mode, entry.map_base,
                entry.char_base, entry.size, entry.rows);
            let actual = *hashes.entry(class).or_insert_with(|| layer_key(
                &log.vram, entry.map_base, entry.char_base, entry.size, false,
                planes, entry.rows,
            ));
            if u64::from_str_radix(&entry.key, 16).ok() != Some(actual) {
                continue;
            }
            let priority = if entry.palette_key.is_empty() {
                1
            } else {
                let current = palette_key(
                    &log.vram, &log.cgram_start, bg, entry.mode, entry.map_base,
                    entry.size, false, planes, entry.rows, entry.fill,
                );
                if u64::from_str_radix(&entry.palette_key, 16).ok() != Some(current) {
                    continue;
                }
                2
            };
            // Two palette variants of the same rows must not both be displayed.
            if let Some(at) = chosen.iter().position(|(old, _)| {
                let other = &self.layers[*old];
                other.bg == entry.bg && other.mode == entry.mode
                    && other.map_base == entry.map_base
                    && other.char_base == entry.char_base
                    && other.size == entry.size && other.rows == entry.rows
            }) {
                if priority > chosen[at].1 {
                    chosen[at] = (index, priority);
                }
            } else {
                chosen.push((index, priority));
            }
            let _ = shown; // The line's registers were checked above.
        }
        // Match the upstream HD renderer's far-to-near BG slot order and two-slot budget.
        chosen.sort_by_key(|(index, _)| std::cmp::Reverse(self.layers[*index].bg));
        self.active = chosen.into_iter().take(2).map(|(index, _)| index).collect();
        self.pictures.retain(|index, _| self.active.contains(index));
    }

    pub fn matches_json(&self) -> String {
        let items: Vec<_> = self.active.iter().map(|&index| {
            let e = &self.layers[index];
            serde_json::json!({
                "index": index, "id": e.id, "art": e.art,
                "bg": e.bg + 1, "ready": self.pictures.contains_key(&index)
            })
        }).collect();
        serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn add_picture(&mut self, index: usize, width: usize, height: usize, rgba: &[u8])
        -> Result<(), String>
    {
        if !self.active.contains(&index) {
            return Err("背景已切換，略過過時圖片".to_string());
        }
        let e = &self.layers[index];
        let (w, _) = layer_size(e.size, false);
        let expected_width = w * 4;
        let expected_height = usize::from(e.rows[1] - e.rows[0]) * 8 * 4;
        let bytes = width.checked_mul(height).and_then(|px| px.checked_mul(4))
            .ok_or_else(|| "HD 圖片尺寸過大".to_string())?;
        if width != expected_width || height != expected_height
            || bytes > MAX_ART_BYTES || rgba.len() != bytes
        {
            return Err(format!("HD 圖片尺寸不符：{}（{}×{}）", e.id, width, height));
        }
        self.pictures.insert(index, Picture { width, rgba: rgba.to_vec() });
        Ok(())
    }

    /// Replace only pixels that the real PPU rendered from the matching BG.
    /// Object sprites and other BG layers remain in front. This is an initial,
    /// deliberately conservative pass: arbitrary SNES colour math is not reconstructed.
    pub fn composite(&self, log: &FrameLog, owners: &[u8], out: &mut [u8],
        width: usize, height: usize) -> u32
    {
        if self.active.is_empty() || width != WIDTH * SCALE
            || height != usize::from(log.height) * SCALE
            || owners.len() != WIDTH * usize::from(log.height)
            || out.len() != width * height * 4
        {
            return 0;
        }
        let mut drawn = 0;
        for (n, line) in log.entries.iter().enumerate() {
            let y = usize::from(line.scanline).saturating_sub(1);
            if line.scanline == 0 || y >= usize::from(log.height)
                || line.regs.forced_blanking
            {
                continue;
            }
            let from = usize::from(line.from_pixel).min(WIDTH);
            let to = log.entries.get(n + 1)
                .filter(|next| next.scanline == line.scanline)
                .map_or(WIDTH, |next| usize::from(next.from_pixel).min(WIDTH));
            if from >= to { continue; }
            let regs = &line.regs;
            for x in from..to {
                let owner = owners[y * WIDTH + x];
                // The same BG can have two art entries for different tile-row bands.
                for &index in &self.active {
                    let e = &self.layers[index];
                    let Some(img) = self.pictures.get(&index) else { continue };
                    let bg = usize::from(e.bg);
                    if regs.bg_mode != e.mode || !regs.main_bg[bg]
                        || regs.bg_map_base[bg] != e.map_base
                        || regs.bg_char_base[bg] != e.char_base
                        || regs.bg_screen_size[bg] != e.size
                        || regs.bg_large_tiles[bg]
                        || (owner != e.bg + 1 && !(owner == 0 && e.fill))
                    {
                        continue;
                    }
                    // Match bgart::Slot::position: the first displayed line is 1-based.
                    let (layer_width, layer_height) = layer_size(e.size, false);
                    let gx = (x + usize::from(regs.bg_h_scroll[bg])) % layer_width;
                    let gy = (usize::from(line.scanline)
                        + usize::from(regs.bg_v_scroll[bg])) % layer_height;
                    let first_row = usize::from(e.rows[0]) * 8;
                    if gy < first_row || gy >= usize::from(e.rows[1]) * 8 {
                        continue;
                    }
                    let ax = gx * 4;
                    let ay = (gy - first_row) * 4;
                    for sy in 0..SCALE {
                        for sx in 0..SCALE {
                            // The shared art is 4x; Enhanced mode is 2x.
                            let src = ((ay + sy * 2 + 1) * img.width + ax + sx * 2 + 1) * 4;
                            let dst = ((y * SCALE + sy) * width + x * SCALE + sx) * 4;
                            let alpha = u32::from(img.rgba[src + 3]);
                            if alpha == 0 { continue; }
                            for c in 0..3 {
                                let mut colour = u32::from(img.rgba[src + c]);
                                if regs.brightness < 15 {
                                    colour = (colour * u32::from(regs.brightness) + 7) / 15;
                                }
                                out[dst + c] = ((colour * alpha
                                    + u32::from(out[dst + c]) * (255 - alpha) + 127) / 255) as u8;
                            }
                            out[dst + 3] = 255;
                        }
                    }
                    drawn += 1;
                    // One frontmost layer per original pixel.
                    break;
                }
            }
        }
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::LiveBackgrounds;

    #[test]
    fn malformed_and_unbounded_catalogues_are_rejected() {
        let mut live = LiveBackgrounds::default();
        assert!(live.load_catalog("[{}]").is_err());
        assert!(live.load_catalog("[]").is_ok());
        assert!(live.matches_json() == "[]");
    }
}
