// Test-only HD fighter trace. Never records ROM/IPS, save-state, HD images,
// screenshots, browser identifiers or file paths. All data remains on device
// until the player explicitly downloads a bounded JSON report.
export const FIGHTER_TRACE_VERSION = 'fighter-pose-trace-v2';
const SAMPLE_MS = 200; // At most 5 FPS; expensive WASM/JSON probes only while recording.
const MAX_MS = 60_000;
const MAX_SAMPLES = 300;
const MAX_MATCHES = 8;
const MAX_KEYS = 3;
const MAX_REJECTED = 8;
const MAX_SUMMARY_REJECTIONS = 48;
const REJECTION_REASONS = new Set([
  'source_key_unmatched','size_mismatch','palette_mismatch',
  'ambiguous_palette','ambiguous_luma','oam_overlap','unsupported_category'
]);
const POSE_KEY = /^[0-9a-f]{16}$/i;
const SAFE_ID = /^[a-z0-9._-]{1,90}$/i;
const KEY_RE = /^[0-9a-f]{16}\/\d{1,3}x\d{1,3}$/i;
const COUNT_KEYS = [
  'poses','keyHits','paletteRejected','matched','ready','lumaMatched',
  'fallbackPoses','fallbackKeyHits'
];

function safeMatches(input) {
  if(!Array.isArray(input))return null;
  return input.slice(0,MAX_MATCHES).map(item=>({
    id:typeof item?.id==='string'?item.id.slice(0,90):null,
    key:typeof item?.key==='string'&&/^[0-9a-f]{1,16}$/i.test(item.key)
      ?item.key.toLowerCase():null,
    ready:item?.ready===true,
    luma:item?.luma===true,
    variantReady:item?.variantReady===true,
    w:Number.isInteger(item?.width)?item.width:null,
    h:Number.isInteger(item?.height)?item.height:null
  }));
}
// Sanitize every field: the exporter must not accept arbitrary JSON from the
// user's ROM, local art-pack indices or external data as a file path or text.
function safeRejections(input) {
  if(!Array.isArray(input))return [];
  const cleaned=[];
  for(const item of input.slice(0,MAX_REJECTED)){
    if(!item||!POSE_KEY.test(item.key)||!REJECTION_REASONS.has(item.reason)
      ||!['primary','refined'].includes(item.origin)
      ||!Number.isInteger(item.w)||item.w<1||item.w>255
      ||!Number.isInteger(item.h)||item.h<1||item.h>239)continue;
    cleaned.push({
      key:item.key.toLowerCase(),
      w:item.w,h:item.h,reason:item.reason,origin:item.origin,
      candidateIds:Array.isArray(item.candidateIds)?
        item.candidateIds.filter(v=>typeof v==='string'&&SAFE_ID.test(v))
          .slice(0,3):[]
    });
  }
  return cleaned;
}
function safeProbe(input) {
  if(!input||typeof input!=='object'||Array.isArray(input))return null;
  const out={};
  for(const key of COUNT_KEYS){
    if(Number.isInteger(input[key])&&input[key]>=0)out[key]=input[key];
  }
  out.reason=typeof input.reason==='string'?input.reason.slice(0,100):null;
  out.unindexedKeys=Array.isArray(input.unindexedKeys)
    ?input.unindexedKeys.filter(v=>typeof v==='string'&&KEY_RE.test(v))
      .slice(0,MAX_KEYS).map(s=>s.toLowerCase())
    :[];
  out.fallbackAttempted=input.fallbackAttempted===true;
  out.fallbackAccepted=Number.isInteger(input.fallbackAccepted)&&input.fallbackAccepted>=0
    ?Math.min(input.fallbackAccepted,MAX_MATCHES):0;
  out.rejectedPoses=safeRejections(input.rejectedPoses);
  return out;
}
function readJson(game,name) {
  if(typeof game?.[name]!=='function')return null;
  try{return JSON.parse(game[name]())}catch{return null}
}
export function classifyFighterSample(sample) {
  if(!sample.hdActive)return 'hd_inactive';
  if(!sample.catalogReady)return 'catalog_missing';
  if(sample.fighterEnabled===false)return 'fighter_disabled';
  const p=sample.probe,m=sample.matches;
  if(!p||!m)return 'probe_unavailable';
  if((p.poses??0)+(p.fallbackPoses??0)===0)return 'no_fighter_pose';
  if(m.length>0){
    if(sample.replacedPixels>0)return 'hd_pixels_replaced';
    if(m.some(x=>x.ready))return 'ready_but_not_painted';
    return 'matched_art_not_ready';
  }
  if((p.keyHits??0)+(p.fallbackKeyHits??0)===0)return 'source_key_unmatched';
  return 'palette_size_or_ambiguity';
}

function cleanSample(game,settings,now,startAt) {
  const probe=safeProbe(readJson(game,'fighter_probe'));
  const matches=safeMatches(readJson(game,'fighter_matches'));
  const replacedPixels=game.hd_fighter_pixels?.();
  const frame=game.frame_count?.();
  const sample={
    ms:Math.max(0,Math.round(now-startAt)),
    frame:Number.isInteger(frame)?frame:null,
    hdActive:game.hd_active?.()===true,
    catalogReady:settings.catalogReady===true,
    catalogEntries:Number.isInteger(settings.catalogEntries)?settings.catalogEntries:null,
    fighterEnabled:settings.fighterEnabled===true,
    loading:Number.isInteger(settings.loading)?settings.loading:null,
    failed:Number.isInteger(settings.failed)?settings.failed:null,
    replacedPixels:Number.isInteger(replacedPixels)&&replacedPixels>=0?replacedPixels:null,
    probe,matches,
    presentation:settings.presentation?{
      held:settings.presentation.held===true,
      displayedFrame:Number.isInteger(settings.presentation.displayedFrame)
        ?settings.presentation.displayedFrame:null
    }:null
  };
  sample.category=classifyFighterSample(sample);
  return sample;
}

