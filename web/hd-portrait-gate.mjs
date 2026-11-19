// Presentation-only guard for the first frames of a newly matched class1 face.
// Never pause the emulator or audio. Never wait indefinitely for missing art.
// Do not claim a match merely because the last frame had one.
export class PortraitFrameGate {
  constructor({pendingFrames=36,transitionFrames=8}={}) {
    if (!Number.isInteger(pendingFrames)||pendingFrames<1 ||
      !Number.isInteger(transitionFrames)||transitionFrames<0)
      throw new RangeError('portrait gate limits');
    this.pendingFrames=pendingFrames;
    this.transitionFrames=transitionFrames;
    this.reset();
  }
  reset(){
    this.hasPresented=false;
    this.lastHdFrame=null;
    this.waitFrom=null;
    this.waitKey='';
    this.expiredKey='';
    this.isHolding=false;
    this.heldFramesTotal=0;
  }
  shouldDefer(frame,{enabled=false,pending=false,ready=false,pixels=0,key=''}={}){
    this.isHolding=false;
    if(!enabled||!Number.isFinite(frame)){
      this.reset();return false;
    }
    if(pixels>0){
      this.lastHdFrame=frame;this.waitFrom=null;
      this.waitKey='';this.expiredKey='';
      return false;
    }
    if(!this.hasPresented)return false;
    const recent=this.lastHdFrame!==null &&
      frame>=this.lastHdFrame && frame-this.lastHdFrame<=this.transitionFrames;
    const waiting=Boolean(pending) || (recent && !ready);
    if(!waiting){
      this.waitFrom=null;this.waitKey='';return false;
    }
    const identity=pending ? 'pending:'+key : 'handoff';
    if(identity===this.expiredKey)return false;
    if(this.waitFrom===null||this.waitKey!==identity||frame<this.waitFrom){
      this.waitFrom=frame;this.waitKey=identity;
    }
    const budget=pending?this.pendingFrames:this.transitionFrames;
    if(frame-this.waitFrom>=budget){
      this.expiredKey=identity;this.waitFrom=null;this.waitKey='';
      return false;
    }
    this.isHolding=true;this.heldFramesTotal++;
    return true;
  }
  didPresent(){this.hasPresented=true;this.isHolding=false;}
}
