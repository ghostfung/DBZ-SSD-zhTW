// Experimental in-frame Traditional Chinese overlay. No changes to ROM or SRAM.
// We only replace text when a recognizable ivory SNES dialogue box is present.
// For other scenes, the verified external subtitle remains available as fallback.

const MARK_X = 26;
function isIvory(r,g,b) {
  return r >= 245 && g >= 218 && b >= 160 && r-g >= 5 && g-b >= 15;
}
function imagePixel(image, width, x, y) {
  return (y*width+x)*4;
}

// Find an ivory message box by sampling its left-side interior; reject non-dialogue scenes.
// Output logical pixel coordinates (not device/CSS pixels), or null.
export function findDialogueBox(pixels,width,height) {
  if (!pixels || pixels.length !== width*height*4 || width < 160 || height < 100) return null;
  // The upstream 2x Enhanced frame preserves the same logical picture position.
  const sx=width/256, sy=height/239;
  const anchorX=Math.round(MARK_X*sx);
  let start=-1, best=null;
  for(let ly=18;ly<=229;ly++) {
    const y=Math.round(ly*sy);
    if (y >= height) break;
    const i=imagePixel(pixels,width,anchorX,y);
    const ivory=isIvory(pixels[i],pixels[i+1],pixels[i+2]);
    if(ivory && start<0) start=ly;
    if((!ivory||ly===229) && start>=0) {
      const end=ivory?ly:ly-1;
      if(end-start>=28 && (!best || end-start>best.end-best.start))best={start,end};
      start=-1;
    }
  }
  if(!best)return null;
  const middleY=Math.round((best.start+best.end)/2*sy);
  let ivoryCount=0;
  for(const lx of [26,42,57,73,101,127,158,195,215,225]){
    const x=Math.round(lx*sx);
    const i=imagePixel(pixels,width,x,middleY);
    if(isIvory(pixels[i],pixels[i+1],pixels[i+2]))ivoryCount++;
  }
  if(ivoryCount<7)return null;
  const pixelIndex=imagePixel(pixels,width,anchorX,Math.round((best.start+5)*sy));
  return {
    top:best.start,
    bottom:best.end,
    fill:`rgb(${pixels[pixelIndex]},${pixels[pixelIndex+1]},${pixels[pixelIndex+2]})`,
    sx,sy,
  };
}

function splitLine(ctx,text,maxWidth) {
  const out=[];
  let line='';
  for(const character of text) {
    if(ctx.measureText(line+character).width > maxWidth && line.length) {
      out.push(line);
      line=character;
    } else {
      line+=character;
    }
  }
  if(line)out.push(line);
  return out;
}

export function drawChineseInBox(ctx, image, width, height, dialogue) {
  if (!dialogue?.text) return false;
  const box=findDialogueBox(image,width,height);
  if(!box)return false;
  const lines=[];
  ctx.save();
  ctx.scale(box.sx,box.sy);
  ctx.font='bold 12px "PingFang TC","Noto Sans TC","Microsoft JhengHei",sans-serif';
  ctx.textBaseline='top';
  for(const explicit of dialogue.text.split('\n')){
    if(!explicit.trim()) continue;
    lines.push(...splitLine(ctx,explicit.trim(),190));
  }
  if(!lines.length){ctx.restore();return false;}
  const paddingTop=7, paddingBottom=4;
  const heightAvailable=box.bottom-box.top-paddingTop-paddingBottom;
  const lineHeight=Math.min(15, Math.floor(heightAvailable / lines.length));
  // Avoid masking a dialogue box we cannot safely fit without obscuring its frame.
  if(lineHeight<9){ctx.restore();return false;}
  // Leave a two-pixel margin around the original border. Cover only English glyphs.
  ctx.fillStyle=box.fill;
  ctx.fillRect(30,box.top+4,198,box.bottom-box.top-7);
  ctx.fillStyle='#08082b';
  // The original translator uses gradual text reveal. It may appear a little early
  // at the beginning of the page; the underlying engine remains authoritative.
  const full=lines.join('');
  const shown=Math.max(0,Math.ceil(full.length * Math.max(0,Math.min(1,dialogue.progress ?? 1))));
  let remaining=shown;
  for(let i=0;i<lines.length;i++){
    const line=lines[i];
    const cut=line.slice(0,Math.max(0,Math.min(line.length,remaining)));
    if(cut)ctx.fillText(cut,32,box.top+paddingTop+i*lineHeight);
    remaining-=line.length;
  }
  if(dialogue.state === 2 || dialogue.state === 3) {
    // Redraw a minimal continue indicator, previously covered with the English text.
    ctx.fillStyle='#e0480f';
    ctx.beginPath();
    ctx.moveTo(160,box.bottom-13);
    ctx.lineTo(167,box.bottom-13);
    ctx.lineTo(163.5,box.bottom-8);
    ctx.fill();
  }
  ctx.restore();
  return true;
}
