// Bounded presentation-only hold policy for transient HD sprites.
// The emulator, audio, input, translation and PPU never wait. A background
// hand-off and the English-flash guard remain independent of this budget.
export class HdPresentationBudget {
  constructor({maxBurst=3,windowFrames=60,maxWindowHeld=8}={}) {
    if(!Number.isInteger(maxBurst)||maxBurst<1||maxBurst>12||
       !Number.isInteger(windowFrames)||windowFrames<10||windowFrames>600||
       !Number.isInteger(maxWindowHeld)||maxWindowHeld<maxBurst||
       maxWindowHeld>windowFrames)
      throw new RangeError('HD presentation budget');
    Object.assign(this,{maxBurst,windowFrames,maxWindowHeld});
    this.reset();
  }
  reset(){
    this.heldFrames=[];this.burst=0;this.lastFrame=null;
    this.totalHeld=0;this.totalBypassed=0;
  }
  allow(frame,wanted){
    if(!Number.isFinite(frame)||frame<0){this.reset();return false;}
    if(this.lastFrame!==null&&frame<this.lastFrame)this.reset();
    this.lastFrame=frame;
    while(this.heldFrames.length&&frame-this.heldFrames[0]>=this.windowFrames)
      this.heldFrames.shift();
    if(!wanted){this.burst=0;return false;}
    if(this.burst>=this.maxBurst||this.heldFrames.length>=this.maxWindowHeld){
      this.burst=0;this.totalBypassed++;
      return false;
    }
    this.burst++;this.heldFrames.push(frame);this.totalHeld++;
    return true;
  }
  summary(){
    return {windowFrames:this.windowFrames,maxBurst:this.maxBurst,
      maxWindowHeld:this.maxWindowHeld,recentHeld:this.heldFrames.length,
      totalHeld:this.totalHeld,totalBypassed:this.totalBypassed};
  }
}
