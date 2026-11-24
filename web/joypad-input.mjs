// SNES input is sampled once per emulated frame, independently of browser refresh.
// Touch ABXY repeats after 24 frames, with real release edges; other buttons
// and hardware keyboard input retain their ordinary held semantics.
export class JoypadInput {
  constructor(){this.keys=new Map();this.pending=0;}
  press(bit,source,repeat=false){
    let key=this.keys.get(bit);
    if(!key){key={sources:new Map(),age:0};this.keys.set(bit,key);this.pending|=1<<bit;}
    key.sources.set(source,repeat);
  }
  release(bit,source,cancel=false){
    const key=this.keys.get(bit);
    if(!key||!key.sources.delete(source))return;
    if(!key.sources.size){this.keys.delete(bit);if(cancel)this.pending&=~(1<<bit);}
  }
  reset(){this.keys.clear();this.pending=0;}
  sample(){
    let mask=this.pending;this.pending=0;
    for(const [bit,key] of this.keys){
      const repeat=[...key.sources.values()].every(Boolean);
      // First hold is uninterrupted; then four released and four pressed ticks.
      if(!repeat||key.age<24||(key.age-24)%8>=4)mask|=1<<bit;
      key.age++;
    }
    return mask;
  }
}
