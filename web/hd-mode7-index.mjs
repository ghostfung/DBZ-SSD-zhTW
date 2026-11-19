// Official Mode 7 flights: the 1024x1024 plane is 4x art in 16 tile images.
// Only use a matching source plane_key AND the original 256-colour palette.
const HEX=/^[0-9a-f]{16}$/i;
const COLOUR=/^[0-9a-f]{6}$/i;
const IMAGE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const safe=t=>typeof t==='string'&&t.length>0&&t.length<=200&&
    IMAGE.test(t)&&!t.split('/').some(x=>x==='.'||x==='..')&&
    t.includes('{x}')&&t.includes('{y}');
export function parseMode7Index(raw){
  if(!raw||raw.version!==1||raw.scale!==4||raw.tile!==256||
     !Array.isArray(raw.planes))throw Error('官方 Mode7 索引版本或格式不相符');
  if(raw.planes.length>48)throw Error('Mode7 索引超出安全上限');
  const items=[];let ignored=0;
  for(const entry of raw.planes){
    if(typeof entry?.id!=='string'||!entry.id||entry.id.length>128||
      typeof entry.key!=='string'||!HEX.test(entry.key)||
      !safe(entry.tiles)||!Array.isArray(entry.palette)||
      entry.palette.length!==256||!entry.palette.every(c=>
        typeof c==='string'&&COLOUR.test(c))){
      ignored++;continue;
    }
    items.push({id:entry.id,key:entry.key.toLowerCase(),
      palette:entry.palette.map(x=>x.toLowerCase()),tiles:'mode7/'+entry.tiles});
  }
  return {items,count:items.length,ignored};
}
export async function readMode7Catalog(assets){
  if(!assets?.has('mode7/index.json'))
    return {items:[],count:0,ignored:0,optional:true};
  const raw=await assets.readJson('mode7/index.json',{maxBytes:4*1024*1024});
  return parseMode7Index(raw);
}
