// maps/index.json -> exact ROM layout identity. Do not infer from map name,
// colours, camera position, or an adjacent HD background.
const POINTER=/^[0-9a-f]{2}:[0-9a-f]{4}$/i;
const TEMPLATE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const safe=s=>typeof s==='string'&&s.length>0&&s.length<=200&&
  TEMPLATE.test(s)&&!s.split('/').some(part=>part==='.'||part==='..');
export function parseMapIndex(data){
  if(!data||data.version!==1||data.scale!==4||data.tile!==256||
      !Array.isArray(data.layers))throw Error('官方地圖索引版本或格式不相符');
  if(data.layers.length>256)throw Error('地圖層數超過安全上限');
  const items=[];let ignored=0;
  for(const entry of data.layers){
    const blocks=entry?.blocks;
    if(typeof entry?.id!=='string'||!entry.id||entry.id.length>128||
       !Array.isArray(blocks)||blocks.length!==2||
       !blocks.every(n=>Number.isInteger(n)&&n>=1&&n<=12)||
       !POINTER.test(entry.grid||'')||!POINTER.test(entry.metatiles||'')||
       !safe(entry.tiles)||!entry.tiles.includes('{x}')||!entry.tiles.includes('{y}')||
       !Array.isArray(entry.maps)||!entry.maps.every(n=>Number.isInteger(n)&&n>=0&&n<=255)){
      ignored++;continue;
    }
    items.push({id:entry.id,maps:entry.maps,blocks,
      grid:entry.grid,metatiles:entry.metatiles,source:entry.source||'',
      palette:'',palette_variant:Boolean(entry.palette_variant),
      tiles:'maps/'+entry.tiles});
  }
  return {items,count:items.length,ignored};
}
export async function readMapCatalog(assets){
  if(!assets.has('maps/index.json'))return {items:[],count:0,ignored:0,optional:true};
  const raw=await assets.readJson('maps/index.json',{maxBytes:4*1024*1024});
  return parseMapIndex(raw);
}
