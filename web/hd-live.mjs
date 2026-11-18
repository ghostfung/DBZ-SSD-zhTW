// The player's HD ZIP remains local. Only matching backgrounds are decoded on demand.
// The Rust PPU matcher decides which assets are actually shown: previews alone never count.
export async function decodeBackground(assets, name) {
  const raw = await assets.readImage(name);
  const type = name.toLowerCase().endsWith('.png') ? 'image/png' : 'image/webp';
  const bitmap = await createImageBitmap(new Blob([raw], {type}));
  try {
    if (bitmap.width * bitmap.height * 4 > 24 * 1024 * 1024)
      throw Error('HD 背景圖片太大');
    const canvas = document.createElement('canvas');
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext('2d', {willReadFrequently: true});
    if (!ctx) throw Error('Safari 無法建立 HD 圖片解碼畫布');
    ctx.drawImage(bitmap, 0, 0);
    return {width: bitmap.width, height: bitmap.height,
      rgba: ctx.getImageData(0, 0, bitmap.width, bitmap.height).data};
  } finally {
    bitmap.close?.();
  }
}

export class LiveHdArt {
  constructor(status, decoder=decodeBackground) {
    this.status = status;
    this.decoder = decoder;
    this.assets = null;
    this.catalog = [];
    this.game = null;
    this.generation = 0;
    this.loading = new Set();
    this.failed = new Set();
  }

  setCatalog(catalog, assets) {
    this.catalog = catalog?.items || [];
    this.assets = assets || null;
    this.generation++;
    this.loading.clear();
    this.failed.clear();
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
    if (this.assets) game.set_background_index(JSON.stringify(this.catalog));
  }

  detach() {
    this.generation++;
    this.game = null;
    this.loading.clear();
    this.failed.clear();
  }

  pulse() {
    const game = this.game;
    if (!game || !this.assets || !this.catalog.length || !game.hd_mode()) {
      this.status(!this.assets ? '尚未載入 HD 美術包；目前使用遊戲原生渲染。'
        : 'HD 2x 未啟用，背景美術不會替換。');
      return;
    }
    let matches;
    try { matches = JSON.parse(game.background_matches()); }
    catch (e) { this.status('HD 背景狀態讀取失敗：' + e.message); return; }
    const ready = matches.filter(m => m.ready).length;
    const painted = game.hd_art_pixels?.() ?? 0;
    this.status(matches.length
      ? 'PPU 命中 ' + matches.length + ' 個背景，已載入 HD 美術 ' + ready
        + ' 個；目前實際替換 ' + painted + ' 個遊戲像素（其餘維持原版）。'
      : 'HD 素材就緒；此場景未符合背景索引，維持原版畫面。');
    for (const match of matches) {
      if (!match.ready && !this.loading.has(match.index) && !this.failed.has(match.index)) {
        void this.load(match, game);
      }
    }
  }

  async load(match, target) {
    const generation = this.generation;
    const assets = this.assets;
    this.loading.add(match.index);
    try {
      const img = await this.decoder(assets, match.art);
      if (this.generation !== generation || this.game !== target) return;
      target.set_background_art(match.index, img.width, img.height, img.rgba);
    } catch (e) {
      if (this.generation === generation) {
        this.failed.add(match.index);
        this.status('HD 背景 ' + match.id + ' 無法套用：' + (e?.message || String(e)));
      }
    } finally {
      this.loading.delete(match.index);
    }
  }
}
