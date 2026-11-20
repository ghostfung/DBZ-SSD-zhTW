// Translate measured WASM stage durations to a compact screenshot-ready report.
// Values are averages of 1-in-8 actual emulated frames, not made-up FPS.
export function describeHdPhases(raw,readAhead){
  let data;
  try{data=typeof raw==='string'?JSON.parse(raw):raw;}catch{return 'HD 分段耗時資料錯誤';}
  if(!data||!Number.isInteger(data.samples)||data.samples<1||!data.stages)
    return 'HD 分段耗時：尚未有足夠樣本';
  const duration=key=>{
    const v=data.stages[key];
    return v&&Number.isFinite(v.avg)&&v.avg>=0?v.avg:0;
  };
  const ms=n=>n.toFixed(1);
  const pair=(name,key)=>name+' '+ms(duration(key));
  const status=[
    pair('模擬', 'emulation'),pair('PPU 2x','ppu'),
    pair('背景','background'),pair('地圖','map'),
    pair('Mode7','mode7'),pair('人物／特效','fighters'),
    pair('頭像','portrait'),pair('卡牌','cards'),
    pair('繁中','translation')
  ].join('／');
  const map=readAhead?.metrics;
  const extra=map?
    '；地圖預讀 '+(map.prefetchCount||0)+
    '、命中 '+(map.prefetchHits||0)+
    '、預讀耗時 '+ms(map.prefetchMs||0)+' ms': '';
  return 'HD 分段平均耗時（'+data.samples+' 個樣本，ms）：'+status+extra;
}
