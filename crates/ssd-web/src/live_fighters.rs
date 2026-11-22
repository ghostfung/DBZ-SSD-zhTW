//! Experimental exact-key HD fighter overlay for the iPhone browser.
//! The official class2 art is never bundled or uploaded. Only a pose assembled
//! by the author's PPU/OAM sprite parser and matched to its source-key may be
//! used. The mask is the actual winning OAM entry at each visible pixel.
//! Limitations: the first pass replaces the *visible original silhouette*;
//! enlarged silhouettes, pose variants, recolours, field sprites and effects
//! still require the upstream two-pass art renderer.
use serde::Deserialize;
use ssd_hd::LiveFighterPose;
use crate::hd_luma;
use ssd_oracle::FrameLog;
use std::collections::HashMap;

const MAX_ENTRIES: usize = 20_000;
const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_POSES: usize = 8;
const MAX_REJECTED: usize = 8;
const MAX_REJECT_CANDIDATES: usize = 3;
const WIDTH: usize = 256;

/// Identity from active roster records, following the desktop's Goku rules.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
enum GokuForm { Normal, SuperSaiyan }

fn goku_form_from_wram(wram: &[u8]) -> Option<GokuForm> {
    let start=(ssd_game::data::PARTY_RECORDS & 0xFFFF) as usize;
    let (mut normal,mut super_saiyan)=(false,false);
    for slot in 0..9 {
        let record=wram.get(start+slot*32..start+(slot+1)*32)?;
        if record[0]&0x80==0 {continue;}
        match record[1] {
            0 if record[0x1C]==0 => normal=true,
            0x3B => super_saiyan=true,
            _=>{}
        }
    }
    match (normal,super_saiyan) {
        (true,false)=>Some(GokuForm::Normal),
        (false,true)=>Some(GokuForm::SuperSaiyan),
        _=>None
    }
}
fn goku_battle_id(id:&str)->Option<GokuForm>{
    if id.starts_with("battle-goku-ssj-"){Some(GokuForm::SuperSaiyan)}
    else if id.starts_with("battle-goku-"){Some(GokuForm::Normal)}
    else {None}
}

/// Only distinguish exactly one normal Goku and one SSJ Goku using the
/// upstream's common-palette-colour comparison. Roster breaks colour ties.
/// Other characters, duplicated battle assets and unknown states stay SNES.
fn resolve_goku_collision(entries:&[Entry],exact:&[usize],pose:&LiveFighterPose,
                          hint:Option<GokuForm>)->Option<usize>{
    let (mut normal,mut super_saiyan)=(None,None);
    for &index in exact{
        match goku_battle_id(&entries[index].id){
            Some(GokuForm::Normal)=>{
                if normal.replace(index).is_some(){return None;}
            }
            Some(GokuForm::SuperSaiyan)=>{
                if super_saiyan.replace(index).is_some(){return None;}
            }
            None if entries[index].id.starts_with("field-goku-")=>{}
            None=>return None,
        }
    }
    let (n,ss)=(normal?,super_saiyan?);
    let mut normal_distance=0_u32;
    let mut super_distance=0_u32;
    // Compare only colour indices used by BOTH palettes, never unused hair.
    for (slot,(n_rgb,ss_rgb)) in entries[n].palette.iter()
        .zip(&entries[ss].palette).enumerate().skip(1){
        if let (Some(n_rgb),Some(ss_rgb))=(n_rgb,ss_rgb){
            for channel in 0..3{
                normal_distance+=u32::from(pose.colours[slot][channel].abs_diff(n_rgb[channel]));
                super_distance+=u32::from(pose.colours[slot][channel].abs_diff(ss_rgb[channel]));
            }
        }
    }
    match normal_distance.cmp(&super_distance){
        std::cmp::Ordering::Less=>Some(n),
        std::cmp::Ordering::Greater=>Some(ss),
        std::cmp::Ordering::Equal=>match hint{
            Some(GokuForm::Normal)=>Some(n),
            Some(GokuForm::SuperSaiyan)=>Some(ss),
            None=>None,
        }
    }
}


#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    key: String,
    width: u16,
    height: u16,
    art: String,
    available: bool,
    #[serde(default)]
    luma: bool,
    palette: Vec<Option<[u8; 3]>>,
    #[serde(default)]
    variant: Option<Variant>,
}