export class HdFighterDiagnostic {
  constructor({sampleMs=SAMPLE_MS,maxMs=MAX_MS,maxSamples=MAX_SAMPLES}={}) {
    if(!Number.isInteger(sampleMs)||sampleMs<150
       ||!Number.isInteger(maxMs)||maxMs<1000||maxMs>90_000
       ||!Number.isInteger(maxSamples)||maxSamples<1||maxSamples>300)
      throw new RangeError('人物 HD 診斷上限不合法');
    this.sampleMs=sampleMs;this.maxMs=maxMs;this.maxSamples=maxSamples;
    this.reset();
  }
  reset() {
    this.recording=false;this.startedAt=null;this.lastSampleAt=null;
    this.endReason=null;this.finishedAt=null;this.samples=[];
  }
  start(now=0) {
    if(!Number.isFinite(now))throw new TypeError('invalid diagnostic time');
    this.reset();this.recording=true;this.startedAt=now;
  }
  stop(reason='manual',now=null) {
    if(!this.recording)return false;
    this.recording=false;
    this.endReason=String(reason).slice(0,120);
    this.finishedAt=Number.isFinite(now)?now:null;
    return true;
  }
  capture(game,settings={},now=0) {
    if(!this.recording)return false;
    if(!Number.isFinite(now))return false;
    if(now-this.startedAt>=this.maxMs || this.samples.length>=this.maxSamples){
      this.stop('time_or_sample_limit',now);return false;
    }
    if(this.lastSampleAt!==null&&now-this.lastSampleAt<this.sampleMs)return false;
    if(!game)return false;
    const frame=game.frame_count?.();
    if(!Number.isInteger(frame)||frame<=0)return false;
    if(this.samples.at(-1)?.frame===frame)return false;
    const result=cleanSample(game,settings,now,this.startedAt);
    this.samples.push(result);this.lastSampleAt=now;
    if(this.samples.length>=this.maxSamples)this.stop('time_or_sample_limit',now);
    return true;
  }
  report({build='unknown'}={}) {
    const categories={};
    const missingKeys=new Map();
    const matchedIds=new Set();
    const rejectionCounts=new Map();
    const rejectedExamples=new Map();
    let totalReplaced=0;
    let samplesWithActiveHD=0;
    let partialMatches=0;
    let rejectedSamples=0;
    let fallbackAttemptedSamples=0;
    let fallbackAccepted=0;
    for(const sample of this.samples) {
      categories[sample.category]=(categories[sample.category]||0)+1;
      if(sample.hdActive)samplesWithActiveHD++;
      totalReplaced+=sample.replacedPixels??0;
      for(const key of sample.probe?.unindexedKeys??[]){
        missingKeys.set(key,(missingKeys.get(key)||0)+1);
      }
      for(const item of sample.matches??[]){
        if(item.id)matchedIds.add(item.id);
      }
      const rejected=sample.probe?.rejectedPoses??[];
      if(rejected.length) {
        rejectedSamples++;
        if(sample.matches?.length && sample.replacedPixels>0)partialMatches++;
      }
      if(sample.probe?.fallbackAttempted)fallbackAttemptedSamples++;
      fallbackAccepted+=sample.probe?.fallbackAccepted||0;
      for(const item of rejected){
        rejectionCounts.set(item.reason,(rejectionCounts.get(item.reason)||0)+1);
        const signature=[item.key,item.w+'x'+item.h,item.reason,item.origin].join(':');
        const previous=rejectedExamples.get(signature);
        if(previous)previous.count++;
        else rejectedExamples.set(signature,{...item,count:1});
      }
    }
    return {
      format:FIGHTER_TRACE_VERSION,
      build:String(build).slice(0,80),
      scope:'player_initiated_hd_fighter_pose_samples',
      note:'只表示本次 60 秒抽樣中至少部分 HD 像素有替換，不保證所有人物均為 HD；拒絕原因與來源鍵值不等於已證實角色身分。',
      privacy:'No ROM, IPS, save-state, pixels, screenshots, HD ZIP, URLs, or device identifiers.',
      sampling:{intervalMs:this.sampleMs,maximumDurationMs:this.maxMs,
        maximumSamples:this.maxSamples},
      stopped:this.recording===false,
      endReason:this.endReason??(this.recording?'recording':'not_started'),
      summary:{
        sampleCount:this.samples.length,samplesWithActiveHD,
        totalReplacedPixelsAcrossSamples:totalReplaced,
        categoryCounts:categories,
        samplesWithRejectedPoses:rejectedSamples,
        partialMatches,
        fallbackAttemptedSamples,
        acceptedRefinedPosesAcrossSamples:fallbackAccepted,
        rejectionReasons:Object.fromEntries(rejectionCounts),
        rejectedPoseExamples:[...rejectedExamples.values()]
          .sort((a,b)=>b.count-a.count).slice(0,MAX_SUMMARY_REJECTIONS),
        matchedAssetIds:[...matchedIds].slice(0,32),
        unindexedSourceKeys:[...missingKeys].sort((a,b)=>b[1]-a[1])
          .slice(0,48).map(([key,count])=>({key,count}))
      },
      samples:this.samples.slice()
    };
  }
}
