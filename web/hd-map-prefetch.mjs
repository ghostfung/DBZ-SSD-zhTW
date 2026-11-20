// Conservative, source-keyed field-map prefetch planner.
// The caller may use this only AFTER ROM layout + PPU BG2 verification;
// never infer a map from a scene name, previous screenshot, or camera alone.
export function adjacentMapArt(matches,entry,has,limit=2,motion=[0,0]){
  if(!Array.isArray(matches)||!matches.length||
     matches.some(m=>!m.ready)||!entry||
     !Array.isArray(entry.blocks)||entry.blocks.length!==2||
     typeof entry.tiles!=='string'||!entry.tiles.includes('{x}')||
     !entry.tiles.includes('{y}'))return [];
  const [cols,rows]=entry.blocks;
  if(!Number.isInteger(cols)||!Number.isInteger(rows)||
     cols<1||rows<1||cols>12||rows>12)return [];
  const index=matches[0].index;
  if(!Number.isInteger(index)||matches.some(m=>m.index!==index))return [];
  const xs=matches.map(m=>m.tileX),ys=matches.map(m=>m.tileY);
  if([...xs,...ys].some(n=>!Number.isInteger(n)))return [];
  const left=Math.min(...xs),right=Math.max(...xs);
  const top=Math.min(...ys),bottom=Math.max(...ys);
  const visible=new Set(matches.map(m=>m.tileX+':'+m.tileY));
  const possible=[];
  // One 256px art tile beyond the exact currently visible tile rectangle.
  for(let y=Math.max(0,top-1);y<=Math.min(rows-1,bottom+1);y++){
    for(let x=Math.max(0,left-1);x<=Math.min(cols-1,right+1);x++){
      if(visible.has(x+':'+y))continue;
      const art=entry.tiles.replaceAll('{x}',String(x)).replaceAll('{y}',String(y));
      if(!has(art))continue;
      const distance=Math.max(left-x,0,x-right)+Math.max(top-y,0,y-bottom);
      const dx=x<left?-1:x>right?1:0,dy=y<top?-1:y>bottom?1:0;
      const lead=-(dx*Math.sign(motion[0]||0)+dy*Math.sign(motion[1]||0));
      possible.push({index,x,y,art,distance,lead});
    }
  }
  possible.sort((a,b)=>a.distance-b.distance||a.lead-b.lead||
     a.y-b.y||a.x-b.x);
  return possible.slice(0,Math.max(0,Math.min(2,limit)));
}