#[derive(Debug, Deserialize)]
struct Variant {
    art: String,
    margin: u16,
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
    pose: LiveFighterPose,
}

/// Compact per-frame evidence only. No pixels, palette data, ROM, or paths.
#[derive(Debug)]
struct RejectedPose {
    key: u64,
    width: u16,
    height: u16,
    origin: &'static str,
    reason: &'static str,
    candidates: Vec<usize>,
}

impl RejectedPose {
    fn new(pose: &LiveFighterPose, origin: &'static str, reason: &'static str,
           indices: &[usize]) -> Self {
        Self {
            key: pose.key, width: pose.width, height: pose.height, origin, reason,
            candidates: indices.iter().copied().take(MAX_REJECT_CANDIDATES).collect(),
        }
    }
}

#[derive(Default)]
pub(crate) struct LiveFighters {
    entries: Vec<Entry>,
    lookup: HashMap<u64, Vec<usize>>,
    active: Vec<Matching>,
    pictures: HashMap<usize, Picture>,
    variants: HashMap<usize, Picture>,
    probes: usize,
    key_hits: usize,
    palette_rejections: usize,
    fallback_probes: usize,
    fallback_key_hits: usize,
    fallback_attempted: bool,
    fallback_accepted: usize,
    unindexed_keys: Vec<String>,
    rejected: Vec<RejectedPose>,
    goku_form: Option<GokuForm>,
    goku_ambiguities_resolved: usize,
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

/// A pose's exact 64-bit source key and dimensions are mandatory. Choose
/// exactly one matching colour asset; if there is no colour-exact candidate,
/// allow exactly one greyscale transfer asset for the same source. Multiple
/// indistinguishable candidates are never chosen by catalogue order.
fn choose_pose(entries:&[Entry],indices:&[usize],pose:&LiveFighterPose)->Option<usize>{
    choose_pose_for(entries,indices,pose,None)
}
fn choose_pose_for(entries:&[Entry],indices:&[usize],pose:&LiveFighterPose,
                   form_hint:Option<GokuForm>)->Option<usize>{
    let sized:Vec<usize>=indices.iter().copied()
        .filter(|&i|entries[i].width==pose.width && entries[i].height==pose.height)
        .collect();
    let exact:Vec<usize>=sized.iter().copied()
        .filter(|&i|!entries[i].luma&&palette_matches(&entries[i],pose))
        .collect();
    match exact.as_slice(){
        [only]=>return Some(*only),
        [_,_,..]=>return resolve_goku_collision(entries,&exact,pose,form_hint),
        _=>{}
    }
    let greys:Vec<usize>=sized.iter().copied()
        .filter(|&i|entries[i].luma).collect();
    match greys.as_slice(){
        [only]=>Some(*only),
        _=>None,
    }
}

/// Explain exactly why an existing source key was not accepted. Never
/// weaken the original unique-palette / unique-luma matching rule.
fn pose_rejection_reason(entries: &[Entry], indices: &[usize],
                         pose: &LiveFighterPose) -> &'static str {
    let mut sized = 0_usize;
    let mut exact = 0_usize;
    let mut greys = 0_usize;
    for &i in indices {
        let entry = &entries[i];
        if entry.width != pose.width || entry.height != pose.height { continue; }
        sized += 1;
        if entry.luma { greys += 1; }
        else if palette_matches(entry, pose) { exact += 1; }
    }
    if sized == 0 { "size_mismatch" }
    else if exact > 1 { "ambiguous_palette" }
    else if exact == 0 && greys > 1 { "ambiguous_luma" }
    else { "palette_mismatch" }
}

impl LiveFighters {
    fn record_rejection(&mut self, rejected: RejectedPose) {
        if self.rejected.len() < MAX_REJECTED {
            self.rejected.push(rejected);
        }
    }

