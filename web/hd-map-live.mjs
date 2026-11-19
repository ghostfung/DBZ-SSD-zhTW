// Local, bounded HD map tile reader. The Rust map matcher must verify
// exact ROM grid/metatile layout AND the currently rendered BG2 tile map.
import {decodeBackground} from './hd-live.mjs';
const MAX_CACHE=12*1024*1024;
export class LiveHdMaps {
  constructor(status,decode=decodeBackground){
    this.status=status;this.decode=decode;
    this.game=null;this.assets=null;this.catalog=[];
    this.generation=0;this.loading=new Map();this.failed=new Map();
    this.cache=new Map();this.cacheBytes=0;this.pulses=0;
  }
  setCatalog(catalog,assets){
    this.generation++;this.catalog=catalog?.items||[];this.assets=assets||null;
    this.loading.clear();this.failed.clear();this.cache.clear();this.cacheBytes=0;
    if(this.game&&typeof this.game.set_map_index==='function')
      this.game.set_map_index(JSON.stringify(this.catalog));
    this.status(this.assets?'HD 地圖索引已就緒，等待 ROM／VRAM 佈局校驗。':
      '此美術包沒有地圖索引或尚未匯入。');
  }
  attach(game){
    this.generation++;this.game=game;this.loading.clear();this.failed.clear();
    if(this.assets&&typeof game.set_map_index==='function')
      game.set_map_index(JSON.stringify(this.catalog));
  }
  detach(){this.generation++;this.game=null;this.loading.clear();this.failed.clear();}
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
      (stats.different||0)+'；'+(stats.reason||'')+'。');
    for(const m of matches){
      if(this.loading.size>=2)break;
      const key=m.index+':'+m.tileX+':'+m.tileY;
      const failed=this.failed.get(key);
      if(m.ready||this.loading.has(key)||!this.assets.has(m.art)||
         (failed&&(failed.count>=2||this.pulses<failed.after)))continue;
      void this.load(m,g,key);
    }
  }
  async load(m,g,key){
    const token={generation:this.generation};this.loading.set(key,token);
    try{
      let image=this.cache.get(m.art);
      if(!image){
        image=await this.decode(this.assets,m.art);
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
