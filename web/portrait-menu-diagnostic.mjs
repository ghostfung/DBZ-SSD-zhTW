// v57 read-only portrait flicker export. Uses the already-running bounded
// HdSceneTrace; never reads/sends ROM, save-state, ZIP, images or device IDs.
export function makePortraitMenuReport(snapshot){
  const raw=Array.isArray(snapshot?.recentSceneTrace?.samples)
    ?snapshot.recentSceneTrace.samples:[];
  const frames=raw.slice(-120).map(sample=>{
    const p=sample?.portrait??{};
    const probe=p.probe??{};
    const n=v=>Number.isSafeInteger(v)&&v>=0?v:null;
    return {
      frame:n(sample?.frame),presentedFrame:n(sample?.presentation?.displayedFrame),
      presentationHeld:sample?.presentation?.held===true,
      holdReasons:Array.isArray(sample?.presentation?.reasons)
        ?sample.presentation.reasons.filter(v=>typeof v==='string').slice(0,8):[],
      portrait:{
        matches:n(p.matched),ready:n(p.ready),replacedPixels:n(p.pixels),
        entries:Array.isArray(p.items)?p.items.slice(0,8).map(m=>({
          id:typeof m.id==='string'?m.id.slice(0,90):null,
          ready:m.ready===true,
          x:n(m.x),y:n(m.y)
        })):[],
        probe:{
          candidate:n(probe.candidate),matched:n(probe.matched),
          ready:n(probe.ready),weak:n(probe.weak),
          paletteRejected:n(probe.paletteRejected),
          sourceWriteRejected:n(probe.sourceWriteRejected),
          reason:typeof probe.reason==='string'?probe.reason.slice(0,90):null
        }
      }
    };
  }).filter(s=>s.frame!==null);
  const absent=frames.filter(s=>s.portrait.matches===0).length;
  const noPixels=frames.filter(s=>s.portrait.replacedPixels===0).length;
  const readyButNoPixels=frames.filter(s=>s.portrait.ready>0 &&
    s.portrait.replacedPixels===0).length;
  const withPixels=frames.filter(s=>s.portrait.replacedPixels>0).length;
  const weak=frames.filter(s=>s.portrait.probe.weak>0).length;
  const sourceWrite=frames.filter(s=>s.portrait.probe.sourceWriteRejected>0).length;
  return {
    format:'dbz-namek-portrait-trace-v1',
    scope:'last_bounded_class1_portrait_frames',
    note:'每約 6 個模擬影格取樣一次；只統計匹配、已載入數和實際 HD 像素。請與娜美星選單畫面比對，不應單憑計數認定原因。',
    privacy:'No ROM, IPS, save-state, HD ZIP, pixels, screenshots, network or device identifiers.',
    build:{
      page:typeof snapshot?.build?.client==='string'?snapshot.build.client.slice(0,90):null,
      coreMatched:snapshot?.build?.matched===true
    },
    summary:{samples:frames.length,withHdPixels:withPixels,noHdPixels:noPixels,
      noMatches:absent,readyButNoPixels,framesWithWeakCandidates:weak,
      framesWithChangedSourceRejected:sourceWrite},
    frames
  };
}

export function installPortraitMenuDiagnostic(doc=document){
  const button=doc?.getElementById?.('portrait-menu-export');
  const output=doc?.getElementById?.('portrait-menu-feedback');
  if(!button||!output)return false;
  button.addEventListener('click',()=>{
    const snapshot={};
    doc.dispatchEvent(new CustomEvent('hd-coverage-runtime-snapshot',{detail:snapshot}));
    if(snapshot?.runtime?.status!=='sampled'){
      output.textContent='請先進入 HD 2× 遊戲，停留在人物頭像閃爍的選單約 10 秒再下載。';
      return;
    }
    const report=makePortraitMenuReport(snapshot);
    if(report.frames.length<10){
      output.textContent='有效樣本不足，請返回遊戲讓娜美星選單閃爍約 10 秒，再試一次。';
      return;
    }
    const url=URL.createObjectURL(new Blob([JSON.stringify(report,null,2)],
      {type:'application/json'}));
    const a=doc.createElement('a');
    a.href=url;a.download='dbz-namek-portrait-trace-v57.json';
    doc.body.appendChild(a);a.click();a.remove();
    setTimeout(()=>URL.revokeObjectURL(url),3000);
    output.textContent='已匯出 '+report.summary.samples+' 個頭像取樣影格。請將 JSON 傳回聊天。';
  });
  return true;
}
if(typeof document!=='undefined')installPortraitMenuDiagnostic(document);