    /// Primary fighter poses have priority over all fallback candidates.
    /// Enforce exclusive OAM ownership rather than ever combining two
    /// competing figures in a shared SNES sprite group.
    fn accept_primary(&mut self, pose: LiveFighterPose, owned: &mut u128) -> bool {
        self.probes += 1;
        let Some(indices) = self.lookup.get(&pose.key) else {
            if self.unindexed_keys.len() < 3 {
                let description = format!("{:016x}/{}x{}",
                    pose.key, pose.width, pose.height);
                if !self.unindexed_keys.contains(&description) {
                    self.unindexed_keys.push(description);
                }
            }
            self.record_rejection(RejectedPose::new(
                &pose, "primary", "source_key_unmatched", &[]));
            return false;
        };
        self.key_hits += 1;
        let Some(index) = choose_pose_for(&self.entries, indices, &pose, self.goku_form) else {
            self.palette_rejections += 1;
            let reason = pose_rejection_reason(&self.entries, indices, &pose);
            let rejected = RejectedPose::new(&pose, "primary", reason, indices);
            self.record_rejection(rejected);
            return false;
        };
        if pose.oam_bits & *owned != 0 {
            self.record_rejection(RejectedPose::new(
                &pose, "primary", "oam_overlap", &[]));
            return false;
        }
        *owned |= pose.oam_bits;
        if choose_pose(&self.entries,indices,&pose).is_none()
            && goku_battle_id(&self.entries[index].id).is_some(){
            self.goku_ambiguities_resolved+=1;
        }
        self.active.push(Matching { index, pose });
        true
    }

    fn should_try_refinement(&self, missing_oam: u128) -> bool {
        self.active.len() < MAX_POSES &&
            (self.active.is_empty() || missing_oam != 0)
    }

    /// Match extra exact-key field/refined poses only for *unclaimed* OAM
    /// groups. In a multi-person battle, a matched Saibaman must not stop
    /// attempts to identify the still-unmatched Goku.
    fn accept_refined(&mut self, pose: LiveFighterPose, owned: &mut u128,
                      missing_oam: u128, had_primary: bool) -> bool {
        self.fallback_probes += 1;
        if pose.oam_bits & *owned != 0 {
            self.record_rejection(RejectedPose::new(
                &pose, "refined", "oam_overlap", &[]));
            return false;
        }
        if had_primary && pose.oam_bits & missing_oam == 0 {
            // An unrelated NPC must never become a battle fighter.
            return false;
        }
        let Some(indices) = self.lookup.get(&pose.key) else {
            self.record_rejection(RejectedPose::new(
                &pose, "refined", "source_key_unmatched", &[]));
            return false;
        };
        self.fallback_key_hits += 1;
        let Some(index) = choose_pose_for(&self.entries, indices, &pose, self.goku_form) else {
            self.palette_rejections += 1;
            let reason = pose_rejection_reason(&self.entries, indices, &pose);
            let rejected = RejectedPose::new(&pose, "refined", reason, indices);
            self.record_rejection(rejected);
            return false;
        };
        if !(self.entries[index].id.starts_with("battle-")
            || self.entries[index].id.starts_with("field-fg-"))
        {
            self.record_rejection(RejectedPose::new(
                &pose, "refined", "unsupported_category", &[index]));
            return false;
        }
        *owned |= pose.oam_bits;
        if choose_pose(&self.entries,indices,&pose).is_none()
            && goku_battle_id(&self.entries[index].id).is_some(){
            self.goku_ambiguities_resolved+=1;
        }
        self.active.push(Matching { index, pose });
        self.fallback_accepted += 1;
        true
    }

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
            if let Some(variant)=&entry.variant {
                let ok_name=variant.art.starts_with("variants/")
                    && (variant.art.ends_with(".png") || variant.art.ends_with(".webp"))
                    && variant.art.len()<=240 && !variant.art.contains("..")
                    && !variant.art.contains('\\');
                if !entry.available || entry.luma || !ok_name
                    || variant.margin>96 || variant.margin%4!=0 {
                    return Err(format!("人物 {} 的擴展圖不安全",entry.id));
                }
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
        self.variants.clear();
        self.probes=0;
        self.key_hits=0;
        self.palette_rejections=0;
        self.fallback_probes=0;
        self.fallback_key_hits=0;
        self.fallback_attempted=false;
        self.fallback_accepted=0;
        self.unindexed_keys.clear();
        self.rejected.clear();
        self.goku_form=None;
        self.goku_ambiguities_resolved=0;
        Ok(self.entries.len())
    }

