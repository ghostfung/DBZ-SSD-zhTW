import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {LiveHdArt} from '../web/hd-live.mjs';

test('A to B to A reuses the locally decoded 2x image without another ZIP decode',async()=>{
  let active=[{index:0,id:'A',art:'bg/a.webp',ready:false}];
  let decodes=0, installs=0;
  const game={
    set_background_index(){},hd_mode(){return true;},
    background_matches(){return JSON.stringify(active);},
    set_background_art(){installs++;active[0].ready=true;}
  };
  const live=new LiveHdArt(()=>{},async()=>{
    decodes++;
    return {width:16,height:8,rgba:new Uint8Array(16*8*4)};
  });
  live.setCatalog({items:[{id:'A',art:'bg/a.webp'}]}, {readImage(){}});
  live.attach(game);
  live.pulse();
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(installs,1);
  assert.equal(decodes,1);
  active=[];live.pulse();
  active=[{index:0,id:'A',art:'bg/a.webp',ready:false}];live.pulse();
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(installs,2);
  assert.equal(decodes,1);
  assert.ok(live.decodedBytes>0);
});

test('decoded cache stays under 8 MiB and clears on different HD ZIP',()=>{
  const live=new LiveHdArt(()=>{});
  const make=(bytes)=>({rgba:new Uint8Array(bytes)});
  live.rememberArt('bg/a.webp',make(5*1024*1024));
  live.rememberArt('bg/b.webp',make(5*1024*1024));
  assert.ok(!live.decoded.has('bg/a.webp'));
  assert.ok(live.decoded.has('bg/b.webp'));
  assert.equal(live.decodedBytes,5*1024*1024);
  live.setCatalog({items:[]},null);
  assert.equal(live.decoded.size,0);
  assert.equal(live.decodedBytes,0);
});

test('PPU scan cadence accelerates in scenes with missing HD art, not during stable rendering',()=>{
  const rust=readFileSync(new URL('../crates/ssd-web/src/live_bg.rs',import.meta.url),'utf8');
  const app=readFileSync(new URL('../web/app.mjs',import.meta.url),'utf8');
  assert.match(rust,/let interval = if all_ready \{ 12 \} else \{ 4 \}/);
  assert.match(rust,/if !log\.complete \|\| !log\.vram_writes\.is_empty\(\)/);
  assert.match(app,/const interval = hdPending \|\| \(hdEnabled && \(liveFighters\.assets \|\| livePortraits\.assets \|\| liveSpecials\.assets \|\| liveMaps\.assets \|\| liveCards\.assets \|\| liveMode7\.assets\)\) \? 90 : 450/);
  assert.match(app,/hdFrameGate\.shouldDefer\(/);
  assert.match(app,/if \(!storyHold && !hdHold && !fighterHold && !portraitHold && !specialHold && !mapHold && !cardHold && !mode7Hold\)/);
  assert.match(app,/hdFrameGate\.didPresent\(pixels\)/);
  assert.match(app,/hdFrameGate\.reset\(\)/);
});
