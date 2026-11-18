import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {StoryFrameGate} from '../web/story-frame-gate.mjs';

const read = path => readFileSync(new URL('../'+path,import.meta.url),'utf8');
const engine=read('crates/ssd-catalogue/src/engine.rs');
const bridge=read('crates/ssd-web/src/lib.rs');
const hd=read('crates/ssd-hd/src/lib.rs');
const translations=read('web/zh-tw.tsv');

test('detect transient portrait dialogue (str2), not only narration (str3)',()=>{
  assert.match(engine,/matches!\(current\.table, 2 \| 3\)/);
  assert.match(engine,/string_address\(view, current\.table, index\)/);
  assert.match(engine,/string_key\(current\.table, u32::from\(index\)\)/);
  assert.match(bridge,/matches!\(page\.table, 2 \| 3\)/);
  assert.match(hd,/matches!\(page\.table, 2 \| 3\)/);
});
test('cache candidates by table and allow prior portrait page matching',()=>{
  assert.match(bridge,/story_candidates_for: Option<\(u8, u16, u16\)>/);
  assert.match(bridge,/let location = \(page\.table, page\.string, page\.base\)/);
  assert.match(bridge,/self\.hd_renderer\.visible_translation_score\(page\)/);
  assert.match(bridge,/now\.saturating_sub\(\*seen\) <= 18/);
  assert.match(translations,/str2:0x002\t[^\n]+/);
  assert.match(translations,/str2:0x002\/p2\t[^\n]+/);
});
test('a portrait conversation can flash when English exists but Chinese paint samples are still zero',()=>{
  // Real diagnostic: at frame 1846, current key str2:0x002/p2,
  // English "I'd offer to help" still on PPU; changed pixel samples = 0.
  const gate = new StoryFrameGate(10);
  gate.didPresent();
  assert.equal(gate.shouldDefer(1846,true,true),true);
  assert.equal(gate.shouldDefer(1848,false,true),false);
  assert.equal(gate.isHolding,false);
  assert.ok(bridge.includes('self.native_story_ascii_cells > 0'));
});
