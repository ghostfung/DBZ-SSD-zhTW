import test from 'node:test';
import assert from 'node:assert/strict';
import {LiveHdMaps} from '../web/hd-map-live.mjs';
const image={width:512,height:512,rgba:new Uint8Array(512*512*4)};
const entry={id:'map',blocks:[4,4],tiles:'maps/map/t{x}_{y}.png'};
const create=()=>{
 let scheduled=null,canceled=[];
 const idle=cb=>{scheduled=cb;return 12;};
 const cancelIdle=h=>{canceled.push(h);scheduled=null;};
 const g={
   current:[{index:0,tileX:1,tileY:1,ready:true,
     art:'maps/map/t1_1.png'}],
   hd_mode:()=>true,
   set_map_index:()=>{},
   map_matches(){return JSON.stringify(this.current);},
   map_probe:()=>JSON.stringify({checked:100,different:0,reason:'verified'}),
   hd_map_pixels:()=>800
 };
 let calls=[];
 const asset={has:name=>name.startsWith('maps/map/')};
 const live=new LiveHdMaps(()=>{},async (_source,name)=>{
   calls.push(name);return image;
 },{idle,cancelIdle});
 live.setCatalog({items:[entry]},asset);
 live.attach(g);
 return {live,g,calls,asset,fire:deadline=>scheduled?.(deadline),
   scheduled:()=>scheduled,canceled};
};
test('preloads only after exact ROM/BG2 match and ready visible tile',async()=>{
 const t=create();
 t.live.pulse(true);
 assert.equal(typeof t.scheduled(),'function');
 t.fire({timeRemaining:()=>24});
 await new Promise(resolve=>setTimeout(resolve,0));
 assert.equal(t.calls.length,1);
 assert.ok(t.calls[0].startsWith('maps/map/t'));
 assert.equal(t.live.metrics.prefetchCount,1);
 assert.equal(t.live.cache.size,1);
 assert.ok(t.live.cacheBytes<=12*1024*1024);
});
test('cannot prefetch if current tile is missing or idle budget is tiny',async()=>{
 const t=create();
 t.g.current[0].ready=false;
 t.live.pulse(true);
 assert.equal(t.scheduled(),null);
 // The pending foreground read must finish before idle prefetch is eligible.
 await new Promise(resolve=>setTimeout(resolve,0));
 t.g.current[0].ready=true;
 t.live.pulse(true);
 assert.ok(t.scheduled());
 t.fire({timeRemaining:()=>1});
 await new Promise(resolve=>setTimeout(resolve,0));
 assert.equal(t.calls.length,0);
});
test('stale scene cannot keep a tile decoded after transition',async()=>{
 const t=create();
 let unblock;
 t.live.decode=()=>new Promise(resolve=>{unblock=resolve;});
 t.live.pulse(true);
 t.fire({timeRemaining:()=>24});
 t.g.current=[]; // PPU has moved to an unverified new scene
 t.live.pulse(true);
 unblock(image);
 await new Promise(resolve=>setTimeout(resolve,0));
 assert.equal(t.live.cache.size,0);
 assert.equal(t.live.metrics.prefetchCount,0);
});
