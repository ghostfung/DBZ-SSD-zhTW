import test from 'node:test';
import assert from 'node:assert/strict';
import {adjacentMapArt} from '../web/hd-map-prefetch.mjs';
const entry={blocks:[4,4],tiles:'maps/map/t{x}_{y}.png'};
const has=()=>true;
test('prefetches max two adjacent tiles only after validated visible map art ready',()=>{
 const matches=[{index:3,tileX:1,tileY:1,ready:true}];
 const out=adjacentMapArt(matches,entry,has,2,[1,0]);
 assert.equal(out.length,2);
 assert.ok(out.every(m=>m.index===3&&m.x>=0&&m.y>=0&&
  m.x<4&&m.y<4&&m.art==='maps/map/t'+m.x+'_'+m.y+'.png'));
 assert.equal(out[0].x,2); // forward direction first
});
test('does not guess from unverified/mixed scene, unready or missing ZIP art',()=>{
 assert.deepEqual(adjacentMapArt([{index:2,tileX:1,tileY:1,ready:false}],entry,has),[]);
 assert.deepEqual(adjacentMapArt([{index:2,tileX:1,tileY:1,ready:true},
  {index:3,tileX:2,tileY:1,ready:true}],entry,has),[]);
 assert.deepEqual(adjacentMapArt([{index:2,tileX:1,tileY:1,ready:true}],entry,()=>false),[]);
 assert.deepEqual(adjacentMapArt([],entry,has),[]);
});
test('tile plan cannot cross map bounds, and never includes displayed coordinates',()=>{
 const all=[{index:0,tileX:0,tileY:0,ready:true}];
 const out=adjacentMapArt(all,{blocks:[2,2],tiles:'maps/a{x}_{y}.png'},has,2);
 assert.ok(out.every(x=>x.x<2&&x.y<2&&!(x.x===0&&x.y===0)));
 assert.equal(out.length,2);
});
