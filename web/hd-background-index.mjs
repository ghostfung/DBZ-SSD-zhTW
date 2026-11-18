// Decode the public art pack's bg/index.json without modifying the ROM or rendering.
// The exact fields mirror ssd-hdpack/src/import.rs::import_bg. Matching a live PPU layer
// by its key belongs to the Rust renderer and is NOT implemented by this index reader.
const HEX_KEY=/^[0-9a-fA-F]{1,16}$/;
const PATH=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const uint=(x,max)=>Number.isInteger(x)&&x>=0&&x<=max;
const validPath=(s)=>typeof s==='string' && PATH.test(s)
  && !s.startsWith('/') && !s.split('/').some(p=>p==='..'||p==='.');

export function parseBackgroundIndex(data,hasFile=()=>true){
  if(!data||!Array.isArray(data.layers))throw Error('背景索引缺少 layers 清單');
  if(data.layers.length>4096)throw Error('背景索引數量異常');
  const items=[];
  const missing=[];
  for(const [i,entry] of data.layers.entries()){
    if(typeof entry?.id!=='string' || !entry.id || entry.id.length>160 ||
       typeof entry.key!=='string' || !HEX_KEY.test(entry.key) ||
       !uint(entry.bg,3)||!uint(entry.mode,7)||
       !uint(entry.map_base,0xffff)||!uint(entry.char_base,0xffff)||!uint(entry.size,3)||
       !Array.isArray(entry.rows)||entry.rows.length!==2||
       !uint(entry.rows[0],63)||!uint(entry.rows[1],64)||entry.rows[0]>=entry.rows[1]||
       !validPath(entry.file))throw Error('背景索引第 '+(i+1)+' 項格式錯誤');
    const art='bg/'+entry.file;
    const available=hasFile(art);
    if(!available)missing.push(art);
    items.push({
      id:entry.id,
      key:entry.key.padStart(16,'0').toLowerCase(),
      bg:entry.bg,mode:entry.mode,mapBase:entry.map_base,charBase:entry.char_base,
      size:entry.size,rows:entry.rows,fill:Boolean(entry.fill),
      paletteKey:typeof entry.palette_key==='string'?entry.palette_key:'',
      art,available
    });
  }
  return {items,missing,count:items.length};
}

export async function readBackgroundCatalog(localAssets){
  const data=await localAssets.readJson('bg/index.json');
  return parseBackgroundIndex(data,name=>localAssets.has(name));
}
