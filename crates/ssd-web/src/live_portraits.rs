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
const MAX_ACTIVE: usize = 6;
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
        Ok(self.entries.len())
    }

    pub(crate) fn has_entries(&self)->bool { !self.entries.is_empty() }

    pub(crate) fn refresh(&mut self, log: &FrameLog, samples: &[LiveTilePixel]) {
        self.active.clear();
        self.probes=0;
        self.recognised=0;
        if self.lookup.is_empty() || !log.complete
            || !log.vram_writes.is_empty() || !log.cgram_writes.is_empty()
            || samples.len()!=WIDTH*usize::from(log.height)
        {return;}
        let h=usize::from(log.height);
        if h<32 {return;}
        let mut cache=HashMap::new();
        let mut candidate=HashSet::new();
        // Sample at 4-game-pixel intervals; use actual PPU tile positions to
        // infer the picture origin. Never hard-code face or dialogue positions.
        for y in (0..h).step_by(4) {
            for x in (0..WIDTH).step_by(4) {
                let p=samples[y*WIDTH+x];
                if p.bpp!=4 || p.layer==0 || p.pos&0xc0!=0 {continue;}
                let Some(key)=current_key(&log.vram,p,&mut cache) else {continue};
                let Some(cells)=self.lookup.get(&key) else {continue};
                for &(index,cell) in cells {
                    let left=x as i32-i32::from(p.pos&7)-(cell%4*8) as i32;
                    let top=y as i32-i32::from((p.pos>>3)&7)-(cell/4*8) as i32;
                    if left>=0 && top>=0 && left<=224 && top+32<=h as i32 {
                        candidate.insert((index,left as usize,top as usize));
                    }
                }
            }
        }
        self.probes=candidate.len() as u32;
        let mut found=Vec::new();
        for (index,x,y) in candidate {
            let e=&self.entries[index];
            let mut matched=0;
            let mut disagree=0;
            let mut observed=None;
            for cell in 0..16_usize {
                let (cx,cy)=(x+(cell%4)*8,y+(cell/4)*8);
                let mut recognised=false;
                let mut covered=false;
                for &(dx,dy) in &[(3,3),(1,1),(6,6),(4,2),(2,5)] {
                    let p=samples[(cy+dy)*WIDTH+cx+dx];
                    if p.bpp!=4 || p.layer==0 {continue;}
                    covered=true;
                    if current_key(&log.vram,p,&mut cache)==Some(e.keys[cell]){
                        recognised=true;
                        observed.get_or_insert(p);
                        break;
                    }
                }
                if recognised {matched+=1}
                else if covered {disagree+=1;}
            }
            // Fail closed on mixed portraits. 10 independently matched 8x8
            // cells are required, including the palette's identity evidence.
            if matched<10 || disagree>3 {continue;}
            let Some(p)=observed else {continue;};
            let Some(palette_count)=palette_distance(e,p,&log.cgram_start) else {continue;};
            if palette_count==0 && matched<13 {continue;}
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
            "ready":self.active.iter().filter(|m|self.pictures.contains_key(&m.index)).count()}).to_string()
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
            for cy in 0..32_usize {
                for cx in 0..32_usize {
                    let x=matched.x+cx;
                    let y=matched.y+cy;
                    let p=samples[y*WIDTH+x];
                    if p.layer==0 || p.bpp!=4 || p.pos&0xc0!=0
                        || usize::from(p.pos&7)!=cx%8
                        || usize::from((p.pos>>3)&7)!=cy%8
                        || current_key(&log.vram,p,&mut cache)!=Some(entry.keys[(cy/8)*4+cx/8])
                    {continue;}
                    // Only replace the exact source pixel that the PPU won
                    // with. This preserves text, other sprites and HUD layers.
                    let mut different=false;
                    for sy in 0..2 {
                        for sx in 0..2 {
                            let src=((cy*2+sy)*64+cx*2+sx)*4;
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
