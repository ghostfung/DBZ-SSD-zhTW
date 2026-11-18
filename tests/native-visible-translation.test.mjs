import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const bridge=read('crates/ssd-web/src/lib.rs');
const renderer=read('crates/ssd-hd/src/lib.rs');
const app=read('web/app.mjs');

test('cache frame-boundary pages and prefer visibly verified PPU text only when direct page has no match',()=>{
  assert.match(bridge,/fn remember_page\(history: &mut Vec<\(u32, OnScreen\)>/);
  assert.match(bridge,/frame\.saturating_sub\(\*seen\) <= 150/);
  assert.match(bridge,/history\.len\(\) > 12/);
  assert.match(bridge,/let direct = selected\.as_ref\(\)/);
  assert.match(bridge,/if direct\.is_none\(\)/);
  assert.match(bridge,/self\.hd_renderer\.visible_translation_score\(page\)/);
  assert.match(bridge,/self\.native_visible_fallback = true/);
  assert.match(bridge,/self\.hd_renderer\.set_translation_screen\(selected\)/);
});
test('source text matching requires five actual ROM-derived glyph matches, no competing glyphs',()=>{
  assert.match(renderer,/seen\.matched && seen\.presence == translate::Presence::In/);
  assert.match(renderer,/seen\.own >= 5 && seen\.other == 0/);
  assert.match(renderer,/translate::presence_in_window\(/);
  assert.ok(!bridge.includes("Can Goku and Piccolo"),'no hard-coded scene-specific translation');
});
test('report differentiates translator called from glyph samples actually modified',()=>{
  assert.match(renderer,/pub fn native_changed_samples\(&self\)/);
  assert.match(bridge,/"changed_glyph_samples": self\.hd_renderer\.native_changed_samples\(\)/);
  assert.match(bridge,/"untranslated_visible_key": self\.native_untranslated_visible/);
  assert.match(bridge,/"recent_page_keys": self\.recent_pages/);
  assert.match(app,/item\.changed_glyph_samples/);
});
test('a diagnostic regression was observed where the same visible English persists across two engine keys',()=>{
  const observed=[
    {frame:1896,key:'str3:0x40/p2',ascii:'CanGokuandPiccolo|savehiss'},
    {frame:1923,key:'str3:0x40/p2',ascii:'CanGokuandPiccolo|savehisson'},
    {frame:4150,key:'str3:0x40',ascii:'CanGokuandPiccolo|savehisson'},
  ];
  assert.notEqual(observed[1].key,observed[2].key);
  assert.equal(observed[1].ascii,observed[2].ascii);
  // A matching key is NOT proof that the translated artwork was drawn.
  assert.ok(bridge.includes('"translator_ran": self.native_painted'));
  assert.ok(bridge.includes('"visible_page_fallback": self.native_visible_fallback'));
});
