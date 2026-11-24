import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,statSync,readdirSync} from 'node:fs';
import vm from 'node:vm';
import {createMigrationBundle,readMigrationBundle} from '../web/migration-bundle.mjs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
const slot=n=>({
 slot:n,savedAt:1764000000000+n,romKey:'deadbeef:10203040',
 state:new Blob([new Uint8Array([0,97,115,109,n])]),thumbnail:null
});
test('legacy migration contains no game core and does not download WASM',()=>{
 const assets=readdirSync(new URL('../web/',import.meta.url));
 assert.deepEqual(assets.sort(),[
  'index.html','migration-bundle.mjs','migration.mjs','save-slots.mjs',
  'service-worker.js','vercel.json'
 ].sort());
 const page=read('web/index.html'),worker=read('web/service-worker.js');
 assert.ok(page.includes('一鍵備份全部存檔'));
 assert.ok(page.includes('dbz-ssd-zhtw.pages.dev'));
 assert.ok(!page.includes('app.mjs'));
 assert.ok(page.includes('id="slots"'));
 assert.ok(page.includes('請勿刪除主畫面'));
 const ui=read('web/migration.mjs');
 assert.ok(ui.includes('i<=SLOT_COUNT'));
 assert.ok(ui.includes('createMigrationBundle(valid)'));
 assert.ok(!ui.includes('fetch('));
 assert.ok(!worker.includes('ssd_web_bg.wasm'));
 assert.ok(worker.includes('dbz-ssd-migration-only-v1'));
 assert.ok(!worker.includes('caches.delete'));
 assert.ok(!worker.includes('indexedDB.deleteDatabase'));
});
test('portable complete save bundle is readable without a game emulator',async()=>{
 const result=createMigrationBundle([slot(8),slot(1),slot(6)]);
 const decoded=await readMigrationBundle(result.blob);
 assert.deepEqual(decoded.map(x=>x.slot),[1,6,8]);
 assert.equal((await decoded[1].state.arrayBuffer()).byteLength,5);
 assert.equal(decoded[2].romKey,'deadbeef:10203040');
});
test('migration worker intercepts navigations only and leaves existing game assets alone',async()=>{
 const listeners={},CACHE='dbz-ssd-migration-only-v1',requested=[];
 const contents=new Map([
  ['https://old.example/index.html',new Response('<p>moved</p>')],
  ['https://old.example/migration.mjs',new Response('export const moved=true')],
  ['https://old.example/migration-bundle.mjs',new Response('export const bundle=true')],
  ['https://old.example/save-slots.mjs',new Response('export const slots=true')]
 ]);
 const storage={open:async name=>{
  assert.equal(name,CACHE);
  return {
   addAll:async list=>{requested.push(...list);},
   match:async url=>contents.get(url)?.clone()
  };
 }};
 const self={
  registration:{scope:'https://old.example/'},
  location:{origin:'https://old.example'},
  skipWaiting:async()=>{},
  clients:{claim:async()=>{}},
  addEventListener:(type,fn)=>{listeners[type]=fn;}
 };
 vm.runInNewContext(read('web/service-worker.js'),{self,caches:storage,URL,
  fetch:async()=>{throw Error('must use local offline migration page');}});
 let install;
 listeners.install({waitUntil:p=>{install=p;}});
 await install;
 assert.equal(requested.length,4);
 assert.ok(requested.every(url=>!url.endsWith('.wasm')));
 let activated;
 listeners.activate({waitUntil:p=>{activated=p;}});
 await activated;
 const request=async(path,mode='cors')=>{
   let response;
   listeners.fetch({request:{method:'GET',url:'https://old.example/'+path,mode},
      respondWith:p=>{response=p;}});
   return response;
 };
 const page=await request('index.html','navigate');
 assert.match(await page.text(),/moved/);
 const js=await request('migration.mjs');
 assert.match(await js.text(),/moved=true/);
 assert.equal(await request('pkg/ssd_web_bg.wasm'),undefined);
 assert.equal(await request('some-old-game-file.mjs'),undefined);
});
