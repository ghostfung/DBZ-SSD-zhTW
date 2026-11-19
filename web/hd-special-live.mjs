// One demand-loader for exact OAM class3 field sprites and class2 effects.
// No user-provided ROM, IPS or HD bytes are sent over the network.
import {decodeBackground} from './hd-live.mjs';
const MAX_CACHE=8*1024*1024;
export class LiveHdSpecials {
  constructor(status,decoder=decodeBackground){
    this.status=status;this.decoder=decoder;
    this.game=null;this.assets=null;this.catalog=[];
    this.generation=0;this.loading=new Map();this.failed=new Map();
    this.decoded=new Map();this.cacheBytes=0;this.pulses=0;
  }
  setCatalog(catalog,assets){
    this.generation++;this.catalog=catalog?.items||[];this.assets=assets||null;
    this.loading.clear();this.failed.clear();this.decoded.clear();this.cacheBytes=0;
    if(this.game&&typeof this.game.set_special_index==='function')
      this.game.set_special_index(JSON.stringify(this.catalog));
    this.status(this.assets?'地圖物件／戰鬥特效索引已載入，等待 PPU 檢驗。':
      '尚未載入 class3 物件及 class2 特效索引。');
  }
  attach(game){
    this.generation++;this.game=game;this.loading.clear();this.failed.clear();
    if(this.assets&&typeof game.set_special_index==='function')
      game.set_special_index(JSON.stringify(this.catalog));
  }
  detach(){
    this.generation++;this.game=null;this.loading.clear();this.failed.clear();
  }
  remember(name,image){
    const bytes=image?.rgba?.byteLength||0;if(!bytes||bytes>MAX_CACHE)return;
    const old=this.decoded.get(name);
    if(old){this.cacheBytes-=old.rgba.byteLength;this.decoded.delete(name);}
    while(this.decoded.size&&this.cacheBytes+bytes>MAX_CACHE){
      const first=this.decoded.keys().next().value;
      this.cacheBytes-=this.decoded.get(first).rgba.byteLength;this.decoded.delete(first);
    }
    this.decoded.set(name,image);this.cacheBytes+=bytes;
  }
  pulse(silent=false){
    const game=this.game;
    if(!game||!this.assets||!game.hd_mode()||
      typeof game.special_matches!=='function')return;
    let matches,stats;
    try {
      matches=JSON.parse(game.special_matches());
      stats=JSON.parse(game.special_probe());
    }catch(e){if(!silent)this.status('特效匹配狀態讀取失敗：'+e.message);return;}
    this.pulses++;
    if(!silent){
      const fields=matches.filter(m=>m.kind===1),effects=matches.filter(m=>m.kind===2);
      const count=arr=>arr.filter(m=>m.ready).length;
      this.status('地圖物件 HD：'+fields.length+' 組／已載入 '+count(fields)+
        '；招式特效 HD：'+effects.length+' 組／已載入 '+count(effects)+
        '；實際改變 '+(game.hd_special_pixels?.()??0)+' 像素。'+
        'PPU 檢出 '+stats.poses+'、鍵值命中 '+stats.keyHits+
        '、調色盤拒絕 '+stats.paletteRejected+'。');
    }
    for(const match of matches){
      if(this.loading.size>=2)break;
      const rejected=this.failed.get(match.index);
      if(match.ready||this.loading.has(match.index)||
        (rejected&&(rejected.count>=2||this.pulses<rejected.after)))continue;
      void this.load(match,game);
    }
  }
  async load(match,game){
    const token={generation:this.generation};
    this.loading.set(match.index,token);
    try {
      let img=this.decoded.get(match.art);
      if(!img){
        img=await this.decoder(this.assets,match.art);
        if(token.generation!==this.generation||this.game!==game)return;
        this.remember(match.art,img);
      }
      const current=JSON.parse(game.special_matches());
      if(token.generation!==this.generation||this.game!==game||
        !current.some(m=>m.index===match.index&&m.art===match.art&&!m.ready))return;
      game.set_special_art(match.index,img.width,img.height,img.rgba);
      this.failed.delete(match.index);
    } catch(e){
      if(token.generation===this.generation&&this.game===game){
        const msg=e?.message||String(e);
        if(/已切換|過期 HD/.test(msg))return;
        const count=(this.failed.get(match.index)?.count||0)+1;
        this.failed.set(match.index,{count,after:this.pulses+count*4});
        this.status('物件素材無法解碼 '+match.id+'：'+msg);
      }
    }finally{
      if(this.loading.get(match.index)===token)this.loading.delete(match.index);
    }
  }
}
