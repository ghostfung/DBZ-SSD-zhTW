// Local, bounded HD map tile reader. The Rust map matcher must verify
// exact ROM grid/metatile layout AND the currently rendered BG2 tile map.
import {decodeBackground} from './hd-live.mjs';
import {adjacentMapArt} from './hd-map-prefetch.mjs';
const MAX_CACHE=12*1024*1024;
export class LiveHdMaps {
  constructor(status,decode=decodeBackground,{
    idle=(cb)=>typeof globalThis.requestIdleCallback==='function' ?
      globalThis.requestIdleCallback(cb) : null,
    cancelIdle=(handle)=>globalThis.cancelIdleCallback?.(handle)
  }={}){
    this.status=status;this.decode=decode;
    this.game=null;this.assets=null;this.catalog=[];
    this.generation=0;this.loading=new Map();this.failed=new Map();
    this.cache=new Map();this.cacheBytes=0;this.pulses=0;
    this.idle=idle;this.cancelIdle=cancelIdle;
    this.prefetchHandle=null;this.prefetchEpoch=0;
    this.prefetchIndex=null;this.prefetchCentre=null;
    this.prefetchTried=new Set();this.warming=null;
    this.metrics={decodeCount:0,decodeMs:0,prefetchCount:0,
      prefetchMs:0,prefetchHits:0};
  }
  setCatalog(catalog,assets){
    this.generation++;this.catalog=catalog?.items||[];this.assets=assets||null;
    this.cancelPrefetch(true);
    this.loading.clear();this.failed.clear();this.cache.clear();this.cacheBytes=0;
    this.metrics={decodeCount:0,decodeMs:0,prefetchCount:0,
      prefetchMs:0,prefetchHits:0};
    if(this.game&&typeof this.game.set_map_index==='function')
      this.game.set_map_index(JSON.stringify(this.catalog));
    this.status(this.assets?'HD 地圖索引已就緒，等待 ROM／VRAM 佈局校驗。':
      '此美術包沒有地圖索引或尚未匯入。');
  }
  attach(game){
    this.cancelPrefetch(true);
    this.generation++;this.game=game;this.loading.clear();this.failed.clear();
    if(this.assets&&typeof game.set_map_index==='function')
      game.set_map_index(JSON.stringify(this.catalog));
  }
  detach(){
    this.cancelPrefetch(true);
    this.generation++;this.game=null;this.loading.clear();this.failed.clear();
  }
  cancelPrefetch(reset=false){
    this.prefetchEpoch++;
    if(this.prefetchHandle!==null)this.cancelIdle(this.prefetchHandle);
    this.prefetchHandle=null;
    if(reset){
      this.prefetchIndex=null;this.prefetchCentre=null;
      this.prefetchTried.clear();
    }
  }
  // A verified live ROM/BG2 map is required. No idle work is performed
  // if Safari has no idle budget, a visible tile is missing, or PPU
  // transitions made the map identity uncertain.
  considerPrefetch(matches){
    if(!this.assets||!this.game||!this.game.hd_mode()||
       !matches.length||matches.some(m=>!m.ready)||
       this.loading.size>0){
      this.cancelPrefetch(true);return;
    }
    const index=matches[0].index;
    const entry=this.catalog[index];
    if(!entry||matches.some(m=>m.index!==index)){
      this.cancelPrefetch(true);return;
    }
    if(this.prefetchIndex!==index){
      this.cancelPrefetch(true);
      this.prefetchIndex=index;
    }
    const center=[
      matches.reduce((n,m)=>n+m.tileX,0)/matches.length,
      matches.reduce((n,m)=>n+m.tileY,0)/matches.length
    ];
    const motion=this.prefetchCentre ?
      [center[0]-this.prefetchCentre[0],center[1]-this.prefetchCentre[1]]:[0,0];
    this.prefetchCentre=center;
    if(this.prefetchHandle!==null||this.warming!==null)return;
    const targets=adjacentMapArt(matches,entry,
      name=>this.assets.has(name)&&!this.cache.has(name)&&
        !this.prefetchTried.has(name),2,motion);
    if(!targets.length)return;
    const candidate=targets[0],targetGame=this.game,assets=this.assets;
    const epoch=this.prefetchEpoch;
    const handle=this.idle((deadline)=>{
      this.prefetchHandle=null;
      if(this.prefetchEpoch!==epoch||this.game!==targetGame||
         this.assets!==assets||!targetGame.hd_mode()||
         this.loading.size>0||!deadline||deadline.timeRemaining()<8)return;
      let current;
      try{current=JSON.parse(targetGame.map_matches());}
      catch{return;}
      if(!current.length||current.some(m=>!m.ready||m.index!==index))
        return;
      this.prefetchTried.add(candidate.art);
      void this.warm(candidate.art,assets,epoch);
    });
    if(handle!==null&&handle!==undefined)this.prefetchHandle=handle;
  }
  async warm(name,assets,epoch){
    const started=performance.now();
    const promise=this.decode(assets,name);
    this.warming={name,promise};
    try{
      const image=await promise;
      if(epoch!==this.prefetchEpoch||this.assets!==assets)return;
      this.keep(name,image);
      this.metrics.prefetchCount++;
      this.metrics.prefetchMs+=performance.now()-started;
    }catch{
      // Prefetch is optional. The foreground loader remains authoritative.
    }finally{
      if(this.warming?.promise===promise)this.warming=null;
    }
  }
  cached(name){
    const image=this.cache.get(name);
    if(image){this.cache.delete(name);this.cache.set(name,image);}
    return image;
  }
  keep(name,img){
    const size=img?.rgba?.byteLength||0;
    if(!size||size>MAX_CACHE)return;
    const old=this.cache.get(name);
    if(old){this.cacheBytes-=old.rgba.byteLength;this.cache.delete(name);}
    while(this.cache.size&&this.cacheBytes+size>MAX_CACHE){
      const key=this.cache.keys().next().value;
      this.cacheBytes-=this.cache.get(key).rgba.byteLength;
      this.cache.delete(key);
    }
    this.cache.set(name,img);this.cacheBytes+=size;
  }
  pulse(silent=false){
    const g=this.game;
    if(!g||!this.assets||!g.hd_mode()||typeof g.map_matches!=='function')return;
    let matches=[],stats={};
    try{matches=JSON.parse(g.map_matches());stats=JSON.parse(g.map_probe());}
    catch(e){if(!silent)this.status('HD 地圖配對資料錯誤：'+e.message);return;}
    this.pulses++;
    if(!silent)this.status('HD 地圖：'+matches.length+' 磚／就緒 '+
      matches.filter(m=>m.ready).length+'；替換 '+(g.hd_map_pixels?.()||0)+
      ' 像素。BG2 校驗 '+(stats.checked||0)+' 磚、不符 '+
      (stats.different||0)+'；'+(stats.reason||'')+
      '。周邊預讀 '+this.metrics.prefetchCount+
      ' 張／命中 '+this.metrics.prefetchHits+
      '；主動解碼 '+this.metrics.decodeCount+' 次。');
    for(const m of matches){
      if(this.loading.size>=2)break;
      const key=m.index+':'+m.tileX+':'+m.tileY;
      const failed=this.failed.get(key);
      if(m.ready||this.loading.has(key)||!this.assets.has(m.art)||
         (failed&&(failed.count>=2||this.pulses<failed.after)))continue;
      void this.load(m,g,key);
    }
    if(this.loading.size===0)this.considerPrefetch(matches);
  }
  async load(m,g,key){
    const token={generation:this.generation};this.loading.set(key,token);
    try{
      let image=this.cached(m.art);
      if(image && this.prefetchTried.has(m.art)){
        this.metrics.prefetchHits++;
        this.prefetchTried.delete(m.art);
      }
      if(!image){
        const start=performance.now();
        if(this.warming?.name===m.art){
          try{image=await this.warming.promise;}catch{}
        }
        if(!image)image=await this.decode(this.assets,m.art);
        this.metrics.decodeCount++;
        this.metrics.decodeMs+=performance.now()-start;
        if(token.generation!==this.generation||this.game!==g)return;
        this.keep(m.art,image);
      }
      const current=JSON.parse(g.map_matches());
      if(token.generation!==this.generation||this.game!==g||
         !current.some(x=>x.index===m.index&&x.tileX===m.tileX&&
           x.tileY===m.tileY&&x.art===m.art&&!x.ready))return;
      g.set_map_tile(m.index,m.tileX,m.tileY,image.width,image.height,image.rgba);
      this.failed.delete(key);
    }catch(e){
      if(token.generation===this.generation&&this.game===g){
        const count=(this.failed.get(key)?.count||0)+1;
        this.failed.set(key,{count,after:this.pulses+count*4});
        if(!/已切換|過期/.test(e?.message||''))
          this.status('HD 地圖磚處理失敗：'+m.art+' · '+(e?.message||e));
      }
    }finally{
      if(this.loading.get(key)===token)this.loading.delete(key);
    }
  }
}
