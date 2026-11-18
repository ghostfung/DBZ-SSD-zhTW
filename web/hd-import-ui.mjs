// HD archive import UI. All reads are local File/OPFS, never fetch/POST player assets.
import {inspectZip,extractZipEntry,verifyOfficialPack,OFFICIAL_PACK} from './hd-pack.mjs';
import {opfsAvailable,saveVerifiedHdPack,readSavedHdPack,clearSavedHdPack} from './hd-storage.mjs';
import {LocalHdAssets} from './hd-local-assets.mjs';
import {readBackgroundCatalog} from './hd-background-index.mjs';

const $=id=>document.getElementById(id);
let verified=null;
let activeFile=null;
let busy=false;
let persisted=false;
let previewObjectUrl=null;
let sampleEntry=null;
let library=null;
let bgCatalog=null;

function clearPreview(){
  if(previewObjectUrl)URL.revokeObjectURL(previewObjectUrl);
  previewObjectUrl=null;
  $('hd-preview-panel').hidden=true;
  $('hd-preview-image').removeAttribute('src');
  $('hd-preview-name').textContent='';
}

function status(message){$('hd-status').textContent=message;}
function sessionStatus(message){$('hd-session-status').textContent=message;}
function previewEntry(info){return info.entries.find(x=>/\.(png|webp)$/i.test(x.name)&&x.unpacked>=128&&x.unpacked<=2*1024*1024)??null;}
function busyMode(value){
  busy=value;
  $('hd-zip').disabled=value;
  $('hd-verify').disabled=value||!$('hd-zip').files?.[0];
  $('hd-cache').disabled=value||!verified||!opfsAvailable()||persisted;
  $('hd-preview-btn').disabled=value||!sampleEntry||!verified;
  $('hd-remove').disabled=value;
  $('hd-asset-search').disabled=value||!library;
  $('hd-asset-list').disabled=value||!library;
  $('hd-preview-selected').disabled=value||!library||!$('hd-asset-list').value;
  $('hd-bg-preview').disabled=value||!selectedBackground()?.available;
}
function progress(phase,done,total){
  const value=total?Math.round(done/total*100):0;
  $('hd-progress').hidden=false;
  $('hd-progress').value=value;
  status(`${phase}：${value}%（${(done/1048576).toFixed(0)} / ${(total/1048576).toFixed(0)} MiB）`);
}
function refreshAssetList(){
  const select=$('hd-asset-list');
  select.replaceChildren();
  if(!library)return;
  const term=$('hd-asset-search').value.trim();
  const matches=library.search(term,{limit:40});
  for(const item of matches){
    const option=document.createElement('option');
    option.value=item.name;
    option.textContent=item.name+' ('+(item.bytes/1024).toFixed(0)+' KiB)';
    select.appendChild(option);
  }
  if(select.options.length)select.selectedIndex=0;
  $('hd-preview-selected').disabled=!select.value||busy;
  $('hd-asset-info').textContent=matches.length
    ? '顯示符合搜尋條件的前 '+matches.length+' 張。ZIP 不會整包解壓；只有預覽時才讀取個別圖片。'
    : '沒有符合的圖片，請嘗試其他名稱。';
}
function selectedBackground(){
  const index=Number($('hd-bg-list').value);
  return bgCatalog?.items[index]??null;
}
function updateBackgroundSelection(){
  const item=selectedBackground();
  $('hd-bg-preview').disabled=busy||!item?.available;
  $('hd-bg-details').textContent=item
    ? '鍵值 '+item.key+'｜BG'+(item.bg+1)+'｜模式 '+item.mode+
      '｜map '+item.mapBase.toString(16).toUpperCase()+
      '｜tiles '+item.charBase.toString(16).toUpperCase()+
      '｜列 '+item.rows.join('–')+'｜圖片 '+item.art+
      (item.available?'':'（ZIP 內沒有這張圖片）')
    : '尚未選取背景圖層。';
}
$('hd-bg-list').addEventListener('change',updateBackgroundSelection);
$('hd-bg-preview').addEventListener('click',()=>previewLocalImage(selectedBackground()?.art));
async function loadBackgroundIndex(source){
  $('hd-bg-catalog').hidden=false;
  $('hd-bg-count').textContent='正在從玩家本機 ZIP 讀取 bg/index.json…';
  $('hd-bg-list').replaceChildren();
  $('hd-bg-preview').disabled=true;
  bgCatalog=null;
  try{
    const catalogue=await readBackgroundCatalog(source);
    if(library!==source)return; // User selected a different ZIP while reading.
    bgCatalog=catalogue;
    document.dispatchEvent(new CustomEvent('hd-catalog-ready', {
      detail: {catalog: catalogue, assets: source}
    }));
    const items=catalogue.items;
    const select=$('hd-bg-list');
    for(let i=0;i<Math.min(items.length,50);i++){
      const item=items[i];
      const option=document.createElement('option');
      option.value=String(i);
      option.textContent=item.id+' · BG'+(item.bg+1)+' · '+item.key.slice(-8);
      select.appendChild(option);
    }
    if(select.options.length)select.selectedIndex=0;
    $('hd-bg-count').textContent='已解析 '+items.length+' 筆官方背景圖層匹配條件；'+
      catalogue.missing.length+' 張圖片未找到。以下顯示前 50 筆。';
    updateBackgroundSelection();
  }catch(error){
    if(library!==source)return;
    $('hd-bg-count').textContent='場景索引無法讀取：'+(error?.message||String(error));
    $('hd-bg-details').textContent='HD 單張圖片按需讀取仍可正常使用。';
    document.dispatchEvent(new CustomEvent('hd-catalog-ready', {
      detail: {catalog: null, assets: null}
    }));
  }
}
function prepareLocalReader(file,info){
  library?.clearCache();
  library=new LocalHdAssets(file,info);
  bgCatalog=null;
  $('hd-asset-search').value='';
  $('hd-asset-browser').hidden=false;
  refreshAssetList();
  void loadBackgroundIndex(library);
  sessionStatus('本次分頁已就緒：'+library.images.length.toLocaleString('en-US')+
    ' 張 HD 圖片可按需讀取，無須複製整份 ZIP。關閉或重新載入頁面後可能需要再次選擇檔案。');
}
async function previewLocalImage(name){
  if(!library||!name||busy)return;
  busyMode(true);
  try{
    clearPreview();
    const data=await library.readImage(name);
    const png=name.toLowerCase().endsWith('.png');
    previewObjectUrl=URL.createObjectURL(new Blob([data],{type:png?'image/png':'image/webp'}));
    $('hd-preview-image').src=previewObjectUrl;
    $('hd-preview-name').textContent='由本機 ZIP 按需解壓：'+name+
      ' ('+(data.byteLength/1024).toFixed(0)+' KiB；本次分頁圖片快取 '+
      (library.cacheBytes/1048576).toFixed(1)+'／16 MiB)。此處僅為預覽，實際背景需 PPU 匹配後才會套用。';
    $('hd-preview-panel').hidden=false;
  }catch(error){
    status('本機 HD 圖片讀取失敗：'+(error?.message||String(error)));
  }finally{busyMode(false);}
}
$('hd-asset-search').addEventListener('input',refreshAssetList);
$('hd-asset-list').addEventListener('change',()=>{
  $('hd-preview-selected').disabled=busy||!$('hd-asset-list').value;
});
$('hd-preview-selected').addEventListener('click',()=>previewLocalImage($('hd-asset-list').value));

