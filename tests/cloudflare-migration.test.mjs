import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,statSync} from 'node:fs';
import {
  BUNDLE_MAGIC,BUNDLE_FORMAT,createMigrationBundle,readMigrationBundle
} from '../web/migration-bundle.mjs';
const state=n=>new Blob([new Uint8Array([1,2,3,n])]);
const record=n=>({
  slot:n,savedAt:1764000000000+n,romKey:'12345678:87654321',
  state:state(n),thumbnail:n%2?new Blob(['preview'],{type:'image/webp'}):null
});
test('one-slot bundle roundtrips no WASM and retains ROM fingerprint',async()=>{
  const {blob,count}=createMigrationBundle([record(3)]);
  assert.equal(count,1);
  const restored=await readMigrationBundle(blob);
  assert.equal(restored.length,1);
  assert.equal(restored[0].slot,3);
  assert.equal(restored[0].romKey,'12345678:87654321');
  assert.deepEqual([...new Uint8Array(await restored[0].state.arrayBuffer())],[1,2,3,3]);
  assert.equal(await restored[0].thumbnail.text(),'preview');
});
test('all 10 slots archive and import without changing slot order or timestamps',async()=>{
  const {blob,count}=createMigrationBundle(Array.from({length:10},(_,i)=>record(10-i)));
  assert.equal(count,10);
  const decoded=await readMigrationBundle(blob);
  assert.deepEqual(decoded.map(x=>x.slot),[1,2,3,4,5,6,7,8,9,10]);
  for(const slot of decoded){
    assert.equal(slot.savedAt,1764000000000+slot.slot);
    assert.equal(slot.state.size,4);
  }
});
test('archive detects truncation, wrong signature, duplicate slot, invalid metadata',async()=>{
  const {blob}=createMigrationBundle([record(1),record(2)]);
  await assert.rejects(readMigrationBundle(blob.slice(0,blob.size-1)),/不完整/);
  await assert.rejects(readMigrationBundle(new Blob(['not-a-bundle'])),/有效|大小/);
  assert.throws(()=>createMigrationBundle([record(2),record(2)]),/重複/);
  assert.throws(()=>createMigrationBundle([{...record(3),romKey:''}]),/識別碼/);
  assert.throws(()=>createMigrationBundle([]),/找不到/);
});
test('Cloudflare Pages serves unchanged compiled game WASM and new archive module',()=>{
  const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
  const release=JSON.parse(read('web/offline-release.json'));
  const installed=read('web/offline-installer.mjs');
  const html=read('web/index.html');
  const app=read('web/app.mjs');
  assert.match(release.version,/^v[0-9]+(?:\.[0-9]+)*$/);
  assert.equal(release.wasmSize,statSync(new URL('../web/pkg/ssd_web_bg.wasm',import.meta.url)).size);
  assert.ok(release.files.includes('migration-bundle.mjs'));
  assert.ok(installed.includes('migration-bundle.mjs'));
  assert.ok(installed.includes("OFFLINE_RELEASE='"+release.version+"'"));
  for(const id of ['backup-bundle-import','backup-bundle-export']){
    assert.ok(html.includes('id="'+id+'"'));
    assert.ok(app.includes("$('"+id+"')"));
  }
  assert.ok(html.includes('offline-installer.mjs?offline='+release.version));
  assert.ok(app.includes('loadOfflineWasm(wasmUrl)'));
  assert.ok(read('web/_headers').includes('Content-Type: application/wasm'));
});
