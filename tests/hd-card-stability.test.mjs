import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('card HD selection has no randomized 140-tile truncation or 4x4 redundant sampling',()=>{
  const rust=read('crates/ssd-web/src/live_cards.rs');
  assert.match(rust,/const MAX_VISIBLE:usize=32\*30/);
  assert.match(rust,/found\.sort_unstable_by_key\(/);
  assert.ok(rust.indexOf('found.sort_unstable_by_key(')<
    rust.indexOf('found.into_iter().take(MAX_VISIBLE)'));
  assert.doesNotMatch(rust,/const MAX_VISIBLE:usize=140/);
  assert.match(rust,/for y in \(0\.\.h\)\.step_by\(8\)/);
  assert.match(rust,/for x in \(0\.\.W\)\.step_by\(8\)/);
  assert.match(rust,/fn card_tiles_do_not_randomly_switch_back_to_snes_after_140_visible_tiles/);
});

test('Safari requests HD images on the same frame only when source art is missing',()=>{
  const app=read('web/app.mjs');
  const rust=read('crates/ssd-web/src/lib.rs');
  assert.match(rust,/pub fn hd_pending_mask\(&self\)->u32/);
  for(const [name,bit] of [
    ['liveFighters',1],['livePortraits',2],['liveSpecials',4],
    ['liveMaps',8],['liveCards',16],['liveMode7',32]
  ]){
    assert.ok(app.includes('if(mask&'+bit+')'+name+'.pulse(true)'),
      name+' should skip expensive JSON when ready');
  }
  assert.match(app,/const pendingMask=game\.hd_pending_mask\?\.\(\)/);
  assert.match(app,/const interval = hdPending \? 180 : hdEnabled \? 750 : 900/);
});

test('Safari reuses a full-frame pixel buffer rather than allocating each frame',()=>{
  const app=read('web/app.mjs');
  assert.match(app,/let frameBytes=null, frameImage=null/);
  assert.match(app,/frameBytes\.set\(rgba\)/);
  assert.match(app,/ctx\.putImageData\(frameImage,0,0\)/);
  assert.doesNotMatch(app,/putImageData\(new ImageData\(/);
  assert.ok(app.indexOf("      $('frames').textContent=")>
     app.indexOf('if(now-countWindow>=1000)'));
});
