//! The frame-end renderer of the HD route (M6):
//! re-renders a frame from what the PPU read to draw it, a [`FrameLog`] (per-scanline registers,
//! the CGRAM writes, VRAM and OAM at the end of the display), without the machine.
//!
//! The renderer only reads the log. Its 1x output with no art swapped in is the PPU's own frame,
//! pixel for pixel: it models the layers the game uses (BG modes 0 and 1, Mode 7 with its per-line
//! matrices, objects with their per-line limits, windows, color math, mosaic, brightness and
//! forced blanking). What it does not model (hi-res, interlace, the other BG modes) it refuses
//! with [`Unsupported`], so a frame is never wrong silently.
//!
//! A [`Renderer`] keeps the line buffers the PPU keeps between scanlines: the object line buffer
//! is only rebuilt when the PPU rebuilt it, so the last frame's sprites can show on the first
//! line of the next one. Feed it the frames of a run in order.
//!
//! The HD pack (`pack`, written by `xtask hd-convert`) indexes the tiles of the game by their
//! content: [`Renderer::render_bound`] says, for each pixel, the character it shows and the pack
//! tile bound to it ([`Binder`], the run rule of `binding`), which is what the art swap needs.

mod art;
pub mod art_index;
mod bg;
pub mod bgart;
mod binding;
pub mod cards;
mod enhanced;
pub mod font;
pub mod lightning;
pub mod mapart;
mod menu_art;
mod mode7;
pub mod mode7art;
pub mod pack;
pub mod png;
mod pool;
mod screen;
mod sprites;
mod tooltip;
mod transfer;
pub mod translate;
pub mod world;

use ssd_oracle::{CgramWrite, Color, FrameLog, LineEntry, OamWrite, VramWrite};
use std::sync::Arc;

pub use art::art_loads_on_draw;
pub use art::{
    ArtSource, ArtStats, COLOUR_CLASSES, Conditions, DrawCost, Drawn, DrawnPose, Fate, FrameTally,
    Image, ItemKey, KindPixels, PortraitUse, PoseShape, PoseStat, TallyCount, TallyItem,
    TallyLayer, colour_class,
};
pub use binding::Binder;
pub use cards::{CARD_WORDS, CardArt, CardPart, Preview as CardPreview};
pub use enhanced::{HdFrame, HdOptions, Look, Rect};
pub use font::{Font, FontError};
pub use menu_art::MenuPicture;
pub use pack::{Index, TileRef};
pub use ssd_oracle::{LineRegs, SpriteLine};
pub use tooltip::symbol_words;

/// One battle fighter's real, grouped OAM pose in a completed PPU frame.
/// The key uses the upstream capture's SHA-1 algorithm, not a filename guess.
#[derive(Debug, Clone)]
pub struct LiveFighterPose {
    pub key: u64,
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
    /// The source OAM entries represented by this pose.
    pub oam_bits: u128,
    /// Exact current 15-bit SNES sprite-palette colours expanded to RGB.
    pub colours: [[u8; 3]; 16],
}

/// Return only whole-group/class2 fighter poses whose original source pixel
/// hashes can be checked against the user's official HD art catalogue.
/// This does not draw art and intentionally excludes effects, field objects,
/// ambiguous mid-frame OAM/VRAM writes and other unsafe substitutions.
#[must_use]
pub fn live_fighter_poses(log: &FrameLog) -> Vec<LiveFighterPose> {
    if !log.complete || !log.vram_writes.is_empty() || !log.oam_writes.is_empty()
        || log.vram.len() < 0x8000 || log.cgram_start.len() < 256
        || log.oam_low.len() < 512 || log.oam_high.len() < 32
    {
        return Vec::new();
    }
    let view=art::poses::View::window();
    let sprites=art::poses::sprites_in(log,&view);
    // The desktop engine also treats OAM palettes 3..7 as fighters
    // during formations, but ONLY if their source key exists in the art
    // index. The web bridge performs that exact-key check immediately
    // afterwards, so don't lose valid standing enemies here.
    let mut found=art::poses::poses_in(log,&sprites,&view);
    found.extend(art::poses::fighter_candidates(&sprites));
    found.into_iter()
        .filter(|pose| pose.fighter && !pose.field && !pose.bg &&
            pose.sprites != 0 && pose.w>0 && pose.h>0 &&
            pose.w<=255 && pose.h<=239)
        .map(|pose| {
            let start=128+usize::from(pose.palette)*16;
            let colours=std::array::from_fn(|slot|
                art::poses::rgb555(log.cgram_start[start+slot]));
            LiveFighterPose {
                key:pose.key,
                x:pose.org.0,
                y:pose.org.1+art::poses::ROW_OF_CAPTURE,
                width:pose.w as u16,
                height:pose.h as u16,
                oam_bits:pose.sprites,
                colours,
            }
        }).collect()
}


/// Additional *catalogue-gated* fighter candidates assembled using the
/// upstream field-group/refine-field rules. Opening cutscenes may rotate OAM
/// entries or split a person into separate runs, even when their pixel array
/// exists in the official class2 battle catalogue.
///
/// This does not identify characters by position, colour, or frame history:
/// only complete source-key + source-size matches supplied by the caller are
/// returned. An unrelated field NPC can never be promoted without that key.
/// Refuse partially written PPU frames, and cap the combinatorial refinement
/// work to avoid stalling an iPhone during a busy scene.
#[must_use]
pub fn live_field_fighter_poses(
    log: &FrameLog,
    has_art: &mut dyn FnMut(u64, u16, u16) -> bool,
) -> Vec<LiveFighterPose> {
    if !log.complete || !log.vram_writes.is_empty() || !log.oam_writes.is_empty()
        || log.vram.len() < 0x8000 || log.cgram_start.len() < 256
        || log.oam_low.len() < 512 || log.oam_high.len() < 32
    {
        return Vec::new();
    }
    let view = art::poses::View::window();
    let sprites = art::poses::sprites_in(log, &view);
    let fields: Vec<_> = art::poses::poses_in(log, &sprites, &view)
        .into_iter().filter(|p| p.field && !p.bg && !p.shadow).collect();
    if fields.is_empty() || fields.len() > 12 {
        return Vec::new();
    }
    let mut known = |pose: &art::poses::Pose| {
        pose.field && !pose.cut && !pose.bg && !pose.shadow
            && pose.sprites != 0 && pose.w > 0 && pose.h > 0
            && pose.w <= 255 && pose.h <= 239
            && has_art(pose.key, pose.w as u16, pose.h as u16)
    };
    // The upstream refinement rejoins OAM groups whose ordering has rotated,
    // and peels away small unrelated pieces only if the remaining figure and
    // leftover satisfy the catalogue. It never chooses a nearest-looking art.
    let refined = art::poses::refine_field(fields, &sprites, log, &view, &mut known);
    refined.into_iter().filter(|pose| known(pose)).map(|pose| {
        let start = 128 + usize::from(pose.palette) * 16;
        let colours = std::array::from_fn(|slot|
            art::poses::rgb555(log.cgram_start[start + slot]));
        LiveFighterPose {
            key: pose.key,
            x: pose.org.0,
            y: pose.org.1 + art::poses::ROW_OF_CAPTURE,
            width: pose.w as u16,
            height: pose.h as u16,
            oam_bits: pose.sprites,
            colours,
        }
    }).collect()
}

