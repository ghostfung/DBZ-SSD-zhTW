import test from 'node:test';
import assert from 'node:assert/strict';
import {LocalHdAssets} from '../web/hd-local-assets.mjs';

function crc32(data){
  let c=0xffffffff;
  for(const n of data){c^=n;for(let i=0;i<8;i++)c=(c>>>1)^((c&1)?0xedb88320:0);}
  return (c^0xffffffff)>>>0;
}
function makeImage(name,content){
  const bytes=Uint8Array.from(content);
  const encoded=new TextEncoder().encode(name);
  const header=new Uint8Array(30+encoded.length);
  const v=new DataView(header.buffer);
  v.setUint32(0,0x04034b50,true);
  v.setUint16(4,20,true);
  v.setUint16(8,0,true); // STORED
  v.setUint32(14,crc32(bytes),true);
  v.setUint32(18,bytes.length,true);
  v.setUint32(22,bytes.length,true);
  v.setUint16(26,encoded.length,true);
  header.set(encoded,30);
  return {header,bytes};
}
const PNG=Uint8Array.from([137,80,78,71,13,10,26,10,...new Array(25).fill(1)]);
const PNG2=Uint8Array.from([137,80,78,71,13,10,26,10,...new Array(25).fill(2)]);
const WEBP=Uint8Array.from([...Buffer.from('RIFF'),8,0,0,0,...Buffer.from('WEBP'),...new Array(20).fill(3)]);
function fixture(){
  const paths=[['bg/one.png',PNG],['bg/two.png',PNG2],['battle/three.webp',WEBP]];
  const data=[];
  const entries=[];
  let pos=0;
  for(const [name,bytes] of paths){
    const {header}=makeImage(name,bytes);
    data.push(header,bytes);
    entries.push({name,start:pos,unpacked:bytes.byteLength,packed:bytes.byteLength,crc32:crc32(bytes),method:0});
    pos+=header.length+bytes.byteLength;
  }
  const blob=new Blob(data);
  let reads=0;
  const file={size:blob.size,slice(...args){reads++;return blob.slice(...args);}};
  return {file,verified:{bytes:file.size,entries},get reads(){return reads;}};
}

test('HD library indexes art from directory without touching ZIP payload',()=>{
  const f=fixture();
  const lib=new LocalHdAssets(f.file,f.verified);
  assert.equal(f.reads,0);
  assert.deepEqual(lib.search('bg/').map(x=>x.name),['bg/one.png','bg/two.png']);
  assert.equal(lib.search('three.webp')[0].name,'battle/three.webp');
  assert.equal(lib.search('absent').length,0);
  assert.equal(f.reads,0);
});

test('HD library reads one image on demand; second use is served by bounded cache',async()=>{
  const f=fixture();
  const lib=new LocalHdAssets(f.file,f.verified,{memoryLimit:PNG.byteLength+1});
  assert.deepEqual(await lib.readImage('bg/one.png'),PNG);
  const first=f.reads;
  assert.ok(first>=2);
  assert.deepEqual(await lib.readImage('bg/one.png'),PNG);
  assert.equal(f.reads,first);
  assert.equal(lib.readCount,1);
  assert.deepEqual(await lib.readImage('bg/two.png'),PNG2);
  assert.ok(lib.cacheBytes<=PNG.byteLength+1);
  assert.deepEqual(await lib.readImage('bg/one.png'),PNG);
  assert.equal(lib.readCount,3); // second file evicted the first one
});

test('HD library rejects non-image names and invalid pack size',async()=>{
  const f=fixture();
  const lib=new LocalHdAssets(f.file,f.verified);
  await assert.rejects(lib.readImage('../unknown.png'),/不存在/);
  assert.throws(()=>new LocalHdAssets(f.file,{...f.verified,bytes:1}),/不一致/);
  const bad=fixture();
  bad.verified.entries[0].crc32=123;
  await assert.rejects(new LocalHdAssets(bad.file,bad.verified).readImage('bg/one.png'),/CRC32/);
});
