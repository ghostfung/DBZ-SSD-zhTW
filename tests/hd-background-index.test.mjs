import test from 'node:test';
import assert from 'node:assert/strict';
import {parseBackgroundIndex,readBackgroundCatalog} from '../web/hd-background-index.mjs';

const layer={id:'dash-red',key:'a1b2c3d4e5f60708',bg:1,mode:1,
  map_base:24576,char_base:12288,size:0,rows:[0,32],fill:true,file:'dash-red.webp'};
test('reads original upstream bg/index.json schema and links to exact art path',async()=>{
  const source={readJson:async name=>{
    assert.equal(name,'bg/index.json');
    return {layers:[layer,{...layer,id:'dash-night',file:'dash-night.webp',key:'000000000000000f'}]};
  },has:name=>name==='bg/dash-red.webp'};
  const result=await readBackgroundCatalog(source);
  assert.equal(result.count,2);
  assert.equal(result.items[0].art,'bg/dash-red.webp');
  assert.equal(result.items[0].key,layer.key);
  assert.equal(result.items[0].mapBase,24576);
  assert.equal(result.items[0].fill,true);
  assert.deepEqual(result.missing,['bg/dash-night.webp']);
});
test('missing, malformed or unsafe scene index fails closed',()=>{
  assert.throws(()=>parseBackgroundIndex({}),/layers/);
  assert.throws(()=>parseBackgroundIndex({layers:[{...layer,rows:[32,16]}]}),/格式錯誤/);
  assert.throws(()=>parseBackgroundIndex({layers:[{...layer,file:'../outside.webp'}]}),/格式錯誤/);
  assert.throws(()=>parseBackgroundIndex({layers:[{...layer,key:'wrong key'}]}),/格式錯誤/);
});
