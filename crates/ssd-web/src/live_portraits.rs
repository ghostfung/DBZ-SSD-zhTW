//! Exact class1 portrait/face HD compositing for the on-device iPhone Web build.
//! Source tiles come only from the player's normalized ROM; approved pictures
//! come only from the player's verified, local v0.3.9 HD ZIP.
//! Never infer a character from a screen coordinate or a portrait label.
use serde::Deserialize;
use ssd_hd::{pack::tile_key, LiveTilePixel};
use ssd_oracle::FrameLog;
use std::collections::{HashMap, HashSet};

const WIDTH: usize = 256;
const MAX_ENTRIES: usize = 512;
// Namek's party status menu renders four faces on each side (eight total).
// A six-match cap randomly excluded two validated portraits each refresh.
const MAX_ACTIVE: usize = 8;
const MAX_ART_BYTES: usize = 64 * 64 * 4;
const CELLS: [[usize; 16]; 2] = [
    [0,1,2,3,16,17,18,19,4,5,6,7,20,21,22,23],
    [8,9,10,11,24,25,26,27,12,13,14,15,28,29,30,31],
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Incoming {
    id: String,
    source: u32,
    half: usize,
    art: String,
    available: bool,
    palette: Vec<Option<[u8;3]>>,
}

struct Entry {
    id: String,
    art: String,
    keys: [u64; 16],
    palette: Vec<Option<[u8; 3]>>,
}

struct Picture {
    rgba: Vec<u8>,
}

#[derive(Clone)]
struct Matched {
    index: usize,
    x: usize,
    y: usize,
}

#[derive(Default)]
pub(crate) struct LivePortraits {
    entries: Vec<Entry>,
    lookup: HashMap<u64, Vec<(usize, usize)>>,
    pictures: HashMap<usize, Picture>,
    active: Vec<Matched>,
    probes: u32,
    recognised: u32,
    weak: u32,
    palette_rejected: u32,
    source_write_rejected: u32,
    reason: &'static str,
}

fn source_keys(rom: &[u8], source: u32, half: usize) -> Option<[u64;16]> {
    let offset=ssd_catalogue::RomView::new(rom).ok()?.offset(source)?;
    let mut stream=ssd_catalogue::graphics::lz_decode(rom,offset)?.data;
    if stream.len()<32*32 || stream.len()%32!=0 { return None; }
    ssd_catalogue::graphics::regroup(&mut stream);
    Some(std::array::from_fn(|cell| {
        let tile=CELLS[half][cell];
        tile_key(&stream[tile*32..(tile+1)*32])
    }))
}

fn current_key(vram: &[u16], sample: LiveTilePixel, cache: &mut HashMap<u16,u64>) -> Option<u64> {
    if sample.bpp!=4 { return None; }
    if sample.address % 16 != 0 { return None; }
    let address=usize::from(sample.address);
    if address+16>vram.len() {return None;}
    Some(*cache.entry(sample.address).or_insert_with(|| {
        let mut bytes=[0_u8;32];
        for (i,word) in vram[address..address+16].iter().enumerate(){
            bytes[i*2..i*2+2].copy_from_slice(&word.to_le_bytes());
        }
        tile_key(&bytes)
    }))
}

fn palette_distance(entry: &Entry, sample: LiveTilePixel, cgram: &[u16]) -> Option<u32> {
    let offset=if sample.layer==5 {128} else {0};
    let base=offset+usize::from(sample.palette)*16;
    if base+16>cgram.len() {return None;}
    let mut sum=0;
    let mut count=0;
    for (i,source) in entry.palette.iter().enumerate().take(16) {
        if i==0 {continue;}
        if let Some(source)=source {
            let word=cgram[base+i];
            let actual=[
                (((u32::from(word&31))*255+15)/31) as u8,
                (((u32::from((word>>5)&31))*255+15)/31) as u8,
                (((u32::from((word>>10)&31))*255+15)/31) as u8,
            ];
            sum+=source.iter().zip(actual).map(|(a,b)| u32::from(a.abs_diff(b))).sum::<u32>();
            count+=1;
        }
    }
    // The upstream has unknown-palette entries. Accept those only on a
    // strong multi-tile source match, not a guessed RGB or character ID.
    (count==0 || sum <= 150).then_some(count)
}

/// A cutscene may stream unrelated glyphs or animate unrelated palettes
/// mid-frame. Reject a portrait only when a write overlaps one of its
/// *verified visible* source tiles or its own CGRAM bank. Global write flags
/// used to blank otherwise unchanged faces during dialogue animations.
fn source_changed_for_portrait(
    log: &FrameLog, samples: &[LiveTilePixel], entry: &Entry, x: usize, y: usize,
) -> bool {
    if log.vram_writes.is_empty() && log.cgram_writes.is_empty() { return false; }
    let mut cache=HashMap::new();
    let mut tile_words=HashSet::new();
    let mut palettes=HashSet::new();
    for cy in 0..32_usize {
        for cx in 0..32_usize {
            let p=samples[(y+cy)*WIDTH+x+cx];
            if p.layer==0 || p.bpp!=4 {continue;}
            let dx=if p.pos&0x40!=0 {7-(p.pos&7)} else {p.pos&7};
            let dy=if p.pos&0x80!=0 {7-((p.pos>>3)&7)} else {(p.pos>>3)&7};
            if usize::from(dx)!=cx%8 || usize::from(dy)!=cy%8
                || current_key(&log.vram,p,&mut cache)
                    !=Some(entry.keys[(cy/8)*4+cx/8])
            { continue; }
            tile_words.insert(usize::from(p.address));
            palettes.insert((if p.layer==5 {128} else {0})
                +usize::from(p.palette)*16);
        }
    }
    log.vram_writes.iter().any(|write| {
        let at=write.address as usize;
        tile_words.iter().any(|base|at>=*base && at<*base+16)
    }) || log.cgram_writes.iter().any(|write| {
        let at=write.index as usize;
        palettes.iter().any(|base|at>=*base && at<*base+16)
    })
}

impl LivePortraits {
    pub(crate) fn load_catalog(&mut self, json: &str, rom: &[u8]) -> Result<usize,String> {
        let incoming:Vec<Incoming>=serde_json::from_str(json)
            .map_err(|e|format!("HD 頭像索引格式錯誤：{e}"))?;
        if incoming.len()>MAX_ENTRIES {return Err("HD 頭像數量超過安全上限".into());}
        let mut entries=Vec::new();
        let mut lookup:HashMap<u64,Vec<(usize,usize)>>=HashMap::new();
        for item in incoming {
            if item.id.len()>160 || item.id.is_empty() || item.half>1
                || item.source>>16 != 0x16 || item.source&0xffff < 0x8000
                || !item.art.starts_with("class1/")
                || item.art.contains("..") || item.art.contains('\\')
                || item.art.len()>240
                || !(item.art.ends_with(".png") || item.art.ends_with(".webp"))
                || item.palette.len()!=16
            {return Err("HD 頭像索引包含不安全的項目".into());}
            if !item.available {continue;}
            let Some(keys)=source_keys(rom,item.source,item.half) else {continue};
            // A valid face has several identifiable characters; skip streams
            // whose 4x4 picture is entirely blank or repeated.
            let distinct=keys.iter().copied().collect::<HashSet<_>>().len();
            if distinct<5 {continue;}
            let index=entries.len();
            for (cell,key) in keys.iter().copied().enumerate() {
                lookup.entry(key).or_default().push((index,cell));
            }
            entries.push(Entry{id:item.id,art:item.art,keys,palette:item.palette});
        }
        self.entries=entries;
        self.lookup=lookup;
        self.active.clear();
        self.pictures.clear();
        self.probes=0;
        self.recognised=0;
        self.weak=0;
        self.palette_rejected=0;
        self.source_write_rejected=0;
        self.reason="尚未掃描";
        Ok(self.entries.len())
    }

    pub(crate) fn has_entries(&self)->bool { !self.entries.is_empty() }

    /// Transitions get checked twice as often as a stable matched frame.
    /// Still bounded: a missing class1 asset cannot force a full scan at 60 Hz.
    pub(crate) fn has_pending_art(&self)->bool {
        self.active.iter().any(|m| !self.pictures.contains_key(&m.index))
    }

    pub(crate) fn needs_attention(&self)->bool {
        self.active.is_empty() ||
            self.active.iter().any(|m|!self.pictures.contains_key(&m.index))
    }

    pub(crate) fn refresh(&mut self, log: &FrameLog, samples: &[LiveTilePixel]) {
        self.active.clear();
        self.probes=0;
        self.recognised=0;
        self.weak=0;
        self.palette_rejected=0;
        self.source_write_rejected=0;
        self.reason="尚未匹配";
        if self.lookup.is_empty(){self.reason="沒有可用的 class1 ROM 圖塊";return;}
        if !log.complete {self.reason="PPU 畫面紀錄未完整";return;}
        if samples.len()!=WIDTH*usize::from(log.height){
            self.reason="PPU 圖塊追蹤資料不足";return;
        }
        let h=usize::from(log.height);
        if h<32 {return;}
        let mut cache=HashMap::new();
        let mut candidate=HashSet::new();
        // PPU pictures in cutscenes sometimes expose only thin slivers of
        // original pixels. Try a 2-pixel lattice only if the usual 4-pixel
        // lattice finds no candidate at all. No guessed character coordinates.
        for step in [4_usize,2_usize] {
            for y in (0..h).step_by(step) {
                for x in (0..WIDTH).step_by(step) {
                    let p=samples[y*WIDTH+x];
                    if p.bpp!=4 || p.layer==0 {continue;}
                    let Some(key)=current_key(&log.vram,p,&mut cache) else {continue};
                    let Some(cells)=self.lookup.get(&key) else {continue};
                    // The PPU position already reflects tile mirroring. Undo
                    // a tile flip only for deriving the on-screen tile origin.
                    let dx=if p.pos&0x40!=0 {7-(p.pos&7)} else {p.pos&7};
                    let dy=if p.pos&0x80!=0 {7-((p.pos>>3)&7)} else {(p.pos>>3)&7};
                    for &(index,cell) in cells {
                        let left=x as i32-i32::from(dx)-(cell%4*8) as i32;
                        let top=y as i32-i32::from(dy)-(cell/4*8) as i32;
                        if left>=0 && top>=0 && left<=224 && top+32<=h as i32 {
                            candidate.insert((index,left as usize,top as usize));
                        }
                    }
                }
            }
            if !candidate.is_empty(){break;}
        }
        self.probes=candidate.len() as u32;
        let mut found=Vec::new();
        for (index,x,y) in candidate {
            let e=&self.entries[index];
            let mut matched=0;
            let mut disagree=0;
            let mut rows=0_u8;
            let mut observed=None;
            for cell in 0..16_usize {
                let (cx,cy)=(x+(cell%4)*8,y+(cell/4)*8);
                let mut recognised=false;
                let mut covered=false;
                for &(dx,dy) in &[(3,3),(1,1),(6,6),(4,2),(2,5),(0,0),(7,7),(0,7),(7,0)] {
                    let p=samples[(cy+dy)*WIDTH+cx+dx];
                    if p.bpp!=4 || p.layer==0 {continue;}
                    covered=true;
                    if current_key(&log.vram,p,&mut cache)==Some(e.keys[cell]){
                        recognised=true;
                        observed.get_or_insert(p);
                        break;
                    }
                }
                if recognised {matched+=1;rows|=1<<(cell/4);}
                else if covered {disagree+=1;}
            }
            // Eight independently keyed tiles across three distinct rows
            // are enough *only* when the exact ROM palette also agrees.
            // A palette-less picture still needs thirteen exact tiles.
            if matched<8 || disagree>3 || rows.count_ones()<3 {
                self.weak+=1;continue;
            }
            let Some(p)=observed else {continue;};
            let Some(palette_count)=palette_distance(e,p,&log.cgram_start) else {
                self.palette_rejected+=1;continue;
            };
            if (palette_count==0 && matched<13) || (matched<10 && palette_count<2) {
                self.weak+=1;continue;
            }
            if source_changed_for_portrait(log,samples,e,x,y) {
                self.source_write_rejected+=1;continue;
            }
            found.push((matched,Matched{index,x,y}));
        }
        found.sort_by_key(|(score,_)|std::cmp::Reverse(*score));
        for (_,item) in found {
            if self.active.iter().any(|old| {
                let x=old.x.abs_diff(item.x);
                let y=old.y.abs_diff(item.y);
                x<32 && y<32
            }) {continue;}
            self.active.push(item);
            if self.active.len()>=MAX_ACTIVE {break;}
        }
        self.recognised=self.active.len() as u32;
        self.reason=if self.recognised>0 {"已驗證可見頭像"}
            else if self.probes==0 {"沒有對應的可見 4bpp 圖塊"}
            else if self.palette_rejected>0 {"來源圖塊存在，但調色盤不吻合"}
            else if self.source_write_rejected>0 {"人物圖塊本幀有更新，安全略過"}
            else {"來源圖塊存在，但有效輪廓不足"};
    }

    pub(crate) fn matches_json(&self)->String {
        serde_json::Value::Array(self.active.iter().map(|m|{
            let entry=&self.entries[m.index];
            serde_json::json!({"index":m.index,"id":entry.id,
                "art":entry.art,"x":m.x,"y":m.y,
                "ready":self.pictures.contains_key(&m.index)})
        }).collect()).to_string()
    }

    pub(crate) fn probe_json(&self)->String {
        serde_json::json!({"candidate":self.probes,"matched":self.recognised,
            "ready":self.active.iter().filter(|m|self.pictures.contains_key(&m.index)).count(),
            "weak":self.weak,"paletteRejected":self.palette_rejected,
            "sourceWriteRejected":self.source_write_rejected,
            "reason":self.reason}).to_string()
    }

    pub(crate) fn add_picture(&mut self,index:usize,width:usize,height:usize,rgba:&[u8])->Result<(),String>{
        let Some(_) = self.active.iter().find(|m|m.index==index) else {
            return Err("頭像已切換，略過過期 HD 圖片".into());
        };
        if width!=64 || height!=64 || rgba.len()!=MAX_ART_BYTES {
            return Err("HD 頭像圖像尺寸不符，應為 64×64（2x）".into());
        }
        self.pictures.insert(index,Picture{rgba:rgba.to_vec()});
        if self.pictures.len()>12 {
            self.pictures.retain(|k,_|self.active.iter().any(|m|m.index==*k));
        }
        Ok(())
    }

    pub(crate) fn composite(&self,log:&FrameLog,samples:&[LiveTilePixel],
        rgba:&mut[u8],width:usize,height:usize)->u32
    {
        let rows=usize::from(log.height);
        if width!=512 || height!=rows*2 || samples.len()!=rows*WIDTH
            || rgba.len()!=width*height*4 {return 0;}
        let mut cache=HashMap::new();
        let mut changed=0;
        for matched in &self.active {
            let Some(img)=self.pictures.get(&matched.index) else {continue;};
            let entry=&self.entries[matched.index];
            // A previously matched face may be reused on an alternate scan
            // cadence, but never across a write to that face's actual source.
            if source_changed_for_portrait(log,samples,entry,matched.x,matched.y) {
                continue;
            }
            for cy in 0..32_usize {
                for cx in 0..32_usize {
                    let x=matched.x+cx;
                    let y=matched.y+cy;
                    let p=samples[y*WIDTH+x];
                    let display_x=if p.pos&0x40!=0 {7-(p.pos&7)} else {p.pos&7};
                    let display_y=if p.pos&0x80!=0 {7-((p.pos>>3)&7)} else {(p.pos>>3)&7};
                    if p.layer==0 || p.bpp!=4
                        || usize::from(display_x)!=cx%8
                        || usize::from(display_y)!=cy%8
                        || current_key(&log.vram,p,&mut cache)!=Some(entry.keys[(cy/8)*4+cx/8])
                    {continue;}
                    // Only replace the exact source pixel that the PPU won
                    // with. This preserves text, other sprites and HUD layers.
                    let mut different=false;
                    for sy in 0..2 {
                        for sx in 0..2 {
                            // For an actual SNES tile flip, use the source
                            // pixel coordinate, never a fake screen overlay.
                            let tx=usize::from(p.pos&7)*2+if p.pos&0x40!=0 {1-sx} else {sx};
                            let ty=usize::from((p.pos>>3)&7)*2+if p.pos&0x80!=0 {1-sy} else {sy};
                            let src=((((cy/8)*8*2+ty)*64)+
                                ((cx/8)*8*2+tx))*4;
                            let dst=((y*2+sy)*width+x*2+sx)*4;
                            let a=u32::from(img.rgba[src+3]);
                            if a==0 {continue;}
                            for ch in 0..3 {
                                let old=u32::from(rgba[dst+ch]);
                                let next=((u32::from(img.rgba[src+ch])*a+old*(255-a)+127)/255) as u8;
                                different|=next!=rgba[dst+ch];
                                rgba[dst+ch]=next;
                            }
                            rgba[dst+3]=255;
                        }
                    }
                    changed+=u32::from(different);
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn class1_half_uses_upstream_4x4_tile_order(){
        assert_eq!(CELLS[0][..4],[0,1,2,3]);
        assert_eq!(CELLS[0][4..8],[16,17,18,19]);
        assert_eq!(CELLS[1][..4],[8,9,10,11]);
        assert_eq!(CELLS[1][12..16],[28,29,30,31]);
    }
    #[test]
    fn exact_sixteen_tiles_composite_only_their_visible_pixels(){
        let mut vram=vec![0_u16;32768];
        let mut keys=[0_u64;16];
        for (cell,key) in keys.iter_mut().enumerate(){
            vram[cell*16]=(cell+1) as u16;
            let mut raw=[0_u8;32];
            raw[..2].copy_from_slice(&((cell+1) as u16).to_le_bytes());
            *key=tile_key(&raw);
        }
        let log=FrameLog{
            complete:true,height:32,h_hi_res:false,v_hi_res:false,
            entries:Vec::new(),cgram_start:vec![0;256],
            cgram_writes:Vec::new(),vram,vram_writes:Vec::new(),
            oam_low:vec![0;512],oam_high:vec![0;32],oam_writes:Vec::new(),
        };
        let mut samples=vec![LiveTilePixel::default();256*32];
        for y in 0..32_usize{
            for x in 0..32_usize{
                let cell=(y/8)*4+x/8;
                samples[y*256+x]=LiveTilePixel{
                    address:(cell*16) as u16,
                    pos:(((y%8)<<3)|(x%8)) as u8,
                    palette:0,layer:1,bpp:4,
                };
            }
        }
        let mut live=LivePortraits::default();
        live.entries.push(Entry{id:"test".into(),art:"class1/test.png".into(),
            keys,palette:vec![None;16]});
        for (cell,key) in keys.iter().enumerate(){
            live.lookup.entry(*key).or_default().push((0,cell));
        }
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),1);
        assert_eq!((live.active[0].x,live.active[0].y),(0,0));
        let mut rgba=vec![0_u8;512*64*4];
        let mut image=vec![0_u8;64*64*4];
        for pixel in image.chunks_exact_mut(4){
            pixel.copy_from_slice(&[250,50,10,255]);
        }
        live.add_picture(0,64,64,&image).unwrap();
        assert_eq!(live.composite(&log,&samples,&mut rgba,512,64),1024);
        assert_eq!(&rgba[..4], &[250,50,10,255]);
        let outside=80*4;
        assert_eq!(&rgba[outside..outside+4], &[0,0,0,0]);

        // Actual PPU tile flips are accepted only if the same ROM source
        // characters and their mirrored pixel positions remain coherent.
        for y in 0..32_usize {
            for x in 0..32_usize {
                let pixel=&mut samples[y*256+x];
                pixel.pos=0xc0|((7-(y%8))<<3) as u8|(7-(x%8)) as u8;
            }
        }
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),1);
        rgba.fill(0);
        assert_eq!(live.composite(&log,&samples,&mut rgba,512,64),1024);
        assert_eq!(&rgba[..4], &[250,50,10,255]);

        // A partly occluded portrait may still be unambiguous with nine
        // exact tiles in three rows and two matching palette colours.
        for y in 0..32_usize {
            for x in 0..32_usize {
                if x/8>=3 || y/8>=3 {
                    samples[y*256+x]=LiveTilePixel::default();
                } else {
                    samples[y*256+x].pos=(((y%8)<<3)|(x%8)) as u8;
                }
            }
        }
        live.entries[0].palette[1]=Some([255,0,0]);
        live.entries[0].palette[2]=Some([0,255,0]);
        let mut log=log;
        log.cgram_start[1]=0x001f;
        log.cgram_start[2]=0x03e0;
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),1);
        rgba.fill(0);
        assert_eq!(live.composite(&log,&samples,&mut rgba,512,64),576);
        assert_eq!(&rgba[..4], &[250,50,10,255]);
        assert_eq!(&rgba[((8*2)*512+8*2)*4..][..4], &[250,50,10,255]);
        assert_eq!(&rgba[((8*2)*512+27*2)*4..][..4], &[0,0,0,0]);
    }

    #[test]
    fn dialogue_updates_elsewhere_do_not_blank_an_unchanged_class1_face() {
        let mut log=FrameLog{
            complete:true,height:32,h_hi_res:false,v_hi_res:false,
            entries:Vec::new(),cgram_start:vec![0;256],
            cgram_writes:Vec::new(),vram:vec![0;32768],
            vram_writes:Vec::new(),oam_low:vec![0;512],
            oam_high:vec![0;32],oam_writes:Vec::new(),
        };
        let mut keys=[0_u64;16];
        for (cell,key) in keys.iter_mut().enumerate() {
            log.vram[cell*16]=(cell+1) as u16;
            let mut tile=[0_u8;32];
            tile[..2].copy_from_slice(&((cell+1) as u16).to_le_bytes());
            *key=tile_key(&tile);
        }
        let mut samples=vec![LiveTilePixel::default();256*32];
        for y in 0..32_usize {
            for x in 0..32_usize {
                let cell=(y/8)*4+x/8;
                samples[y*256+x]=LiveTilePixel{
                    address:(cell*16) as u16,
                    pos:(((y%8)<<3)|(x%8)) as u8,
                    palette:0,layer:1,bpp:4,
                };
            }
        }
        let mut live=LivePortraits::default();
        live.entries.push(Entry{id:"face".into(),art:"class1/face.png".into(),
            keys,palette:vec![None;16]});
        for (cell,key) in keys.iter().enumerate(){
            live.lookup.entry(*key).or_default().push((0,cell));
        }
        log.vram_writes.push(ssd_oracle::VramWrite{
            entry:0,address:4096,value:123,
        });
        log.cgram_writes.push(ssd_oracle::CgramWrite{
            entry:0,index:64,value:123,
        });
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),1,"unrelated text and palette updates cannot blank a portrait");
        let mut image=vec![0;64*64*4];
        for px in image.chunks_exact_mut(4){
            px.copy_from_slice(&[200,90,40,255]);
        }
        live.add_picture(0,64,64,&image).unwrap();
        let mut output=vec![0;512*64*4];
        assert_eq!(live.composite(&log,&samples,&mut output,512,64),1024);

        log.vram_writes[0].address=16;
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),0,"a changed source tile must not be reused");
        // Even on a stale 2-frame-cadence match, composite must fail closed.
        log.vram_writes[0].address=4096;
        live.refresh(&log,&samples);
        log.cgram_writes[0].index=1;
        output.fill(0);
        assert_eq!(live.composite(&log,&samples,&mut output,512,64),0,
            "source palette writes must never produce the wrong HD face");
    }

    #[test]
    fn eight_namek_menu_faces_remain_hd_in_every_frame_and_switch_safely(){
        // Reproduce the eight 32x32 class1 panels measured in the player's
        // Namek trace: x=16/208, y=15/47/79/111. All eight have distinct ROM
        // tile hashes and exact valid 4bpp PPU samples, not guessed identity.
        assert_eq!(MAX_ACTIVE,8);
        let mut log=FrameLog{
            complete:true,height:224,h_hi_res:false,v_hi_res:false,
            entries:Vec::new(),cgram_start:vec![0;256],
            cgram_writes:Vec::new(),vram:vec![0;32768],
            vram_writes:Vec::new(),oam_low:vec![0;512],
            oam_high:vec![0;32],oam_writes:Vec::new(),
        };
        let mut samples=vec![LiveTilePixel::default();WIDTH*224];
        let mut live=LivePortraits::default();
        let mut slots=Vec::new();
        for row in 0..4_usize {
            for col in 0..2_usize {
                let face=row*2+col;
                let x=if col==0 {16} else {208};
                let y=15+row*32;
                slots.push((face,x,y));
                let mut keys=[0_u64;16];
                for cell in 0..16_usize {
                    let address=(face*16+cell)*16;
                    let word=(face*16+cell+1) as u16;
                    log.vram[address]=word;
                    let mut tile=[0_u8;32];
                    tile[..2].copy_from_slice(&word.to_le_bytes());
                    keys[cell]=tile_key(&tile);
                }
                live.entries.push(Entry{
                    id:format!("face-{face}"),
                    art:format!("class1/face-{face}.png"),
                    keys,palette:vec![None;16],
                });
                for (cell,key) in keys.iter().enumerate() {
                    live.lookup.entry(*key).or_default().push((face,cell));
                }
                for cy in 0..32_usize {
                    for cx in 0..32_usize {
                        let cell=(cy/8)*4+cx/8;
                        samples[(y+cy)*WIDTH+x+cx]=LiveTilePixel{
                            address:((face*16+cell)*16) as u16,
                            pos:(((cy%8)<<3)|(cx%8)) as u8,
                            palette:0,layer:1,bpp:4,
                        };
                    }
                }
            }
        }
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),8,"all eight Namek menu faces must match");
        for (face,x,y) in &slots {
            assert!(live.active.iter().any(|item|{
                item.index==*face && item.x==*x && item.y==*y
            }),"one of the eight Namek portraits was evicted");
        }
        for (face,_,_) in &slots {
            let mut art=vec![0_u8;64*64*4];
            for px in art.chunks_exact_mut(4){
                px.copy_from_slice(&[70+(*face as u8)*10,35,95,255]);
            }
            live.add_picture(*face,64,64,&art).unwrap();
        }
        let mut out=vec![0_u8;512*448*4];
        for _frame in 0..24 {
            live.refresh(&log,&samples);
            assert_eq!(live.active.len(),8,"a steady eight-face menu must not flicker");
            assert_eq!(live.probes,8);
            out.fill(0);
            assert_eq!(live.composite(&log,&samples,&mut out,512,448),8192);
            for (face,x,y) in &slots {
                let at=((y*2)*512+x*2)*4;
                assert_eq!(out[at],70+(*face as u8)*10,
                    "each panel retains its own HD identity");
            }
        }
        // A genuine character change invalidates only that portrait:
        // remaining seven retain HD art, never reuse the previous identity.
        let (face,x,y)=slots[0];
        for cy in 0..32_usize {
            for cx in 0..32_usize {
                samples[(y+cy)*WIDTH+x+cx]=LiveTilePixel::default();
            }
        }
        live.refresh(&log,&samples);
        assert_eq!(live.active.len(),7);
        assert!(!live.active.iter().any(|item|item.index==face));
        out.fill(0);
        assert_eq!(live.composite(&log,&samples,&mut out,512,448),7168);
        assert_eq!(out[((y*2)*512+x*2)*4],0);
    }

    #[test]
    fn palette_guards_against_wrong_character(){
        let entry=Entry{id:"goku".into(),art:"class1/goku.png".into(),keys:[0;16],
            palette:(0..16).map(|i|if i==1 {Some([255,0,0])}else{None}).collect()};
        let sample=LiveTilePixel{address:0,pos:0,palette:0,layer:1,bpp:4};
        let mut colors=vec![0_u16;256];
        colors[1]=31;
        assert!(palette_distance(&entry,sample,&colors).is_some());
        colors[1]=0x7c00;
        assert!(palette_distance(&entry,sample,&colors).is_none());
    }
}