function explanation(result){
  return `官方 ${result.version} HD 美術包已通過 SHA-256 驗證。
${result.bytes.toLocaleString('en-US')} bytes，ZIP 內有 ${result.count.toLocaleString('en-US')} 個項目、${result.imageCount.toLocaleString('en-US')} 張 PNG/WebP。
官方原始素材 ZIP 沒有 hdpack.toml／index.bin 是預期情況；已支援背景匹配，人物與特效 HD 尚未完整整合。
驗證只在 iPhone 本機進行；匹配的背景將在載入後套用。`;
}

$('hd-zip').addEventListener('change',()=>{
  verified=null;activeFile=null;sampleEntry=null;persisted=false;library?.clearCache();library=null;bgCatalog=null;$('hd-bg-catalog').hidden=true;clearPreview();
  $('hd-asset-browser').hidden=true;
  document.dispatchEvent(new CustomEvent('hd-catalog-ready', {
    detail: {catalog: null, assets: null}
  }));
  sessionStatus('尚未啟用本次分頁的 HD 素材讀取。');
  $('hd-preview-btn').disabled=true;
  $('hd-progress').hidden=true;
  const file=$('hd-zip').files?.[0];
  $('hd-verify').disabled=!file;
  $('hd-cache').disabled=true;
  status(file?`已選擇 ${file.name}（${(file.size/1048576).toFixed(1)} MiB），請按「驗證 HD 美術包」。`:'尚未選擇 HD 美術包。');
});
$('hd-verify').addEventListener('click',async()=>{
  const file=$('hd-zip').files?.[0];
  if(!file||busy)return;
  busyMode(true);
  verified=null;activeFile=null;sampleEntry=null;persisted=false;library?.clearCache();library=null;clearPreview();
  $('hd-asset-browser').hidden=true;
  try {
    status('正在檢查 ZIP 結構...');
    // Import is deliberately strict: only the original signed GitHub asset digest is accepted.
    const checked=await verifyOfficialPack(file,{onProgress:(n,total)=>progress('正在驗證 SHA-256',n,total)});
    let extra='';
    if(checked.manifest){
      const index=await inspectZip(file);
      const entry=index.entries.find(x=>x.name===checked.manifest);
      if(entry?.unpacked<=256*1024){
        try {
          const raw=await extractZipEntry(file,entry,{maxBytes:256*1024});
          const text=new TextDecoder().decode(raw);
          const match=text.match(/^\s*format\s*=\s*(\d+)/m);
          extra=match?`\n素材索引 format=${match[1]}`:'';
        }catch(e){extra=`\n索引文字檢查未完成：${e.message}`;}
      }
    }
    verified=checked;activeFile=file;
    sampleEntry=previewEntry(checked);
    prepareLocalReader(file,checked);
    status(explanation(checked)+extra+'\n已可直接從本次選擇的 ZIP 按需讀取 HD 圖片，不需要儲存整包；重新開啟分頁可能要再選檔。');
    $('hd-progress').value=100;
  } catch(e){
    $('hd-progress').hidden=true;
    status('HD 素材包未通過驗證：'+e.message);
  } finally {busyMode(false);}
});