/// An exactly keyed non-fighter OAM group, with source PPU sprite ownership.
/// kind 1 = field/NPC/object (class3), 2 = battle effect (class2).
/// Neither a battle fighter nor an unverified position-based overlay may
/// enter this list. Full game-frame integrity is required.
#[derive(Debug, Clone)]
pub struct LiveSpecialPose {
    pub kind: u8,
    pub key: u64,
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
    pub oam_bits: u128,
    pub colours: [[u8; 3]; 16],
}

/// Assemble field groups and effects by the upstream capture's actual OAM
/// source-key algorithm. The browser then validates dimensions, palettes,
/// image availability and visible OAM ownership before drawing any art.
#[must_use]
pub fn live_special_poses(log: &FrameLog) -> Vec<LiveSpecialPose> {
    // Class2 RGBA effect keys are derived from the actual scanline colours;
    // the upstream Palettes::of(log) already applies CGRAM writes at their
    // corresponding PPU entries. Rejecting *all* effects whenever a palette
    // write occurs causes the opening flight HD to vanish mid-animation.
    // VRAM/OAM writes still invalidate source pose membership and stay gated.
    if !log.complete || !log.vram_writes.is_empty() || !log.oam_writes.is_empty()
        || log.vram.len() < 0x8000 || log.cgram_start.len() < 256
        || log.oam_low.len() < 512 || log.oam_high.len() < 32
    { return Vec::new(); }
    let view=art::poses::View::window();
    let sprites=art::poses::sprites_in(log,&view);
    art::poses::poses_in(log,&sprites,&view).into_iter().filter_map(|pose|{
        // Field/class3 keys use palette *indices*, not RGB, and cannot
        // prove on-scanline colour stability. Continue to skip field art
        // during palette writes, while allowing source-colour-keyed effects.
        let kind=if pose.field && !pose.cut && !pose.fighter && !pose.bg
            && log.cgram_writes.is_empty() {
            1_u8
        } else if !pose.field && !pose.fighter && !pose.bg && !pose.shadow
            && !pose.rgba.is_empty() {
            2_u8
        } else { return None };
        if pose.sprites==0 || pose.w==0 || pose.h==0 ||
            pose.w>255 || pose.h>239 {return None;}
        let start=128+usize::from(pose.palette)*16;
        let colours=std::array::from_fn(|slot|
            art::poses::rgb555(log.cgram_start[start+slot]));
        Some(LiveSpecialPose {
            kind,key:pose.key,x:pose.org.0,
            y:pose.org.1+art::poses::ROW_OF_CAPTURE,
            width:pose.w as u16,height:pose.h as u16,
            oam_bits:pose.sprites,colours
        })
    }).collect()
}

/// Plane coordinates from the *same* SNES Mode 7 matrix used by the
/// original renderer, at 2x subpixels. Returns 1/256th map-pixel units.
/// Never infer an artwork position from the visible frame's RGB values.
#[must_use]
pub fn live_mode7_point(regs:&LineRegs,x:i32,y:u16,sx:u8,sy:u8)
    ->Option<(u32,u32)> {
    if regs.bg_mode!=7 || regs.forced_blanking || x<0||x>=256 ||
        sx>1 ||sy>1 {return None;}
    let plane=mode7::Plane::of(regs);
    let (mx,my)=plane.map_position(2,(x,i64::from(sx)),(y,i64::from(sy)));
    let plane_size=1024_i64*256;
    let wrap=|v:i64|match regs.m7_out_of_bounds {
        0=>Some(v.rem_euclid(plane_size)),
        _=>(0..plane_size).contains(&v).then_some(v),
    };
    Some((wrap(mx)? as u32,wrap(my)? as u32))
}

/// Width of the frame the renderer produces (and of the PPU's, without hi-res).
pub const WIDTH: usize = 256;

/// The version of what the renderer draws a frame from: bump it when the art matching or the
/// drawing changes which pixels the art covers, so the results kept of earlier runs (the parts of
/// `xtask hd-tour`) are not mistaken for the new ones.
pub const RENDERER_VERSION: u32 = 11;

/// The largest scale of an Enhanced picture (see [`Renderer::render_hd`]).
pub const MAX_SCALE: u32 = 8;

/// What the renderer does not model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    /// The frame was not logged from its first scanline to the end of its display.
    Incomplete,
    /// A hi-res (mode 5 or 6, pseudo-hi-res) or interlaced frame.
    HiRes,
    /// A BG mode other than 0, 1 and 7.
    BgMode(u8),
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete => f.write_str("the frame was not logged from its start"),
            Self::HiRes => f.write_str("hi-res or interlaced frame"),
            Self::BgMode(mode) => write!(f, "BG mode {mode}"),
        }
    }
}

impl std::error::Error for Unsupported {}

/// A rendered frame: `width * height` pixels, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Color>,
    /// What each pixel shows, row by row, when the frame was rendered with
    /// [`Renderer::render_bound`]; empty otherwise.
    pub sources: Vec<Source>,
}

/// What a pixel of a frame shows: the layer it was drawn from, and for a character its VRAM
/// address and the pack tile it is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The backdrop: nothing is drawn there, or the line is forced blank.
    Backdrop,
    /// A Mode 7 plane pixel: the plane is not made of characters.
    Plane,
    /// A BG character (modes 0 and 1) or an object tile. `address` is its VRAM word address,
    /// `bpp` its bit depth, and `tile` the pack tile bound to it, if any.
    Char { object: bool, bpp: u8, address: u16, tile: Option<TileRef> },
}

/// VRAM and OAM (low table as bytes, high table) as the entry being rendered sees them.
#[derive(Clone, Copy)]
pub(crate) struct Memories<'a> {
    pub vram: &'a [u16],
    pub oam: (&'a [u8], &'a [u8]),
}

/// One pixel of a BG or object line buffer. `char` is the VRAM word address of the character
/// the pixel comes from (0 in Mode 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pixel {
    pub palette: u8,
    pub color: u8,
    pub priority: u8,
    pub char: u16,
    /// Where in its character the pixel is: column, row (both in the character's own pixels,
    /// flips applied) and the two flips, see [`pos`].
    pub pos: u8,
    /// The OAM entry an object pixel comes from; [`NO_OBJECT`] for the other layers.
    pub oam: u8,
}

/// `Pixel::oam` of a pixel that is not an object's.
pub(crate) const NO_OBJECT: u8 = 0xFF;

/// `Pixel::pos`: the column and row of the pixel in its character (0 to 7, as the character
/// stores them: the flips are already undone) and whether the picture shows it flipped.
pub(crate) fn pos(col: u8, row: u8, x_flip: bool, y_flip: bool) -> u8 {
    col | row << 3 | u8::from(x_flip) << 6 | u8::from(y_flip) << 7
}

impl Pixel {
    pub const TRANSPARENT: Self =
        Self { palette: 0, color: 0, priority: 0, char: 0, pos: 0, oam: NO_OBJECT };

    pub fn is_transparent(self) -> bool {
        self.color == 0
    }
}

