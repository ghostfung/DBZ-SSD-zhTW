// The approved v0.3.9 ZIP supplies class2/index.json, not a ready-made
// binary renderer pack. Each asset has an exact source_key hash of its
// grouped SNES fighter/effect sprites. This first pass handles fighters only.
// Everything stays in the player's local ZIP (never uploaded).
const HEX=/^[\da-f]{1,16}$/i;
const IMAGE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const COLOUR=/^#[\da-f]{6}$/i;
const safe=(name)=>typeof name==='string' && name.length<=220
  && IMAGE.test(name) && !name.split('/').some(x=>x==='..'||x==='.');
import {parseVariantIndex} from './hd-variant-index.mjs';
const positive=(x,max)=>Number.isInteger(x)&&x>0&&x<=max;

// Keep the same known incorrect identity mappings excluded by the desktop
// renderer (ssd-hd/art_index.rs::WRONG_OBJECTS).
const WRONG_OBJECTS=new Set([
  'battle-zarbon-801:b7d6b360fc00302f',
  'fgfx-695459fedb:695459fedb6cecfd',
  'battle-goku-802:e688dcec697ef299',
  'field-frieza-force-004:3bb2d6bf3c99f517',
  'field-frieza-force-005:ac772095857a6244',
  'battle-nail-803:3bb2d6bf3c99f517',
  'battle-nail-804:ac772095857a6244'
]);

export function parseFighterIndex(data,hasFile=()=>true) {
  if(!data||!Array.isArray(data.assets_list))
    throw Error('官方 class2 索引缺少 assets_list');
  if(data.assets_list.length>20000)
    throw Error('人物姿勢索引超過安全上限');
  const items=[],missing=[];
  let ignored=0,noPalette=0;
  for(const entry of data.assets_list){
    const width=entry?.source_size?.[0],height=entry?.source_size?.[1];
    if(typeof entry?.id!=='string'||!entry.id||entry.id.length>160||
       typeof entry.source_key!=='string'||!HEX.test(entry.source_key)||
       !positive(width,255)||!positive(height,239)||!safe(entry.file)){
      ignored++;continue;
    }
    // The official importer uses this palette evidence to distinguish
    // sprites whose index pixels match but which are different characters
    // or transformations. We do not substitute an unknown-colour pose.
    const palette=Array(16).fill(null);
    const used=entry.palette_indexes_used;
    if(used && typeof used==='object' && !Array.isArray(used)){
      for(const [index,hex] of Object.entries(used)){
        const n=Number(index);
        if(!Number.isInteger(n)||n<0||n>15||!COLOUR.test(hex))continue;
        palette[n]=[1,3,5].map(i=>parseInt(hex.slice(i,i+2),16));
      }
    }
    const colourCount=palette.filter(Boolean).length;
    if(colourCount<2 && !entry.luma)noPalette++;
    const art='class2/'+entry.file;
    // Greyscale transfer is not plain RGB: its hue must come from the exact
    // current PPU OAM pixels, and the Rust compositor enforces ownership.
    // Fighter luma is limited to official battle pose IDs; effects use the
    // separate class2 special compositor.
    const knownWrong=WRONG_OBJECTS.has(entry.id+':'+
      entry.source_key.padStart(16,'0').toLowerCase());
    const luma=entry.luma===true;
    const available=!knownWrong && hasFile(art) &&
      (luma ? entry.id.startsWith('battle-') : colourCount>=2);
    if(!available)missing.push(art);
    items.push({
      id:entry.id,key:entry.source_key.padStart(16,'0').toLowerCase(),
      width,height,art,available,palette,
      luma
    });
  }
  return {items,count:items.length,missing,ignored,noPalette};
}

export async function readFighterCatalog(assets){
  // Official class2 index may be larger than the 2 MiB background index.
  const raw=await assets.readJson('class2/index.json',{maxBytes:8*1024*1024});
  const catalog=parseFighterIndex(raw,name=>assets.has(name));
  let variantCount=0,variantIgnored=0,variantMissing=0,variantConditional=0;
  if(assets.has('variants/index.json')){
    const index=await assets.readJson('variants/index.json',{maxBytes:512*1024});
    const variants=parseVariantIndex(index,catalog.items,name=>assets.has(name));
    for(const entry of catalog.items){
      const variant=variants.byId.get(entry.id);
      if(variant)entry.variant=variant;
    }
    variantCount=variants.count;variantIgnored=variants.ignored;
    variantMissing=variants.missing;variantConditional=variants.conditional;
  }
  return {...catalog,variantCount,variantIgnored,variantMissing,variantConditional};
}
