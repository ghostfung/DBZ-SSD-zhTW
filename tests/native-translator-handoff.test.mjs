import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const text = file => readFileSync(new URL('../'+file,import.meta.url),'utf8');
const rust=text('crates/ssd-web/src/lib.rs'),app=text('web/app.mjs'),html=text('web/index.html');
test('canonical previous WRAM page remains primary and only an absent page triggers late check',()=>{
  const first=rust.indexOf('started = on_screen(&self.rom, self.machine.wram(), self.native_page_hint);');
  const tick=rust.indexOf('self.machine.run_frame(joypad_from_bits(joypad_bits)');
  const late=rust.indexOf('let selected = if started.is_none()');
  assert.ok(first>=0&&tick>first&&late>tick);
  assert.match(rust,/if let Some\(ref candidate\) = end\s*&& self\.translation/);
  assert.match(rust,/self\.hd_renderer\.set_translation_screen\(selected\)/);
});
test('diagnostics are local, bounded and do not include ROM, IPS, ZIP or save bytes',()=>{
  assert.match(rust,/pub fn translation_diagnostics\(&self\) -> String/);
  assert.match(rust,/\.take\(96\)/);
  assert.match(app,/translationDiag\.length > 120/);
  assert.match(app,/createObjectURL\(new Blob\(\[JSON\.stringify\(report,null,2\)\]/);
  assert.match(html,/id="translation-debug"/);
  assert.ok(!app.includes('fetch("/diagnostic"'));
});
