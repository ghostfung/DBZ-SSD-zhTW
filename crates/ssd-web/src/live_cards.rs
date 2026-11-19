//! Exact 2bpp tile-bound HD card art. The official cards assets are two
//! composited layers: base (fixed RGB) and roles (weights for palette 3/2/1).
//! Never match by a screenshot colour or card position.
use serde::Deserialize;
use ssd_hd::{pack::tile_key, LiveTilePixel};
use ssd_oracle::FrameLog;
use std::collections::{HashMap, HashSet};

const W: usize=256;
const SIDE: usize=16;
const TILE_BYTES:usize=SIDE*SIDE*4;
const UI_OFFSET:usize=0x11*0x8000+0x36;
const MAX_ENTRIES:usize=152; // Upstream CARD_BASE=104, u8::MAX - 104 + 1 = 152.
const MAX_VISIBLE:usize=140;

#[derive(Debug, Deserialize)]
struct Input {
    code:u16,
    #[serde(default)] set:Option<String>,
    #[serde(default)] carve:bool,
    #[serde(default)] base:Option<String>,
    #[serde(default)] roles:Option<String>,
    #[serde(default)] available:bool,
}
struct Entry {
    code:u16, title:bool, carve:bool, base:Option<String>, roles:Option<String>,
}
#[derive(Clone)]
struct Match {index:usize,x:usize,y:usize}
#[derive(Default)]
pub(crate) struct LiveCards {
    entries:Vec<Entry>,
    keys:HashMap<u64,usize>,
    art:HashMap<(usize,u8),Vec<u8>>,
    active:Vec<Match>,
    probes:usize,recognised:usize,
    reason:&'static str,
}
fn source_key(rom:&[u8],code:u16,title:Option<&[u8]>)->Option<u64>{
    let at=usize::from(code)*16;
    let bytes=match title{
        Some(sheet)=>sheet.get(at..at+16)?,
        None=>rom.get(UI_OFFSET+at..UI_OFFSET+at+16)?
    };
    Some(tile_key(bytes))
}
fn visible_key(log:&FrameLog,p:LiveTilePixel,cache:&mut HashMap<u16,u64>)->Option<u64>{
    if p.bpp!=2 || p.address%8!=0 {return None}
    let at=usize::from(p.address);
    let words=log.vram.get(at..at+8)?;
    Some(*cache.entry(p.address).or_insert_with(||{
        let mut b=[0_u8;16];
        for (i,&word) in words.iter().enumerate(){
            b[i*2..i*2+2].copy_from_slice(&word.to_le_bytes());
        }
        tile_key(&b)
    }))
}
fn row_colors(log:&FrameLog,p:LiveTilePixel)->Option<[[u8;3];4]>{
    if p.layer==0 || p.layer==5 || log.cgram_start.len()<256 {return None;}
    // Mode 0 uses 2bpp per BG and 4 palette entries. Mode 1 uses
    // 2bpp BG3; BG1/2 are 4bpp and deliberately cannot reach here.
    let mode=log.entries.first()?.regs.bg_mode;
    let offset=match(mode,p.layer){
        (0,1)=>0,(0,2)=>0x20,(0,3)=>0x40,(0,4)=>0x60,
        (1,3)=>0,_=>return None
    };
    let base=offset+usize::from(p.palette)*4;
    if base+4>256{return None;}
    Some(std::array::from_fn(|i|{
        let w=log.cgram_start[base+i];
        let ch=|v:u16|((u32::from(v&31)*255+15)/31) as u8;
        [ch(w),ch(w>>5),ch(w>>10)]
    }))
}
impl LiveCards{
    pub(crate) fn has_entries(&self)->bool{!self.keys.is_empty()}
    pub(crate) fn load_catalog(&mut self,json:&str,rom:&[u8])->Result<usize,String>{
        let input:Vec<Input>=serde_json::from_str(json)
            .map_err(|e|format!("HD 卡牌索引格式錯誤：{e}"))?;
        if input.len()>MAX_ENTRIES{return Err("HD 卡牌索引超過渲染字形上限".into());}
        let title=ssd_catalogue::graphics::lz_decode(rom,
            ssd_catalogue::RomView::new(rom).map_err(|e|e.to_string())?
                .offset(0x00_df39).ok_or("找不到標題圖塊來源")?)
            .map(|mut x|{ssd_catalogue::graphics::regroup(&mut x.data);x.data});
        let mut entries=Vec::new();
        let mut keys=HashMap::new();
        let mut duplicates=HashSet::new();
        for item in input {
            let is_title=matches!(item.set.as_deref(),Some("title"));
            let safe=|p:&str|p.starts_with("cards/")&&p.len()<=220&&
                !p.contains("..")&&!p.contains('\\')&&
                (p.ends_with(".png")||p.ends_with(".webp"));
            if item.set.as_deref().is_some_and(|v|v!="title") ||
                item.code>=if is_title {256} else {384} ||
                item.base.as_deref().is_some_and(|v|!safe(v))||
                item.roles.as_deref().is_some_and(|v|!safe(v))
            {return Err("HD 卡牌資料含無效字形碼或圖片路徑".into());}
            if !item.available || item.base.is_none() && item.roles.is_none(){continue;}
            let Some(key)=source_key(rom,item.code,if is_title{title.as_deref()}else{None})
                else{continue};
            // A single PPU tile hash must identify one code unambiguously.
            // Repeated blank/font tiles are never guessed as card artwork.
            let i=entries.len();
            if keys.contains_key(&key) || duplicates.contains(&key) {
                duplicates.insert(key);
            } else { keys.insert(key,i); }
            entries.push(Entry{code:item.code,title:is_title,carve:item.carve,
                base:item.base,roles:item.roles});
        }
        for key in duplicates {keys.remove(&key);}
        self.keys=keys;self.entries=entries;self.art.clear();
        self.active.clear();self.probes=0;self.recognised=0;
        self.reason="等待可見 2bpp 圖塊";
        Ok(self.entries.len())
    }
    pub(crate) fn refresh(&mut self,log:&FrameLog,samples:&[LiveTilePixel]){
        self.active.clear();self.probes=0;self.recognised=0;
        if self.keys.is_empty(){self.reason="無唯一的 2bpp 卡牌圖塊";return;}
        if !log.complete || !log.vram_writes.is_empty() ||
           !log.cgram_writes.is_empty() ||samples.len()!=W*usize::from(log.height){
            self.reason="PPU 或 VRAM／CGRAM 正在變動";return;
        }
        let mut cache=HashMap::new();
        let mut found=HashSet::new();
        let h=usize::from(log.height);
        for y in (0..h).step_by(4){
            for x in (0..W).step_by(4){
                let p=samples[y*W+x];
                if p.bpp!=2 || p.layer==0 || p.layer==5 {continue;}
                let Some(key)=visible_key(log,p,&mut cache)else{continue};
                self.probes+=1;
                let Some(&index)=self.keys.get(&key)else{continue};
                if row_colors(log,p).is_none(){continue}
                let dx=if p.pos&0x40!=0{7-(p.pos&7)}else{p.pos&7};
                let dy=if p.pos&0x80!=0{7-((p.pos>>3)&7)}else{(p.pos>>3)&7};
                let (left,top)=(x as i32-i32::from(dx),y as i32-i32::from(dy));
                if left<0||top<0||left+8>W as i32||top+8>h as i32{continue;}
                found.insert((index,left as usize,top as usize));
            }
        }
        for (index,x,y) in found.into_iter().take(MAX_VISIBLE){
            self.active.push(Match{index,x,y});
        }
        self.recognised=self.active.len();
        self.reason=if self.recognised==0{"無符合卡牌索引的唯一圖塊"}
            else{"已核對可見 2bpp 原始圖塊"};
    }
    pub(crate) fn matches_json(&self)->String{
        serde_json::Value::Array(self.active.iter().map(|m|{
            let e=&self.entries[m.index];
            let base=e.base.as_ref().map(|art|serde_json::json!({
                "index":m.index,"layer":0,"art":art,
                "ready":self.art.contains_key(&(m.index,0))}));
            let roles=e.roles.as_ref().map(|art|serde_json::json!({
                "index":m.index,"layer":1,"art":art,
                "ready":self.art.contains_key(&(m.index,1))}));
            let layers=[base,roles].into_iter().flatten().collect::<Vec<_>>();
            serde_json::json!({"index":m.index,"code":e.code,
                "title":e.title,"x":m.x,"y":m.y,"layers":layers})
        }).collect()).to_string()
    }
    pub(crate) fn probe_json(&self)->String{
        serde_json::json!({"reason":self.reason,"candidate":self.probes,
            "matched":self.recognised}).to_string()
    }
    pub(crate) fn add_art(&mut self,index:usize,layer:u8,width:usize,height:usize,
        rgba:&[u8])->Result<(),String>{
        let Some(e)=self.entries.get(index)else{return Err("卡牌索引不存在".into())};
        if layer>1 || !self.active.iter().any(|m|m.index==index) ||
           (if layer==0{e.base.is_none()}else{e.roles.is_none()}) {
            return Err("卡牌已切換，拒絕過期素材".into())
        }
        if width!=SIDE||height!=SIDE||rgba.len()!=TILE_BYTES{
            return Err("卡牌 tile 圖像必須是 16×16 (2x)".into())
        }
        self.art.insert((index,layer),rgba.to_vec());
        Ok(())
    }
    pub(crate) fn composite(&self,log:&FrameLog,samples:&[LiveTilePixel],
        rgba:&mut[u8],width:usize,height:usize)->u32{
        if width!=512||height!=usize::from(log.height)*2||
           samples.len()!=W*usize::from(log.height)||
           rgba.len()!=width*height*4 || !log.vram_writes.is_empty() ||
           !log.cgram_writes.is_empty(){return 0}
        let mut cache=HashMap::new();let mut changed=0;
        for m in &self.active {
            let e=&self.entries[m.index];
            let base=self.art.get(&(m.index,0));
            let roles=self.art.get(&(m.index,1));
            if base.is_none()&&roles.is_none(){continue;}
            for cy in 0..8 {
                for cx in 0..8 {
                    let x=m.x+cx;let y=m.y+cy;
                    let p=samples[y*W+x];
                    let Some(key)=visible_key(log,p,&mut cache)else{continue};
                    if self.keys.get(&key)!=Some(&m.index){continue}
                    let dx=if p.pos&0x40!=0{7-(p.pos&7)}else{p.pos&7};
                    let dy=if p.pos&0x80!=0{7-((p.pos>>3)&7)}else{(p.pos>>3)&7};
                    if usize::from(dx)!=cx||usize::from(dy)!=cy{continue;}
                    let Some(colors)=row_colors(log,p)else{continue;};
                    let mut different=false;
                    for sy in 0..2 {
                        for sx in 0..2 {
                            let ix=usize::from(p.pos&7)*2+if p.pos&0x40!=0{1-sx}else{sx};
                            let iy=usize::from((p.pos>>3)&7)*2+if p.pos&0x80!=0{1-sy}else{sy};
                            let src=(iy*SIDE+ix)*4;
                            let dst=((y*2+sy)*width+x*2+sx)*4;
                            let before=[rgba[dst],rgba[dst+1],rgba[dst+2]];
                            if e.carve{rgba[dst..dst+3].copy_from_slice(&colors[1]);}
                            let mut blend=|fg:[u8;3],alpha:u8|{
                                let a=u32::from(alpha);
                                for ch in 0..3 {
                                    let old=u32::from(rgba[dst+ch]);
                                    rgba[dst+ch]=((u32::from(fg[ch])*a+old*(255-a)+127)/255) as u8;
                                }
                            };
                            if let Some(img)=base{
                                // Upstream base art only appears where colour 1 is white.
                                if colors[1].iter().all(|&c|c>=245){
                                    blend([img[src],img[src+1],img[src+2]],img[src+3]);
                                }
                            }
                            if let Some(img)=roles{
                                let weights=[u32::from(img[src]),u32::from(img[src+1]),
                                    u32::from(img[src+2])];
                                let sum=weights.iter().sum::<u32>().max(1);
                                let fg=std::array::from_fn(|ch|{
                                    let num=weights[0]*u32::from(colors[3][ch])+
                                        weights[1]*u32::from(colors[2][ch])+
                                        weights[2]*u32::from(colors[1][ch]);
                                    ((num+sum/2)/sum) as u8
                                });
                                blend(fg,img[src+3]);
                            }
                            rgba[dst+3]=255;
                            different|=rgba[dst..dst+3]!=before[..];
                        }
                    }
                    changed+=u32::from(different);
                }
            }
        }
        changed
    }
}
