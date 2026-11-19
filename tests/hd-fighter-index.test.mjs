import test from 'node:test';
import assert from 'node:assert/strict';
import {parseFighterIndex,readFighterCatalog} from '../web/hd-fighter-index.mjs';
const entry=(overrides={})=>({
  id:'battle-test',source_key:'ABCDEF1234',source_size:[16,24],
  file:'goku.png',palette_indexes_used:{1:'#ff0000',2:'#00ff00'},...overrides
});
test('official class2 source_key and palette bind to safe local ZIP art',()=>{
 const r=parseFighterIndex({assets_list:[entry()]},name=>name==='class2/goku.png');
 assert.equal(r.count,1);
 assert.equal(r.items[0].key,'000000abcdef1234');
 assert.deepEqual(r.items[0].palette[1],[255,0,0]);
 assert.deepEqual(r.items[0].palette[2],[0,255,0]);
 assert.equal(r.items[0].available,true);
 assert.equal(r.items[0].art,'class2/goku.png');
});
test('luma palette-transfer art is not treated as RGB art',()=>{
 const r=parseFighterIndex({assets_list:[entry({luma:true})]},()=>true);
 assert.equal(r.items[0].available,false);
});
test('no palette evidence, missing file and path traversal never activate an HD pose',()=>{
 const r=parseFighterIndex({assets_list:[
  entry({id:'unknown',palette_indexes_used:{}}),
  entry({id:'unsafe',file:'../escape.png'}),
  entry({id:'missing',file:'bad.webp'})
 ]},()=>false);
 assert.equal(r.count,2);
 assert.equal(r.ignored,1);
 assert.equal(r.items.every(x=>!x.available),true);
 assert.equal(r.noPalette,1);
});
test('local reader fetches only class2 index and checks ZIP directory entries',async()=>{
 const calls=[];
 const r=await readFighterCatalog({
  readJson:async path=>{calls.push(path);return {assets_list:[entry()]};},
  has:path=>path==='class2/goku.png'
 });
 assert.deepEqual(calls,['class2/index.json']);
 assert.equal(r.count,1);
});
test('exclude original upstream known wrong-character asset bindings',()=>{
 const result=parseFighterIndex({assets_list:[entry({
   id:'battle-zarbon-801',source_key:'b7d6b360fc00302f'
 })]},()=>true);
 assert.equal(result.items[0].available,false,
   'known Vegeta portrait pixels must not be displayed using Zarbon art');
});
test('reject malformed and oversized catalogue without creating a partial list',()=>{
 assert.throws(()=>parseFighterIndex({assets:[]}),/assets_list/);
 assert.throws(()=>parseFighterIndex({assets_list:Array(20001).fill({})}),/安全上限/);
});
