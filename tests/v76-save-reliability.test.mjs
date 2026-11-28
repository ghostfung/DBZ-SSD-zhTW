import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {openSlotDB,transact,optionalThumbnail} from '../web/save-slots.mjs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');

test('stalled IndexedDB open returns an error and closes a late connection',async()=>{
 let closed=0;
 const request={result:{close(){closed++}}};
 await assert.rejects(openSlotDB({open(){return request}},5),/逾時/);
 request.onsuccess();
 assert.equal(closed,1);
});
test('blocked IndexedDB open reports actionable error and closes a late connection',async()=>{
 let closed=0;const req={result:{close(){closed++}}};
 const p=openSlotDB({open(){return req}},100);
 req.onblocked();
 await assert.rejects(p,/其他視窗/);
 req.onsuccess();assert.equal(closed,1);
});
test('stalled write aborts transaction, never reports a successful save',async()=>{
 let aborted=0,closed=0;
 const req={};
 const tx={objectStore(){return {}},abort(){aborted++}};
 const db={transaction(){return tx},close(){closed++}};
 const factory={open(){const r={result:db};queueMicrotask(()=>r.onsuccess());return r}};
 await assert.rejects(transact('readwrite',()=>req,{factory,timeoutMs:5}),/逾時.*尚未確認/);
 assert.equal(aborted,1);assert.equal(closed,1);
 req.result=1;req.onsuccess();tx.oncomplete();
});
test('missing, throwing or late thumbnail encoder does not block a valid state',async()=>{
 assert.equal(await optionalThumbnail(()=>new Promise(()=>{}),5),null);
 assert.equal(await optionalThumbnail(()=>{throw Error('encoder failed')},5),null);
 assert.equal(await optionalThumbnail(()=>Promise.reject(Error('memory')),5),null);
 const png=new Blob(['image'],{type:'image/png'});
 assert.equal(await optionalThumbnail(()=>Promise.resolve(png),100),png);
 assert.equal(await optionalThumbnail(()=>new Blob([]),100),null);
});
test('save UI asks for a visible second confirmation and supports recovery from failure',async()=>{
 const app=read('web/app.mjs'),html=read('web/index.html');
 const start=app.indexOf("$('save-slots-confirm').addEventListener('click',async()=>{");
 const end=app.indexOf('// Do not register a Service Worker.',start);
 const source=app.slice(start,end);
 let handler,choiceUpdates=0,writes=0,thumbs=0,closes=0,fail=false;
 const nodes=new Map();
 const $=id=>{if(!nodes.has(id))nodes.set(id,{hidden:true,textContent:'',addEventListener:(event,fn)=>handler=fn});return nodes.get(id)};
 const deps={game:{save_state:()=>Uint8Array.of(1,2)},slotAvailable:()=>true,currentRomKey:'ROM:IPS',currentSlotRecords:new Map([[1,{slot:1}]]),
  validStateBytes:b=>b,optionalThumbnail:async()=>{thumbs++;return null},captureSaveThumbnail:()=>{},writeSaveSlot:async()=>{writes++;if(fail)throw Error('QuotaExceededError')},updateSlotChoice:()=>{choiceUpdates++},closeSlotPicker:()=>{closes++},gameToast:()=>{}};
 const names=Object.keys(deps);
 const env=new Function('$',...names,"let slotSelection=1,slotMode='save',slotBusy=false,slotListReady=true,slotOverwrite=null;\n"+source+"\nreturn {state:()=>({slotBusy,slotListReady,slotOverwrite}),retry:()=>{slotListReady=true},choose:n=>{slotSelection=n;slotOverwrite=null}};")($,...Object.values(deps));
 await handler();assert.equal(writes,0);assert.equal(env.state().slotOverwrite,1);
 assert.match($('save-slots-feedback').textContent,/再次點/);
 await handler();assert.equal(writes,1);assert.equal(thumbs,1);assert.equal(closes,1);
 env.choose(2);fail=true;await handler();
 assert.equal(writes,2);assert.equal(env.state().slotBusy,false);assert.equal(env.state().slotListReady,false);
 assert.equal($('save-slots-retry').hidden,false);
 assert.match($('save-slots-feedback').textContent,/QuotaExceededError/);
 await handler();assert.equal(writes,2);
 env.retry();fail=false;await handler();assert.equal(writes,3);
 assert.ok(choiceUpdates>0);assert.match(html,/id="save-slots-retry"/);
 assert.match(app,/!running&&!cheatPanelOpen&&!speedNoticeOpen&&!slotMode&&!backupOpen/);
});
