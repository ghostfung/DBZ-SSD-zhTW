// Official class1/index.json: 32x32 faces/portraits, authored as 128x128
// artwork. Their original ROM graphics are always decoded locally in Rust.
const SOURCE=/^rom\s+([0-9a-f]{2}):([0-9a-f]{4})\s+half\s+([01])(?:\s|$)/i;
const HEX=/^#[0-9a-f]{6}$/i;
const IMAGE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const safe=name=>typeof name==='string' && name.length>0 && name.length<=220 &&
  IMAGE.test(name) && !name.startsWith('/') &&
  !name.split('/').some(part=>part==='.'||part==='..');

export function parsePortraitIndex(data,hasFile=()=>true){
  if(!data || !Array.isArray(data.assets))
    throw Error('官方 class1 索引缺少 assets');
  if(data.assets.length>512)throw Error('官方頭像素材數量超過安全上限');
  const items=[];
  let ignored=0,missing=0;
  for(const entry of data.assets){
    const source=typeof entry?.source==='string'?entry.source.match(SOURCE):null;
    const id=entry?.id;
    const file=entry?.file??(typeof id==='string'?id+'.png':'');
    if(typeof id!=='string'||!id||id.length>160 ||!source||!safe(file)){
      ignored++;continue;
    }
    const bank=parseInt(source[1],16),offset=parseInt(source[2],16);
    if(bank!==0x16 || offset<0x8000){ignored++;continue;}
    const palette=Array(16).fill(null);
    const colors=entry.palette_colours;
    if(colors && typeof colors==='object' && !Array.isArray(colors)){
      for(const [index,color] of Object.entries(colors)){
        const n=Number(index);
        if(!Number.isInteger(n)||n<0||n>=16||typeof color!=='string'||!HEX.test(color))continue;
        palette[n]=[1,3,5].map(i=>parseInt(color.slice(i,i+2),16));
      }
    }
    const art='class1/'+file;
    const available=hasFile(art);
    if(!available)missing++;
    items.push({id,source:(bank<<16)|offset,half:Number(source[3]),
      palette,art,available});
  }
  return {items,count:items.length,ignored,missing};
}

export async function readPortraitCatalog(assets){
  const raw=await assets.readJson('class1/index.json',{maxBytes:4*1024*1024});
  return parsePortraitIndex(raw,name=>assets.has(name));
}
