// Presentation-only HD scene handoff: do not expose the SNES fallback
// for a few frames while the official local art is matched and decoded.
// Emulation, input, audio and the Rust Chinese painter run without delay.
export class HdFrameGate {
  constructor({unknownFrames=24,pendingFrames=120}={}) {
    if (!Number.isInteger(unknownFrames)||unknownFrames<1 ||
        !Number.isInteger(pendingFrames)||pendingFrames<unknownFrames)
      throw new RangeError('HD frame gate limits');
    this.unknownFrames=unknownFrames;
    this.pendingFrames=pendingFrames;
    this.reset();
  }
  reset() {
    this.hasPresented=false;
    this.hasShownHd=false;
    this.pendingFrom=null;
    this.expired=false;
    this.isHolding=false;
    this.heldFramesTotal=0;
  }
  shouldDefer(frame,{enabled=false,pixels=0,pending=false}={}) {
    this.isHolding=false;
    if (!enabled || !Number.isFinite(frame)) {
      this.reset();
      return false;
    }
    if (pixels>0) {
      this.pendingFrom=null;
      this.expired=false;
      return false;
    }
    if (!this.hasPresented || this.expired || (!this.hasShownHd && !pending))
      return false;
    if (this.pendingFrom===null || frame<this.pendingFrom)
      this.pendingFrom=frame;
    const limit=pending?this.pendingFrames:this.unknownFrames;
    if (frame-this.pendingFrom>=limit) {
      // Fail open: a scene may have no HD art or the ZIP may be broken.
      // Do not repeatedly freeze the same transition.
      this.expired=true;
      this.hasShownHd=false;
      return false;
    }
    this.isHolding=true;
    this.heldFramesTotal++;
    return true;
  }
  didPresent(pixels=0) {
    this.hasPresented=true;
    if (pixels>0) {
      this.hasShownHd=true;
      this.pendingFrom=null;
      this.expired=false;
    }
    this.isHolding=false;
  }
}
