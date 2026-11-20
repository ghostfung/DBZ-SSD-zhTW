// A local, read-only *single-frame* diagnostic. Refuse an all-zero
// "coverage" report when HD is off or the browser core has not started.
const SOURCES=[
  ['background','background_matches','hd_art_pixels'],
  ['battle_pose','fighter_matches','hd_fighter_pixels'],
  ['portrait','portrait_matches','hd_portrait_pixels'],
  ['field_object_and_effect','special_matches','hd_special_pixels'],
  ['maps','map_matches','hd_map_pixels'],
  ['cards','card_matches','hd_card_pixels'],
  ['mode7','mode7_matches','hd_mode7_pixels']
];
export function attachHdRuntimeSnapshot(report,game){
  if(!report||typeof report!=='object')return;
  const frame=Number.isFinite(game?.frame_count?.())?game.frame_count():null;
  const enabled=game?.hd_active?.()===true;
  if(!game||!enabled||frame===null||frame<=0){
    const reason=!game?'遊戲尚未啟動':
      !enabled?'HD 2x 尚未啟用；不能以全部零值判斷配對結果':
        '遊戲尚未產生可採樣的畫面';
    report.runtime={
      status:'not_sampled',hdActive:enabled,frame,
      reason,categories:null
    };
    return;
  }
  const categories={};
  for(const [name,matcher,pixelGetter] of SOURCES){
    const hasMatch=typeof game[matcher]==='function';
    let matched=null,ready=null,lumaMatches=null,lumaReady=null;
    if(hasMatch){
      try{
        const parsed=JSON.parse(game[matcher]());
        if(Array.isArray(parsed)){
          matched=parsed.length;
          ready=parsed.filter(x=>x?.ready===true).length;
          // Only count luma if this WASM bridge actually reports the flag.
          // Old JS or old WASM must not silently fabricate a zero.
          if(parsed.some(x=>typeof x?.luma==='boolean')){
            lumaMatches=parsed.filter(x=>x.luma===true).length;
            lumaReady=parsed.filter(x=>x.luma===true&&x.ready===true).length;
          }
        }
      }catch{/* no fabricated zero when diagnostic is unreadable */}
    }
    const changed=game[pixelGetter]?.();
    categories[name]={
      matches:matched,ready,lumaMatches,lumaReady,
      replacedPixels:typeof changed==='number'&&Number.isFinite(changed) ?
        changed:null,
      matchDataAvailable:matched!==null
    };
  }
  report.runtime={
    status:'sampled',scope:'single_frame',
    explanation:'僅為一個有效 HD 影格；不代表全遊戲覆蓋率',
    hdActive:true,frame,categories
  };
}

/**
 * Small, private diagnostic window for short-lived HD regressions. Store only
 * matcher IDs, source hashes, counts and reasons -- never ROM bytes, save
 * states, screenshots, or the user's ZIP contents.
 *
 * A single-frame export can show a successful HD pose even though the next
 * animation pose fails. Sampling at 10 Hz for ~24 seconds distinguishes these
 * without a continuous 60 Hz WASM/JSON/DOM workload on iPhone Safari.
 */
export class HdSceneTrace {
  constructor({stepFrames=6,maxSamples=240}={}){
    if(!Number.isInteger(stepFrames)||stepFrames<1||
       !Number.isInteger(maxSamples)||maxSamples<1||maxSamples>240)
      throw new RangeError('HD scene trace limits');
    this.stepFrames=stepFrames;this.maxSamples=maxSamples;this.reset();
  }
  reset(){this.lastFrame=null;this.samples=[];}
  capture(game){
    if(!game || game.hd_active?.()!==true)return false;
    const frame=game.frame_count?.();
    if(!Number.isFinite(frame)||frame<=0)return false;
    if(this.lastFrame!==null&&frame<this.lastFrame)this.reset();
    if(this.lastFrame!==null&&frame-this.lastFrame<this.stepFrames)return false;
    this.lastFrame=frame;
    const read=(name)=>{
      if(typeof game[name]!=='function')return null;
      try{return JSON.parse(game[name]());}catch{return null;}
    };
    const part=(matchesName,probeName,pixelsName)=>{
      const matches=read(matchesName);
      const probe=read(probeName);
      const pixels=game[pixelsName]?.();
      const list=Array.isArray(matches)?matches.slice(0,12):null;
      const ready=list?.filter(m=>m?.ready===true).length??null;
      return {
        matched:list?.length??null,
        ready,
        pixels:Number.isFinite(pixels)?pixels:null,
        // Official IDs/keys help distinguish an unindexed animation pose
        // from a decoded artwork and a failed composite. No pixel payloads.
        items:list?.map(m=>({
          id:String(m.id??'').slice(0,100),
          key:typeof m.key==='string'?m.key.slice(0,16):null,
          ready:m.ready===true,
          luma:m.luma===true
        }))??null,
        probe:probe&&typeof probe==='object'&&!Array.isArray(probe)
          ?Object.fromEntries(Object.entries(probe).filter(([key,value])=>
            ['poses','keyHits','paletteRejected','candidate','weak',
             'sourceWriteRejected','reason','matched','ready','lumaMatched',
             'fallbackPoses','fallbackKeyHits'].includes(key)
            && (typeof value==='number'||typeof value==='string'))
            // Exact source-key fingerprints only: never ROM data or textures.
            .concat(Array.isArray(probe.unindexedKeys)?[[
              'unindexedKeys',probe.unindexedKeys.filter(value=>
                typeof value==='string' &&
                /^[0-9a-f]{16}\/\d{1,3}x\d{1,3}$/i.test(value)).slice(0,3)
            ]]:[])) :null
      };
    };
    this.samples.push({frame,
      fighter:part('fighter_matches','fighter_probe','hd_fighter_pixels'),
      portrait:part('portrait_matches','portrait_probe','hd_portrait_pixels'),
      special:part('special_matches','special_probe','hd_special_pixels')
    });
    if(this.samples.length>this.maxSamples)this.samples.shift();
    return true;
  }
  report(){
    return {
      scope:'recent_sampled_frames',
      explanation:'最近遊玩影格的約 10Hz 配對快照；不代表遊戲覆蓋率，無 ROM／存檔／圖片資料',
      samples:this.samples.slice()
    };
  }
}
