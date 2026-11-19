// Small, allocation-bounded RAF/CPU telemetry for iPhone stutter triage.
// No device identifiers, network upload or performance observers.
export class FrameMetrics {
  constructor(windowMs=2000) {
    this.windowMs=windowMs;
    this.reset();
  }
  reset() {
    this.gaps=[];this.cpu=[];this.draw=[];this.catchups=0;
    this.slow=0;this.count=0;this.windowStart=null;
  }
  add({now,gap=0,tickMs=0,drawMs=0,ticks=0}) {
    if (!Number.isFinite(now)) return null;
    if (this.windowStart===null) this.windowStart=now;
    if (gap>0&&Number.isFinite(gap)) {
      this.gaps.push(gap);
      if (gap>=33.4)this.slow++;
    }
    this.cpu.push(Math.max(0,tickMs));
    this.draw.push(Math.max(0,drawMs));
    if(ticks>1)this.catchups++;
    this.count++;
    if (now-this.windowStart<this.windowMs) return null;
    const percentile=(values,q)=>{
      if(!values.length)return 0;
      const sorted=values.slice().sort((a,b)=>a-b);
      return sorted[Math.min(sorted.length-1,Math.ceil(sorted.length*q)-1)];
    };
    const result={
      p95Gap:percentile(this.gaps,.95),
      maxGap:Math.max(0,...this.gaps),
      p95Tick:percentile(this.cpu,.95),
      maxTick:Math.max(0,...this.cpu),
      p95Draw:percentile(this.draw,.95),
      maxDraw:Math.max(0,...this.draw),
      slow:this.slow,catchups:this.catchups,samples:this.count
    };
    this.reset();
    this.windowStart=now;
    return result;
  }
  static describe(v){
    const ms=n=>n.toFixed(1);
    return '卡頓監測（'+v.samples+' 幀）：間隔 P95 '+ms(v.p95Gap)+
      ' ms／最高 '+ms(v.maxGap)+' ms；模擬 P95 '+ms(v.p95Tick)+
      ' ms；畫面 P95 '+ms(v.p95Draw)+' ms；延遲幀 '+v.slow+
      '；補幀 '+v.catchups;
  }
}
