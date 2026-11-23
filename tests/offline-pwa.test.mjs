import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {
  OFFLINE_CACHE,OFFLINE_CONTROL,OFFLINE_FILES,OFFLINE_WASM_SIZE,
  OFFLINE_RELEASE,OFFLINE_INTERVAL_MS,installOfflineFiles,
  inspectOfflineCache,loadOfflineWasm,markInitialRelease,
  getActiveRelease,getPendingRelease,newerVersion,
  stageOfflineUpdate,activatePendingUpdate,checkReleaseOnceDaily
} from '../web/offline-installer.mjs';
const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const ROOT='https://example.test/dbz/';
const wasm=new Uint8Array(OFFLINE_WASM_SIZE);
wasm.set([0,97,115,109,1,0,0,0]);
function mockStore(){
  const all=new Map(),stats={adds:0,network:0},broken={path:null};
  const getCache=name=>{
    if(all.has(name))return all.get(name);
    const entries=new Map();
    const key=url=>typeof url==='string'?url:url.url;
    const cache={
      async match(url,{ignoreSearch=false}={}){
        const target=new URL(key(url));
        const pair=[...entries].find(([name])=>{
          const u=new URL(name);
          return ignoreSearch?u.origin===target.origin&&u.pathname===target.pathname
            :u.href===target.href;
        });
        return pair?.[1]?.clone();
      },
      async put(url,response){entries.set(key(url),response.clone());},
      async delete(url){return entries.delete(key(url));},
      async add(request){
        stats.adds++;
        if(broken.path&&request.url.endsWith(broken.path))throw Error('offline download failed');
        const value=request.url.endsWith('/pkg/ssd_web_bg.wasm')?wasm:
          new TextEncoder().encode('cached in '+name);
        entries.set(request.url,new Response(value));
        stats.network++;
      }
    };
    all.set(name,cache);
    return cache;
  };
  return {storage:{open:async name=>getCache(name)},cache:getCache,all,stats,broken};
}
async function install(mock){
  const result=await installOfflineFiles({storage:mock.storage,root:ROOT});
  await markInitialRelease({storage:mock.storage,root:ROOT});
  return result;
}
test('release manifest contains complete static app, excludes private player data',()=>{
  const m=JSON.parse(source('web/offline-release.json'));
  assert.equal(m.version,OFFLINE_RELEASE);
  assert.equal(m.wasmSize,OFFLINE_WASM_SIZE);
  assert.deepEqual(m.files,OFFLINE_FILES);
  assert.equal(OFFLINE_FILES.at(-1),'pkg/ssd_web_bg.wasm');
  assert.ok(OFFLINE_FILES.length>=48);
  for(const required of ['index.html','app.mjs','offline-installer.mjs','offline-release.json',
    'pkg/ssd_web.js','zh-tw.tsv','zh-hk.tsv','hd-import-ui.mjs',
    'pkg/snippets/ssd-web-78d3277d0c70cc4b/inline0.js'])assert.ok(OFFLINE_FILES.includes(required));
  assert.ok(!OFFLINE_FILES.includes('service-worker.js'));
  assert.equal(new Set(OFFLINE_FILES).size,OFFLINE_FILES.length);
  assert.ok(OFFLINE_FILES.every(file=>!/\.(sav|sfc|ips)$|hd-pack\.zip|\brom\.bin/i.test(file)));
  assert.ok(newerVersion('v62','v61'));
  assert.ok(newerVersion('v61.1','v61'));
  assert.ok(!newerVersion('v59','v61'));
});
test('installed offline shell loads full WASM locally and relaunches with zero assets downloaded',async()=>{
  const mock=mockStore(),progress=[];
  const first=await installOfflineFiles({storage:mock.storage,root:ROOT,
    onProgress:(done,total,file)=>progress.push([done,total,file])});
  assert.deepEqual(first,{ready:true,found:OFFLINE_FILES.length,total:OFFLINE_FILES.length});
  await markInitialRelease({storage:mock.storage,root:ROOT});
  assert.equal(mock.stats.adds,OFFLINE_FILES.length);
  assert.equal(progress.at(-1)[0],OFFLINE_FILES.length);
  assert.equal(mock.all.has(OFFLINE_CONTROL),true);
  assert.equal((await getActiveRelease({storage:mock.storage,root:ROOT})).version,'v61');
  assert.ok((await inspectOfflineCache({storage:mock.storage,root:ROOT})).ready);
  const bytes=await loadOfflineWasm(ROOT+'pkg/ssd_web_bg.wasm?build=v53',
    {storage:mock.storage,root:ROOT});
  assert.equal(bytes?.length,OFFLINE_WASM_SIZE);
  const relaunch=await installOfflineFiles({storage:mock.storage,root:ROOT,online:()=>false});
  assert.equal(relaunch.ready,true);
  assert.equal(mock.stats.adds,OFFLINE_FILES.length);
});
test('interrupted initial installation never reports ready or destroys a previous release',async()=>{
  const mock=mockStore();mock.broken.path='/zh-tw.tsv';
  await assert.rejects(installOfflineFiles({storage:mock.storage,root:ROOT}),/download failed/);
  assert.equal((await inspectOfflineCache({storage:mock.storage,root:ROOT})).ready,false);
  await assert.rejects(markInitialRelease({storage:mock.storage,root:ROOT}),/未完整/);
});
test('24h release check downloads to separate cache and activates only on restart',async()=>{
  const mock=mockStore();await install(mock);
  const remote={version:'v62',wasmSize:OFFLINE_WASM_SIZE,files:[...OFFLINE_FILES]};
  let networkChecks=0;
  const args={storage:mock.storage,root:ROOT,now:()=>10_000_000,
    online:()=>true,fetcher:async(url,options)=>{
      networkChecks++;assert.equal(new URL(url).pathname,'/dbz/offline-release.json');
      assert.equal(options.cache,'no-store');
      return new Response(JSON.stringify(remote),{status:200});
    }};
  const first=await checkReleaseOnceDaily(args);
  assert.deepEqual([first.checked,first.newer,first.ready,first.version],
    [true,true,true,'v62']);
  assert.equal(networkChecks,1);
  assert.equal((await getActiveRelease({storage:mock.storage,root:ROOT})).version,'v61');
  assert.equal((await getPendingRelease({storage:mock.storage,root:ROOT})).version,'v62');
  const oldCache=await mock.cache(OFFLINE_CACHE).match(ROOT+'app.mjs');
  assert.match(await oldCache.text(),/v61/);
  const now=await checkReleaseOnceDaily({...args,now:()=>10_000_000+3600000});
  assert.equal(now.checked,false);
  assert.equal(networkChecks,1,'automatic checks are limited to one per 24h');
  assert.equal((await getActiveRelease({storage:mock.storage,root:ROOT})).version,'v61');
  assert.equal(await activatePendingUpdate({storage:mock.storage,root:ROOT}),'v62');
  assert.equal((await getActiveRelease({storage:mock.storage,root:ROOT})).version,'v62');
  assert.equal(await getPendingRelease({storage:mock.storage,root:ROOT}),null);
  const newer=await loadOfflineWasm(ROOT+'pkg/ssd_web_bg.wasm?build=v53',
    {storage:mock.storage,root:ROOT});
  assert.equal(newer.length,OFFLINE_WASM_SIZE);
  assert.equal((await mock.cache(OFFLINE_CACHE).match(ROOT+'app.mjs')).ok,true,
    'the preceding fully playable release remains available for rollback');
  assert.equal(mock.stats.network,OFFLINE_FILES.length*2);
  assert.equal(OFFLINE_INTERVAL_MS,86400000);
});
test('failed or incomplete new version never changes current offline game',async()=>{
  const mock=mockStore();await install(mock);
  mock.broken.path='/zh-tw.tsv';
  const next={version:'v62',wasmSize:OFFLINE_WASM_SIZE,files:[...OFFLINE_FILES]};
  await assert.rejects(stageOfflineUpdate(next,{storage:mock.storage,root:ROOT}),
    /download failed/);
  assert.equal((await getActiveRelease({storage:mock.storage,root:ROOT})).version,'v61');
  assert.equal(await getPendingRelease({storage:mock.storage,root:ROOT}),null);
  await assert.rejects(activatePendingUpdate({storage:mock.storage,root:ROOT}),/待套用/);
  assert.ok((await inspectOfflineCache({storage:mock.storage,root:ROOT})).ready);
  assert.ok((await loadOfflineWasm(ROOT+'pkg/ssd_web_bg.wasm',{storage:mock.storage,root:ROOT}))?.length);
});
test('offline worker reads active version only, serves cached navigation without requests',async()=>{
  const mock=mockStore();await install(mock);
  const handlers={},hit=[];
  const self={location:{origin:new URL(ROOT).origin},registration:{scope:ROOT},
    clients:{claim:async()=>{}},skipWaiting:async()=>{},
    addEventListener(name,fn){handlers[name]=fn;}};
  vm.runInNewContext(source('web/service-worker.js'),{
    self,URL,caches:mock.storage,fetch:async req=>{hit.push(req.url);throw Error('offline');}
  });
  async function navigate(path,{mode='cors',cache='default'}={}){
    const request={method:'GET',url:ROOT+path,mode,cache};
    let answer;
    handlers.fetch({request,respondWith:promise=>answer=promise});
    return answer;
  }
  const before=await navigate('app.mjs?build=v53&ui=v66');
  assert.match(await before.text(),/v61/);
  const reload=await navigate('index.html',{mode:'navigate',cache:'reload'});
  assert.match(await reload.text(),/v61/);
  assert.equal(await navigate('pkg/ssd_web_bg.wasm?build=v53'),undefined);
  assert.equal(await navigate('offline-release.json'),undefined);
  const v62={version:'v62',wasmSize:OFFLINE_WASM_SIZE,files:[...OFFLINE_FILES]};
  await stageOfflineUpdate(v62,{storage:mock.storage,root:ROOT});
  const stillOld=await navigate('app.mjs');
  assert.match(await stillOld.text(),/v61/);
  await activatePendingUpdate({storage:mock.storage,root:ROOT});
  const newest=await navigate('app.mjs');
  assert.match(await newest.text(),/v62/);
  assert.deepEqual(hit,[]);
});
test('malformed release manifest refuses an upgrade; existing offline cache stays safe',async()=>{
  const mock=mockStore();await install(mock);
  const check=await checkReleaseOnceDaily({storage:mock.storage,root:ROOT,
    now:()=>90_000_000,online:()=>true,fetcher:async()=>new Response(JSON.stringify({
      version:'v62',wasmSize:OFFLINE_WASM_SIZE,
      files:['index.html','../external-script.js','pkg/ssd_web_bg.wasm']
    }),{status:200})});
  assert.ok(check.error);
  assert.equal((await getActiveRelease({storage:mock.storage,root:ROOT})).version,'v61');
  assert.equal(await getPendingRelease({storage:mock.storage,root:ROOT}),null);
});
test('v61 user interface exposes safe update controls and keeps save/HD gameplay',()=>{
  const html=source('web/index.html'),app=source('web/app.mjs');
  const offline=source('web/offline-installer.mjs'),worker=source('web/service-worker.js');
  assert.ok(html.includes('id="offline-status-message"'));
  assert.ok(html.includes('id="offline-check"'));
  assert.ok(html.includes('id="offline-update-apply"'));
  assert.ok(html.includes('offline-installer.mjs?offline=v61'));
  assert.ok(html.includes('id="cheat-open"'));
  assert.ok(html.includes('id="save-slots-panel"'));
  assert.ok(app.includes('loadOfflineWasm(wasmUrl)'));
  assert.ok(app.includes('dbz-offline-update-ready'));
  assert.ok(!html.includes('registration.unregister()'));
  assert.ok(worker.includes("url.pathname.endsWith('/pkg/ssd_web_bg.wasm')"));
  assert.ok(worker.includes("url.pathname.endsWith('/offline-release.json')"));
  assert.ok(offline.includes("cache:'no-store'"));
});
