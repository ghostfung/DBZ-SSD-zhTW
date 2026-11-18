//! Experimental exact-key HD fighter overlay for the iPhone browser.
//! The official class2 art is never bundled or uploaded. Only a pose assembled
//! by the author's PPU/OAM sprite parser and matched to its source-key may be
//! used. The mask is the actual winning OAM entry at each visible pixel.
//! Limitations: the first pass replaces the *visible original silhouette*;
//! enlarged silhouettes, pose variants, recolours, field sprites and effects
//! still require the upstream two-pass art renderer.
use serde::Deserialize;
use ssd_hd::LiveFighterPose;
use ssd_oracle::FrameLog;
use std::collections::HashMap;

const MAX_ENTRIES: usize = 20_000;
const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_POSES: usize = 8;
const WIDTH: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    key: String,
    width: u16,
    height: u16,
    art: String,
    available: bool,
    palette: Vec<Option<[u8; 3]>>,
}

#[derive(Debug)]
struct Picture {
    width: usize,
    rgba: Vec<u8>,
}

#[derive(Clone)]
struct Matching {
    index: usize,
    pose: LiveFighterPose,
}

#[derive(Default)]
pub(crate) struct LiveFighters {
    entries: Vec<Entry>,
    lookup: HashMap<u64, Vec<usize>>,
    active: Vec<Matching>,
    pictures: HashMap<usize, Picture>,
}

fn palette_matches(entry: &Entry, pose: &LiveFighterPose) -> bool {
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
    // An unspecified palette does not prove that two different fighters'
    // colours agree (e.g. Super Saiyan hair variants).
    used >= 2
}

