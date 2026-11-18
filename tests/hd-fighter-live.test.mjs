import test from 'node:test';
import assert from 'node:assert/strict';
import {LiveHdFighters} from '../web/hd-fighter-live.mjs';

test('match the official class2 sprite and install decoded pixels only in current pose',async()=>{
 let ready=false,reads=0;
 const game={
   hd_mode:()=>true,
   set_fighter_index(json){assert.equal(JSON.parse(json).length,1);},
   fighter_matches:()=>JSON.stringify([{index:0,id:'goku',art:'class2/goku.webp',ready}]),
   set_fighter_art(index,w,h,rgba){assert.deepEqual([index,w,h,rgba.byteLength],[0,8,8,256]);ready=true;},
   hd_fighter_pixels:()=>0
 };
 const live=new LiveHdFighters(()=>{},async()=>{reads++;return {width:8,height:8,rgba:new Uint8Array(256)};});
 live.setCatalog({items:[{id:'goku',art:'class2/goku.webp'}]},{readImage(){}});
 live.attach(game);live.pulse();
 await new Promise(resolve=>setImmediate(resolve));
 assert.equal(ready,true);
 assert.equal(reads,1);
 live.pulse();
 assert.equal(reads,1);
});
test('changing game or ZIP during decode must not install stale character art',async()=>{
 let resolve;
 const pending=new Promise(done=>{resolve=done;});
 const old={hd_mode:()=>true,set_fighter_index(){},fighter_matches:()=>JSON.stringify([{index:1,id:'x',art:'class2/x.png',ready:false}]),
   set_fighter_art(){throw Error('stale picture installed');}};
 const live=new LiveHdFighters(()=>{},()=>pending);
 live.setCatalog({items:[{id:'x',art:'class2/x.png'}]},{readImage(){}});
 live.attach(old);live.pulse();live.detach();
 resolve({width:2,height:2,rgba:new Uint8Array(16)});
 await new Promise(done=>setImmediate(done));
 assert.equal(live.game,null);
});
test('failed decompression does not retry endlessly',async()=>{
 let attempts=0;
 const game={hd_mode:()=>true,set_fighter_index(){},
   fighter_matches:()=>JSON.stringify([{index:0,id:'x',art:'class2/x.png',ready:false}])};
 const live=new LiveHdFighters(()=>{},async()=>{attempts++;throw Error('bad PNG');});
 live.setCatalog({items:[{id:'x',art:'class2/x.png'}]},{readImage(){}});
 live.attach(game);live.pulse();
 await new Promise(resolve=>setImmediate(resolve));
 for(let i=0;i<8;i++)live.pulse();
 await new Promise(resolve=>setImmediate(resolve));
 assert.equal(attempts,2);
});
