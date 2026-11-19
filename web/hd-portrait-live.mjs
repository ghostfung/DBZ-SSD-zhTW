// Demand-load only exact class1 portrait matches from the verified user's ZIP.
// Neither images nor game data ever leave the browser.
import {decodeBackground} from './hd-live.mjs';
const MAX_CACHE=4*1024*1024;
const MAX_CONCURRENT=2;

export class LiveHdPortraits {
  constructor(status,decoder=decodeBackground) {
    this.status=status;this.decoder=decoder;
    this.assets=null;this.catalog=[];this.game=null;
    this.generation=0;this.loading=new Map();this.failed=new Map();
    this.decoded=new Map();this.cacheBytes=0;this.pulses=0;
  }
  setCatalog(catalog,assets){
    this.generation++;this.assets=assets||null;
    this.catalog=catalog?.items||[];
    this.loading.clear();this.failed.clear();this.decoded.clear();this.cacheBytes=0;
    if(this.game && typeof this.game.set_portrait_index==='function')
      this.game.set_portrait_index(JSON.stringify(this.catalog));
    this.status(this.assets?'HD 頭像 class1 索引已載入，等待 PPU 圖塊驗證。':
      '尚未匯入 HD 頭像，使用原版肖像。');
  }
  attach(game){
    this.generation++;this.game=game;this.loading.clear();this.failed.clear();
    if(this.assets && typeof game.set_portrait_index==='function')
      game.set_portrait_index(JSON.stringify(this.catalog));
  }
  detach(){
    this.generation++;this.game=null;this.loading.clear();this.failed.clear();
  }
  remember(name,picture){
    const bytes=picture?.rgba?.byteLength||0;
    if(!bytes||bytes>MAX_CACHE)return;
    const old=this.decoded.get(name);
    if(old){this.cacheBytes-=old.rgba.byteLength;this.decoded.delete(name);}
    while(this.decoded.size && this.cacheBytes+bytes>MAX_CACHE){
      const key=this.decoded.keys().next().value;
      this.cacheBytes-=this.decoded.get(key).rgba.byteLength;this.decoded.delete(key);
    }
    this.decoded.set(name,picture);this.cacheBytes+=bytes;
  }
  pulse(silent=false){
    const game=this.game;
    if(!game||!this.assets||!game.hd_mode())return;
    if(typeof game.portrait_matches!=='function'){
      this.status('HD 頭像 WASM 尚未更新，保留原版頭像。');return;
    }
    let matches,probe;
    try{
      matches=JSON.parse(game.portrait_matches());
      probe=JSON.parse(game.portrait_probe());
    }catch(e){this.status('HD 頭像診斷失敗：'+e.message);return;}
    this.pulses++;
    const ready=matches.filter(m=>m.ready).length;
    const painted=game.hd_portrait_pixels?.()??0;
    if(!silent)this.status('人物頭像 HD：匹配 '+matches.length+' 組／載入 '+ready+
      ' 組；本幀實際改變 '+painted+' 個像素。候選 '+probe.candidate+
      ' 組。未符合 '+(probe.reason||'無')+'；弱候選 '+(probe.weak||0)+
      '、調色盤拒絕 '+(probe.paletteRejected||0)+'。');
    for(const match of matches){
      if(this.loading.size>=MAX_CONCURRENT)break;
      const failed=this.failed.get(match.index);
      if(match.ready || this.loading.has(match.index) ||
        (failed && (failed.times>=2 || this.pulses<failed.after)))continue;
      void this.load(match,game);
    }
  }
  async load(match,target){
    const token={generation:this.generation,index:match.index};
    this.loading.set(match.index,token);
    try{
      let picture=this.decoded.get(match.art);
      if(!picture){
        picture=await this.decoder(this.assets,match.art);
        if(token.generation!==this.generation||this.game!==target)return;
        this.remember(match.art,picture);
      }
      if(token.generation!==this.generation||this.game!==target)return;
      const current=JSON.parse(target.portrait_matches());
      if(!current.some(m=>m.index===match.index&&m.art===match.art&&!m.ready))return;
      target.set_portrait_art(match.index,picture.width,picture.height,picture.rgba);
      this.failed.delete(match.index);
    }catch(error){
      if(token.generation===this.generation&&this.game===target){
        const message=error?.message||String(error);
        if(/頭像已切換|過期 HD/.test(message))return;
        const times=(this.failed.get(match.index)?.times||0)+1;
        this.failed.set(match.index,{times,after:this.pulses+3*times});
        this.status('HD 頭像圖像處理失敗：'+match.id+' · '+message);
      }
    }finally{
      if(this.loading.get(match.index)===token)this.loading.delete(match.index);
    }
  }
}
