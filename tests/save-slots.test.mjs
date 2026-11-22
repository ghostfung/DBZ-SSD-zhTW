import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {
  SLOT_COUNT,MAX_STATE_BYTES,assertSlotNumber,validStateBytes,
  listSaveSlots,readSaveSlot,writeSaveSlot
} from '../web/save-slots.mjs';

const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

function memoryIndexedDB(){
  const data=new Map();
  const db={
    objectStoreNames:{contains(name){return name==='slots';}},
    createObjectStore(){},
    close(){},
    transaction(){
      const tx={oncomplete:null,onabort:null,onerror:null,abort(){}};
      const dispatch=run=>{
        const req={result:undefined,error:null,onsuccess:null,onerror:null};
        queueMicrotask(()=>{
          try{req.result=run();req.onsuccess?.();queueMicrotask(()=>tx.oncomplete?.());}
          catch(error){req.error=error;req.onerror?.();tx.onabort?.();}
        });
        return req;
      };
      tx.objectStore=()=>({
        get:key=>dispatch(()=>data.get(key)),
        getAll:()=>dispatch(()=>Array.from(data.values())),
        put:item=>dispatch(()=>{if(db.failPut)throw Error('QuotaExceededError');data.set(item.slot,item);return item.slot;})
      });
      return tx;
    }
  };
  return {
    data,db,
    open(){
      const req={result:db,onupgradeneeded:null,onsuccess:null,onerror:null,onblocked:null};
      queueMicrotask(()=>{req.onupgradeneeded?.();req.onsuccess?.();});
      return req;
    }
  };
}
test('ten bounded slots accept real binary saves but reject out-of-range or empty files',()=>{
  assert.equal(SLOT_COUNT,10);
  assert.equal(assertSlotNumber(1),1);
  assert.equal(assertSlotNumber(10),10);
  for(const slot of [0,-1,11,1.5,NaN,'2'])assert.throws(()=>assertSlotNumber(slot));
  assert.throws(()=>validStateBytes(new Uint8Array(0)));
  assert.throws(()=>validStateBytes(new Uint8Array(MAX_STATE_BYTES+1)));
  assert.equal(validStateBytes(new Uint8Array([1])).byteLength,1);
});
test('read/write ten independent slots and atomically overwrite one',async()=>{
  const fake=memoryIndexedDB();const previous=globalThis.indexedDB;
  globalThis.indexedDB=fake;
  try{
    const thumb=new Blob(['thumbnail'],{type:'image/webp'});
    for(let slot=1;slot<=10;slot++){
      await writeSaveSlot(slot,new Uint8Array([slot,9,8]),{
        savedAt:1700000000000+slot,romKey:'ROM:IPS',thumbnail:thumb
      });
    }
    let list=await listSaveSlots();
    assert.equal(list.length,10);
    assert.equal(list[0].thumbnail.size,9);
    assert.equal((await readSaveSlot(10)).savedAt,1700000000010);
    await writeSaveSlot(4,new Uint8Array([88,77]),{
      savedAt:1700000001000,romKey:'ROM:IPS',thumbnail:null
    });
    assert.equal((await readSaveSlot(4)).state.size,2);
    assert.equal((await readSaveSlot(4)).savedAt,1700000001000);
    assert.deepEqual([...new Uint8Array(await (await readSaveSlot(1)).state.arrayBuffer())],[1,9,8]);
    list=await listSaveSlots();assert.equal(list.length,10);
  }finally{globalThis.indexedDB=previous;}
});
test('quota rejection keeps the previous slot and reports failure',async()=>{
  const fake=memoryIndexedDB();const prior=globalThis.indexedDB;
  globalThis.indexedDB=fake;
  try{
    await writeSaveSlot(1,new Uint8Array([1]),{savedAt:1700000000000,romKey:'ROM:IPS'});
    fake.db.failPut=true;
    await assert.rejects(writeSaveSlot(1,new Uint8Array([2]),{
      savedAt:1700000000001,romKey:'ROM:IPS'
    }),/QuotaExceededError/);
    const old=await readSaveSlot(1);
    assert.equal(old.savedAt,1700000000000);
    assert.deepEqual([...new Uint8Array(await old.state.arrayBuffer())],[1]);
  }finally{globalThis.indexedDB=prior;}
});
test('PWA preserves game modification button, legacy backup and no network save',()=>{
  const html=source('web/index.html'),app=source('web/app.mjs'),css=source('web/style.css');
  for(const text of ['儲存遊戲','讀取遊戲','備份紀錄','遊戲修改','匯入進度（.sav）','匯出進度（.sav）'])
    assert.ok(html.includes(text),text);
  for(const bit of [0,1,2,3,4,5,6,7,8,9])
    assert.equal([...html.matchAll(new RegExp('data-bit="'+bit+'"','g'))].length,1);
  assert.match(app,/await writeSaveSlot\(slot,state/);
  assert.match(app,/await applyLoadedSave\(new Uint8Array\(await record.state.arrayBuffer\(\)\)\)/);
  assert.match(app,/captureSaveThumbnail/);
  assert.match(app,/game\.save_state\(\)/);
  assert.match(app,/restoreActiveCheats\(\)/);
  assert.match(app,/window\.confirm\('Slot '/);
  assert.match(css,/\.save-slots-list/);
  assert.doesNotMatch(source('web/save-slots.mjs'),/fetch\(|XMLHttpRequest|navigator\.sendBeacon/);
  assert.match(html,/app\.mjs\?build=v53&ui=v62/);
});