$('hd-preview-btn').addEventListener('click',()=>previewLocalImage(sampleEntry?.name));
$('hd-cache').addEventListener('click',async()=>{
  if(!verified||!activeFile||busy)return;
  busyMode(true);
  try {
    await saveVerifiedHdPack(activeFile,verified,{onProgress:(n,total)=>progress('正在存到 iPhone 本機 OPFS',n,total)});
    const confirmed=await readSavedHdPack();
    if(!confirmed)throw Error('寫入後無法重新讀取 HD 素材包，請保留原始 ZIP');
    persisted=true;
    status(explanation(verified)+'\n本機儲存成功，已重新讀取驗證。重新開啟相同網站會自動辨識；請保留原始 ZIP 作備份。');
    $('hd-progress').value=100;
  }catch(e){
    $('hd-progress').hidden=true;
    let diagnostic='';
    try{
      const estimate=await navigator.storage.estimate?.();
      if(estimate?.quota!=null&&estimate?.usage!=null) {
        const remaining=Math.max(0,estimate.quota-estimate.usage);
        diagnostic='\\nSafari 回報剩餘空間約 '+(remaining/1048576).toFixed(0)+' MiB（僅供估算）。';
      }
    }catch{}
    status('瀏覽器整包快取未成功：'+e.message+diagnostic+'\\n無須再重試：本次分頁仍可直接從已驗證的本機 ZIP 按需讀取 HD 圖片。');
  }
  finally{busyMode(false);}
});
$('hd-remove').addEventListener('click',async()=>{
  if(busy)return;
  busyMode(true);
  try {
    await clearSavedHdPack();
    persisted=false; verified=null; activeFile=null; sampleEntry=null;
    clearPreview();
    library?.clearCache();library=null;bgCatalog=null;
    document.dispatchEvent(new CustomEvent('hd-catalog-ready', {
      detail: {catalog: null, assets: null}
    }));
    status('本機快取已清除。請重新選擇 ZIP；「檔案」App 的原始檔案不受影響。');
    $('hd-progress').hidden=true;
  }catch(e){status('無法清除本機快取：'+e.message);}
  finally{busyMode(false);}
});

void (async()=>{
  if(!opfsAvailable()){
    status('目前瀏覽器沒有 OPFS，仍可從「檔案」App 選擇 ZIP，完成本機驗證。');
    return;
  }
  try {
    const cached=await readSavedHdPack();
    if(cached) {
      const zip=await inspectZip(cached.file);
      activeFile=cached.file;
      verified={...OFFICIAL_PACK,count:zip.count,imageCount:zip.imageCount,manifest:zip.manifest,index:zip.index,entries:zip.entries};
      sampleEntry=previewEntry(verified);
      persisted=true;
      prepareLocalReader(cached.file,verified);
      busyMode(false);
      status('已自動恢復上次儲存在 iPhone 的官方 '+OFFICIAL_PACK.version+' HD 素材包（'+(cached.file.size/1048576).toFixed(1)+' MiB，'+zip.count.toLocaleString('en-US')+' 個檔案）。\n可直接預覽 HD 圖片，無須重新選擇 ZIP。上次匯入時已驗證 SHA-256；本次僅核對大小和 ZIP 目錄。\n已支援匹配背景；人物、選單 HD 美術尚未完整整合。');
    } else {
      status('未找到此網站先前保存的 HD 素材包。請匯入並驗證 ZIP；只驗證並不會自動保存。');
    }
  }catch(e){status('讀取舊版 HD 快取失敗：'+e.message);}
})();
