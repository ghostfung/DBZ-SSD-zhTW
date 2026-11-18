// Presentation-only, bounded protection against a one-frame English flash.
// The emulator, audio and input keep running. We only retain the last painted
// canvas while the native Rust translator is between PPU dialogue pages.
export class StoryFrameGate {
  constructor(maxFrames=10) {
    if (!Number.isInteger(maxFrames) || maxFrames<1) throw new RangeError('maxFrames');
    this.maxFrames=maxFrames;
    this.reset();
  }
  reset() {
    this.hasPresented=false;
    this.pendingFrom=null;
    this.isHolding=false;
    this.heldFramesTotal=0;
  }
  shouldDefer(frame,pending,enabled=true) {
    this.isHolding=false;
    if (!enabled || !Number.isFinite(frame)) {
      this.pendingFrom=null;
      return false;
    }
    if (!pending) {
      this.pendingFrom=null;
      return false;
    }
    // No prior valid frame to keep (initial boot, canvas resize or reload).
    if (!this.hasPresented) return false;
    if (this.pendingFrom===null || frame<this.pendingFrom) this.pendingFrom=frame;
    if (frame-this.pendingFrom >= this.maxFrames) return false;
    this.isHolding=true;
    this.heldFramesTotal++;
    return true;
  }
  didPresent() {
    this.hasPresented=true;
    this.isHolding=false;
  }
}