    pub(crate) fn refresh(&mut self, log: &FrameLog, wram: &[u8]) {
        self.active.clear();
        self.probes=0;
        self.key_hits=0;
        self.palette_rejections=0;
        self.fallback_probes=0;
        self.fallback_key_hits=0;
        self.fallback_attempted=false;
        self.fallback_accepted=0;
        self.unindexed_keys.clear();
        self.rejected.clear();
        self.goku_ambiguities_resolved=0;
        self.goku_form=goku_form_from_wram(wram);
        if self.lookup.is_empty() { return; }

        // Respect primary battle grouping and its exact source key, size,
        // colour and OAM mask before trying any refined candidate.
        let mut owned=0_u128;
        let mut missing_oam=0_u128;
        for pose in ssd_hd::live_fighter_poses(log) {
            let missed_bits=pose.oam_bits;
            if !self.accept_primary(pose, &mut owned) {
                missing_oam |= missed_bits;
            }
            if self.active.len() >= MAX_POSES { return; }
        }

        // Previously this ran only when *no* fighter matched, leaving Goku
        // as SNES art whenever Saibaman already matched. Try the upstream
        // field/refine logic for unmatched OAM, but never steal the source
        // OAM bits belonging to an already verified fighter.
        if self.should_try_refinement(missing_oam) {
            self.fallback_attempted=true;
            let candidates={
                let entries=&self.entries;
                let lookup=&self.lookup;
                let mut has_battle_art=|key,width,height| {
                    lookup.get(&key).is_some_and(|indices| indices.iter().any(|&i|
                        (entries[i].id.starts_with("battle-")
                            || entries[i].id.starts_with("field-fg-"))
                            && entries[i].width==width && entries[i].height==height))
                };
                ssd_hd::live_field_fighter_poses(log,&mut has_battle_art)
            };
            let had_primary=owned!=0;
            for pose in candidates {
                self.accept_refined(pose, &mut owned, missing_oam, had_primary);
                if self.active.len()>=MAX_POSES { break; }
            }
        }
    }

    pub(crate) fn probe_json(&self) -> String {
        serde_json::json!({
            "poses":self.probes, "keyHits":self.key_hits,
            "paletteRejected":self.palette_rejections,
            "matched":self.active.len(),
            "fallbackPoses":self.fallback_probes,
            "fallbackKeyHits":self.fallback_key_hits,
            "fallbackAttempted":self.fallback_attempted,
            "fallbackAccepted":self.fallback_accepted,
            "gokuAmbiguitiesResolved":self.goku_ambiguities_resolved,
            "gokuFormEvidence":match self.goku_form {
                Some(GokuForm::Normal)=>"normal",
                Some(GokuForm::SuperSaiyan)=>"super",
                None=>"unknown",
            },
            "unindexedKeys":&self.unindexed_keys,
            "rejectedPoses":self.rejected.iter().map(|rejected| {
                serde_json::json!({
                    "key":format!("{:016x}",rejected.key),
                    "w":rejected.width,"h":rejected.height,
                    "origin":rejected.origin,"reason":rejected.reason,
                    "candidateIds":rejected.candidates.iter()
                        .filter_map(|&i|self.entries.get(i).map(|e|e.id.as_str()))
                        .collect::<Vec<_>>()
                })
            }).collect::<Vec<_>>(),
            "reason": if self.active.is_empty() {
                if self.key_hits+self.fallback_key_hits==0 {
                    "來源鍵值未命中官方 class2 素材"
                } else {
                    "來源鍵值命中，但尺寸／調色盤／歧義驗證未通過"
                }
            } else { "已驗證人物姿勢" },
            "lumaMatched":self.active.iter().filter(|m|self.entries[m.index].luma).count(),
            "ready":self.active.iter()
                .filter(|p| self.pictures.contains_key(&p.index)).count(),
        }).to_string()
    }

