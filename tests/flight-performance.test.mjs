import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');

test('fast-flight pacing limits expensive HD catch-up bursts',()=>{
  const s=source('web/app.mjs');
  assert.match(s,/accumulator \+= Math\.min\(rafGap, STEP\*2\)/);
  assert.match(s,/while\(accumulator >= STEP && n < maxCatchup\)/);
  assert.match(s,/const maxCatchup=averageTickMs>=STEP\*\.90 \? 1 : 2/);
  assert.match(s,/if \(n\) drawFrame\(\)/);
  assert.match(s,/const perf=frameMetrics\.add/);
  assert.match(s,/FrameMetrics\.describe\(perf\)/);
});
test('background art can start loading within the same frame without 90 ms status scans',()=>{
  const js=source('web/app.mjs');
  const bg=source('crates/ssd-web/src/live_bg.rs');
  const lib=source('crates/ssd-web/src/lib.rs');
  assert.match(js,/if\(mask&64\)liveHd\.pulse\(true\)/);
  assert.match(bg,/pub fn has_pending_art\(&self\)->bool/);
  assert.match(lib,/pub fn hd_background_pending\(&self\)->bool/);
  assert.match(lib,/u32::from\(self\.live_backgrounds\.has_pending_art\(\)\) << 6/);
});
test('Mode7 verification is frame-local and rejects changes in VRAM or palette',()=>{
  const m=source('crates/ssd-web/src/live_mode7.rs');
  const lib=source('crates/ssd-web/src/lib.rs');
  assert.match(m,/negative_key:Option<\(u64,u64\)>/);
  assert.match(m,/frame_key:Option<\(u64,u64\)>/);
  assert.match(m,/pub\(crate\) fn needs_attention\(&mut self,log:&FrameLog\)/);
  assert.match(m,/self\.frame_key!=Some\(\(self\.matching_key,self\.matching_palette\)\)/);
  assert.match(lib,/self\.live_mode7\.is_active\(\)/);
  assert.match(m,/invalid_or_unsupported_planes_are_rechecked_only_on_identity_change/);
});
test('only ZIP-backed HD modules run in enabled A/B mode',()=>{
  const s=source('crates/ssd-web/src/lib.rs');
  for(const cat of ['hd_map_enabled','hd_card_enabled','hd_mode7_enabled']){
    assert.ok(s.includes('if self.'+cat+' && self.live_'));
  }
});
