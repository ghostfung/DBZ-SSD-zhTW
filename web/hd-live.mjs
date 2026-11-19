// The player's HD ZIP remains local. Only matching backgrounds are decoded on demand.
// The Rust PPU matcher decides which assets are actually shown: previews alone never count.
// HD pack imagery is 4x but the iPhone Enhanced output is 2x.
// Downsample once locally instead of keeping 4x bytes in WASM.
export async function decodeBackground(assets, name) {
  const raw = await assets.readImage(name);
  const type = name.toLowerCase().endsWith('.png') ? 'image/png' : 'image/webp';
  const bitmap = await createImageBitmap(new Blob([raw], {type}));
  try {
    const bytes = bitmap.width * bitmap.height * 4;
    if (!Number.isSafeInteger(bytes) || bytes > 24*1024*1024)
      throw Error('HD 背景圖片太大');
    if (bitmap.width<2 || bitmap.height<2 || bitmap.width%2 || bitmap.height%2)
      throw Error('HD 背景圖片尺寸不是偶數');
    const canvas=document.createElement('canvas');
    canvas.width=bitmap.width/2;
    canvas.height=bitmap.height/2;
    const ctx=canvas.getContext('2d',{willReadFrequently:true});
    if (!ctx) throw Error('Safari 無法建立 HD 圖片解碼畫布');
    ctx.imageSmoothingEnabled=true;
    ctx.imageSmoothingQuality='high';
    ctx.drawImage(bitmap,0,0,canvas.width,canvas.height);
    return {width:canvas.width,height:canvas.height,
      rgba:ctx.getImageData(0,0,canvas.width,canvas.height).data};
  } finally {
    bitmap.close?.();
  }
}

const MAX_DECODED_HD_BYTES = 8 * 1024 * 1024;

export class LiveHdArt {
  constructor(status, decoder=decodeBackground) {
    this.status = status;
    this.decoder = decoder;
    this.assets = null;
    this.catalog = [];
    this.game = null;
    this.generation = 0;
    this.loading = new Map();
    this.failed = new Map();
    this.sceneKey = '';
    this.pulses = 0;
    // Keep recent local 2x RGBA frames ready for A -> B -> A scene returns.
    // Bound the cache to protect iPhone Safari's memory.
    this.decoded = new Map();
    this.decodedBytes = 0;
  }

  cachedArt(name) {
    const img=this.decoded.get(name);
    if (!img) return null;
    this.decoded.delete(name);
    this.decoded.set(name,img);
    return img;
  }

  rememberArt(name,img) {
    const bytes=img?.rgba?.byteLength || 0;
    if (!bytes || bytes>MAX_DECODED_HD_BYTES) return;
    const previous=this.decoded.get(name);
    if(previous) {
      this.decodedBytes-=previous.rgba.byteLength;
      this.decoded.delete(name);
    }
    while(this.decodedBytes+bytes>MAX_DECODED_HD_BYTES && this.decoded.size) {
      const oldest=this.decoded.keys().next().value;
      const old=this.decoded.get(oldest);
      this.decodedBytes-=old.rgba.byteLength;
      this.decoded.delete(oldest);
    }
    this.decoded.set(name,img);
    this.decodedBytes+=bytes;
  }

  setCatalog(catalog, assets) {
    this.catalog = catalog?.items || [];
    this.assets = assets || null;
    this.generation++;
    this.loading.clear();
    this.failed.clear();
    this.sceneKey = '';
    // The ZIP may be a different file with identical internal image names.
    this.decoded.clear();
    this.decodedBytes = 0;
    if (this.game) this.game.set_background_index(JSON.stringify(this.catalog));
    this.status(this.assets
      ? '已讀取 HD 背景索引；等待 PPU 實際場景匹配。'
      : '尚未匯入 HD 素材，使用 Enhanced 2x 畫面。');
  }

  attach(game) {
    this.generation++;
    this.game = game;
    this.loading.clear();
    this.failed.clear();
    this.sceneKey = '';
    if (this.assets) game.set_background_index(JSON.stringify(this.catalog));
  }

  detach() {
    this.generation++;
    this.game = null;
    this.loading.clear();
    this.failed.clear();
    this.sceneKey = '';
  }

  pulse(silent=false) {
    const game = this.game;
    if (!game || !this.assets || !this.catalog.length || !game.hd_mode()) {
      if(!silent)this.status(!this.assets ? '尚未載入 HD 美術包；目前使用遊戲原生渲染。'
        : 'HD 2x 未啟用，背景美術不會替換。');
      return;
    }
    let matches;
    try { matches = JSON.parse(game.background_matches()); }
    catch (e) { if(!silent)this.status('HD 背景狀態讀取失敗：' + e.message); return; }
    this.pulses++;
    // Stale loading failures should not follow a different PPU scene.
    const scene=matches.map(m=>m.index+':'+m.art).sort().join('|');
    if(scene!==this.sceneKey){
      this.sceneKey=scene;
      this.failed.clear();
    }
    const ready = matches.filter(m => m.ready).length;
    const painted = game.hd_art_pixels?.() ?? 0;
    if(!silent)this.status(matches.length
      ? 'PPU 命中 ' + matches.length + ' 個背景，已載入 HD 美術 ' + ready
        + ' 個；目前實際替換 ' + painted + ' 個遊戲像素（其餘維持原版）。'
      : 'HD 素材就緒；此場景未符合背景索引，維持原版畫面。');
    for (const match of matches) {
      const failure=this.failed.get(match.index);
      if (!match.ready && !this.loading.has(match.index) &&
          (!failure || (failure.retries<2 && this.pulses>=failure.after))) {
        void this.load(match, game);
      }
    }
  }

  async load(match, target) {
    const generation = this.generation;
    const assets = this.assets;
    const token={generation,scene:this.sceneKey};
    this.loading.set(match.index,token);
    try {
      let img=this.cachedArt(match.art);
      if (!img) {
        img=await this.decoder(assets,match.art);
        if (this.generation !== generation || this.game !== target) return;
        this.rememberArt(match.art,img);
      }
      if (this.generation !== generation || this.game !== target) return;
      // The scene may have switched while Safari was decoding the file.
      const current=JSON.parse(target.background_matches());
      if (!current.some(item=>item.index===match.index &&
          item.art===match.art && !item.ready)) return;
      target.set_background_art(match.index,img.width,img.height,img.rgba);
      this.failed.delete(match.index);
    } catch (e) {
      if (this.generation === generation && this.game === target) {
        const message=e?.message || String(e);
        if (/背景已切換|過時圖片/.test(message)) return;
        const retries=(this.failed.get(match.index)?.retries || 0)+1;
        this.failed.set(match.index,{retries,after:this.pulses+3*retries});
        this.status('HD 背景 '+match.id+' 讀取失敗'+
          (retries<2?'，稍後重試':'（此場景暫停重試）')+'：'+message);
      }
    } finally {
      // A retired ZIP request must not clear another request with same index.
      if (this.loading.get(match.index)===token) this.loading.delete(match.index);
    }
  }
}
