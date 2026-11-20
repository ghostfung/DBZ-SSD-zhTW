// Exact, local-only HD ZIP coverage audit. Counts index entries and ZIP
// directory files without decompressing *any* PNG/WebP. Never claims that
// an indexed source has actually appeared during gameplay.
import {parseBackgroundIndex} from './hd-background-index.mjs';
import {parseFighterIndex} from './hd-fighter-index.mjs';
import {parsePortraitIndex} from './hd-portrait-index.mjs';
import {parseSpecialIndex} from './hd-special-index.mjs';
import {parseCardIndex} from './hd-card-index.mjs';
import {parseMapIndex} from './hd-map-index.mjs';
import {parseMode7Index} from './hd-mode7-index.mjs';

const TYPES=[
  ['background','bg/index.json','layers'],
  ['portrait','class1/index.json','assets'],
  ['battle_pose','class2/index.json','assets_list'],
  ['field_object','class3/index.json','assets_list'],
  ['cards','cards/index.json','tiles'],
  ['maps','maps/index.json','layers'],
  ['mode7','mode7/index.json','planes'],
  ['variants','variants/index.json','variants']
];
const imgPath=(dir,s)=>typeof s==='string'&&s&&!s.startsWith('/')&&
  !s.includes('\\')&&!s.split('/').some(x=>x==='..'||x==='.')
    ?dir+'/'+s:null;
