import test from 'node:test';
import assert from 'node:assert/strict';
import {OFFICIAL_PACK} from '../web/hd-pack.mjs';
import {readSavedHdPack,clearSavedHdPack} from '../web/hd-storage.mjs';

function mockOrigin(metadata,bytes=OFFICIAL_PACK.bytes){
  const records = new Map([
    ['hd-pack-v0.3.9.json',new Blob([JSON.stringify(metadata)])],
    ['hd-pack-v0.3.9.zip',{size:bytes}]
  ]);
  const directory={
    async getFileHandle(name) {
      if(!records.has(name)) throw new Error('NotFoundError');
      return {async getFile(){return records.get(name);}};
    },
    async removeEntry(name){records.delete(name);}
  };
  const root={async getDirectoryHandle(name){assert.equal(name,'dbz-ssd-private');return directory;}};
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{storage:{async getDirectory(){return root;}}}});
  return records;
}

test('HD archive is recognised after re-opening an OPFS origin',async()=>{
  const records=mockOrigin({version:1,sha256:OFFICIAL_PACK.sha256,bytes:OFFICIAL_PACK.bytes,
    name:OFFICIAL_PACK.name,savedAt:'2026-11-18T00:00:00Z',count:11120,imageCount:11108});
  const result=await readSavedHdPack();
  assert.equal(result.file.size,OFFICIAL_PACK.bytes);
  assert.equal(result.metadata.count,11120);
  await clearSavedHdPack();
  assert.equal(records.size,0);
  assert.equal(await readSavedHdPack(),null);
});

test('OPFS archive with mismatching metadata or truncated size is not restored',async()=>{
  mockOrigin({version:1,sha256:'bad',bytes:OFFICIAL_PACK.bytes});
  assert.equal(await readSavedHdPack(),null);
  mockOrigin({version:1,sha256:OFFICIAL_PACK.sha256,bytes:OFFICIAL_PACK.bytes},OFFICIAL_PACK.bytes-1);
  assert.equal(await readSavedHdPack(),null);
});
