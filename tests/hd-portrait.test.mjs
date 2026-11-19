import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {parsePortraitIndex} from '../web/hd-portrait-index.mjs';
const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('class1 index validates the author’s exact portrait stream and half',()=>{
  const entry={id:'face-example',source:'rom 16:C60A half 1 (32x32, 4bpp)',
    file:'face-example.webp',palette_colours:{'1':'#FFFFFF','2':'#336699'}};
  const result=parsePortraitIndex({assets:[entry]},name=>name==='class1/face-example.webp');
  assert.equal(result.count,1);
  assert.equal(result.items[0].source,0x16c60a);
  assert.equal(result.items[0].half,1);
  assert.equal(result.items[0].art,'class1/face-example.webp');
  assert.deepEqual(result.items[0].palette[2],[0x33,0x66,0x99]);
  assert.equal(result.items[0].available,true);
});

test('reject unsupported sources, unsafe paths and unknown files',()=>{
  const asset=(id,source,file)=>({id,source,file});
  const result=parsePortraitIndex({assets:[
    asset('other','rom 09:8000 half 0','no.png'),
    asset('escape','rom 16:8000 half 0','../secret.png'),
    asset('invalid','rom 16:8000 half 2','invalid.png'),
    asset('missing','rom 16:8000 half 0','missing.png')
  ]},()=>false);
  assert.equal(result.ignored,3);
  assert.equal(result.missing,1);
  assert.equal(result.count,1);
  assert.equal(result.items[0].available,false);
  assert.throws(()=>parsePortraitIndex({}),/缺少 assets/);
});

test('portrait integration is local-only, after fighter art and before native Chinese',()=>{
  const bridge=source('crates/ssd-web/src/lib.rs');
  const graphics=source('crates/ssd-web/src/live_portraits.rs');
  const app=source('web/app.mjs');
  const ui=source('web/hd-import-ui.mjs');
  const index=source('web/index.html');
  assert.match(graphics,/ssd_catalogue::graphics::lz_decode/);
  assert.match(graphics,/tile_key\(&bytes\)/);
  assert.match(graphics,/current_key\(&log\.vram,p/);
  assert.match(graphics,/p\.layer==0 \|\| p\.bpp!=4/);
  assert.ok(bridge.indexOf('self.live_fighters.composite(')<
    bridge.indexOf('self.live_portraits.composite('));
  assert.ok(bridge.indexOf('self.live_portraits.composite(')<
    bridge.indexOf('self.hd_renderer.paint_native_translation('));
  assert.match(ui,/readPortraitCatalog\(source\)/);
  assert.match(app,/livePortraits\.pulse\(\)/);
  assert.match(app,/livePortraits\.detach\(\)/);
  assert.match(index,/id="portrait-hd-toggle"/);
  assert.match(source('web/hd-portrait-live.mjs'),/this\.decoder\(this\.assets,match\.art\)/);
  assert.doesNotMatch(app,/fetch\(.+class1/);
});