/// The winning source character and source-pixel coordinate at an on-screen
/// position. Used only by the browser's opt-in class1 HD portrait matcher.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct LiveTilePixel {
    pub address: u16,
    pub pos: u8,
    pub palette: u8,
    /// 1..4 BG, 5 OBJ; 0 is backdrop, and cannot receive portrait art.
    pub layer: u8,
    pub bpp: u8,
}

/// The renderer: line buffers and the CGRAM of the entry being rendered.
pub struct Renderer {
    pub(crate) bg_pixels: [[Pixel; WIDTH]; 4],
    pub(crate) obj_pixels: [Pixel; WIDTH],
    /// The PPU sprite line buffer at the start of the latest full render.
    /// Reused to draw the true background beneath a matched HD fighter.
    pub(crate) before_obj_pixels: [Pixel; WIDTH],
    pub(crate) cgram: [u16; 256],
    /// VRAM as the frame changes it, when it does.
    vram_work: Vec<u16>,
    /// OAM (low table as bytes, high table) as the frame changes it, when it does.
    oam_work: (Vec<u8>, Vec<u8>),
    pub(crate) main: screen::ScreenBuffer,
    pub(crate) sub: screen::ScreenBuffer,
    /// The window position of the first pixel of the rows drawn: 0 for the PPU's own frame. A
    /// chunk of the wide view sits at another place in the window, so the windows and Mode 7 are
    /// evaluated at `origin + x` (positions outside the screen's 256 pixels are in no window).
    pub(crate) origin: i32,
    /// Optional pixel-layer ownership for the browser's local HD background compositor.
    track_layers: bool,
    layer_ids: Vec<u8>,
    oam_ids: Vec<u8>,
    /// On-demand exact character evidence for locally imported HD faces.
    track_portrait_pixels: bool,
    portrait_pixels: Vec<LiveTilePixel>,
    /// What the Enhanced picture needs, recorded while `capturing` (see `enhanced`).
    capture: enhanced::Capture,
    capturing: bool,
    /// Objects (bit `n` for OAM entry `n`) whose pixels are left out of the object line buffer:
    /// the picture under the sprites an HD pose replaces. They still take their places in the
    /// 32 sprites and 34 tiles a line the PPU counts.
    pub(crate) hidden_objects: u128,
    /// The frame rows (first, past the last) the picture under the poses is drawn for; the other
    /// lines are left black (see [`art::pose_rows`]). `None` draws every line.
    pub(crate) only_rows: Option<(usize, usize)>,
    /// The pack's art, when the pictures draw it (see [`Renderer::set_art`]).
    pub(crate) art: Option<Box<art::ArtState>>,
    /// The HD text font, when the pictures draw their text from it (see [`Renderer::set_font`]).
    font: Option<Arc<Font>>,
    /// Capture ROM glyphs without redrawing the English text before Chinese output.
    text_capture_only: bool,
    /// The original translator renders all translated text AFTER local HD art is applied.
    native_translation: Option<translate::Translator>,
    native_text_frame: Option<Frame>,
    native_changed_samples: u32,
    /// The text cells of the frame being drawn, noted while `text_on`.
    text: font::TextCapture,
    text_on: bool,
    /// The lightning of the Super Saiyan transformation, when its pictures were found (see [`Renderer::set_lightning`]).
    lightning: Option<lightning::Overlay>,
    /// The frames since the transformation started, while it goes on: the conditions' (see
    /// [`Conditions::with_ssj_lightning`]).
    ssj_lightning: Option<u32>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bg_pixels: [[Pixel::TRANSPARENT; WIDTH]; 4],
            obj_pixels: [Pixel::TRANSPARENT; WIDTH],
            before_obj_pixels: [Pixel::TRANSPARENT; WIDTH],
            cgram: [0; 256],
            vram_work: Vec::new(),
            oam_work: (Vec::new(), Vec::new()),
            main: screen::ScreenBuffer::new(),
            sub: screen::ScreenBuffer::new(),
            origin: 0,
            track_layers: false,
            layer_ids: Vec::new(),
            oam_ids: Vec::new(),
            track_portrait_pixels: false,
            portrait_pixels: Vec::new(),
            capture: enhanced::Capture::default(),
            capturing: false,
            hidden_objects: 0,
            only_rows: None,
            art: None,
            font: None,
            text_capture_only: false,
            native_translation: None,
            native_text_frame: None,
            native_changed_samples: 0,
            text: font::TextCapture::default(),
            text_on: false,
            lightning: None,
            ssj_lightning: None,
        }
    }

    /// Makes the Enhanced pictures ([`Renderer::render_hd`]) draw the pictures of `lightning` over the frame while
    /// Goku's first Super Saiyan transformation goes on (see [`lightning`]); `None` (the start) draws none. The 1x frame
    /// of [`Renderer::render`] never has it.
    pub fn set_lightning(&mut self, lightning: Option<Arc<lightning::Lightning>>) {
        self.lightning = lightning.map(lightning::Overlay::new);
    }

    /// Draws the lightning over the Enhanced picture `hd`, while the transformation goes on and there are pictures.
    pub(crate) fn draw_lightning(&mut self, hd: &mut enhanced::HdFrame) {
        if let (Some(overlay), Some(frames)) = (self.lightning.as_mut(), self.ssj_lightning) {
            overlay.draw(hd, frames);
        }
    }

    /// Makes the Enhanced pictures of a scale of 2 or more ([`Renderer::render_hd`]) draw the
    /// game's UI text from `font` (see [`font`]); `None` (the start) leaves the text as the look
    /// draws it. The 1x frame of [`Renderer::render`] never has HD text.
    pub fn set_font(&mut self, font: Option<Arc<Font>>) {
        self.font = font;
    }

    /// Connect the upstream, stateful Traditional Chinese renderer.
    /// Unlike a browser colour-box overlay, it can identify real text cells,
    /// partial names, numeric columns, layered windows and page transitions.
    pub fn set_native_translation(&mut self, overlay: Option<Arc<translate::Overlay>>) {
        self.native_translation = overlay.map(translate::Translator::new);
        self.native_text_frame = None;
        self.native_changed_samples = 0;
    }

    /// The page named at the START of the SNES frame, as required by the
    /// upstream stateful translation engine (not the next frame's WRAM).
    pub fn set_translation_screen(
        &mut self, screen: Option<ssd_catalogue::engine::OnScreen>,
    ) {
        if let Some(translator) = self.native_translation.as_mut() {
            translator.screen = screen;
        }
    }

    /// The intro uses BG1 and BG2 exclusively for its scrolling ROM text.
    /// Only hide a layer when the original translator confirms every line on
    /// that layer has a translation. Keep the untouched log for text positions.
    fn quiet_intro_log(&self, log: &FrameLog) -> Option<FrameLog> {
        let quiet = self.native_translation.as_ref()?.quiet_layers();
        if !quiet.iter().any(|&hide| hide) { return None; }
        let mut underlay = log.clone();
        for entry in &mut underlay.entries {
            for (bg, hide) in quiet.iter().enumerate() {
                if *hide { entry.regs.main_bg[bg] = false; }
            }
        }
        Some(underlay)
    }

    /// Browser translation is a final compositing pass, unlike the desktop
    /// renderer. Build the intro underlay first, without the two English text
    /// planes, using a separate renderer to preserve the captured glyph cells
    /// and source frame. Live HD artwork uses the clean layer ownership.
    pub fn render_intro_underlay(&mut self, log: &FrameLog, options: HdOptions) -> Option<HdFrame> {
        let underlay = self.quiet_intro_log(log)?;
        let mut renderer = Renderer::new();
        renderer.set_layer_tracking(self.track_layers);
        let picture = renderer.render_hd(&underlay, options).ok()?;
        if self.track_layers {
            self.layer_ids = renderer.layer_ids;
            self.oam_ids = renderer.oam_ids;
        }
        Some(picture)
    }

    /// Confirm that a page's ROM-derived English is actually visible on the
    /// current PPU frame. Do not infer the text window from background colour.
    /// This is used ONLY when the frame-start page is not visible.
    #[must_use]
    pub fn visible_translation_score(
        &self, page: &ssd_catalogue::engine::OnScreen,
    ) -> Option<usize> {
        let (Some(frame), Some(font)) = (self.native_text_frame.as_ref(), self.font.as_ref())
        else { return None };
        if page.english.is_empty() || self.text.cells().len() < 3 { return None; }
        let seen = translate::presence_in_window(
            page, self.text.cells(), &|glyph| font.character(glyph),
            self.origin, (frame.width as i32, frame.height as i32), None, None,
        );
        (seen.matched && seen.presence == translate::Presence::In &&
            seen.own >= 5 && seen.other == 0).then_some(seen.own)
    }

    /// Count visible ROM ASCII glyphs on a portrait dialogue or narration page's PPU text rows.
    /// This is a read-only, bounded check for a transition frame that may
    /// briefly show English before the stateful translator has placed Chinese.
    /// It never treats a generic ivory UI/status panel as a dialogue box.
    #[must_use]
    pub fn story_ascii_cells(&self, page: &ssd_catalogue::engine::OnScreen) -> usize {
        let Some(font) = self.font.as_ref() else { return 0 };
        if !matches!(page.table, 2 | 3) || page.english.is_empty() { return 0; }
        let (column, row) = page.base_cell();
        let left = i32::from(column) * 8 - 16;
        let top = i32::from(row) * 8 - 1;
        let rows = page.english.len().min(4);
        self.text.cells().iter().filter(|cell| {
            cell.mask != 0 && cell.half == 0 && cell.x >= left && cell.x < 256
                && (0..rows).any(|line| {
                    (cell.y - (top + line as i32 * 16)).abs() <= 2
                })
                && font.character(cell.glyph)
                    .is_some_and(|letter| letter.is_ascii_alphanumeric())
        }).take(3).count()
    }

    /// Sample locations within real text cells that changed after native
    /// translation. Zero proves only that none of these samples changed,
    /// NOT that no translated glyph was painted elsewhere.
    #[must_use]
    pub fn native_changed_samples(&self) -> u32 { self.native_changed_samples }

    /// Capture only the 1x PPU text source cells for Classic Chinese mode.
    /// No 2x filters, HD layers, art decoding or sprite replacement is run.
    /// The source frame is used solely to verify native SNES text positions.
    pub fn capture_classic_text(&mut self, log: &FrameLog) -> bool {
        if self.native_translation.is_none() || self.font.is_none() {
            self.native_text_frame = None;
            return false;
        }
        self.text.clear();
        self.text_on = true;
        self.capturing = false;
        let frame = self.render_log(log, None, false);
        self.text_on = false;
        self.native_text_frame = frame.ok();
        self.native_text_frame.is_some()
    }

    /// Paint sharp 2x Chinese glyphs on a nearest-neighbour copy of the
    /// native 1x PPU frame. This NEVER enables HD art/PPU filters.
    pub fn paint_classic_translation(
        &mut self, log: &FrameLog, rgba: &mut [u8], width: usize, height: usize,
    ) -> bool {
        self.paint_native_translation_scaled(log, rgba, width, height, 2)
    }

    /// Draw Traditional Chinese after the HD art passes on the 2x image.
    pub fn paint_native_translation(
        &mut self, log: &FrameLog, rgba: &mut [u8], width: usize, height: usize,
    ) -> bool {
        self.paint_native_translation_scaled(log, rgba, width, height, 2)
    }

    fn paint_native_translation_scaled(
        &mut self,
        log: &FrameLog,
        rgba: &mut [u8],
        width: usize,
        height: usize,
        scale: usize,
    ) -> bool {
        let (Some(translator), Some(frame), Some(font)) = (
            self.native_translation.as_mut(),
            self.native_text_frame.as_ref(),
            self.font.as_ref(),
        ) else {
            return false;
        };
        if width != frame.width * scale || height != frame.height * scale
            || rgba.len() != width.saturating_mul(height).saturating_mul(4)
        {
            return false;
        }
        // Sample a few source text-cell pixels, never diff the whole screen.
        let offsets = [(2_usize, 2_usize), (4, 4), (6, 6)];
        let positions: Vec<usize> = self.text.cells().iter().take(180)
            .flat_map(|cell| offsets.into_iter().filter_map(move |(dx, dy)| {
                let x = usize::try_from(cell.x.checked_mul(scale as i32)?).ok()?
                    .checked_add(dx * scale)?;
                let y = usize::try_from(cell.y.checked_mul(scale as i32)?).ok()?
                    .checked_add(dy * scale)?;
                (x < width && y < height).then_some(4 * (y * width + x))
            }))
            .collect();
        let before: Vec<[u8; 3]> = positions.iter()
            .map(|&at| [rgba[at], rgba[at + 1], rgba[at + 2]]).collect();
        let mut target = translate::Target { rgba, width, scale };
        let mut beneath = |_cells: &[(i32, i32)]| None;
        translator.draw(
            &mut target, self.text.cells(), frame, log, 0,
            &|glyph| font.character(glyph), &mut beneath, None,
        );
        self.native_changed_samples = positions.iter().zip(before)
            .map(|(&at, colour)| target.rgba[at..at + 3] != colour[..])
            .filter(|changed| *changed)
            .count() as u32;
        true
    }

    /// Enable capture of glyph positions without the HD font replacement pass.
    pub fn set_text_capture_only(&mut self, enabled: bool) {
        self.text_capture_only = enabled;
    }

    /// Visible English glyphs at game-pixel coordinates, with the in-game ink and paper.
    /// This is a read-only view of pixels actually drawn by the SNES PPU.
    #[must_use]
    pub fn observed_text_cells(&self) -> Vec<(i32, i32, String, [u8; 3], [u8; 3])> {
        let Some(font) = self.font.as_ref() else { return Vec::new() };
        self.text.cells().iter().filter_map(|cell| {
            if cell.mask == 0 || cell.half != 0 { return None; }
            let ch = font.character(cell.glyph)?;
            (ch.is_ascii() && !ch.is_ascii_control()).then(||
                (cell.x, cell.y, ch.to_string(), cell.fg, cell.bg))
        }).take(1536).collect()
    }

    /// Record visible winning BG (1-4), OBJ (5), or backdrop (0) for each 1x pixel.
    /// The browser bridge uses this to keep sprites above the substituted HD backgrounds.
    pub fn set_layer_tracking(&mut self, enabled: bool) {
        self.track_layers = enabled;
        if !enabled { self.layer_ids.clear(); self.oam_ids.clear(); }
    }

    /// Enable exact PPU tile-source capture only while the user has selected
    /// a verified class1 index. No RGB guessing or fixed screen coordinates.
    pub fn set_portrait_tracking(&mut self, enabled: bool) {
        self.track_portrait_pixels = enabled;
        if !enabled { self.portrait_pixels.clear(); }
    }

    /// Winning PPU source tile evidence for the most recent normal HD frame.
    #[must_use]
    pub fn portrait_pixels(&self) -> &[LiveTilePixel] { &self.portrait_pixels }

    /// The visible pixel-layer owners of the most recently rendered frame.
    #[must_use]
    pub fn layer_ids(&self) -> &[u8] { &self.layer_ids }

    /// Which OAM sprite actually won each pixel in the visible SNES picture
    /// (255 for BG, backdrop, or no OBJ). Only for layer-tracked renders.
    /// A browser HD pose may replace pixels of its own OAM members, never
    /// paint over a higher-priority BG/UI or another sprite.
    #[must_use]
    pub fn oam_ids(&self) -> &[u8] { &self.oam_ids }

    /// Reconstruct the picture below just the verified fighter sprites.
    /// Like the original desktop art renderer, this uses the PPU's own
    /// priority rules instead of erasing the character with a guessed colour.
    /// Restore all renderer state required for normal future frames and for
    /// the already-captured zh-TW glyph positions before returning.
    #[must_use]
    pub fn render_fighter_beneath(
        &mut self, log: &FrameLog, hidden: u128, options: HdOptions,
    ) -> Option<(HdFrame, Vec<u8>)> {
        if hidden == 0 || !self.track_layers || self.art.is_some()
            || self.capturing || self.text_on || options.scale != 2
        {
            return None;
        }
        let after_obj=self.obj_pixels;
        let old_hidden=std::mem::replace(&mut self.hidden_objects,hidden);
        let original_layers=std::mem::take(&mut self.layer_ids);
        let original_owners=std::mem::take(&mut self.oam_ids);
        // The underpass is a hidden second rendering. Its BG ownership must
        // never replace the portrait evidence from the original PPU picture.
        let portrait_tracking=std::mem::replace(&mut self.track_portrait_pixels,false);
        self.obj_pixels=self.before_obj_pixels;
        // Do not reintroduce intro English when replacing Goku's OAM pose.
        let quiet_log=self.quiet_intro_log(log);
        let under=self.render_log(quiet_log.as_ref().unwrap_or(log),None,false).ok();
        let under_layers=std::mem::take(&mut self.layer_ids);
        // Rendering the under-picture must be invisible to the next regular
        // PPU frame and to the native translation's original text capture.
        self.obj_pixels=after_obj;
        self.hidden_objects=old_hidden;
        self.layer_ids=original_layers;
        self.oam_ids=original_owners;
        self.track_portrait_pixels=portrait_tracking;
        let frame=under?;
        let capture=enhanced::Capture::default();
        Some((enhanced::picture(&frame,&capture,Some(log),options),under_layers))
    }

    /// Makes the Enhanced pictures ([`Renderer::render_hd`]) draw the art of `pack`; `None` (the
    /// start) draws none. The 1x frame of [`Renderer::render`] never has art.
    pub fn set_art(&mut self, pack: Option<Arc<pack::Pack>>) {
        self.art = pack.map(|pack| Box::new(art::ArtState::new(pack)));
    }

    /// Makes the pictures with art measure the colours the poses draw against the original's
    /// ([`ArtStats::drawn`]).
    pub fn set_art_measure(&mut self, on: bool) {
        if let Some(art) = self.art.as_mut() {
            art.measure = on;
        }
    }

    /// Reads the pack's art on a thread of its own (`true`) instead of in the picture that wants it (`false`, the
    /// start): a picture draws the art read so far and asks for the rest, drawn as the game draws it until the picture
    /// after it is read. The pictures every art is read for are the same. `SSD_HD_SERIAL` in the environment keeps the
    /// art read by the pictures. Call it after [`Renderer::set_art`].
    pub fn set_art_async(&mut self, on: bool) {
        if let Some(art) = self.art.as_mut() {
            art.set_async(on);
        }
    }

    /// The conditions of the frame the next picture with art is drawn from: the pack's pose variants whose condition holds
    /// are drawn ([`Conditions::of_work_ram`] reads them from the work RAM).
    pub fn set_conditions(&mut self, conditions: Conditions) {
        self.ssj_lightning = conditions.ssj_lightning();
        if let Some(art) = self.art.as_mut() {
            art.conditions = conditions;
        }
    }

    /// What the art did in the last picture drawn with it.
    #[must_use]
    pub fn art_stats(&self) -> ArtStats {
        self.art.as_ref().map(|art| art.stats.clone()).unwrap_or_default()
    }

    /// What the art's stages of the last picture drawn with art cost (see [`DrawCost`]): the whole picture's time is
    /// the caller's to measure. Zero without art.
    #[must_use]
    pub fn art_cost(&self) -> DrawCost {
        self.art.as_ref().map(|art| art.cost()).unwrap_or_default()
    }

    /// Renders the frame `log` describes, at 1x with no art swapped in.
    ///
    /// # Errors
    ///
    /// The frame uses something the renderer does not model ([`Unsupported`]).
    pub fn render(&mut self, log: &FrameLog) -> Result<Frame, Unsupported> {
        self.render_log(log, None, false)
    }

    /// [`Renderer::render`], and also what each pixel shows and the pack tile of its character
    /// ([`Frame::sources`]). The binding follows the VRAM of each line: a VRAM write during the
    /// frame rebinds from the line it applies to.
    ///
    /// # Errors
    ///
    /// As for [`Renderer::render`].
    pub fn render_bound(
        &mut self,
        log: &FrameLog,
        binder: &mut Binder<'_>,
    ) -> Result<Frame, Unsupported> {
        self.render_log(log, Some(binder), true)
    }

    fn render_log(
        &mut self,
        log: &FrameLog,
        mut binder: Option<&mut Binder<'_>>,
        sources: bool,
    ) -> Result<Frame, Unsupported> {
        if !log.complete {
            return Err(Unsupported::Incomplete);
        }
        if log.h_hi_res || log.v_hi_res {
            return Err(Unsupported::HiRes);
        }
        for entry in &log.entries {
            let regs = &entry.regs;
            if regs.interlaced || regs.pseudo_h_hi_res || regs.pseudo_obj_hi_res {
                return Err(Unsupported::HiRes);
            }
            if !matches!(regs.bg_mode, 0 | 1 | 7) {
                return Err(if regs.bg_mode == 5 || regs.bg_mode == 6 {
                    Unsupported::HiRes
                } else {
                    Unsupported::BgMode(regs.bg_mode)
                });
            }
        }

        let height = usize::from(log.height);
        let sourced = binder.is_some() && sources;
        let mut frame = Frame {
            width: WIDTH,
            height,
            pixels: vec![Color::BLACK; WIDTH * height],
            sources: if sourced { vec![Source::Backdrop; WIDTH * height] } else { Vec::new() },
        };
        if self.track_portrait_pixels {
            self.portrait_pixels.resize(WIDTH * height, LiveTilePixel::default());
            self.portrait_pixels.fill(LiveTilePixel::default());
        }
        if self.track_layers {
            self.layer_ids.resize(WIDTH * height, 0);
            self.layer_ids.fill(0);
            self.oam_ids.resize(WIDTH * height, NO_OBJECT);
            self.oam_ids.fill(NO_OBJECT);
        }
        self.cgram.copy_from_slice(&log.cgram_start);
        if let Some(binder) = binder.as_deref_mut() {
            binder.update(&log.vram);
        }
        // The game turns the display off and loads VRAM mid-frame: those writes apply from the
        // entry they precede, to a copy of VRAM
        let mut work = std::mem::take(&mut self.vram_work);
        if !log.vram_writes.is_empty() {
            work.clear();
            work.extend_from_slice(&log.vram);
        }
        let mut oam = std::mem::take(&mut self.oam_work);
        if !log.oam_writes.is_empty() {
            oam.0.clear();
            oam.0.extend_from_slice(&log.oam_low);
            oam.1.clear();
            oam.1.extend_from_slice(&log.oam_high);
        }
        let mut cgram_writes = log.cgram_writes.iter().peekable();
        let mut vram_writes = log.vram_writes.iter().peekable();
        let mut oam_writes = log.oam_writes.iter().peekable();
        let mut stale = false;
        for (index, entry) in log.entries.iter().enumerate() {
            while let Some(&CgramWrite { index: slot, value, .. }) =
                cgram_writes.next_if(|write| usize::from(write.entry) <= index)
            {
                self.cgram[usize::from(slot)] = value;
            }
            let mut vram_changed = false;
            while let Some(&VramWrite { address, value, .. }) =
                vram_writes.next_if(|write| usize::from(write.entry) <= index)
            {
                work[usize::from(address)] = value;
                vram_changed = true;
            }
            stale |= vram_changed;
            if vram_changed {
                self.capture.vram_changed();
            }
            while let Some(&OamWrite { high, index, value, .. }) =
                oam_writes.next_if(|write| usize::from(write.entry) <= index)
            {
                let index = usize::from(index);
                if high {
                    oam.1[index] = value as u8;
                } else {
                    oam.0[2 * index..2 * index + 2].copy_from_slice(&value.to_le_bytes());
                }
            }
            let vram = if log.vram_writes.is_empty() { &log.vram } else { &work };
            let objects = if log.oam_writes.is_empty() {
                (&log.oam_low[..], &log.oam_high[..])
            } else {
                (&oam.0[..], &oam.1[..])
            };
            // The binding is only made for a line that shows something
            if stale && !entry.regs.forced_blanking {
                if let Some(binder) = binder.as_deref_mut() {
                    binder.update(vram);
                }
                stale = false;
            }
            self.render_entry(
                Memories { vram, oam: objects },
                entry,
                index,
                &mut frame,
                binder.as_deref(),
                sourced,
            );
        }
        self.vram_work = work;
        self.oam_work = oam;
        Ok(frame)
    }

    /// Draws the entry `index` of the log: the pixels of its line from its first one on.
    fn render_entry(
        &mut self,
        memories: Memories<'_>,
        entry: &LineEntry,
        index: usize,
        frame: &mut Frame,
        binder: Option<&Binder<'_>>,
        sourced: bool,
    ) {
        let line = usize::from(entry.scanline);
        if line == 0 || line > frame.height {
            return;
        }
        let from = usize::from(entry.from_pixel);
        if from >= WIDTH {
            return;
        }
        let span = (line - 1) * WIDTH..line * WIDTH;
        let regs = &entry.regs;

        if entry.from_pixel == 0 && entry.sprites.rebuilt {
            self.build_sprite_line(&memories, &entry.sprites, 0);
        }

        if self.only_rows.is_some_and(|(first, past)| !(first..past).contains(&(line - 1))) {
            return;
        }
        if regs.forced_blanking {
            frame.pixels[span.clone()][from..].fill(Color::BLACK);
            if self.track_layers {
                self.layer_ids[span.clone()][from..].fill(0);
                self.oam_ids[span.clone()][from..].fill(NO_OBJECT);
            }
            if self.capturing {
                self.capture_line(index, line, from, regs, memories.vram);
            }
            return;
        }
        self.render_bg_layers(memories.vram, regs, entry.scanline, from);
        self.render_scanline(regs, &mut frame.pixels[span.clone()], from);
        if self.track_layers {
            for (i, pixel) in self.main.rendered[from..].iter().enumerate() {
                self.oam_ids[span.start + from + i] =
                    if pixel.layer == screen::Layer::Obj { pixel.oam } else { NO_OBJECT };
                self.layer_ids[span.start + from + i] = match pixel.layer {
                    screen::Layer::Backdrop => 0,
                    screen::Layer::Bg1 => 1,
                    screen::Layer::Bg2 => 2,
                    screen::Layer::Bg3 => 3,
                    screen::Layer::Bg4 => 4,
                    screen::Layer::Obj => 5,
                };
            }
        }
        if self.track_portrait_pixels {
            for (x, src) in self.main.rendered[from..].iter().enumerate() {
                let layer=match src.layer {
                    screen::Layer::Backdrop=>0,
                    screen::Layer::Bg1=>1,
                    screen::Layer::Bg2=>2,
                    screen::Layer::Bg3=>3,
                    screen::Layer::Bg4=>4,
                    screen::Layer::Obj=>5,
                };
                let bpp=if layer==5 {4} else if (1..=4).contains(&layer)
                    && matches!(regs.bg_mode,0|1) {
                    bg::bg_bitplanes(regs.bg_mode,(layer-1) as usize) as u8
                } else {0};
                self.portrait_pixels[span.start+from+x]=LiveTilePixel {
                    address:src.char,pos:src.pos,palette:src.palette,layer,bpp,
                };
            }
        }
        if self.capturing {
            self.capture_line(index, line, from, regs, memories.vram);
        }
        if let (true, Some(binder)) = (sourced, binder) {
            self.sources_of_line(regs, binder, &mut frame.sources[span], from);
        }
        self.capture_art(regs, binder, line, from);
        if self.text_on {
            self.capture_text(regs, memories.vram, line, from);
        }
    }

    /// Notes the text cells of the line just drawn (see [`font`]).
    fn capture_text(&mut self, regs: &LineRegs, vram: &[u16], line: usize, from: usize) {
        let Some(font) = self.font.clone() else { return };
        let mut text = std::mem::take(&mut self.text);
        text.new_entry();
        let slots = self.art.as_ref().map_or(&[][..], |art| art.bg_slots());
        for x in from..WIDTH {
            if let Some(bg) = bg::of_layer(self.main.rendered[x].layer)
                && bgart::covers(slots, regs, bg, self.origin + x as i32, line)
            {
                continue;
            }
            text.note(
                &font,
                line - 1,
                x as i32,
                &self.main.rendered[x],
                || self.plan(regs, x),
                regs,
                &self.cgram,
                vram,
            );
        }
        self.text = text;
    }

    /// Records what the art passes need of the line just drawn, when they are on.
    fn capture_art(
        &mut self,
        regs: &LineRegs,
        binder: Option<&Binder<'_>>,
        line: usize,
        from: usize,
    ) {
        if let (Some(binder), true) = (binder, self.art.as_ref().is_some_and(|art| art.active)) {
            let mut art = self.art.take().expect("art is on");
            art.capture_line(self, regs, binder, line, from);
            self.art = Some(art);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame log of 224 lines with the same registers on each, empty memories.
    fn log(regs: LineRegs, sprites: SpriteLine) -> FrameLog {
        let entries = (1..=224)
            .map(|scanline| LineEntry { scanline, from_pixel: 0, regs, sprites })
            .collect();
        FrameLog {
            complete: true,
            height: 224,
            h_hi_res: false,
            v_hi_res: false,
            entries,
            cgram_start: vec![0; 256],
            cgram_writes: Vec::new(),
            vram: vec![0; 32768],
            vram_writes: Vec::new(),
            oam_low: vec![0; 512],
            oam_high: vec![0; 32],
            oam_writes: Vec::new(),
        }
    }

    fn mode_1() -> LineRegs {
        LineRegs {
            brightness: 15,
            bg_mode: 1,
            color_math: 3,
            bg_map_base: [0x0400, 0, 0, 0],
            bg_char_base: [0x1000, 0, 0, 0],
            main_bg: [true, false, false, false],
            ..LineRegs::default()
        }
    }

    fn rgb(color: Color) -> [u8; 3] {
        [color.r, color.g, color.b]
    }

    #[test]
    fn fighter_underpass_restores_ppu_ownership_and_future_render_state() {
        let log=log(mode_1(),SpriteLine::default());
        let mut renderer=Renderer::new();
        renderer.set_layer_tracking(true);
        let options=HdOptions{scale:2,look:Look::Crisp};
        let normal=renderer.render_hd(&log,options).unwrap();
        let owners=renderer.oam_ids().to_vec();
        let layers=renderer.layer_ids().to_vec();
        let saved_obj=renderer.obj_pixels;
        let (under,under_layers)=renderer.render_fighter_beneath(&log,1,options).unwrap();
        assert_eq!((under.width,under.height),(normal.width,normal.height));
        assert_eq!(under_layers.len(),256*224);
        assert_eq!(renderer.oam_ids(),owners);
        assert_eq!(renderer.layer_ids(),layers);
        assert_eq!(renderer.obj_pixels,saved_obj);
        assert_eq!(renderer.hidden_objects,0);
        assert_eq!(renderer.render_hd(&log,options).unwrap().rgba,normal.rgba);
    }

    #[test]
    fn the_backdrop_through_brightness() {
        let mut log = log(mode_1(), SpriteLine::default());
        log.cgram_start[0] = 0x7FFF;
        let mut renderer = Renderer::new();
        let frame = renderer.render(&log).unwrap();
        assert_eq!((frame.width, frame.height), (256, 224));
        assert!(frame.pixels.iter().all(|&p| rgb(p) == [255; 3]));

        for entry in &mut log.entries {
            entry.regs.brightness = 1;
        }
        let frame = renderer.render(&log).unwrap();
        // round(1 / 15 * 31 * 255 / 31) = 17
        assert!(frame.pixels.iter().all(|&p| rgb(p) == [17; 3]));
    }

    #[test]
    fn forced_blanking_is_black_and_a_cgram_write_applies_from_its_line() {
        let mut log = log(mode_1(), SpriteLine::default());
        log.cgram_start[0] = 0x001F;
        log.entries[2].regs.forced_blanking = true;
        log.cgram_writes.push(ssd_oracle::CgramWrite { entry: 100, index: 0, value: 0x03E0 });
        let frame = Renderer::new().render(&log).unwrap();
        assert_eq!(rgb(frame.pixels[0]), [255, 0, 0]);
        assert_eq!(rgb(frame.pixels[2 * WIDTH + 5]), [0, 0, 0]);
        assert_eq!(rgb(frame.pixels[99 * WIDTH]), [255, 0, 0]);
        assert_eq!(rgb(frame.pixels[100 * WIDTH]), [0, 255, 0]);
    }

    #[test]
    fn the_cgram_index_of_a_pixel_is_the_entry_its_colour_comes_from() {
        use crate::screen::{Layer, resolve_color, resolve_index};
        let mut cgram = [0_u16; 256];
        for (at, word) in cgram.iter_mut().enumerate() {
            *word = (at as u16) * 3 + 1;
        }
        for mode in 0..2 {
            for layer in [Layer::Bg1, Layer::Bg2, Layer::Bg3, Layer::Bg4, Layer::Obj] {
                for palette in 0..8 {
                    for color in 1..16 {
                        let pixel =
                            Pixel { palette, color, priority: 0, char: 0, pos: 0, oam: NO_OBJECT };
                        let index = resolve_index(layer, mode, pixel);
                        assert_eq!(
                            cgram[usize::from(index)],
                            resolve_color(&cgram, layer, mode, false, pixel),
                            "{layer:?} mode {mode} palette {palette} colour {color}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_bg_tile_with_flips_and_the_palette_row() {
        let mut log = log(mode_1(), SpriteLine::default());
        // Tile 1 at character base 0x1000: its row 1 has pixel 0 (left) = 2, pixel 7 = 1. The
        // first frame row is scanline 1: a BG shows its row 1 there, as the PPU does
        let tile = 0x1000 + 16 + 1;
        log.vram[tile] = 0b1000_0000_0000_0000 | 0b0000_0001; // plane 0 bit 7, plane 1 bit 0
        // Map entry at (0, 0): tile 1, palette 2, h-flip
        log.vram[0x0400] = 1 | (2 << 10) | 0x4000;
        log.cgram_start[(2 << 4) | 1] = 0x001F;
        log.cgram_start[(2 << 4) | 2] = 0x7C00;
        let frame = Renderer::new().render(&log).unwrap();
        // Flipped: pixel 0 shows the old pixel 7, which is colour 1
        assert_eq!(rgb(frame.pixels[0]), [255, 0, 0]);
        assert_eq!(rgb(frame.pixels[7]), [0, 0, 255]);
        // The next row of the tile is empty: transparent over the backdrop (black)
        assert_eq!(rgb(frame.pixels[WIDTH]), [0, 0, 0]);
    }

    #[test]
    fn sprites_are_limited_to_thirty_two_a_line_and_the_first_is_on_top() {
        let mut regs = mode_1();
        regs.main_bg = [false; 4];
        regs.main_obj = true;
        let sprites = SpriteLine { eval_line: 0, rebuilt: true, ..SpriteLine::default() };
        let mut log = log(regs, sprites);
        // The second line draws from a line buffer rebuilt with no sprite
        log.entries[1].sprites = SpriteLine { blank: true, rebuilt: true, ..sprites };
        // 40 8x8 sprites at y = 0 (a row at scanline 0), x = 2 * n, palette n % 8 + 1... colour 1
        for n in 0..40_usize {
            log.oam_low[n * 4] = (n * 6) as u8;
            log.oam_low[n * 4 + 1] = 0;
            log.oam_low[n * 4 + 2] = 0;
            log.oam_low[n * 4 + 3] = 0;
        }
        for n in 40..128_usize {
            log.oam_low[n * 4 + 1] = 0xF0;
        }
        // Tile 0 of the object character base (word 0): row 0 is all colour 1
        log.vram[0] = 0x00FF;
        log.cgram_start[0x80 + 1] = 0x7FFF;
        let frame = Renderer::new().render(&log).unwrap();
        // Sprites 0..32 are drawn (x = 0 .. 8 and so on, 6 apart: overlapping), 32..40 are not
        assert_eq!(rgb(frame.pixels[0]), [255; 3]);
        assert_eq!(rgb(frame.pixels[31 * 6 + 7]), [255; 3]);
        assert_eq!(rgb(frame.pixels[31 * 6 + 8]), [0; 3]);
        assert_eq!(rgb(frame.pixels[WIDTH]), [0; 3]);
    }

    #[test]
    fn an_oam_write_during_the_frame_shows_in_the_sprites_of_the_lines_after_it() {
        let mut regs = mode_1();
        regs.main_bg = [false; 4];
        regs.main_obj = true;
        let sprites = SpriteLine { eval_line: 0, rebuilt: true, ..SpriteLine::default() };
        let mut log = log(regs, sprites);
        for n in 0..128_usize {
            log.oam_low[n * 4 + 1] = 0xF0;
        }
        log.vram[0] = 0x00FF;
        log.cgram_start[0x80 + 1] = 0x7FFF;
        let frame = Renderer::new().render(&log).unwrap();
        assert_eq!(rgb(frame.pixels[0]), [0; 3]);

        // Sprite 0 moves to the top left (word 0 holds X, then Y) before the first line
        log.oam_writes.push(ssd_oracle::OamWrite { entry: 0, high: false, index: 0, value: 0 });
        let frame = Renderer::new().render(&log).unwrap();
        assert_eq!(rgb(frame.pixels[0]), [255; 3]);
        assert_eq!(log.oam_low[1], 0xF0, "the log itself is untouched");
    }

    #[test]
    fn mode_7_with_an_identity_matrix_reads_the_plane() {
        let mut regs = mode_1();
        regs.bg_mode = 7;
        regs.m7_a = 0x0100;
        regs.m7_d = 0x0100;
        let mut log = log(regs, SpriteLine::default());
        // Plane tile (0, 0) is character 3; its pixel at row 1 (the first frame row is
        // scanline 1), column 1 is colour 9
        log.vram[0] = 3;
        log.vram[64 * 3 + 8 + 1] = 9 << 8;
        log.cgram_start[9] = 0x7FFF;
        let frame = Renderer::new().render(&log).unwrap();
        assert_eq!(rgb(frame.pixels[1]), [255; 3]);
        assert_eq!(rgb(frame.pixels[0]), [0; 3]);
    }

    #[test]
    fn a_vram_write_during_the_frame_applies_from_the_entry_after_it() {
        let mut regs = mode_1();
        regs.bg_mode = 7;
        regs.m7_a = 0x0100;
        regs.m7_d = 0x0100;
        let mut log = log(regs, SpriteLine::default());
        // The frame row r reads the plane row r + 1: colour 1 in column 0 of rows 1 to 7
        for row in 1..8 {
            log.vram[8 * row] = 1 << 8;
        }
        log.cgram_start[1] = 0x001F;
        log.cgram_start[2] = 0x03E0;
        // Plane row 3 becomes colour 2 after the second entry was rendered; plane row 2 after
        // the fourth, too late for its frame row (1)
        log.vram_writes.push(VramWrite { entry: 2, address: 8 * 3, value: 2 << 8 });
        log.vram_writes.push(VramWrite { entry: 4, address: 8 * 2, value: 2 << 8 });
        let frame = Renderer::new().render(&log).unwrap();
        let column = |row: usize| rgb(frame.pixels[row * WIDTH]);
        assert_eq!(column(0), [255, 0, 0]);
        assert_eq!(column(1), [255, 0, 0]);
        assert_eq!(column(2), [0, 255, 0]);
        // The write of plane row 2 shows from then on, and the frame log itself is untouched
        assert_eq!(log.vram[8 * 3], 1 << 8);
    }

    #[test]
    fn the_window_and_color_math() {
        let mut regs = mode_1();
        regs.main_bg = [false; 4];
        regs.color_math = 3;
        regs.backdrop_color_math = true;
        regs.fixed_color = 0x0001;
        // Window 1 over x = 10..=20 clips the color math: outside it only
        regs.window_left = [10, 0];
        regs.window_right = [20, 0];
        regs.math_window_area = [1, 0];
        regs.color_math = 1;
        let mut log = log(regs, SpriteLine::default());
        log.cgram_start[0] = 0x0002;
        let frame = Renderer::new().render(&log).unwrap();
        // Outside the window: 2 + 1 = 3 in red; inside: 2
        assert_eq!(frame.pixels[0].r, channel_for_test(3));
        assert_eq!(frame.pixels[15].r, channel_for_test(2));
    }

    fn channel_for_test(value: u16) -> u8 {
        ((u32::from(value) * 255 + 15) / 31) as u8
    }

    #[test]
    fn a_bg_pixel_names_its_character_and_the_pack_tile_bound_to_it() {
        let mut log = log(mode_1(), SpriteLine::default());
        // BG1 shows tile 1 of the 4-bit character base 0x1000 at the top left: row 1 has colour 2
        // at column 0 (plane 1), which is CGRAM entry 2
        let address = 0x1000 + 16;
        log.vram[0x0400] = 1;
        log.vram[address + 1] = 0x8000;
        log.cgram_start[2] = 0x001F;
        let key = crate::pack::word_key(&log.vram[address..address + 16]);
        let asset = crate::pack::Asset {
            id: "test/tile".to_owned(),
            bpp: 4,
            mode: crate::pack::Mode::Indexed,
            columns: 1,
            keys: vec![key],
            art: "base/test.png".to_owned(),
        };
        let index = Index::new(vec![asset]);

        let mut binder = Binder::new(&index);
        let frame = Renderer::new().render_bound(&log, &mut binder).unwrap();
        assert_eq!(frame.sources.len(), WIDTH * frame.height);
        assert_eq!(
            frame.sources[0],
            Source::Char {
                object: false,
                bpp: 4,
                address: address as u16,
                tile: Some(TileRef { asset: 0, tile: 0 }),
            }
        );
        assert_eq!(frame.sources[1], Source::Backdrop, "a transparent pixel is the backdrop");
        assert_eq!(rgb(frame.pixels[0]), [255, 0, 0]);

        // Without the tile in the pack, the same pixel is still a character, unbound
        let empty = Index::new(Vec::new());
        let frame = Renderer::new().render_bound(&log, &mut Binder::new(&empty)).unwrap();
        assert!(matches!(frame.sources[0], Source::Char { tile: None, .. }));
        assert!(Renderer::new().render(&log).unwrap().sources.is_empty(), "no sources asked");
    }

    #[test]
    fn a_frame_the_renderer_does_not_model_is_refused() {
        let mut log = log(mode_1(), SpriteLine::default());
        log.entries[7].regs.bg_mode = 5;
        assert_eq!(Renderer::new().render(&log), Err(Unsupported::HiRes));
        log.entries[7].regs.bg_mode = 3;
        assert_eq!(Renderer::new().render(&log), Err(Unsupported::BgMode(3)));
        log.complete = false;
        assert_eq!(Renderer::new().render(&log), Err(Unsupported::Incomplete));
    }
}
