import test from 'node:test';
import assert from 'node:assert/strict';
import {parseSpecialIndex} from '../web/hd-special-index.mjs';
const e=(x={})=>({
 id:'fgfx-local-test',source_key:'ABCDEF1234',source_size:[16,16],
 file:'light.png',palette_indexes_used:{},luma:true,...x
});
test('only exact-key class2 effect is eligible for greyscale source-pixel transfer',()=>{
 const f=parseSpecialIndex({assets_list:[e()]},2,n=>n==='class2/light.png');
 assert.equal(f.count,1);
 assert.equal(f.items[0].luma,true);
 assert.equal(f.items[0].available,true);
 assert.equal(f.items[0].kind,2);
});
test('class3 field source cannot bypass palette gate with luma',()=>{
 const f=parseSpecialIndex({assets_list:[e()]},1,()=>true);
 assert.equal(f.items[0].available,false);
 assert.equal(f.items[0].luma,true);
});
test('known wrong origin and missing local file stay denied',()=>{
 const wrong=parseSpecialIndex({assets_list:[e({
 id:'fgfx-695459fedb',source_key:'695459fedb6cecfd'
 })]},2,()=>true);
 assert.equal(wrong.items[0].available,false);
 const missing=parseSpecialIndex({assets_list:[e()]},2,()=>false);
 assert.equal(missing.items[0].available,false);
});
