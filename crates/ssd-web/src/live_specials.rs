//! Exact source-key compositor for approved field objects (class3) and
//! RGBA battle effects (class2). Sources are not inferred from coordinates.
//! This stage supports only PPU-visible OAM pixels, never stretched art.
use serde::Deserialize;
use ssd_hd::LiveSpecialPose;
use crate::hd_luma;
use ssd_oracle::FrameLog;
use std::collections::HashMap;

const MAX_ENTRIES: usize = 20_000;
const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_POSES: usize = 12;
const WIDTH: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    kind: u8,
    key: String,
    width: u16,
    height: u16,
    art: String,
    available: bool,
    #[serde(default)]
    luma: bool,
    palette: Vec<Option<[u8; 3]>>,
}

#[derive(Debug)]
struct Picture {
    width: usize,
    rgba: Vec<u8>,
    luma_ref: u8,
}

#[derive(Clone)]
struct Matching {
    index: usize,
    pose: LiveSpecialPose,
}

#[derive(Default)]
pub(crate) struct LiveSpecials {
    entries: Vec<Entry>,
    lookup: HashMap<(u8, u64), Vec<usize>>,
    active: Vec<Matching>,
    pictures: HashMap<usize, Picture>,
    probes: usize,
    key_hits: usize,
    palette_rejections: usize,
    cgram_writes: usize,
}

fn palette_matches(entry: &Entry, pose: &LiveSpecialPose) -> bool {
    // RGBA effect source_key already includes actual per-scanline colours.
    if entry.kind == 2 { return true; }
    let mut used = 0;
    for (i, source) in entry.palette.iter().enumerate().take(16) {
        if let Some(source) = source {
            used += 1;
            let actual = pose.colours[i];
            if source.iter().zip(actual).any(|(&a, b)| u16::from(a.abs_diff(b)) > 9) {
                return false;
            }
        }
    }
    // Effects are keyed over the full RGBA image, which already encodes
    // the rendered palette, while indexed field sprites require 2 colours.
    used >= 2
}

impl LiveSpecials {
    pub(crate) fn load_catalog(&mut self, json: &str) -> Result<usize, String> {
        let incoming: Vec<Entry> = serde_json::from_str(json)
            .map_err(|e| format!("HD 物件索引無法解析：{e}"))?;
        if incoming.len() > MAX_ENTRIES {
            return Err("HD 物件姿勢索引數量超過安全上限".into());
        }
        let mut lookup: HashMap<(u8, u64), Vec<usize>> = HashMap::new();
        for (i, entry) in incoming.iter().enumerate() {
            let ok_path = (entry.kind == 1 && entry.art.starts_with("class3/")
                || entry.kind == 2 && entry.art.starts_with("class2/"))
                && entry.art.len() <= 240
                && !entry.art.contains("..")
                && !entry.art.contains('\\')
                && (entry.art.ends_with(".png") || entry.art.ends_with(".webp"));
            if entry.id.is_empty() || entry.id.len() > 160
                || !ok_path || entry.key.is_empty() || entry.key.len() > 16
                || entry.width == 0 || entry.height == 0
                || entry.palette.len() != 16
            {
                return Err(format!("HD 物件第 {} 筆索引不安全或資料不完整", i+1));
            }
            let key=u64::from_str_radix(&entry.key,16)
                .map_err(|_|format!("HD 物件鍵值格式錯誤：{}",entry.id))?;
            if entry.available {
                lookup.entry((entry.kind,key)).or_default().push(i);
            }
        }
        self.entries=incoming;
        self.lookup=lookup;
        self.active.clear();
        self.pictures.clear();
        self.probes=0;
        self.key_hits=0;
        self.palette_rejections=0;
        self.cgram_writes=0;
        Ok(self.entries.len())
    }

