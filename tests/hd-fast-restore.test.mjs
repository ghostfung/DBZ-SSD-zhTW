import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {OFFICIAL_PACK,restoreOfficialPackFast} from '../web/hd-pack.mjs';

// A sparse synthetic 484 MB ZIP: only its last 65,557 bytes and the
// directory are materialized. Real fast-path code must never scan the payload.
function sparseOfficialZip({badTail=false}={}){
  const size=OFFICIAL_PACK.bytes;
  const from=size-65557;
  const name=Buffer.from('hd/images/example.png');
  const directory=Buffer.alloc(46+name.length);
  directory.writeUInt32LE(0x02014b50,0);
  directory.writeUInt16LE(20,4);directory.writeUInt16LE(20,6);
  directory.writeUInt32LE(3,20);directory.writeUInt32LE(3,24);
  directory.writeUInt16LE(name.length,28);
  name.copy(directory,46);
  const directoryAt=from-100;
  const tail=Buffer.alloc(65557);
  if(!badTail){
    const end=tail.length-22;
    tail.writeUInt32LE(0x06054b50,end);
    tail.writeUInt16LE(1,end+8);
    tail.writeUInt16LE(1,end+10);
    tail.writeUInt32LE(directory.length,end+12);
    tail.writeUInt32LE(directoryAt,end+16);
  }
  const reads=[];
  return {
    size,
    reads,
    slice(start,end){
      reads.push({start,end});
      if(start===from) return new Blob([tail]);
      if(start===directoryAt&&end===directoryAt+directory.length)
        return new Blob([directory]);
      throw Error('讀取了 ZIP 中不應於快速啟動時掃描的內容');
    }
  };
}
function receipt(version=2,overrides={}){
  return {
    version,name:OFFICIAL_PACK.name,bytes:OFFICIAL_PACK.bytes,
    sha256:OFFICIAL_PACK.sha256,savedAt:'2026-11-21T18:00:00Z',
    imageCount:1,entries:1,count:version===1?1:231,...overrides
  };
}
test('v2 OPFS receipt reopens 484 MB ZIP using ONLY the directory, never full hash',async()=>{
  const file=sparseOfficialZip();
  const verified=await restoreOfficialPackFast(file,receipt(2));
  assert.equal(verified.restoreMethod,'trusted-local-index');
  assert.equal(verified.version,OFFICIAL_PACK.version);
  assert.equal(verified.entries[0].name,'hd/images/example.png');
  assert.equal(verified.imageCount,1);
  assert.ok(file.reads.length<=3);
  assert.ok(file.reads.reduce((s,r)=>s+(r.end-r.start||65557),0)<140000);
});
test('existing legacy v1 cached ZIP reopens without migration or full scan',async()=>{
  const file=sparseOfficialZip();
  const verified=await restoreOfficialPackFast(file,receipt(1));
  assert.equal(verified.count,1);
  assert.equal(verified.restoreMethod,'trusted-local-index');
});
test('reject bad metadata, incorrect item counts, truncated archive and ZIP damage',async()=>{
  for(const data of [
    receipt(2,{sha256:'tampered'}),receipt(2,{bytes:123}),
    receipt(2,{name:'fake.zip'}),receipt(2,{version:9}),
    receipt(2,{entries:2}),receipt(2,{imageCount:999}),
    receipt(2,{entries:undefined}),receipt(2,{imageCount:undefined})
  ]){
    await assert.rejects(restoreOfficialPackFast(sparseOfficialZip(),data));
  }
  await assert.rejects(restoreOfficialPackFast({size:OFFICIAL_PACK.bytes-1},receipt(2)));
  await assert.rejects(restoreOfficialPackFast(sparseOfficialZip({badTail:true}),receipt(2)),/ZIP/);
});
test('PWA fast-restores a healthy pack and performs complete SHA only on anomalies',()=>{
  const ui=readFileSync(new URL('../web/hd-import-ui.mjs',import.meta.url),'utf8');
  const storage=readFileSync(new URL('../web/hd-storage.mjs',import.meta.url),'utf8');
  const html=readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
  assert.ok(ui.includes('await restoreOfficialPackFast(cached.file,cached.metadata)'));
  assert.ok(ui.includes('checked=await verifyOfficialPack(cached.file'));
  assert.ok(ui.indexOf('await restoreOfficialPackFast(cached.file,cached.metadata)')
    <ui.indexOf('checked=await verifyOfficialPack(cached.file'));
  assert.ok(ui.includes('if(epoch!==selectionEpoch)return'));
  assert.ok(storage.includes('const stored=await handle.getFile()'));
  assert.ok(storage.includes('if(part.size!==expectedSize(i,data.bytes))return null'));
  assert.ok(storage.includes('await writeText(dir,META,JSON.stringify('));
  assert.match(html,/hd-import-ui\.mjs\?build=v50&ui=v55/);
});
