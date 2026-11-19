// Loads only cards/index.json tile layers matched to real 2bpp source keys.
// No guessed card rectangle, no full ZIP extraction, no network uploads.
import {decodeCardTile} from './hd-card-index.mjs';
export class LiveHdCards {
  constructor(status,decoder=decodeCardTile){
    this.status=status;this.decoder=decoder;this.assets=null;this.catalog=[];
    this.game=null;this.loading=new Map();this.failed=new Map();
    this.cache=new Map();this.generation=0;this.pulses=0;
  }
  setCatalog(cat,assets){
    this.generation++;this.assets=assets||null;this.catalog=cat?.items||[];
    this.loading.clear();this.failed.clear();this.cache.clear();
    if(this.game&&typeof this.game.set_card_index==='function')
      this.game.set_card_index(JSON.stringify(this.catalog));
    this.status(this.assets?'卡牌／UI 圖塊索引已讀取，等待精確 2bpp 鍵值。':
      'HD 卡牌素材未就緒。');
  }
  attach(game){
    this.generation++;this.game=game;this.loading.clear();this.failed.clear();
    if(this.assets&&typeof game.set_card_index==='function')
      game.set_card_index(JSON.stringify(this.catalog));
  }
  detach(){this.generation++;this.game=null;this.loading.clear();this.failed.clear();}
  pulse(silent=false){
    const g=this.game;
    if(!g||!this.assets||!g.hd_mode()||typeof g.card_matches!=='function')return;
    let matches=[],stats={};
    try{matches=JSON.parse(g.card_matches());stats=JSON.parse(g.card_probe());}
    catch(e){if(!silent)this.status('卡牌狀態無法解析：'+e.message);return;}
    this.pulses++;
    const missing=[];
    for(const m of matches){
      for(const layer of m.layers||[]){
        if(!layer.ready)missing.push(layer);
      }
    }
    if(!silent)this.status('卡牌／UI HD：'+matches.length+
      ' 個可見圖塊，待讀取 '+missing.length+' 層；替換 '+
      (g.hd_card_pixels?.()??0)+' 像素。'+(stats.reason||'')+'。');
    for(const m of missing){
      if(this.loading.size>=2)break;
      const id=m.index+':'+m.layer;
      const rejected=this.failed.get(id);
      if(this.loading.has(id)|| (rejected&&(rejected.count>=2||this.pulses<rejected.after)))continue;
      void this.load(m,id,g);
    }
  }
  async load(m,id,g){
    const token={generation:this.generation};this.loading.set(id,token);
    try{
      let image=this.cache.get(m.art);
      if(!image){
        image=await this.decoder(this.assets,m.art);
        if(token.generation!==this.generation||this.game!==g)return;
        if(this.cache.size>=160)this.cache.delete(this.cache.keys().next().value);
        this.cache.set(m.art,image);
      }
      const active=JSON.parse(g.card_matches());
      if(token.generation!==this.generation||this.game!==g||
        !active.some(x=>x.index===m.index&&x.layers?.some(y=>
          y.layer===m.layer&&y.art===m.art&&!y.ready)))return;
      g.set_card_art(m.index,m.layer,image.width,image.height,image.rgba);
      this.failed.delete(id);
    }catch(e){
      if(token.generation===this.generation&&this.game===g){
        const count=(this.failed.get(id)?.count||0)+1;
        this.failed.set(id,{count,after:this.pulses+count*5});
        const msg=e?.message||String(e);
        if(!/已切換|過期/.test(msg))this.status('卡牌 HD 圖塊失敗：'+m.art+' · '+msg);
      }
    }finally{if(this.loading.get(id)===token)this.loading.delete(id);}
  }
}