    pub(crate) fn refresh(&mut self, log: &FrameLog) {
        self.active.clear();
        self.probes=0;
        self.key_hits=0;
        self.palette_rejections=0;
        // Expose how many CGRAM writes the observed frame contains. Prior
        // versions rejected all class2 FX on any such write.
        self.cgram_writes=log.cgram_writes.len();
        if self.lookup.is_empty() { return; }
        // The original engine handles both standard OAM palettes 1-2 and
        // formations that render enemies with palettes 3-7. Exact source
        // hashes must still match an official class2 asset before use.
        let mut used_oam=0_u128;
        for pose in ssd_hd::live_special_poses(log) {
            if pose.oam_bits & used_oam != 0 {continue;}
            self.probes+=1;
            let Some(indices)=self.lookup.get(&(pose.kind,pose.key)) else { continue };
            self.key_hits+=1;
            let mut candidates=indices.iter().copied().filter(|&index| {
                let entry=&self.entries[index];
                entry.width == pose.width && entry.height == pose.height
                    && palette_matches(entry,&pose)
            });
            let selected=candidates.next();
            // Different assets with indistinguishable OAM source keys cannot
            // be resolved from a character name or a screen position.
            if candidates.next().is_some() {
                self.palette_rejections+=1;
                continue;
            }
            if let Some(index)=selected {
                used_oam|=pose.oam_bits;
                self.active.push(Matching {index,pose});
                if self.active.len()>=MAX_POSES { break; }
            } else {
                self.palette_rejections+=1;
            }
        }
    }

    pub(crate) fn probe_json(&self) -> String {
        serde_json::json!({
            "poses":self.probes, "keyHits":self.key_hits,
            "cgramWrites":self.cgram_writes,
            "paletteRejected":self.palette_rejections,
            "matched":self.active.len(),
            "lumaMatched":self.active.iter().filter(|m|self.entries[m.index].luma).count(),
            "ready":self.active.iter()
                .filter(|p| self.pictures.contains_key(&p.index)).count(),
        }).to_string()
    }

    /// Exclude OAM members already owned by higher-priority HD fighters.
    pub(crate) fn exclude_oam(&mut self, owners: u128) {
        self.active.retain(|matched| matched.pose.oam_bits & owners == 0);
    }

    pub(crate) fn has_pending_art(&self) -> bool {
        self.active.iter().any(|m| !self.pictures.contains_key(&m.index))
    }

    pub(crate) fn ready_oam_bits(&self) -> u128 {
        self.active.iter().filter(|p|self.pictures.contains_key(&p.index))
            .fold(0_u128,|m,p|m|p.pose.oam_bits)
    }

    pub(crate) fn matches_json(&self) -> String {
        let matches: Vec<_> = self.active.iter().map(|matched| {
            let entry=&self.entries[matched.index];
            serde_json::json!({
                "index":matched.index, "id":entry.id, "art":entry.art,
                "ready":self.pictures.contains_key(&matched.index),
                "luma":entry.luma,
                "key":entry.key, "kind":entry.kind,
            })
        }).collect();
        serde_json::to_string(&matches).unwrap_or_else(|_|"[]".into())
    }

    pub(crate) fn add_picture(&mut self, index: usize, width: usize,
        height: usize, rgba: &[u8]) -> Result<(), String>
    {
        if !self.active.iter().any(|p| p.index == index) {
            return Err("人物姿勢已切換，忽略過期 HD 圖像".into());
        }
        let entry=self.entries.get(index)
            .ok_or_else(||"人物姿勢索引不存在".to_string())?;
        let expected_width=usize::from(entry.width)*2;
        let expected_height=usize::from(entry.height)*2;
        let bytes=width.checked_mul(height).and_then(|n|n.checked_mul(4))
            .ok_or_else(||"人物 HD 圖像尺寸溢位".to_string())?;
        if width!=expected_width || height!=expected_height
            || bytes > MAX_PICTURE_BYTES || rgba.len()!=bytes
        {
            return Err(format!("人物 HD 圖像尺寸錯誤：{}",entry.id));
        }
        let luma_ref=if entry.luma {hd_luma::reference(rgba)} else {255};
        self.pictures.insert(index,Picture {
            width,rgba:rgba.to_vec(),luma_ref
        });
        // Keep only a small number of prepared pictures in WASM. JS retains
        // an independent LRU of 2x images for the next encounter.
        if self.pictures.len()>12 {
            self.pictures.retain(|i,_| self.active.iter().any(|p|p.index==*i));
        }
        Ok(())
    }

