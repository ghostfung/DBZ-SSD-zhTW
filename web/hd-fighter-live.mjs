// Match and lazily decode only the official poses that the PPU actually draws.
// Browser downloads no art: all reads are from the user's verified ZIP.
import {decodeBackground} from './hd-live.mjs';

const CACHE_LIMIT=8*1024*1024;
const MAX_CONCURRENT=2;

export class LiveHdFighters {
  constructor(status,decoder=decodeBackground) {
    this.status=status;
    this.decoder=decoder;
    this.assets=null;
    this.catalog=[];
    this.game=null;
    this.generation=0;
    this.loading=new Map();
    this.failed=new Map();
    this.decoded=new Map();
    this.cacheBytes=0;
    this.pulses=0;
  }
  setCatalog(catalog,assets) {
    this.generation++;
    this.assets=assets||null;
    this.catalog=catalog?.items||[];
    this.loading.clear();
    this.failed.clear();
    this.decoded.clear();
    this.cacheBytes=0;
    if(this.game) this.game.set_fighter_index(JSON.stringify(this.catalog));
    this.status(this.assets
      ? '人物 HD 素材索引已載入，等待 SNES OAM 精確匹配（第一輪僅限戰鬥人物）。'
      : '尚未載入人物 HD 索引，維持原版人物。');
  }
  attach(game) {
    this.generation++;
    this.game=game;
    this.loading.clear();
    this.failed.clear();
    if(this.assets) game.set_fighter_index(JSON.stringify(this.catalog));
  }
  detach() {
    this.generation++;
    this.game=null;
    this.loading.clear();
    this.failed.clear();
  }
  remember(name,picture) {
    const bytes=picture?.rgba?.byteLength||0;
    if(!bytes || bytes>CACHE_LIMIT) return;
    const prior=this.decoded.get(name);
    if(prior) {this.cacheBytes-=prior.rgba.byteLength;this.decoded.delete(name);}
    while(this.decoded.size && this.cacheBytes+bytes>CACHE_LIMIT) {
      const oldest=this.decoded.keys().next().value;
      this.cacheBytes-=this.decoded.get(oldest).rgba.byteLength;
      this.decoded.delete(oldest);
    }
    this.decoded.set(name,picture);
    this.cacheBytes+=bytes;
  }
  pulse(silent=false) {
    const game=this.game;
    if(!game || !this.assets || !this.catalog.length || !game.hd_mode()) return;
    let matches;
    try {matches=JSON.parse(game.fighter_matches());}
    catch(error){if(!silent)this.status('人物 HD 姿勢匹配資料無法解析：'+error.message);return;}
    this.pulses++;
    const ready=matches.filter(m=>m.ready).length;
    let probe={poses:0,keyHits:0,paletteRejected:0};
    try {
      if(game.fighter_probe)probe=JSON.parse(game.fighter_probe());
    }catch { /* no optional diagnostic in older runtimes */ }
    if(!silent)this.status(matches.length
      ? '戰鬥人物 HD：匹配 '+matches.length+' 組／已載入 '+ready+
        ' 組；畫面實際改變 '+(game.hd_fighter_pixels?.()??0)+
        ' 個像素。候選 '+probe.poses+' 組，鍵值吻合 '+probe.keyHits+' 組。'
      : '戰鬥人物 HD 尚未匹配：檢出 '+probe.poses+
        ' 組姿勢、來源鍵值吻合 '+probe.keyHits+
        ' 組、調色盤／尺寸拒絕 '+probe.paletteRejected+
        ' 組；目前保留原版人物。');
    for(const match of matches) {
      if(this.loading.size>=MAX_CONCURRENT)break;
      const failed=this.failed.get(match.index);
      if(!match.ready && !this.loading.has(match.index) &&
         (!failed || (failed.times<2 && this.pulses>=failed.after))) {
        void this.load(match,game);
      }
    }
  }
  async load(match,target) {
    const generation=this.generation;
    const token={generation,index:match.index};
    const assets=this.assets;
    this.loading.set(match.index,token);
    try {
      let picture=this.decoded.get(match.art);
      if(!picture) {
        picture=await this.decoder(assets,match.art);
        if(generation!==this.generation || this.game!==target)return;
        this.remember(match.art,picture);
      } else {
        this.decoded.delete(match.art);
        this.decoded.set(match.art,picture);
      }
      if(generation!==this.generation || this.game!==target)return;
      const current=JSON.parse(target.fighter_matches());
      if(!current.some(item=>item.index===match.index &&
        item.art===match.art && !item.ready))return;
      target.set_fighter_art(match.index,picture.width,picture.height,picture.rgba);
      this.failed.delete(match.index);
    } catch(error) {
      if(generation===this.generation && this.game===target) {
        const message=error?.message||String(error);
        if(/姿勢已切換|過期 HD/.test(message))return;
        const times=(this.failed.get(match.index)?.times||0)+1;
        this.failed.set(match.index,{times,after:this.pulses+3*times});
        this.status('人物 HD 圖像處理失敗：'+match.id+' · '+message);
      }
    } finally {
      if(this.loading.get(match.index)===token)this.loading.delete(match.index);
    }
  }
}
