import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {parseVariantIndex} from '../web/hd-variant-index.mjs';
const fighters=[
  {id:'battle-goku-ssj-000',available:true,luma:false},
  {id:'battle-luma',available:true,luma:true}
];
const art='battle-goku-ssj-000',file='battle-goku-ssj-000.png';
const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
test('matches the exact official class2 pose ID, checked margin and local file',()=>{
 const res=parseVariantIndex({variants:[{art,when:'always',margin:76,file}]},fighters,()=>true);
 assert.equal(res.count,1);
 assert.deepEqual(res.byId.get(art),{art:'variants/'+file,margin:76});
});
test('invalid, ambiguous, missing, conditional and greyscale variants never render',()=>{
 const variants=[
  {art,when:'always',margin:76,file},
  {art,when:'always',margin:76,file},
  {art,when:'kaioken',margin:76,file},
  {art:'battle-luma',when:'always',margin:76,file},
  {art,when:'always',margin:75,file},
  {art,when:'always',margin:76,file:'../escape.png'}
 ];
 const r=parseVariantIndex({variants},fighters,()=>true);
 assert.equal(r.count,1);assert.equal(r.conditional,1);
 assert.equal(r.ignored,4);
 const missing=parseVariantIndex({variants:[{art,when:'always',margin:76,file}]},fighters,()=>false);
 assert.equal(missing.count,0);assert.equal(missing.missing,1);
});
test('WASM variant path verifies exact size and preserves unrelated OAM/UI',()=>{
 const matcher=source('crates/ssd-web/src/live_fighters.rs');
 assert.match(matcher,/add_variant\(&mut self/);
 assert.match(matcher,/entry\.width\)\*2\+usize::from\(variant\.margin\)/);
 assert.match(matcher,/variant_overhang=margin>0 && owner==255/);
 assert.match(matcher,/layer_ids\[p\]<=2 && y<176/);
 assert.match(source('crates/ssd-web/src/lib.rs'),/pub fn set_fighter_variant/);
});