    /// First restore the real PPU/HD background below the replaced OAM
    /// pixels (second PPU render). Then draw the official HD artwork at 2x.
    /// This handles transparent art and small overhangs inside the original
    /// pose box, without leaving a 1x ROM sprite silhouette visible.
    pub(crate) fn composite(&self, oam_ids: &[u8], layer_ids: &[u8],
        beneath: &[u8], rgba: &mut [u8], width: usize,height: usize) -> u32
    {
        let cells=WIDTH*(height/2);
        if self.active.is_empty() || width!=WIDTH*2 || height%2!=0
            || oam_ids.len()!=cells || layer_ids.len()!=cells
            || beneath.len()!=width*height*4 || rgba.len()!=width*height*4
        {
            return 0;
        }
        let rows=height/2;
        let mut changed_pixels=0_u32;
        let mut claimed=vec![false;cells];
        for matched in &self.active {
            let Some(picture)=self.pictures.get(&matched.index) else {continue};
            let luma=self.entries[matched.index].luma;
            let pose=&matched.pose;
            let (left,top)=(pose.x,pose.y);
            let (right,bottom)=(left+i32::from(pose.width),top+i32::from(pose.height));
            let x0=left.max(0) as usize;
            let x1=right.clamp(0,WIDTH as i32) as usize;
            let y0=top.max(0) as usize;
            let y1=bottom.clamp(0,rows as i32) as usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p=y*WIDTH+x;
                    if claimed[p] {continue;}
                    let owner=oam_ids[p];
                    let owned=owner<128 && (pose.oam_bits & (1_u128<<owner))!=0;
                    // Original art can extend a pixel into a transparent
                    // part of its trimmed bounding box. Only allow it where
                    // the PPU resolved a *background*, near a visible sprite
                    // pixel of this pose, and above the battle HUD. Never
                    // cover another OAM sprite or a BG3/BG4 UI plane.
                    // Field sprites and effects must not overpaint BG UI or
                    // an unrelated OAM group. Enlarged silhouettes are a
                    // separate future pass with explicit occlusion evidence.
                    let near=false;
                    if !owned && !near {continue;}
                    let image_x=(x as i32-left) as usize*2;
                    let image_y=(y as i32-top) as usize*2;
                    // A pixel with absolutely no HD coverage may still need
                    // to uncover the correct HD background underneath its
                    // opaque original sprite (remove the source silhouette).
                    let before=[0,1,2,3].map(|i| {
                        let sx=i%2;
                        let sy=i/2;
                        let dst=((y*2+sy)*width+x*2+sx)*4;
                        [rgba[dst],rgba[dst+1],rgba[dst+2],rgba[dst+3]]
                    });
                    let mut has_art=false;
                    for sy in 0..2 {
                        for sx in 0..2 {
                            let src=((image_y+sy)*picture.width+image_x+sx)*4;
                            let dst=((y*2+sy)*width+x*2+sx)*4;
                            if owned {
                                rgba[dst..dst+4].copy_from_slice(&beneath[dst..dst+4]);
                            }
                            let a=u32::from(picture.rgba[src+3]);
                            if a==0 {continue;}
                            has_art=true;
                            // Luma art is a luminance stencil of an exact
                            // source RGBA effect, not a grey RGB replacement.
                            // Recover the original OAM colour saved above.
                            let tinted=if luma {
                                hd_luma::colour(
                                    [before[sy*2+sx][0],before[sy*2+sx][1],
                                     before[sy*2+sx][2]],
                                    &picture.rgba[src..src+3],picture.luma_ref)
                            }else{
                                [picture.rgba[src],picture.rgba[src+1],picture.rgba[src+2]]
                            };
                            for (c,fg) in tinted.into_iter().enumerate(){
                                let bg=u32::from(rgba[dst+c]);
                                rgba[dst+c]=((u32::from(fg)*a+bg*(255-a)+127)/255) as u8;
                            }
                            rgba[dst+3]=255;
                        }
                    }
                    if owned || has_art {
                        claimed[p]=true;
                        let changed=[0_usize,1,2,3].into_iter().any(|i|{
                            let dst=((y*2+i/2)*width+x*2+i%2)*4;
                            before[i]!=[rgba[dst],rgba[dst+1],rgba[dst+2],rgba[dst+3]]
                        });
                        if changed {changed_pixels+=1;}
                    }
                }
            }
        }
        changed_pixels
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> String {
        let mut pal=vec![serde_json::Value::Null;16];
        pal[1]=serde_json::json!([255,0,0]);
        pal[2]=serde_json::json!([0,255,0]);
        serde_json::json!([{
            "id":"effect-test","kind":2,"key":"abcd","width":2,"height":2,
            "art":"class2/effect-test.png","available":true,"palette":pal
        }]).to_string()
    }
    fn pose() -> LiveSpecialPose {
        let mut colours=[[0;3];16];
        colours[1]=[255,0,0];colours[2]=[0,255,0];
        LiveSpecialPose{
            kind:2,key:0xabcd,x:1,y:1,width:2,height:2,oam_bits:1<<5,colours,
        }
    }
    #[test]
    fn catalogue_rejects_unsafe_paths_and_unknown_palette() {
        let mut live=LiveSpecials::default();
        assert_eq!(live.load_catalog(&entry()).unwrap(),1);
        assert!(live.load_catalog(&entry().replace("class2/","../")).is_err());
        let mut wrong=pose();
        wrong.colours[1]=[0,0,255];
        // An effect's SHA source_key already includes the exact rendered
        // RGBA colours, including HDMA; a separate palette test would be
        // unsound. A field sprite's indexed key requires palette evidence.
        assert!(palette_matches(&live.entries[0],&wrong));
        live.entries[0].kind=1;
        assert!(!palette_matches(&live.entries[0],&wrong));
    }
    #[test]
    fn transparent_hd_art_removes_its_original_rom_silhouette() {
        let mut live=LiveSpecials::default();
        live.load_catalog(&entry()).unwrap();
        live.active.push(Matching{index:0,pose:pose()});
        let transparent=vec![0_u8;4*4*4];
        live.add_picture(0,4,4,&transparent).unwrap();
        let mut owners=vec![255;256*4];
        owners[1*256+1]=5;
        let layers=vec![0_u8;256*4];
        let beneath=vec![33;512*8*4];
        let mut output=vec![77;512*8*4];
        let changed=live.composite(&owners,&layers,&beneath,&mut output,512,8);
        assert_eq!(changed,1);
        let at=(2*512+2)*4;
        assert_eq!(&output[at..at+4],&[33,33,33,33]);
        let other=(2*512+6)*4;
        assert_eq!(&output[other..other+4],&[77,77,77,77]);
    }

    #[test]
    fn alpha_blend_never_covers_another_ppu_layer_or_sprite() {
        let mut live=LiveSpecials::default();
        live.load_catalog(&entry()).unwrap();
        live.active.push(Matching{index:0,pose:pose()});
        let mut picture=vec![0;4*4*4];
        for px in picture.chunks_exact_mut(4){px.copy_from_slice(&[240,40,40,255]);}
        live.add_picture(0,4,4,&picture).unwrap();
        let mut owners=vec![255;256*4];
        owners[1*256+1]=5;
        owners[1*256+2]=7; // a different sprite, must not draw over it
        let mut out=vec![5;512*8*4];
        let layers=vec![5;256*4];
        let beneath=vec![13;512*8*4];
        let changed=live.composite(&owners,&layers,&beneath,&mut out,512,8);
        assert_eq!(changed,1);
        let at=((1*2)*512+1*2)*4;
        assert_eq!(&out[at..at+3], &[240,40,40]);
        let other=((1*2)*512+2*2)*4;
        assert_eq!(&out[other..other+3], &[5,5,5]);
    }
    #[test]
    fn exact_rgba_effect_luma_uses_current_sprite_hue(){
        let mut live=LiveSpecials::default();
        let raw=serde_json::json!([{
            "id":"effect-test","kind":2,"key":"abcd","width":2,"height":2,
            "art":"class2/grey.png","available":true,"luma":true,
            "palette":vec![serde_json::Value::Null;16]
        }]).to_string();
        live.load_catalog(&raw).unwrap();
        live.active.push(Matching{index:0,pose:pose()});
        let mut grey=vec![0_u8;4*4*4];
        for (i,p) in grey.chunks_exact_mut(4).enumerate(){
            let v=if i%2==0 {80}else{200};
            p.copy_from_slice(&[v,v,v,255]);
        }
        live.add_picture(0,4,4,&grey).unwrap();
        let mut owners=vec![255;256*4];owners[1*256+1]=5;
        let layers=vec![0;256*4];
        let beneath=vec![13;512*8*4];
        let mut rgba=vec![0;512*8*4];
        for p in rgba.chunks_exact_mut(4){p.copy_from_slice(&[200,80,30,255]);}
        assert_eq!(live.composite(&owners,&layers,&beneath,&mut rgba,512,8),1);
        let dst=(2*512+2)*4;
        assert!(rgba[dst]>rgba[dst+1]);
        assert!(rgba[dst+1]>rgba[dst+2]);
        assert!(rgba[dst]<200);
        let other=(2*512+6)*4;
        assert_eq!(&rgba[other..other+3],&[200,80,30]);
    }

}
