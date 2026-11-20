// Official class3 (field/NPC sprites) and class2 (RGBA combat effects).
// Only exact source_key matches may be sent to Rust's OAM compositor.
const HEX=/^[0-9a-f]{1,16}$/i;
const COLOUR=/^#[0-9a-f]{6}$/i;
const IMAGE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
const safe=name=>typeof name==='string'&&name.length>0&&name.length<=220
  &&IMAGE.test(name)&&!name.split('/').some(part=>part==='..'||part==='.');
const MAX=20000;
const WRONG=new Set([
  'battle-zarbon-801:b7d6b360fc00302f',
  'fgfx-695459fedb:695459fedb6cecfd',
  'battle-goku-802:e688dcec697ef299',
  'field-frieza-force-004:3bb2d6bf3c99f517',
  'field-frieza-force-005:ac772095857a6244',
  'battle-nail-803:3bb2d6bf3c99f517',
  'battle-nail-804:ac772095857a6244'
]);
export function parseSpecialIndex(data,kind,hasFile=()=>true){
  if(kind!==1&&kind!==2)throw Error('不明的 HD 物件類別');
  if(!data||!Array.isArray(data.assets_list))throw Error('物件索引缺少 assets_list');
  if(data.assets_list.length>MAX)throw Error('物件索引超過安全上限');
  const folder=kind===1?'class3':'class2';
  const items=[];let ignored=0,missing=0,noPalette=0;
  for(const item of data.assets_list){
    const width=item?.source_size?.[0],height=item?.source_size?.[1];
    const id=item?.id,key=item?.source_key;
    if(typeof id!=='string'||!id||id.length>160||
        typeof key!=='string'||!HEX.test(key)||
        !Number.isInteger(width)||width<1||width>255||
        !Number.isInteger(height)||height<1||height>239||
        !safe(item.file)){ignored++;continue;}
    const normalized=key.padStart(16,'0').toLowerCase();
    const art=folder+'/'+item.file;
    const palette=Array(16).fill(null);
    for(const [index,color] of Object.entries(item.palette_indexes_used||{})){
      const n=Number(index);
      if(!Number.isInteger(n)||n<0||n>15||typeof color!=='string'||!COLOUR.test(color))continue;
      palette[n]=[1,3,5].map(i=>parseInt(color.slice(i,i+2),16));
    }
    const numberOfColors=palette.filter(Boolean).length;
    if(kind===1&&numberOfColors<2)noPalette++;
    const knownWrong=WRONG.has(id+':'+normalized);
    // For class2 effects the exact RGBA source key already includes colour;
    // class3 field OAM remains palette-verified and luma-unsupported.
    const luma=item.luma===true;
    const available=!knownWrong&&(kind===2 || !luma&&numberOfColors>=2)
      &&hasFile(art);
    if(!available)missing++;
    items.push({id,kind,key:normalized,width,height,art,available,palette,luma});
  }
  return {items,count:items.length,ignored,missing,noPalette};
}
export async function readSpecialCatalog(assets){
  if(!assets)throw Error('請先驗證本機 HD ZIP');
  const list=[];
  let missingOptional=false;
  if(assets.has('class3/index.json')){
    const field=await assets.readJson('class3/index.json',{maxBytes:8*1024*1024});
    list.push(...parseSpecialIndex(field,1,name=>assets.has(name)).items);
  }else{missingOptional=true;}
  const effects=await assets.readJson('class2/index.json',{maxBytes:8*1024*1024});
  list.push(...parseSpecialIndex(effects,2,name=>assets.has(name)).items);
  if(list.length>MAX)throw Error('class2／class3 總索引超過安全上限');
  return {items:list,count:list.length,missingOptional};
}
