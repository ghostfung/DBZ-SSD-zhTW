import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {HdSceneTrace} from '../web/hd-coverage-runtime.mjs';
const src=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
test('Safari shell caches JS but never intercepts WASM stream; browser and core IDs still agree',()=>{
 const worker=src('web/service-worker.js');
 assert.match(worker,/addEventListener\('fetch'/);
 assert.ok(worker.includes("url.pathname.endsWith('/pkg/ssd_web_bg.wasm')"));
 assert.match(src('web/app.mjs'),/loadOfflineWasm\(wasmUrl\)/);
 assert.doesNotMatch(src('web/index.html'),/registration\.unregister\(\)/);
 assert.match(src('web/index.html'),/app\.mjs\?build=v53/);
 assert.match(src('web/app.mjs'),/game\.browser_build_tag\?\.\(\)/);
 assert.match(src('crates/ssd-web/src/lib.rs'),/pub fn browser_build_tag\(&self\) -> String/);
});
test('opening samples and latest samples are retained without falsely treating emulation as presentation',()=>{
 const t=new HdSceneTrace({stepFrames:1,maxSamples:240});let frame=1;
 const game={hd_active:()=>true,frame_count:()=>frame};
 for(frame=1;frame<=301;frame++)t.capture(game,{displayedFrame:frame-1,held:frame===2,reasons:frame===2?['portrait']:[]});
 const samples=t.report().samples;
 assert.equal(samples.length,240);
 assert.equal(samples[0].frame,1);assert.equal(samples[119].frame,120);
 assert.equal(samples[120].frame,182);assert.equal(samples.at(-1).frame,301);
 assert.deepEqual(samples[1].presentation,{displayedFrame:1,held:true,reasons:['portrait']});
});
test('per-pose position is recorded, never raw sprite or ROM bytes',()=>{
 const t=new HdSceneTrace({stepFrames:1,maxSamples:3});
 t.capture({hd_active:()=>true,frame_count:()=>1,
 fighter_matches:()=>JSON.stringify([{id:'pose',key:'0123456789abcdef',ready:true,x:88,y:45,width:40,height:32,rgba:'do-not-export'}])});
 const item=t.report().samples[0].fighter.items[0];
 assert.deepEqual([item.x,item.y,item.width,item.height],[88,45,40,32]);
 assert.equal('rgba' in item,false);
});
