// Mode 7 HD art requests are on-device and bound to a verified PPU VRAM plane.
// We keep decoded art bounded: 512² RGBA = 1 MiB per tile, max 12 MiB cache.
import {decodeBackground} from './hd-live.mjs';
const MAX=12*1024*1024;
export class LiveHdMode7 {
  constructor(status,decoder=decodeBackground){
    this.status=status;this.decoder=decoder;this.game=null;
    this.catalog=[];this.assets=null;this.generation=0;
    this.loading=new Map();this.failed=new Map();
    this.cache=new Map();this.bytes=0;this.pulses=0;
  }
  setCatalog(catalog,assets){
    this.generation++;this.catalog=catalog?.items||[];
    this.assets=assets||null;
    this.loading.clear();this.failed.clear();this.cache.clear();this.bytes=0;
    if(this.game&&typeof this.game.set_mode7_index==='function')
      this.game.set_mode7_index(JSON.stringify(this.catalog));
    this.status(this.assets?'Mode7 VRAM 索引已載入，等待飛行場景配對。':
      'Mode7 HD 素材尚未匯入。');
  }
  attach(game){
    this.generation++;this.game=game;this.loading.clear();this.failed.clear();
    if(this.assets&&typeof game.set_mode7_index==='function')
      game.set_mode7_index(JSON.stringify(this.catalog));
  }
  detach(){this.generation++;this.game=null;this.loading.clear();this.failed.clear();}
  remember(key,img){
    const size=img?.rgba?.byteLength||0;if(!size||size>MAX)return;
    const old=this.cache.get(key);
    if(old){this.bytes-=old.rgba.byteLength;this.cache.delete(key);}
    while(this.cache.size&&this.bytes+size>MAX){
      const name=this.cache.keys().next().value;
      this.bytes-=this.cache.get(name).rgba.byteLength;this.cache.delete(name);
    }
    this.cache.set(key,img);this.bytes+=size;
  }
  pulse(silent=false){
    const g=this.game;
    if(!g||!this.assets||!g.hd_mode()||typeof g.mode7_matches!=='function')return;
    let matches=[],probe={};
    try{matches=JSON.parse(g.mode7_matches());probe=JSON.parse(g.mode7_probe());}
    catch(e){if(!silent)this.status('Mode7 資料錯誤：'+e.message);return;}
    this.pulses++;
    if(!silent)this.status('Mode7 HD：可見 '+matches.length+' 張，已載入 '+
      matches.filter(m=>m.ready).length+' 張；替換 '+
      (g.hd_mode7_pixels?.()??0)+' 像素。'+(probe.reason||'')+'。');
    for(const m of matches){
      if(this.loading.size>=2)break;
      const key=m.index+':'+m.x+':'+m.y;
      const failed=this.failed.get(key);
      if(m.ready||this.loading.has(key)||!this.assets.has(m.art)||
        failed&&(failed.count>=2||this.pulses<failed.after))continue;
      void this.load(m,key,g);
    }
  }
  async load(m,key,g){
    const token={generation:this.generation};this.loading.set(key,token);
    try{
      let img=this.cache.get(m.art);
      if(!img){
        img=await this.decoder(this.assets,m.art);
        if(token.generation!==this.generation||this.game!==g)return;
        this.remember(m.art,img);
      }
      const active=JSON.parse(g.mode7_matches());
      if(token.generation!==this.generation||this.game!==g||
        !active.some(p=>p.index===m.index&&p.x===m.x&&p.y===m.y&&
          p.art===m.art&&!p.ready))return;
      g.set_mode7_tile(m.index,m.x,m.y,img.width,img.height,img.rgba);
      this.failed.delete(key);
    }catch(e){
      if(token.generation===this.generation&&this.game===g){
        const count=(this.failed.get(key)?.count||0)+1;
        this.failed.set(key,{count,after:this.pulses+5*count});
        if(!/已切換|過期/.test(e?.message||''))
          this.status('Mode7 畫面素材未載入：'+m.art+' · '+(e?.message||e));
      }
    }finally{
      if(this.loading.get(key)===token)this.loading.delete(key);
    }
  }
}
