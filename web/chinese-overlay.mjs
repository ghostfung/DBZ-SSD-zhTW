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

// Find an actual visible ivory dialogue window, wherever it appears.
// Battle boxes are narrower and farther right than the story windows; the
// former anchor at x=26 falsely returns no match for the battle example.
export function findDialogueBox(pixels,width,height) {
  if(!pixels || pixels.length!==width*height*4 || width<160 || height<100) return null;
  const sx=width/256, sy=sx;
  const logicalHeight=Math.floor(height/sy);
  const candidates=[];
  const ivory=(x,y)=>{
    const i=imagePixel(pixels,width,Math.round(x*sx),Math.round(y*sy));
    return isIvory(pixels[i],pixels[i+1],pixels[i+2]);
  };
  // Each row may contain dark English glyphs; bridge the short gaps between
  // ivory pixels, but reject distant clouds and isolated highlight pixels.
  for(let y=18;y<logicalHeight-2;y++){
    let first=-1,last=-1,hits=0,gap=0,best=null;
    const finish=()=>{
      if(first>=0){
        const span=last-first+2;
        if(span>=92 && hits*2/span>=0.48 &&
          (!best || span>best.right-best.left)){
          best={left:first,right:last,y,width:span};
        }
      }
      first=-1;last=-1;hits=0;gap=0;
    };
    for(let x=8;x<=246;x+=2){
      if(ivory(x,y)){
        if(first<0)first=x;
        last=x;hits++;gap=0;
      }else if(first>=0){
        gap+=2;
        if(gap>14)finish();
      }
    }
    finish();
    if(best)candidates.push(best);
  }
  const groups=[];
  for(const segment of candidates){
    const prev=groups[groups.length-1];
    if(prev && segment.y-prev.bottom<=4 &&
       Math.abs(segment.left-prev.left)<=18 &&
       Math.abs(segment.right-prev.right)<=18){
      prev.bottom=segment.y;
      prev.left=Math.min(prev.left,segment.left);
      prev.right=Math.max(prev.right,segment.right);
      prev.count++;
    } else {
      groups.push({left:segment.left,right:segment.right,
        top:segment.y,bottom:segment.y,count:1});
    }
  }
  const winner=groups.filter(g=>g.bottom-g.top>=24 && g.count>=19)
    .sort((a,b)=>(b.right-b.left)*(b.bottom-b.top)-
                (a.right-a.left)*(a.bottom-a.top))[0];
  if(!winner)return null;
  let fill=null;
  for(let y=winner.top+2;y<=winner.bottom-2&&!fill;y++){
    for(let x=winner.left+4;x<=winner.right-4;x+=4){
      if(ivory(x,y)){
        const at=imagePixel(pixels,width,Math.round(x*sx),Math.round(y*sy));
        fill='rgb('+pixels[at]+','+pixels[at+1]+','+pixels[at+2]+')';
        break;
      }
    }
  }
  if(!fill)return null;
  return {...winner,fill,sx,sy};
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
  const left=box.left+6,right=box.right-6;
  if(right-left<80){ctx.restore();return false;}
  for(const explicit of dialogue.text.split('\n')){
    if(!explicit.trim()) continue;
    lines.push(...splitLine(ctx,explicit.trim(),right-left-4));
  }
  if(!lines.length){ctx.restore();return false;}
  const paddingTop=7, paddingBottom=4;
  const heightAvailable=box.bottom-box.top-paddingTop-paddingBottom;
  const lineHeight=Math.min(15, Math.floor(heightAvailable / lines.length));
  // Avoid masking a dialogue box we cannot safely fit without obscuring its frame.
  if(lineHeight<9){ctx.restore();return false;}
  // Leave a two-pixel margin around the original border. Cover only English glyphs.
  ctx.fillStyle=box.fill;
  ctx.fillRect(left,box.top+4,right-left,box.bottom-box.top-7);
  ctx.fillStyle='#08082b';
  // The original translator uses gradual text reveal. It may appear a little early
  // at the beginning of the page; the underlying engine remains authoritative.
  const full=lines.join('');
  const shown=Math.max(0,Math.ceil(full.length * Math.max(0,Math.min(1,dialogue.progress ?? 1))));
  let remaining=shown;
  for(let i=0;i<lines.length;i++){
    const line=lines[i];
    const cut=line.slice(0,Math.max(0,Math.min(line.length,remaining)));
    if(cut)ctx.fillText(cut,left+2,box.top+paddingTop+i*lineHeight);
    remaining-=line.length;
  }
  if(dialogue.state === 2 || dialogue.state === 3) {
    // Redraw a minimal continue indicator, previously covered with the English text.
    ctx.fillStyle='#e0480f';
    ctx.beginPath();
    ctx.moveTo(right-13,box.bottom-13);
    ctx.lineTo(right-6,box.bottom-13);
    ctx.lineTo(right-9.5,box.bottom-8);
    ctx.fill();
  }
  ctx.restore();
  return true;
}