impl LiveFighters {
    pub(crate) fn load_catalog(&mut self, json: &str) -> Result<usize, String> {
        let incoming: Vec<Entry> = serde_json::from_str(json)
            .map_err(|e| format!("HD 人物索引無法解析：{e}"))?;
        if incoming.len() > MAX_ENTRIES {
            return Err("HD 人物姿勢索引數量超過安全上限".into());
        }
        let mut lookup: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, entry) in incoming.iter().enumerate() {
            let ok_path = entry.art.starts_with("class2/")
                && entry.art.len() <= 240
                && !entry.art.contains("..")
                && !entry.art.contains('\\')
                && (entry.art.ends_with(".png") || entry.art.ends_with(".webp"));
            if entry.id.is_empty() || entry.id.len() > 160
                || !ok_path || entry.key.is_empty() || entry.key.len() > 16
                || entry.width == 0 || entry.height == 0
                || entry.palette.len() != 16
            {
                return Err(format!("HD 人物第 {} 筆索引不安全或資料不完整", i+1));
            }
            let key=u64::from_str_radix(&entry.key,16)
                .map_err(|_|format!("HD 人物鍵值格式錯誤：{}",entry.id))?;
            if entry.available {
                lookup.entry(key).or_default().push(i);
            }
        }
        self.entries=incoming;
        self.lookup=lookup;
        self.active.clear();
        self.pictures.clear();
        Ok(self.entries.len())
    }

    pub(crate) fn refresh(&mut self, log: &FrameLog) {
        self.active.clear();
        if self.lookup.is_empty() { return; }
        // Using author-provided OAM sprite grouping, not an object name,
        // animation frame number or arbitrary rectangular UI mask.
        for pose in ssd_hd::live_fighter_poses(log) {
            let Some(indices)=self.lookup.get(&pose.key) else { continue };
            let selected=indices.iter().copied().find(|&index| {
                let entry=&self.entries[index];
                entry.width == pose.width && entry.height == pose.height
                    && palette_matches(entry,&pose)
            });
            if let Some(index)=selected {
                self.active.push(Matching {index,pose});
                if self.active.len()>=MAX_POSES { break; }
            }
        }
    }

    pub(crate) fn matches_json(&self) -> String {
        let matches: Vec<_> = self.active.iter().map(|matched| {
            let entry=&self.entries[matched.index];
            serde_json::json!({
                "index":matched.index, "id":entry.id, "art":entry.art,
                "ready":self.pictures.contains_key(&matched.index),
                "key":entry.key,
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
        self.pictures.insert(index,Picture { width,rgba:rgba.to_vec() });
        // Keep only a small number of prepared pictures in WASM. JS retains
        // an independent LRU of 2x images for the next encounter.
        if self.pictures.len()>12 {
            self.pictures.retain(|i,_| self.active.iter().any(|p|p.index==*i));
        }
        Ok(())
    }

    /// Paint only where one of the original OAM sprites truly won the PPU's
    /// priority resolution. This cannot cover background/UI or expand a
    /// fighter's silhouette into pixels owned by a different layer.
    pub(crate) fn composite(&self, oam_ids: &[u8], rgba: &mut [u8],
        width: usize,height: usize) -> u32
    {
        if self.active.is_empty() || width != WIDTH*2 || height%2 != 0
            || oam_ids.len()!=WIDTH*(height/2) || rgba.len()!=width*height*4
        {
            return 0;
        }
        let rows=height/2;
        let mut painted=0_u32;
        // Ensure each pixel is claimed at most once by a matching pose.
        let mut claimed=vec![false;WIDTH*rows];
        for matched in &self.active {
            let Some(picture)=self.pictures.get(&matched.index) else { continue };
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
                    let owner=oam_ids[p];
                    if owner>=128 || claimed[p] || pose.oam_bits & (1_u128<<owner)==0 {
                        continue;
                    }
                    let image_x=(x as i32-left) as usize*2;
                    let image_y=(y as i32-top) as usize*2;
                    let mut changed=false;
                    for sy in 0..2 {
                        for sx in 0..2 {
                            let src=((image_y+sy)*picture.width+image_x+sx)*4;
                            let dst=((y*2+sy)*width+x*2+sx)*4;
                            let a=u32::from(picture.rgba[src+3]);
                            if a==0 { continue; }
                            changed=true;
                            for c in 0..3 {
                                let foreground=u32::from(picture.rgba[src+c]);
                                let background=u32::from(rgba[dst+c]);
                                rgba[dst+c]=((foreground*a+background*(255-a)+127)/255) as u8;
                            }
                            rgba[dst+3]=255;
                        }
                    }
                    if changed {claimed[p]=true;painted+=1;}
                }
            }
        }
        painted
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
            "id":"battle-test","key":"abcd","width":2,"height":2,
            "art":"class2/battle-test.png","available":true,"palette":pal
        }]).to_string()
    }
    fn pose() -> LiveFighterPose {
        let mut colours=[[0;3];16];
        colours[1]=[255,0,0];colours[2]=[0,255,0];
        LiveFighterPose{
            key:0xabcd,x:1,y:1,width:2,height:2,oam_bits:1<<5,colours,
        }
    }
    #[test]
    fn catalogue_rejects_unsafe_paths_and_unknown_palette() {
        let mut live=LiveFighters::default();
        assert_eq!(live.load_catalog(&entry()).unwrap(),1);
        assert!(live.load_catalog(&entry().replace("class2/","../")).is_err());
        let mut wrong=pose();
        wrong.colours[1]=[0,0,255];
        assert!(!palette_matches(&live.entries[0],&wrong));
    }
    #[test]
    fn alpha_blend_never_covers_another_ppu_layer_or_sprite() {
        let mut live=LiveFighters::default();
        live.load_catalog(&entry()).unwrap();
        live.active.push(Matching{index:0,pose:pose()});
        let mut picture=vec![0;4*4*4];
        for px in picture.chunks_exact_mut(4){px.copy_from_slice(&[240,40,40,255]);}
        live.add_picture(0,4,4,&picture).unwrap();
        let mut owners=vec![255;256*4];
        owners[1*256+1]=5;
        owners[1*256+2]=7; // a different sprite, must not draw over it
        let mut out=vec![5;512*8*4];
        let changed=live.composite(&owners,&mut out,512,8);
        assert_eq!(changed,1);
        let at=((1*2)*512+1*2)*4;
        assert_eq!(&out[at..at+3], &[240,40,40]);
        let other=((1*2)*512+2*2)*4;
        assert_eq!(&out[other..other+3], &[5,5,5]);
    }
}
