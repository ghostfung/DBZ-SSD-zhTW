import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const read = path => readFileSync(new URL('../'+path,import.meta.url),'utf8');
const web = read('crates/ssd-web/src/lib.rs');
const renderer = read('crates/ssd-hd/src/lib.rs');
const hd = read('crates/ssd-hd/src/enhanced.rs');
const app = read('web/app.mjs');

test('iPhone uses the original author native translation overlay, not guessed menu rectangles',()=>{
  assert.match(web,/TextOverlay::new\(Arc::new\(table\.clone\(\)\), Arc::new\(face\)\)/);
  assert.match(web,/\.with_names\(self\.menu_names\.clone\(\)\)/);
  assert.match(web,/self\.hd_renderer\.set_native_translation\(self\.native_overlay\.clone\(\)\)/);
  assert.match(renderer,/translator\.draw\(/);
  assert.doesNotMatch(app,/drawMenuTranslations|drawChineseInBox|maskPendingDialogue|findDialogueBox/);
});

test('native text is synchronized with PPU and drawn after local HD background art',()=>{
  const screen = web.indexOf('self.hd_renderer.set_translation_screen(screen);');
  const tick = web.indexOf('self.machine.run_frame(joypad_from_bits(joypad_bits)');
  const art = web.indexOf('self.live_backgrounds.composite(');
  const chinese = web.indexOf('self.hd_renderer.paint_native_translation(');
  assert.ok(screen>=0&&tick>=0&&screen<tick,
    'text screen must come from the START of the emulated frame');
  assert.ok(art>=0&&chinese>art,
    'Chinese must be the final pass above locally imported HD art');
  assert.match(hd,/self\.native_text_frame = if self\.native_translation\.is_some\(\)/);
});

test('Chinese UI is rendered by Rust in 2x, with safe caption-only Classic fallback',()=>{
  assert.match(app,/game\.native_translation_active\(\)/);
  assert.match(web,/pub fn native_translation_active\(&self\) -> bool/);
  assert.ok(app.includes("box.hidden = !$(\'inframe\').checked;"));
  assert.match(web,/self\.native_overlay\.clone\(\)/);
  assert.ok(!app.includes('assembleTextRuns('));
});

test('upstream Traditional Chinese OFL font remains in repository',()=>{
  const licence=read('translations/zh-tw/licenses/LXGWWenKaiTC-OFL.txt');
  assert.match(licence,/SIL OPEN FONT LICENSE/i);
  assert.match(web,/LXGWWenKaiTC-Regular\.ttf/);
});
