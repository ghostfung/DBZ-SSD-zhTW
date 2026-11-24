// Transitional worker for old Vercel origin. NO game core, ROM, HD packs
// or old offline caches are requested or deleted.
const CACHE='dbz-ssd-migration-final-v3';
const FILES=['./index.html','./migration.mjs','./migration-bundle.mjs','./save-slots.mjs'];
self.addEventListener('install',event=>{
  event.waitUntil((async()=>{
    const c=await caches.open(CACHE);
    await c.addAll(FILES.map(f=>new URL(f,self.registration.scope).href));
    await self.skipWaiting();
  })());
});
self.addEventListener('activate',event=>{
  // Preserve old dbz-ssd-offline-v60 cache and ALL IndexedDB/OPFS user data.
  // Existing in-memory games can continue until their next navigation.
  event.waitUntil(self.clients.claim());
});
self.addEventListener('fetch',event=>{
  const req=event.request;
  if(req.method!=='GET')return;
  const url=new URL(req.url);
  if(url.protocol!=='https:'&&url.protocol!=='http:')return;
  if(url.origin!==self.location.origin)return;
  const scope=new URL(self.registration.scope);
  if(!url.pathname.startsWith(scope.pathname))return;
  const route=url.pathname.slice(scope.pathname.length);
  const isShell=req.mode==='navigate'||route===''||route==='index.html';
  const allowed=FILES.slice(1).map(f=>f.slice(2));
  if(!isShell&&!allowed.includes(route))return;
  event.respondWith((async()=>{
    const cache=await caches.open(CACHE);
    const file=isShell?'./index.html':'./'+route;
    const response=await cache.match(new URL(file,self.registration.scope).href);
    if(response)return response;
    return fetch(new URL(file,self.registration.scope).href);
  })());
});
