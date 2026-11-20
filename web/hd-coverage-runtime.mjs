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
    let matched=null,ready=null;
    if(hasMatch){
      try{
        const parsed=JSON.parse(game[matcher]());
        if(Array.isArray(parsed)){
          matched=parsed.length;
          ready=parsed.filter(x=>x?.ready===true).length;
        }
      }catch{/* no fabricated zero when diagnostic is unreadable */}
    }
    const changed=game[pixelGetter]?.();
    categories[name]={
      matches:matched,ready,
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
