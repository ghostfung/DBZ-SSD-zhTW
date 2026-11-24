import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createMigrationBundle,readMigrationBundle} from '../web/migration-bundle.mjs';
import {thumbnailDataUrl} from '../web/save-slots.mjs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jf9sAAAAASUVORK5CYII=','base64');
test('all ten slots retain independent PNG bytes and renderable data URLs after archive restore',async()=>{
 const slots=Array.from({length:10},(_,i)=>({slot:i+1,savedAt:1764000000000+i,romKey:'deadbeef:10203040',state:new Blob([Uint8Array.of(i,4,5)]),thumbnail:new Blob([png],{type:'image/png'})}));
 const packed=createMigrationBundle(slots);
 const restored=await readMigrationBundle(packed.blob);
 assert.equal(restored.length,10);
 for(let i=0;i<10;i++){
  assert.equal(restored[i].slot,i+1);
  assert.deepEqual(new Uint8Array(await restored[i].state.arrayBuffer()),Uint8Array.of(i,4,5));
  assert.deepEqual(Buffer.from(await restored[i].thumbnail.arrayBuffer()),png);
  assert.equal(await thumbnailDataUrl(restored[i].thumbnail),'data:image/png;base64,'+png.toString('base64'));
 }
 assert.equal(await thumbnailDataUrl(null),null);
 assert.equal(await thumbnailDataUrl(new Blob(['invalid'],{type:'image/png'})),null);
 assert.equal(await thumbnailDataUrl(new Blob([png],{type:'application/octet-stream'})),'data:image/png;base64,'+png.toString('base64'));
});
test('backup menu only exposes complete archive operations and valid DOM IDs',()=>{
 const page=read('web/index.html'),app=read('web/app.mjs');
 assert.match(page,/一鍵備份全部存檔（\.dbzbackup）/);
 assert.match(page,/還原全部存檔（\.dbzbackup）/);
 assert.doesNotMatch(page,/state-file|backup-export|\.sav/);
 assert.doesNotMatch(app,/pendingImportBytes|state-file|backup-export/);
 for(const match of app.matchAll(/\$\('([^']+)'\)/g))
  assert.ok(page.includes('id="'+match[1]+'"'),'missing DOM id '+match[1]);
 assert.ok(app.includes('thumbnailDataUrl(record.thumbnail)'));
 assert.ok(!app.includes('createObjectURL(record.thumbnail)'));
 const release=JSON.parse(read('web/offline-release.json'));
 assert.equal(release.version,'v63');
 assert.equal(release.wasmSize,18559329);
});