const summary=({key,path,raw,status,parsed,artFiles,present,missing,
  blockers=[],examples=[],note=''})=>({
  key,path,status,rawEntries:raw,parsedEntries:parsed,
  referencedImages:artFiles,existingImages:present,missingImages:missing,
  blockers,examples:examples.slice(0,12),note
});
function gatherFiles(raw,cat,has){
  const refs=[],invalid=[];
  const put=(name)=>{if(!name)return;const p=imgPath(cat,name);
    if(p)refs.push(p);else invalid.push(String(name).slice(0,120));};
  if(cat==='bg')for(const x of raw.layers||[])put(x?.file);
  if(cat==='class1')for(const x of raw.assets||[])put(x?.file??(x?.id?x.id+'.png':null));
  if(cat==='class2'||cat==='class3')for(const x of raw.assets_list||[])put(x?.file);
  if(cat==='cards'){
    for(const x of raw.tiles||[]){put(x?.base);put(x?.roles);}
    for(const x of raw.tips||[])put(x?.image);
  }
  if(cat==='maps'||cat==='mode7'){
    const list=cat==='maps'?raw.layers:raw.planes;
    for(const x of list||[]){
      const [cx,cy]=cat==='mode7'?[4,4]:x?.blocks||[];
      if(!Number.isInteger(cx)||!Number.isInteger(cy)||
         cx<1||cy<1||cx>12||cy>12||
         typeof x.tiles!=='string'||!x.tiles.includes('{x}')||
         !x.tiles.includes('{y}'))continue;
      for(let y=0;y<cy;y++)for(let z=0;z<cx;z++)
        put(x.tiles.replaceAll('{x}',String(z)).replaceAll('{y}',String(y)));
    }
  }
  if(cat==='variants')for(const x of raw.variants||[])put(x?.file);
  const unique=[...new Set(refs)];
  const absent=unique.filter(x=>!has(x));
  return {total:unique.length,found:unique.length-absent.length,
    missing:absent.length,examples:absent.slice(0,12),
    invalid:invalid.slice(0,12)};
}
function indexed(raw,key,has){
  if(key==='background')return parseBackgroundIndex(raw,has);
  if(key==='portrait')return parsePortraitIndex(raw,has);
  if(key==='battle_pose')return parseFighterIndex(raw,has);
  if(key==='field_object')return parseSpecialIndex(raw,1,has);
  if(key==='cards')return parseCardIndex(raw,has);
  if(key==='maps')return parseMapIndex(raw);
  if(key==='mode7')return parseMode7Index(raw);
  return {items:[],ignored:0};
}
export async function buildHdCoverageAudit(assets,{version='v0.3.9'}={}){
  if(!assets?.has||!assets?.readJson)
    throw Error('請先匯入並驗證本機 HD ZIP');
  const report={
    schemaVersion:1,packVersion:version,
    scope:'ZIP 素材索引／檔案存在性 + 選定影格快照；不是遊戲流程覆蓋率',
    generatedAt:new Date().toISOString(),
    imageFiles:assets.images?.length??null,
    categories:[],unsupported:[],runtime:null
  };
  for(const [key,path,field] of TYPES){
    if(!assets.has(path)){
      report.categories.push(summary({key,path,raw:0,status:'index_absent',
        parsed:0,artFiles:0,present:0,missing:0,
        note:'素材包沒有這份索引；不代表此類功能本身故障'}));
      continue;
    }
    try{
      const raw=await assets.readJson(path,{maxBytes:8*1024*1024});
      if(!Array.isArray(raw[field]))throw Error('官方索引缺少 '+field+' 陣列');
      const entries=raw[field];
      if(entries.length>25000)throw Error('索引筆數超過盤點上限');
      const cat=path.slice(0,path.indexOf('/'));
      const fs=gatherFiles(raw,cat,x=>assets.has(x));
      const p=indexed(raw,key,x=>assets.has(x));
      const extra=[];
      if(key==='battle_pose'){
        const luma=entries.filter(x=>x?.luma===true).length;
        const usable=p.items.filter(x=>x.available).length;
        extra.push({reason:'luma 灰階素材尚未支援直接配色',count:luma});
        extra.push({reason:'可安全配對的索引候選',count:usable});
        extra.push({reason:'缺少足夠調色盤',count:p.noPalette??0});
      }else if(key==='field_object'){
        extra.push({reason:'缺少足夠調色盤',count:p.noPalette??0});
      }else if(key==='cards'){
        const tips=raw.tips||[];
        report.unsupported.push({kind:'cards/tips',entries:tips.length,
          filesPresent:tips.filter(x=>assets.has(imgPath('cards',x?.image))).length,
          reason:'提示圖片需獨立的選單觸發時機與合成管線'});
      }else if(key==='variants'){
        report.unsupported.push({kind:'variants',entries:entries.length,
          filesPresent:fs.found,
          reason:'需核對原作者 pose-art 對應與遊戲條件 when，尚未接入'});
      }
      if(p.ignored)extra.push({reason:'索引格式或來源安全條件不符',count:p.ignored});
      report.categories.push(summary({key,path,raw:entries.length,
        status:key==='variants'?'not_implemented':'indexed',
        parsed:key==='variants'?0:p.items.length,
        artFiles:fs.total,present:fs.found,missing:fs.missing,
        blockers:extra,examples:[...fs.examples,...fs.invalid].slice(0,12),
        note:key==='variants'?'只能驗證檔案存在，未實作姿勢條件比對':
          '索引可讀不代表任何遊戲影格已有匹配；請結合 runtime 快照'}));
    }catch(e){
      report.categories.push(summary({key,path,status:'index_error',
        raw:0,parsed:0,artFiles:0,present:0,missing:0,
        examples:[String(e?.message||e).slice(0,180)],
        note:'個別類別失敗，不影響其他類別的盤點'}));
    }
  }
  report.unsupported.push({
    kind:'menu_picture',entries:null,filesPresent:null,
    reason:'原作者以角色／物品的原始圖塊組合與配色重建選單，不能視作獨立 ZIP 圖檔數量'
  },{
    kind:'16:9_and_extended_sprites',entries:null,filesPresent:null,
    reason:'目前 512×478 輸出及人物原始輪廓限制，非純索引問題'
  });
  return report;
}
export function formatHdCoverageAudit(report){
  const rows=report.categories.map(c=>{
    const tag=c.status==='index_error'?'索引錯誤':
      c.status==='index_absent'?'沒有索引':
      c.status==='not_implemented'?'未接入':'已接入索引';
    return c.key+'：'+tag+'｜來源 '+c.rawEntries+
      '｜可解析 '+c.parsedEntries+'｜圖片 '+c.existingImages+
      '/'+c.referencedImages+'｜缺 '+c.missingImages;
  });
  const missing=report.categories.reduce((n,c)=>n+c.missingImages,0);
  return ['HD 素材盤點（只核對本機 ZIP 索引；不等於實際遊戲覆蓋率）',
    '圖片檔總數：'+(report.imageFiles??'未知'),
    ...rows,'缺少被索引參照的圖片：'+missing,
    '未接入：'+report.unsupported.map(x=>x.kind+
      (x.entries===null?'':'（'+x.entries+' 筆）')).join('、'),
    report.runtime?'已附目前遊戲畫面快照。':'尚未取得遊戲影格快照。'
  ].join('\n');
}
