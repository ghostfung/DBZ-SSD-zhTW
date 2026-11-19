//! Exact-plane Mode 7 art in the on-device web build.
//! The original SNES matrix is used at each 2x subpixel: no stretched screenshot.
//! Art is refused on unknown VRAM content, palette changes, or window transitions.
use serde::Deserialize;
use ssd_hd::{mode7art::{self,Index,Entry},LineRegs};
use ssd_oracle::FrameLog;
use std::collections::HashMap;

const SIDE:usize=512;
const BYTES:usize=SIDE*SIDE*4;
const MAX_PLANES:usize=48;
const MAX_TILES:usize=16;

#[derive(Deserialize)]
struct Input {
    id:String,key:String,tiles:String,palette:Vec<String>,
}
#[derive(Clone)]
struct Active {index:usize,wanted:Vec<(usize,usize)>}
#[derive(Default)]
pub(crate) struct LiveMode7 {
    index:Index,
    active:Option<Active>,
    tiles:HashMap<(usize,usize,usize),Vec<u8>>,
    reason:&'static str,
    matching_key:u64,
    matching_palette:u64,
    // Frame-scoped verification. Reuse it for foreground/underpass HD
    // composites instead of rehashing 16K VRAM words two or three times.
    frame_key:Option<(u64,u64)>,
    frame_rows:Vec<LineRegs>,
    // Identical unsupported Mode 7 planes must not run expensive art index
    // and palette matching on every 60Hz simulation frame.
    negative_key:Option<(u64,u64)>,
}

