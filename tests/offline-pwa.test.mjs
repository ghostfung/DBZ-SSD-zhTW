import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {
  OFFLINE_CACHE,OFFLINE_FILES,OFFLINE_WASM_SIZE,inspectOfflineCache,
  installOfflineFiles,loadOfflineWasm
} from '../web/offline-installer.mjs';

const src=f=>readFileSync(new URL('../'+f,import.meta.url),'utf8');
const ROOT='https://example.test/dbz/';
const wasm=new Uint8Array(OFFLINE_WASM_SIZE);
wasm.set([0,97,115,109,1,0,0,0]);
function createFakeStorage({failAt=null}={}){
  const store=new Map(),counts={adds:0,fetches:0};
  function key(uri){return typeof uri==='string'?uri:uri.url;}
  const cache={
    async match(uri,{ignoreSearch=false}={}){
      const requested=new URL(key(uri));
      const found=[...store].find(([url])=>{
        const candidate=new URL(url);
        return ignoreSearch
          ?candidate.origin===requested.origin&&candidate.pathname===requested.pathname
          :url===requested.href;
      });
      return found?.[1]?.clone()??undefined;
    },
    async put(uri,response){store.set(key(uri),response.clone());},
    async delete(uri){return store.delete(key(uri));},
    async add(request){
      counts.adds++;
      if(failAt&&request.url.endsWith(failAt))throw Error('offline network failure');
      const body=request.url.endsWith('/pkg/ssd_web_bg.wasm')
        ?wasm:new TextEncoder().encode('mock file');
      store.set(request.url,new Response(body));
      counts.fetches++;
    }
  };
  return {storage:{open:async name=>{assert.equal(name,OFFLINE_CACHE);return cache;}},cache,counts,store};
}
test('manifest is complete static app only; large WASM always last',()=>{
  assert.ok(OFFLINE_FILES.length>=45);
  assert.equal(OFFLINE_FILES.at(-1),'pkg/ssd_web_bg.wasm');
  for(const file of ['index.html','app.mjs','hd-import-ui.mjs','offline-installer.mjs',
    'pkg/ssd_web.js','pkg/snippets/ssd-web-78d3277d0c70cc4b/inline0.js',
    'service-worker.js','zh-hk.tsv','zh-tw.tsv','style.css',
    'manifest.webmanifest','apple-touch-icon.png']){
    if(file==='service-worker.js')assert.ok(!OFFLINE_FILES.includes(file));
    else assert.ok(OFFLINE_FILES.includes(file),'missing '+file);
  }
  assert.equal(new Set(OFFLINE_FILES).size,OFFLINE_FILES.length);
  assert.ok(OFFLINE_FILES.every(s=>!s.includes('..')&&!s.includes('://')));
  assert.ok(OFFLINE_FILES.every(s=>!/\.sav|\.sfc|\.ips|hd-pack\.zip|rom\.bin/i.test(s)));
});
test('complete installation validates WASM, marks ready and never redownloads',async()=>{
  const fake=createFakeStorage(),progress=[];
  const installed=await installOfflineFiles({
    storage:fake.storage,root:ROOT,onProgress:(done,total,file)=>progress.push([done,total,file])
  });
  assert.deepEqual(installed,{ready:true,found:OFFLINE_FILES.length,total:OFFLINE_FILES.length});
  assert.equal(fake.counts.adds,OFFLINE_FILES.length);
  assert.equal(progress.at(-1)[0],OFFLINE_FILES.length);
  assert.ok((await inspectOfflineCache({storage:fake.storage,root:ROOT})).ready);
  const bytes=await loadOfflineWasm(ROOT+'pkg/ssd_web_bg.wasm?build=v53',{storage:fake.storage});
  assert.equal(bytes?.length,OFFLINE_WASM_SIZE);
  const second=await installOfflineFiles({storage:fake.storage,root:ROOT,online:()=>false});
  assert.equal(second.ready,true);
  assert.equal(fake.counts.adds,OFFLINE_FILES.length,'cached restart sent no requests');
});
test('missing one resource cannot be considered fully offline',async()=>{
  const fake=createFakeStorage({failAt:'/zh-tw.tsv'});
  await assert.rejects(installOfflineFiles({storage:fake.storage,root:ROOT}),/network failure/);
  assert.equal((await inspectOfflineCache({storage:fake.storage,root:ROOT})).ready,false);
});
test('offline worker serves cached static files with zero network; never intercepts WASM',async()=>{
  const listeners={},storage=createFakeStorage(),net=[];
  await storage.cache.put(ROOT+'index.html',new Response('<h1>offline app</h1>'));
  await storage.cache.put(ROOT+'app.mjs',new Response('export const x=1;'));
  const self={
    location:{origin:new URL(ROOT).origin},
    registration:{scope:ROOT},
    skipWaiting:async()=>{},clients:{claim:async()=>{}},
    addEventListener(type,listener){listeners[type]=listener;}
  };
  vm.runInNewContext(src('web/service-worker.js'),{
    self,URL,caches:storage.storage,
    fetch:async req=>{net.push(req.url);throw Error('offline');}
  });
  assert.ok(listeners.fetch&&listeners.install&&listeners.activate);
  const go=async(name,{mode='cors',cache='default'}={})=>{
    const request={method:'GET',url:ROOT+name,mode,cache};
    let result;
    listeners.fetch({request,respondWith:promise=>{result=promise;}});
    return result;
  };
  const js=await go('app.mjs?build=v53&ui=v64');
  assert.match(await js.text(),/export const x/);
  const html=await go('missing-route',{mode:'navigate'});
  assert.match(await html.text(),/offline app/);
  const reloaded=await go('index.html',{mode:'navigate',cache:'reload'});
  assert.match(await reloaded.text(),/offline app/,
    'reload in airplane mode must not bypass the offline navigation cache');
  assert.equal(await go('pkg/ssd_web_bg.wasm?build=v53'),undefined,
    'WASM request must never be intercepted');
  assert.deepEqual(net,[ROOT+'missing-route']);
});
test('page uses local WASM bytes and no longer unregisters worker on load',()=>{
  const html=src('web/index.html'),app=src('web/app.mjs'),worker=src('web/service-worker.js');
  assert.match(html,/id="offline-status-message"/);
  assert.match(html,/id="offline-retry"/);
  assert.match(html,/offline-installer\.mjs\?offline=v59/);
  assert.match(html,/app\.mjs\?build=v53&ui=v64/);
  assert.match(html,/style\.css\?build=v50&ui=v60/);
  assert.doesNotMatch(html,/registration\.unregister\(/);
  assert.match(app,/const offlineWasm=await loadOfflineWasm\(wasmUrl\)/);
  assert.match(app,/module_or_path:offlineWasm\?\?wasmUrl/);
  assert.match(worker,/c\.match\(request,\{ignoreSearch:true\}\)/);
  assert.doesNotMatch(worker,/\.clone\(/);
});
