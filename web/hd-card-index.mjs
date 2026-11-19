// Exact original 2bpp tile code, not a screenshot crop or guessed HUD location.
// Original card art base and roles are separate 64x64 8x assets.
const IMAGE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const safe=s=>typeof s==='string'&&s.length>0&&s.length<=200&&
  IMAGE.test(s)&&!s.split('/').some(p=>p==='.'||p==='..');
export function parseCardIndex(raw,has=()=>true){
  if(!raw||!Array.isArray(raw.tiles))throw Error('卡牌素材缺少 tiles 索引');
  if(raw.tiles.length>152)throw Error('卡牌圖塊數量超過核心 glyph 安全上限');
  const items=[];let ignored=0,missing=0;
  for(const t of raw.tiles){
    const title=t?.set==='title';
    const max=title?256:384;
    if(t?.set!==undefined&&!title || !Number.isInteger(t?.code)||
       t.code<0||t.code>=max||
       t.base!=null&&!safe(t.base)||t.roles!=null&&!safe(t.roles)){
      ignored++;continue;
    }
    const base=typeof t.base==='string'?'cards/'+t.base:null;
    const roles=typeof t.roles==='string'?'cards/'+t.roles:null;
    const available=Boolean((base&&has(base))||(roles&&has(roles)));
    if(!available)missing++;
    items.push({code:t.code,set:title?'title':null,
      carve:Boolean(t.carve),base:base&&has(base)?base:null,
      roles:roles&&has(roles)?roles:null,available});
  }
  return {items,count:items.length,ignored,missing};
}
export async function readCardCatalog(assets){
  if(!assets?.has('cards/index.json'))return {items:[],count:0,ignored:0,missing:0,optional:true};
  const raw=await assets.readJson('cards/index.json',{maxBytes:4*1024*1024});
  return parseCardIndex(raw,name=>assets.has(name));
}
export async function decodeCardTile(assets,name){
  const bytes=await assets.readImage(name);
  const image=await createImageBitmap(new Blob([bytes],{
    type:name.endsWith('.png')?'image/png':'image/webp'}));
  try{
    if(image.width!==64||image.height!==64)throw Error('HD 卡牌圖塊應為 64×64');
    const c=document.createElement('canvas');c.width=16;c.height=16;
    const ctx=c.getContext('2d',{willReadFrequently:true});
    if(!ctx)throw Error('Safari 無法解碼卡牌圖塊');
    ctx.imageSmoothingEnabled=true;ctx.imageSmoothingQuality='high';
    ctx.drawImage(image,0,0,16,16);
    return {width:16,height:16,rgba:ctx.getImageData(0,0,16,16).data};
  }finally{image.close?.();}
}