fn safe_template(path:&str)->bool {
    path.starts_with("mode7/") && path.len()<=220
        && !path.contains("..") && !path.contains('\\')
        && path.contains("{x}") && path.contains("{y}")
        && (path.ends_with(".png") || path.ends_with(".webp"))
}
fn palette_key(cgram:&[u16])->u64{
    let mut hash=0xcbf2_9ce4_8422_2325_u64;
    for word in cgram.iter().take(256) {
        for byte in word.to_le_bytes() {
            hash=(hash^u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}
fn lines(log:&FrameLog)->Option<Vec<LineRegs>> {
    if !log.complete || !log.vram_writes.is_empty() ||
        !log.cgram_writes.is_empty() || !log.oam_writes.is_empty()
        || log.vram.len()<0x4000 || log.cgram_start.len()<256
        || !(1..=239).contains(&log.height) {return None;}
    let h=usize::from(log.height);
    let mut rows=vec![None;h];
    for e in &log.entries {
        if e.scanline==0 || usize::from(e.scanline)>h {continue;}
        if e.from_pixel!=0 {return None;} // no guessed midline matrix
        if e.regs.bg_mode!=7 || e.regs.forced_blanking ||
            e.regs.brightness!=15 || e.regs.bg_color_math[0] ||
            e.regs.mosaic_bg[0] {return None;}
        let row=&mut rows[usize::from(e.scanline)-1];
        if row.is_some() {return None;}
        *row=Some(e.regs);
    }
    rows.into_iter().collect()
}
impl LiveMode7 {
    pub(crate) fn has_entries(&self)->bool{!self.index.plane.is_empty()}
    pub(crate) fn has_pending_art(&self)->bool {
        self.active.as_ref().is_some_and(|a|
            a.wanted.iter().any(|&(x,y)|
                !self.tiles.contains_key(&(a.index,x,y))))
    }
    pub(crate) fn is_active(&self)->bool{self.active.is_some()}
    pub(crate) fn needs_attention(&mut self,log:&FrameLog)->bool{
        // Fail closed on a scene transition: cached proof must never be
        // reused for a frame with different PPU rows or palette.
        self.frame_key=None;
        self.frame_rows.clear();
        // Most scenes are not Mode 7. Avoid touching VRAM at all there.
        if log.entries.first().is_none_or(|e|e.regs.bg_mode!=7) {
            return self.active.is_some();
        }
        let Some(rows)=lines(log) else {return self.active.is_some()};
        self.frame_rows=rows;
        let fingerprint=(mode7art::plane_key(&log.vram),
            palette_key(&log.cgram_start));
        self.frame_key=Some(fingerprint);
        if self.active.is_none() {
            return self.negative_key!=Some(fingerprint);
        }
        if fingerprint!=(self.matching_key,self.matching_palette){
            return true;
        }
        // Camera HDMA can reveal previously unseen texture tiles, so a
        // matching but still-loading plane is refreshed to discover them.
        self.has_pending_art()
    }
    pub(crate) fn load_catalog(&mut self,json:&str)->Result<usize,String>{
        let list:Vec<Input>=serde_json::from_str(json)
            .map_err(|e|format!("Mode7 HD 索引格式錯誤：{e}"))?;
        if list.len()>MAX_PLANES {return Err("Mode7 索引超過安全上限".into());}
        let mut planes=Vec::new();
        for p in list {
            if p.id.is_empty() || p.id.len()>128 ||
                !safe_template(&p.tiles) ||
                p.key.len()!=16 || u64::from_str_radix(&p.key,16).is_err() ||
                p.palette.len()!=256 ||
                p.palette.iter().any(|c|c.len()!=6||u32::from_str_radix(c,16).is_err())
            {return Err("Mode7 素材路徑、key 或調色盤不合法".into());}
            planes.push(Entry{id:p.id,key:p.key,tiles:p.tiles,
                palette:p.palette,replace:false});
        }
        self.index=Index{plane:planes};
        self.active=None;self.tiles.clear();self.reason="等待 Mode7 VRAM 核對";
        self.negative_key=None;self.frame_key=None;self.frame_rows.clear();
        Ok(self.index.plane.len())
    }
    pub(crate) fn refresh(&mut self,log:&FrameLog){
        // Same exact VRAM and CGRAM identity: reuse the already verified
        // plane index while recalculating visible tiles from current HDMA.
        // A new plane or any palette shift still takes the full verification.
        let previous=self.active.as_ref()
            .filter(|_|self.frame_key==Some((self.matching_key,self.matching_palette)))
            .map(|a|a.index);
        self.active=None;
        self.negative_key=None;
        self.reason="Mode7 畫面尚未驗證";
        let Some(fingerprint)=self.frame_key else {
            self.reason="Mode7 畫面非完整或正在轉場";return;};
        let rows=&self.frame_rows;
        if rows.len()!=usize::from(log.height) {
            self.reason="Mode7 掃描線尚未完成";return;
        }
        let Some(index)=previous.or_else(||
            self.index.find_plane(&log.vram,&log.cgram_start)) else {
            self.negative_key=Some(fingerprint);
            self.reason="Mode7 VRAM plane 不存在於官方索引";return;};
        // Palette verification is reusable only with byte-identical VRAM
        // and CGRAM hashes. On any change, recompute against official art.
        if previous.is_none() {
            let Some(colours)=self.index.plane[index].colours() else {
                self.reason="Mode7 調色盤不存在";return;
            };
            if mode7art::colours_agree(&log.vram,&log.cgram_start,&colours)!=1000 {
                self.negative_key=Some(fingerprint);
                self.reason="Mode7 當前 palette 與原始 HD 美術不一致";return;
            }
        }
        let mut used=std::collections::HashSet::new();
        let h=rows.len();
        // Find required 256x256 art cells from the same per-line SNES matrix
        // which will later be used for subpixel compositing.
        for y in (0..h).step_by(8).chain(std::iter::once(h-1)) {
            for x in (0..256_usize).step_by(8).chain(std::iter::once(255)) {
                let r=&rows[y];
                if !(r.main_bg[0] || r.sub_bg[0]){continue;}
                if let Some((wx,wy))=ssd_hd::live_mode7_point(r,x as i32,(y+1) as u16,0,0){
                    used.insert(((wx as usize>>8)/256,(wy as usize>>8)/256));
                }
            }
        }
        if used.is_empty(){self.reason="Mode7 BG1 未顯示";return;}
        if used.len()>MAX_TILES {self.reason="Mode7 可見圖塊超出上限";return;}
        let mut wanted=used.into_iter().collect::<Vec<_>>();
        wanted.sort_unstable();
        self.matching_key=fingerprint.0;
        self.matching_palette=fingerprint.1;
        self.active=Some(Active{index,wanted});
        self.reason="Mode7 完整 VRAM 與 palette 均匹配";
    }
    pub(crate) fn matches_json(&self)->String{
        let Some(a)=&self.active else{return "[]".into()};
        let e=&self.index.plane[a.index];
        serde_json::Value::Array(a.wanted.iter().map(|&(x,y)|serde_json::json!({
            "index":a.index,"x":x,"y":y,"id":e.id,
            "art":e.tile_file(x,y),
            "ready":self.tiles.contains_key(&(a.index,x,y))
        })).collect()).to_string()
    }
    pub(crate) fn probe_json(&self)->String{
        serde_json::json!({"reason":self.reason,
            "matched":self.active.as_ref().map_or(0,|a|a.wanted.len())}).to_string()
    }
    pub(crate) fn add_tile(&mut self,index:usize,x:usize,y:usize,w:usize,h:usize,
        rgba:&[u8])->Result<(),String>{
        let Some(a)=&self.active else{return Err("Mode7 場景已切換".into())};
        if index!=a.index || !a.wanted.contains(&(x,y))|| x>3||y>3{
            return Err("Mode7 圖塊不屬於目前精確匹配場景".into());
        }
        if w!=SIDE || h!=SIDE || rgba.len()!=BYTES {
            return Err("Mode7 HD tile 必須為 512×512（2x）".into());
        }
        self.tiles.insert((index,x,y),rgba.to_vec());
        if self.tiles.len()>MAX_TILES {
            self.tiles.retain(|&(i,xx,yy),_| i==index && a.wanted.contains(&(xx,yy)));
        }
        Ok(())
    }
    pub(crate) fn composite(&self,log:&FrameLog,layers:&[u8],
        rgba:&mut[u8],w:usize,h:usize)->u32{
        let Some(a)=&self.active else{return 0};
        let rows=&self.frame_rows;
        if self.frame_key!=Some((self.matching_key,self.matching_palette)) ||
            w!=512||h!=rows.len()*2||rgba.len()!=w*h*4||
            layers.len()!=rows.len()*256 {return 0;}
        let mut changed=0;
        for (y,r) in rows.iter().enumerate(){
            for x in 0..256_usize{
                if layers[y*256+x]!=1 {continue;} // never cover sprites, HUD, or BG2
                let mut different=false;
                for sy in 0..2_u8{
                    for sx in 0..2_u8{
                        let Some((wx,wy))=ssd_hd::live_mode7_point(
                            r,x as i32,(y+1) as u16,sx,sy)else{continue;};
                        let (mx,my)=((wx as usize>>8),(wy as usize>>8));
                        let (tx,ty)=(mx/256,my/256);
                        let Some(img)=self.tiles.get(&(a.index,tx,ty))else{continue};
                        // Mode 7 rotates/scales the plane. Sample the
                        // plane's own subpixel coordinate, NOT screen sx/sy.
                        let src=(((wy as usize>>7)%SIDE)*SIDE+
                            ((wx as usize>>7)%SIDE))*4;
                        let dst=((y*2+sy as usize)*w+x*2+sx as usize)*4;
                        let alpha=u32::from(img[src+3]);
                        if alpha==0 {continue;}
                        for ch in 0..3{
                            let old=u32::from(rgba[dst+ch]);
                            let value=((u32::from(img[src+ch])*alpha+old*(255-alpha)+127)/255) as u8;
                            different|=value!=rgba[dst+ch];
                            rgba[dst+ch]=value;
                        }
                        rgba[dst+3]=255;
                    }
                }
                changed+=u32::from(different);
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssd_oracle::{LineEntry,SpriteLine};
    #[test]
    fn invalid_or_unsupported_planes_are_rechecked_only_on_identity_change(){
        let mut live=LiveMode7::default();
        let mut log=FrameLog{
            complete:true,height:1,h_hi_res:false,v_hi_res:false,
            entries:vec![],cgram_start:vec![0;256],cgram_writes:vec![],
            vram:vec![0;32768],vram_writes:vec![],
            oam_low:vec![0;512],oam_high:vec![0;32],oam_writes:vec![]
        };
        // Non-Mode7 frames never compute a full-plane lookup.
        assert!(!live.needs_attention(&log));
        log.entries.push(LineEntry{
            scanline:1,from_pixel:0,
            regs:LineRegs{bg_mode:7,brightness:15,main_bg:[true,false,false,false],
                ..LineRegs::default()},
            sprites:SpriteLine::default()
        });
        assert!(live.needs_attention(&log));
        live.refresh(&log);
        assert!(live.negative_key.is_some());
        assert!(!live.needs_attention(&log));
        log.cgram_start[0]=31;
        assert!(live.needs_attention(&log),
            "palette changes must invalidate previous negative lookups");
        log.vram[0]=0xFFFF;
        assert!(live.needs_attention(&log),
            "VRAM changes must not reuse another plane's HD matching");
        log.entries[0].regs.bg_mode=1;
        assert!(!live.needs_attention(&log));
        assert!(live.frame_key.is_none());
    }
}
