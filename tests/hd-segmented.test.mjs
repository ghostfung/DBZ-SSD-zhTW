import test from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import {SegmentedBlob,writeSegmentedParts,PART_SIZE} from '../web/hd-storage.mjs';

function fakeParts({maxSize=PART_SIZE,failAt=-1}={}){
  const records=new Map();
  let writes=0;
  return {
    records,
    async getFileHandle(name,{create}={}){
      if(!records.has(name)&&!create)throw Error('NotFound');
      return {
        async createWritable(){
          let received=null;
          return {
            async write(blob){
              writes++;
              if(writes===failAt)throw Error('Safari transient error');
              if(blob.size>maxSize)throw Error('individual file too large');
              received=blob;
            },
            async close(){records.set(name,received);},
            async abort(){received=null;}
          };
        },
        async getFile(){
          if(!records.has(name))throw Error('not found');
          return records.get(name);
        }
      };
    }
  };
}

test('2 MiB OPFS chunks reconstruct byte-identical ZIP slices across part boundaries',async()=>{
  const original=randomBytes(PART_SIZE*2+741);
  const parts=fakeParts();
  let percent=0;
  const count=await writeSegmentedParts(parts,new Blob([original]),{onProgress:(done,total)=>{
    assert.ok(done<=total);
    percent=done/total;
  }});
  assert.equal(count,3);
  assert.equal(percent,1);
  assert.equal(parts.records.size,3);
  const files=[...parts.records.values()];
  assert.deepEqual(files.map(f=>f.size),[PART_SIZE,PART_SIZE,741]);
  const restored=new SegmentedBlob(files);
  assert.equal(restored.size,original.length);
  const chunks=[
    [0,3],[PART_SIZE-10,PART_SIZE+10],[PART_SIZE+10,2*PART_SIZE+30],
    [2*PART_SIZE-3,original.length],[0,original.length]
  ];
  for(const [from,to] of chunks){
    const actual=Buffer.from(await restored.slice(from,to).arrayBuffer());
    assert.deepEqual(actual,original.subarray(from,to),`slice ${from}..${to}`);
  }
});

test('writer surfaces the failing part and never commits an incomplete part',async()=>{
  const file=new Blob([new Uint8Array(PART_SIZE*2+19)]);
  const parts=fakeParts({failAt:2});
  await assert.rejects(writeSegmentedParts(parts,file),/第 2／3 段寫入失敗.*Safari transient error/);
  assert.equal(parts.records.size,1);
  assert.equal(parts.records.get('part-0000.bin').size,PART_SIZE);
});

test('writer aborts safely before first part when signalled',async()=>{
  const parts=fakeParts();
  const signal=new AbortController();
  signal.abort();
  await assert.rejects(writeSegmentedParts(parts,new Blob([new Uint8Array(128)]),{signal:signal.signal}),/已取消/);
  assert.equal(parts.records.size,0);
});
