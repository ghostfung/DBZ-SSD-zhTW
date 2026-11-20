// Presentation-only defence against a sparse/garbled PPU hand-off frame.
// It is not an asset matcher. A validated HD background can still be
// composited over incomplete game PPU data during a scene transition.
// Only a sudden colourful -> nearly-black-with-small-sprite discontinuity
// is deferred. Real black frames and stable dark scenes pass through.
// Emulation, input, audio and native Chinese glyph rendering never pause.
export function sampleDarkness(rgba,width,height,step=16) {
  if(!Number.isInteger(width)||!Number.isInteger(height)||width<=0||height<=0||
     rgba?.length!==width*height*4) return null;
  let dark=0, count=0;
  // Even grid; ~1024 RGB reads at 512x478, no extra allocations.
  for(let y=step>>1;y<height;y+=step) {
    for(let x=step>>1;x<width;x+=step) {
      const i=(y*width+x)*4;
      if(rgba[i]<=15&&rgba[i+1]<=15&&rgba[i+2]<=15)dark++;
      count++;
    }
  }
  return count ? dark/count : null;
}
export class TransitionArtifactGate {
  constructor({maxFrames=5, minDark=.83, maxDark=.999, previousDark=.60}={}) {
    if(!Number.isInteger(maxFrames)||maxFrames<1||maxFrames>12||
       !(minDark>previousDark&&minDark<maxDark&&maxDark<=1))
      throw new RangeError('transition gate limits');
    Object.assign(this,{maxFrames,minDark,maxDark,previousDark});
    this.reset();
  }
  reset(){
    this.presentedDark=null;
    this.holdFrom=null;
    this.expired=false;
    this.heldFramesTotal=0;
    this.isHolding=false;
  }
  shouldDefer(frame, rgba, width, height, enabled=true){
    this.isHolding=false;
    if(!enabled){this.reset();return false;}
    const dark=sampleDarkness(rgba,width,height);
    if(dark===null||!Number.isFinite(frame))return false;
    const suspicious=this.presentedDark!==null &&
      this.presentedDark<=this.previousDark &&
      dark>=this.minDark && dark<this.maxDark;
    if(!suspicious){
      this.holdFrom=null;this.expired=false;return false;
    }
    if(this.expired)return false;
    if(this.holdFrom===null||frame<this.holdFrom)this.holdFrom=frame;
    if(frame-this.holdFrom>=this.maxFrames){
      this.expired=true;return false;
    }
    this.isHolding=true;this.heldFramesTotal++;
    return true;
  }
  didPresent(rgba,width,height){
    const dark=sampleDarkness(rgba,width,height);
    if(dark!==null)this.presentedDark=dark;
    this.isHolding=false;
  }
}
