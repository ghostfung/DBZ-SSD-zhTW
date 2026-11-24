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
 assert.match(release.version,/^v[0-9]+(?:\.[0-9]+)*$/);
 assert.ok(release.wasmSize>1024);
});

test('Pages manifest starts at canonical root and preserves the PWA identity',()=>{
 const manifest=JSON.parse(read('web/manifest.webmanifest'));
 assert.equal(manifest.start_url,'./');
 assert.equal(manifest.id,'./');
 assert.equal(manifest.scope,'./');
});
test('service worker sanitizes previously redirected cached HTML without deleting data',async()=>{
 const vm=await import('node:vm');
 const events={};
 const oldResponse=new Response('<!doctype html><title>cached game</title>',{headers:{'Content-Type':'text/html','Content-Encoding':'gzip','Content-Length':'123'}});
 Object.defineProperty(oldResponse,'redirected',{value:true});
 const requests=[];
 const scope='https://example.test/';
 const ctx={URL,Response,Headers,fetch:async()=>{throw Error('offline');},
  self:{registration:{scope},location:{origin:'https://example.test'},addEventListener:(k,v)=>events[k]=v},
  caches:{open:async name=>({match:async req=>{
   requests.push([name,typeof req==='string'?req:req.url]);
   return name==='dbz-ssd-offline-control-v1'?new Response(JSON.stringify({version:'v63'})):oldResponse.clone();
  }})}};
 vm.runInNewContext(read('web/service-worker.js'),ctx);
 for(const url of [scope,scope+'index.html']){
  let reply;
  events.fetch({request:{method:'GET',url,mode:'navigate'},respondWith:p=>reply=p});
  const response=await reply;
  assert.equal(response.redirected,false);
  assert.equal(response.status,200);
  assert.equal(response.headers.get('content-encoding'),null);
  assert.match(await response.text(),/cached game/);
 }
 assert.ok(requests.some(([name,url])=>name==='dbz-ssd-offline-v63'&&url===scope+'index.html'));
});

test('v64 checks each launcher entry and never reloads during gameplay',()=>{
 const installer=read('web/offline-installer.mjs'),app=read('web/app.mjs'),css=read('web/style.css');
 assert.match(installer,/force:true,online:/);
 assert.ok(installer.includes("await applyAutomatically(check.version)"));
 assert.ok(installer.includes("classList.contains('playing')"));
 assert.ok(installer.includes("classList.contains('launching')"));
 assert.ok(installer.includes("await activatePendingUpdate();\n  window.location.reload();"));
 assert.ok(app.includes("if(document.body.classList.contains('offline-preparing'))return;"));
 assert.ok(css.includes("justify-content:flex-start;\n align-items:center;min-height:0"));
 assert.ok(css.includes("position:static;align-self:flex-end;order:-2"));
});

test('EXP warning gates the first modifier opening until confirmation, persists per browser, and cancellation does not acknowledge it',()=>{
 const app=read('web/app.mjs'),html=read('web/index.html');
 assert.match(html,/比克（魔童）與那霸（立巴）交戰之前/);
 assert.match(html,/不得超過 Lv\. 8/);
 assert.match(html,/Lv\. 9 或以上/);
 const start=app.indexOf('function openCheatPanel(){');
 const source=app.slice(start,app.indexOf("for(const control of document.querySelectorAll('[data-cheat]'))",start));
 function session(store){
   const nodes=new Map(),handlers=new Map();
   const $=id=>{
     if(!nodes.has(id))nodes.set(id,{hidden:true,focus(){},addEventListener(type,fn){handlers.set(id,fn)}});
     return nodes.get(id);
   };
   const localStorage={getItem:key=>store.get(key),setItem:(key,value)=>store.set(key,value)};
   const env=new Function('$','localStorage','document','sound','cancelAnimationFrame','requestAnimationFrame','refreshCheatPanel',
     "let game={},cheatPanelOpen=false,cheatNoticeAcknowledged=false,held=0,running=true,raf=1,lastTick=0,accumulator=0,speedNoticeOpen=false;const requestFrame=()=>{};const resetControls=()=>{};const CHEAT_NOTICE_KEY='dbz-ssd-cheat-exp-level8-warning-v1';\n"+source+"\nreturn {openCheatPanel,closeCheatPanel};")($,localStorage,{hidden:false},{reset(){}},()=>{},()=>1,()=>{});
   return {...env,$,handlers};
 }
 const store=new Map(),first=session(store);
 first.openCheatPanel();
 assert.equal(first.$('cheat-warning-panel').hidden,false);
 assert.equal(first.$('cheats-panel').hidden,true);
 first.handlers.get('cheat-warning-cancel')();
 assert.equal(store.size,0);
 assert.equal(first.$('cheat-warning-panel').hidden,true);
 first.openCheatPanel();
 first.handlers.get('cheat-warning-confirm')();
 assert.equal(store.size,1);
 assert.equal(first.$('cheat-warning-panel').hidden,true);
 assert.equal(first.$('cheats-panel').hidden,false);
 first.closeCheatPanel();
 first.openCheatPanel();
 assert.equal(first.$('cheat-warning-panel').hidden,true);
 const next=session(store);
 next.openCheatPanel();
 assert.equal(next.$('cheat-warning-panel').hidden,true);
 assert.equal(next.$('cheats-panel').hidden,false);
});
