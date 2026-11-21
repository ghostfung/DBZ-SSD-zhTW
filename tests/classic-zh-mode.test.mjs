import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const src=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('Classic Chinese mode preserves original 1x PPU and never enters HD art',()=>{
  const bridge=src('crates/ssd-web/src/lib.rs');
  const renderer=src('crates/ssd-hd/src/lib.rs');
  assert.match(bridge,/classic_zh_enabled: bool/);
  assert.match(bridge,/pub fn set_classic_zh_mode\(&mut self, enabled: bool\)/);
  assert.match(bridge,/self\.machine\.set_line_log\(self\.hd_enabled \|\| enabled\)/);
  assert.match(bridge,/self\.hd_renderer\.paint_classic_translation\(/);
  assert.match(bridge,/if self\.classic_zh_enabled && !self\.classic_zh_pixels\.is_empty\(\)/);
  const start=bridge.indexOf('if self.classic_zh_enabled && !self.hd_enabled {');
  const end=bridge.indexOf('self.hd_profile.mark(hd_profile::TEXT',start);
  assert.ok(start>0 && end>start);
  const classic=bridge.slice(start,end);
  assert.doesNotMatch(classic,/render_hd\(|\.composite\(|set_background_art\(|set_fighter_art\(/);
  assert.match(renderer,/pub fn capture_classic_text\(&mut self, log: &FrameLog\) -> bool/);
  assert.match(renderer,/pub fn paint_classic_translation\(/);
  // The source remains native 1x PPU: only RGBA pixels and Chinese glyphs
  // are enlarged, avoiding the costly 2x HD PPU renderer.
  assert.match(bridge,/fn nearest_rgba_2x\(source: &\[u8\]/);
  assert.match(bridge,/nearest_rgba_2x\(/);
  assert.match(bridge,/\(width \* 2, height \* 2\)/);
  assert.doesNotMatch(classic,/render_hd\(/);
  assert.match(renderer,/paint_native_translation_scaled\(log, rgba, width, height, 2\)/);
  assert.match(renderer,/self\.render_log\(log, None, false\)/);
});

test('three-way display selection is persisted and updates both WASM switches',()=>{
  const app=src('web/app.mjs');
  assert.match(app,/const modes=\['hd','classic-zh','classic'\]/);
  assert.match(app,/localStorage\.setItem\('dbz-hd-mode',hdPreference\)/);
  assert.match(app,/game\.set_hd_mode\(hdEnabled\)/);
  assert.match(app,/game\.set_classic_zh_mode\(classicZhEnabled\)/);
  assert.match(app,/classicZhEnabled\?'中文 1x':'原版'/);
  assert.match(src('web/index.html'),/Classic 繁中 1x/);
  assert.match(app,/const BUILD_ID='dbz-hd-tw-pwa-v49'/);
  assert.match(src('crates/ssd-web/src/lib.rs'),/dbz-hd-tw-pwa-v49/);
});

test('original audio and save-state handlers remain intact',()=>{
  const app=src('web/app.mjs');
  assert.match(app,/game\.load_state\(new Uint8Array/);
  assert.match(app,/const restored=await restoreAudioAfterInterruption\(\)/);
  assert.match(app,/game\.set_audio_rate\(sound\.sampleRate\)/);
});