    /// Cheap readiness check: avoid generating and parsing matches JSON on
    /// every emulated frame after all visible poses are loaded.
    pub(crate) fn has_pending_art(&self) -> bool {
        self.active.iter().any(|m| !self.pictures.contains_key(&m.index)
            || self.entries[m.index].variant.is_some() && !self.variants.contains_key(&m.index))
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
                "x":matched.pose.x,"y":matched.pose.y,
                "width":matched.pose.width,"height":matched.pose.height,
                "luma":entry.luma,
                "variant":entry.variant.as_ref().map(|v|serde_json::json!({
                    "art":v.art,"margin":v.margin
                })),
                "variantReady":self.variants.contains_key(&matched.index),
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

    /// Local 4x pose-variant art, downsampled to 2x by Safari. Bounded
    /// enlarged art is composited only over the exact underlying class2 pose.
    pub(crate) fn add_variant(&mut self,index:usize,width:usize,height:usize,rgba:&[u8])
        ->Result<(),String>
    {
        if !self.active.iter().any(|m|m.index==index){
            return Err("人物姿勢已切換，忽略過期擴展圖".into());
        }
        let entry=self.entries.get(index).ok_or("人物姿勢索引不存在")?;
        let variant=entry.variant.as_ref().ok_or("該人物沒有擴展圖")?;
        let expected_width=usize::from(entry.width)*2+usize::from(variant.margin);
        let expected_height=usize::from(entry.height)*2+usize::from(variant.margin);
        let bytes=width.checked_mul(height).and_then(|n|n.checked_mul(4))
            .ok_or("擴展圖尺寸溢位")?;
        if width!=expected_width || height!=expected_height
            || bytes>MAX_PICTURE_BYTES || rgba.len()!=bytes {
            return Err(format!("人物 {} 的擴展圖尺寸不符",entry.id));
        }
        self.variants.insert(index,Picture{width,rgba:rgba.to_vec(),luma_ref:255});
        if self.variants.len()>6 {
            self.variants.retain(|i,_|self.active.iter().any(|m|m.index==*i));
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
            let Some(base)=self.pictures.get(&matched.index) else {continue};
            let variant=self.variants.get(&matched.index)
                .zip(self.entries[matched.index].variant.as_ref());
            let (picture,margin)=match variant {
                Some((image,entry))=>(image,i32::from(entry.margin/4)),
                None=>(base,0)
            };
            let luma=self.entries[matched.index].luma && margin==0;
            let pose=&matched.pose;
            let (left,top)=(pose.x-margin,pose.y-margin);
            let (right,bottom)=(left+i32::from(pose.width)+2*margin,
                top+i32::from(pose.height)+2*margin);
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
                    let near=if owned {false} else if owner==255 && y<176
                        && layer_ids[p]<=2
                    {
                        let mut adjacent=false;
                        'search: for dy in -1_i32..=1 {
                            for dx in -1_i32..=1 {
                                let xx=x as i32+dx;
                                let yy=y as i32+dy;
                                if xx<0 || xx>=WIDTH as i32 || yy<0 || yy>=rows as i32 {continue;}
                                let other=oam_ids[yy as usize*WIDTH+xx as usize];
                                if other<128 && pose.oam_bits & (1_u128<<other)!=0 {
                                    adjacent=true;
                                    break 'search;
                                }
                            }
                        }
                        adjacent
                    } else {false};
                    // A greyscale silhouette needs the *original source*
                    // colour. Never invent one for a background-only overhang.
                    // The official variant can extend beyond the original
                    // silhouette. Never cover another OBJ, UI layer or HUD.
                    let variant_overhang=margin>0 && owner==255 &&
                        layer_ids[p]<=2 && y<176;
                    if !owned && !variant_overhang && (!near || luma) {continue;}
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
                            let tinted=if luma {
                                hd_luma::colour(
                                    [before[sy*2+sx][0],before[sy*2+sx][1],
                                     before[sy*2+sx][2]],
                                    &picture.rgba[src..src+3],picture.luma_ref)
                            }else {
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
    fn transparent_hd_art_removes_its_original_rom_silhouette() {
        let mut live=LiveFighters::default();
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
    fn unique_exact_key_luma_tints_from_visible_oam_pixel_not_background(){
        let mut live=LiveFighters::default();
        let only=serde_json::json!([{
            "id":"battle-test","key":"abcd","width":2,"height":2,
            "art":"class2/grey.png","available":true,"luma":true,
            "palette":vec![serde_json::Value::Null;16]
        }]).to_string();
        live.load_catalog(&only).unwrap();
        assert_eq!(choose_pose(&live.entries,&[0],&pose()),Some(0));
        live.active.push(Matching{index:0,pose:pose()});
        let mut art=vec![0_u8;4*4*4];
        for (i,p) in art.chunks_exact_mut(4).enumerate(){
            let brightness=if i%2==0 {70}else{180};
            p.copy_from_slice(&[brightness,brightness,brightness,255]);
        }
        live.add_picture(0,4,4,&art).unwrap();
        let mut owners=vec![255;256*4];
        owners[1*256+1]=5;
        let layers=vec![0_u8;256*4];
        let beneath=vec![20;512*8*4];
        let mut out=vec![0_u8;512*8*4];
        for p in out.chunks_exact_mut(4){p.copy_from_slice(&[50,185,70,255]);}
        let changed=live.composite(&owners,&layers,&beneath,&mut out,512,8);
        assert_eq!(changed,1);
        let at=(2*512+2)*4;
        assert!(out[at+1]>out[at], "original green hue must be retained");
        assert!(out[at+1]>out[at+2], "must not render greyscale");
        assert!(out[at+1]<185, "dark HD pixel must shade source");
        let other=((2*512)+6)*4;
        assert_eq!(&out[other..other+3], &[50,185,70]);
    }
    #[test]
    fn official_variant_draws_hair_outside_pose_box_but_not_over_ui_or_other_sprite(){
        let raw:serde_json::Value=serde_json::from_str(&entry()).unwrap();
        let mut raw=raw;
        raw[0]["variant"]=serde_json::json!({"art":"variants/ssj.png","margin":4});
        let mut live=LiveFighters::default();
        live.load_catalog(&raw.to_string()).unwrap();
        live.active.push(Matching{index:0,pose:pose()});
        let base=vec![0_u8;4*4*4];
        live.add_picture(0,4,4,&base).unwrap();
        let variant=[200_u8,80,30,255].repeat(8*8);
        assert!(live.add_variant(0,7,8,&variant).is_err(),"dimensions are exact");
        live.add_variant(0,8,8,&variant).unwrap();
        assert!(live.variants.contains_key(&0));
        let mut owners=vec![255;256*4];
        owners[256+1]=5; // original core belongs to this pose
        owners[256+3]=7; // unrelated OAM in extended hair's region
        let mut layers=vec![0;256*4];
        layers[256]=4; // foreground UI plane blocks variant outside silhouette
        let beneath=vec![13;512*8*4];
        let mut output=vec![22;512*8*4];
        let changed=live.composite(&owners,&layers,&beneath,&mut output,512,8);
        assert!(changed>0);
        let get=|x:usize,y:usize|{
            let at=(y*2*512+x*2)*4;
            [output[at],output[at+1],output[at+2]]
        };
        assert_eq!(get(0,0),[200,80,30],"transparent-field overhang appears");
        assert_eq!(get(1,1),[200,80,30],"core sprite replaced");
        assert_eq!(get(3,1),[22,22,22],"other sprite remains above the variant");
        assert_eq!(get(0,1),[22,22,22],"foreground UI remains visible");
    }


    #[test]
    fn multi_fighter_refinement_does_not_let_saibaman_block_goku() {
        let mut pal=vec![serde_json::Value::Null;16];
        pal[1]=serde_json::json!([255,0,0]);
        pal[2]=serde_json::json!([0,255,0]);
        let catalog=serde_json::json!([
            {"id":"battle-saibaman-000","key":"abcd","width":2,"height":2,
             "art":"class2/saibaman.png","available":true,"palette":pal},
            {"id":"battle-goku-wrong-palette","key":"ef01","width":2,"height":2,
             "art":"class2/goku-wrong.png","available":true,"palette":pal},
            {"id":"battle-goku-verified","key":"ef02","width":2,"height":2,
             "art":"class2/goku.png","available":true,"palette":pal}
        ]);
        let mut live=LiveFighters::default();
        live.load_catalog(&catalog.to_string()).unwrap();
        let mut owned=0_u128;
        let saibaman=pose();
        assert!(live.accept_primary(saibaman,&mut owned));
        let mut rejected_goku=pose();
        rejected_goku.key=0xef01;
        rejected_goku.oam_bits=1<<7;
        rejected_goku.colours[1]=[0,0,255];
        assert!(!live.accept_primary(rejected_goku,&mut owned));
        assert_eq!(live.palette_rejections,1);
        assert!(live.should_try_refinement(1<<7));
        let mut verified_goku=pose();
        verified_goku.key=0xef02;
        verified_goku.oam_bits=1<<7;
        assert!(live.accept_refined(verified_goku,&mut owned,1<<7,true));
        assert_eq!(live.active.len(),2);
        assert_eq!(live.entries[live.active[0].index].id,"battle-saibaman-000");
        assert_eq!(live.entries[live.active[1].index].id,"battle-goku-verified");
        assert_eq!(owned,(1<<5)|(1<<7));
        assert_eq!(live.fallback_accepted,1);
        let output:serde_json::Value=serde_json::from_str(&live.probe_json()).unwrap();
        assert_eq!(output["rejectedPoses"][0]["reason"],"palette_mismatch");
        assert_eq!(output["rejectedPoses"][0]["key"],"000000000000ef01");
        assert_eq!(output["rejectedPoses"][0]["candidateIds"][0],
            "battle-goku-wrong-palette");
    }

    #[test]
    fn fallback_never_steals_sprite_from_verified_primary_or_unrelated_oam() {
        let mut live=LiveFighters::default();
        live.load_catalog(&entry()).unwrap();
        let mut owned=0_u128;
        assert!(live.accept_primary(pose(),&mut owned));
        assert!(!live.should_try_refinement(0),"all matched; avoid extra work");
        let mut conflict=pose();
        conflict.oam_bits=1<<5;
        assert!(!live.accept_refined(conflict,&mut owned,1<<6,true));
        let mut unrelated=pose();
        unrelated.oam_bits=1<<8;
        assert!(!live.accept_refined(unrelated,&mut owned,1<<6,true));
        assert_eq!(live.active.len(),1);
        assert_eq!(live.fallback_accepted,0);
        assert_eq!(live.rejected[0].reason,"oam_overlap");
        assert_eq!(live.probes,1);
    }

    #[test]
    fn reasons_identify_size_palette_and_ambiguity_without_guessing_character() {
        let mut live=LiveFighters::default();
        live.load_catalog(&entry()).unwrap();
        let mut smaller=pose();
        smaller.width=1;
        assert_eq!(pose_rejection_reason(&live.entries,&[0],&smaller),"size_mismatch");
        let mut wrong=pose();
        wrong.colours[1]=[0,0,255];
        assert_eq!(pose_rejection_reason(&live.entries,&[0],&wrong),"palette_mismatch");
        let exact=pose();
        assert_eq!(pose_rejection_reason(&live.entries,&[0,0],&exact),"ambiguous_palette");
        live.entries[0].luma=true;
        assert_eq!(pose_rejection_reason(&live.entries,&[0,0],&exact),"ambiguous_luma");
        assert_eq!(choose_pose(&live.entries,&[0,0],&exact),None);
    }

    #[test]
    fn rejected_trace_is_bounded_even_when_many_poses_fail() {
        let mut live=LiveFighters::default();
        live.load_catalog(&entry()).unwrap();
        let mut owned=0_u128;
        for k in 0..100 {
            let mut unknown=pose();
            unknown.key=0xf000+k;
            unknown.oam_bits=1<<6;
            assert!(!live.accept_primary(unknown,&mut owned));
        }
        assert_eq!(live.rejected.len(),MAX_REJECTED);
        assert_eq!(live.unindexed_keys.len(),3);
        let report:serde_json::Value=serde_json::from_str(&live.probe_json()).unwrap();
        assert_eq!(report["rejectedPoses"].as_array().unwrap().len(),MAX_REJECTED);
        assert_eq!(report["fallbackAccepted"],0);
    }

    #[test]
    fn luma_rejects_ambiguous_source_key_and_prefers_palette_exact_colour(){
        let mut live=LiveFighters::default();
        let raw=serde_json::json!([
            {"id":"battle-luma","key":"abcd","width":2,"height":2,
             "art":"class2/grey.png","available":true,"luma":true,
             "palette":vec![serde_json::Value::Null;16]},
            {"id":"battle-second-luma","key":"abcd","width":2,"height":2,
             "art":"class2/grey2.png","available":true,"luma":true,
             "palette":vec![serde_json::Value::Null;16]}
        ]).to_string();
        live.load_catalog(&raw).unwrap();
        assert_eq!(choose_pose(&live.entries,&[0,1],&pose()),None);
        live.entries[1].luma=false;
        live.entries[1].palette[1]=Some([255,0,0]);
        live.entries[1].palette[2]=Some([0,255,0]);
        assert_eq!(choose_pose(&live.entries,&[0,1],&pose()),Some(1));
        let mut changed=pose();changed.colours[1]=[0,0,255];
        assert_eq!(choose_pose(&live.entries,&[0,1],&changed),Some(0));
    }

}
