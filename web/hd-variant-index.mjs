// Exact variants/index.json bindings for HD pose enlargement.
// The official art is indexed by class2 pose ID, not screen position.
const SAFE=/^[^\\/][^\\]*\.(?:png|webp)$/i;
export function parseVariantIndex(raw,fighters,has=()=>true){
  if(!raw||!Array.isArray(raw.variants))throw Error('variants/index.json 缺少 variants');
  if(raw.variants.length>128)throw Error('HD 人物擴展姿勢超過安全上限');
  const allowed=new Set((fighters||[]).filter(e=>e?.available===true&&!e.luma).map(e=>e.id));
  const byId=new Map(),seen=new Set();
  let ignored=0,missing=0,conditional=0;
  for(const item of raw.variants){
    const art=item?.art,when=item?.when,margin=item?.margin,file=item?.file;
    if(typeof art!=='string'||art.length>160||!allowed.has(art)||
      !['always','kaioken'].includes(when)||!Number.isInteger(margin)||
      margin<0||margin>96||margin%4!==0||typeof file!=='string'||
      file.length>220||!SAFE.test(file)||file.includes('/')||file.includes('..')){ignored++;continue;}
    const slot=art+':'+when;
    if(seen.has(slot)){ignored++;continue;}
    seen.add(slot);
    const name='variants/'+file;
    if(!has(name)){missing++;continue;}
    // The v0.3.9 ZIP's 27 SSJ variants all use "always".
    // "kaioken" must not render without the exact upstream WRAM condition.
    if(when!=='always'){conditional++;continue;}
    byId.set(art,{art:name,margin});
  }
  return {byId,count:byId.size,ignored,missing,conditional};
}
