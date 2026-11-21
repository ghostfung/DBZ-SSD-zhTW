import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {HdSceneTrace} from '../web/hd-coverage-runtime.mjs';

const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('cutscene fallback uses bounded upstream field reconstruction, not old-frame HD',()=>{
  const native=read('crates/ssd-hd/src/lib.rs');
  const matcher=read('crates/ssd-web/src/live_fighters.rs');
  assert.match(native,/pub fn live_field_fighter_poses\(/);
  assert.match(native,/art::poses::refine_field\(fields, &sprites, log, &view, &mut known\)/);
  assert.match(native,/fields\.len\(\) > 12/,'bounded OAM grouping on iPhone');
  assert.match(native,/!log\.vram_writes\.is_empty\(\) \|\| !log\.oam_writes\.is_empty\(\)/);
  assert.match(native,/has_art\(pose\.key, pose\.w as u16, pose\.h as u16\)/);
  assert.match(matcher,/self\.should_try_refinement\(missing_oam\)/);
  assert.match(matcher,/ssd_hd::live_field_fighter_poses\(log,&mut has_battle_art\)/);
  assert.match(matcher,/entries\[i\]\.id\.starts_with\("battle-"\)/);
  assert.match(matcher,/entries\[i\]\.id\.starts_with\("field-fg-"\)/);
  assert.match(matcher,/choose_pose\(&self\.entries,\s*indices,\s*&pose\)/);
  assert.match(matcher,/pose\.oam_bits & \*owned/);
  assert.doesNotMatch(matcher,/last_hd|last_pose|retain_previous/i);
  assert.match(matcher,/had_primary && pose\.oam_bits & missing_oam == 0/);
  assert.match(matcher,/fallback_accepted \+= 1/);
  assert.match(matcher,/source_key_unmatched/);
  assert.match(matcher,/palette_mismatch/);
});

test('scene report includes only compact verified source hashes and fallback counters',()=>{
  const t=new HdSceneTrace({stepFrames:1,maxSamples:3});
  const game={
    hd_active:()=>true, frame_count:()=>1,
    fighter_matches:()=>JSON.stringify([]),
    fighter_probe:()=>JSON.stringify({
      poses:5,keyHits:0,fallbackPoses:2,fallbackKeyHits:1,
      unmatchedFrameBytes:[99,102],unindexedKeys:[
        '0123456789abcdef/36x24','malformed key','fedcba9876543210/80x40','deadbeef00001111/9x9'
      ],reason:'來源鍵值未命中官方 class2 素材'
    }),
    hd_fighter_pixels:()=>0
  };
  assert.equal(t.capture(game),true);
  const p=t.report().samples[0].fighter.probe;
  assert.equal(p.fallbackPoses,2);
  assert.equal(p.fallbackKeyHits,1);
  assert.deepEqual(p.unindexedKeys,[
    '0123456789abcdef/36x24','fedcba9876543210/80x40','deadbeef00001111/9x9'
  ]);
  assert.equal('unmatchedFrameBytes' in p,false);
  assert.equal(p.reason,'來源鍵值未命中官方 class2 素材');
});
