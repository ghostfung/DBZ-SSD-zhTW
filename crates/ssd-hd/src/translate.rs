//! The translated text of the Enhanced picture: while the message engine writes a string the translation has
//! (`ssd_catalogue::translation`, keyed by the engine's coordinates that `ssd_catalogue::engine` reads from work
//! RAM), the text of the original's window is painted over with the window's own colour and the translation is
//! drawn there with a Traditional Chinese font (the player's Noto Sans HK, loaded at run time), in the colour of the
//! game's letters, after the HD text pass.
//!
//! The window is the one the engine writes into: its text starts at the tile-map cell of `message_text_base`, is
//! as wide as the table's window (measured for the battle, dialogue and story windows; the page's longest line for
//! the others) and has as many lines as the original's page or the translation needs. The letters the frame shows
//! there (the [`TextCell`]s the font noted) give the window's colours and its place on the screen to the pixel. A
//! glyph is 12 pixels of the original square, the lines 16 pixels apart (two tile rows, as the engine's), the text
//! wrapped to the window's width with the basic rules of line breaking (no line starts with a closing mark, none
//! ends with an opening one). It appears as the engine writes: the share of the page's glyphs drawn is the share of
//! the original's cells written. The name and number slots take the names of the translation (the original's
//! when it has none) and the numbers the game wrote.
//!
//! The text stays a few frames after the engine's bytes stop naming it (between two pages, a frame where the
//! window flickers), as long as the window still has letters. A string the translation lacks keeps the original's.

use crate::cards::CARD_BASE;
use crate::font::TextCell;
use ab_glyph::{Font as _, FontVec, PxScale, point};
use rustc_hash::{FxHashMap, FxHashSet};
use ssd_catalogue::engine::{
    ENDING_TABLE, INTRO_LINES, INTRO_TABLE, OnScreen, intro_key, intro_line_top,
};
use ssd_catalogue::translation::{Slot, Translation, render};
use ssd_oracle::{Color, FrameLog};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The side of a glyph's em box, in pixels of the original.
const EM: f32 = 12.0;
/// From one line to the next, in pixels of the original: two tile rows, as the engine's lines.
const PITCH: i32 = 16;
/// How far the em box starts above the line's first tile row (the 12 pixels centre on the 8 of the cell and the
/// descenders under it).
const RISE: i32 = 1;
/// Where the baseline is in the em box, from its top: the ideographic em box of Noto Sans CJK.
const ASCENT: f32 = 0.88;
/// The glyph of a blank cell (the font's index 0).
const BLANK: u8 = 0;
/// Frames the text stays once the engine's bytes no longer name it, while the window still has letters. A page
/// written at once (the favourite card's message, the Item screen's help) stays as long as its window has letters:
/// the engine names it for two frames only (`message_state` `$A1`, `$A3`, then 0), and the window stays up.
const KEEP: u32 = 8;
/// The marks that never start a line, and those that never end one.
const NO_START: &str = "，。、！？：；」』）〕】》〉…—～·．,.!?:;)]}%";
const NO_END: &str = "「『（〔【《〈([{";

/// A Traditional Chinese font, rasterised at the picture's scale on first use of each glyph.
pub struct ChineseFace {
    face: FontVec,
    /// The glyphs drawn so far, by character, scale and em (in tenths of a pixel of the original).
    glyphs: Mutex<FxHashMap<(char, usize, u16), Arc<Glyph>>>,
}

impl std::fmt::Debug for ChineseFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChineseFace").finish_non_exhaustive()
    }
}

/// One glyph at a scale: its coverage (`w` x `h`, from `left`, `top` of the em box's top left, in pixels of the
/// picture) and its advance in pixels of the original.
struct Glyph {
    w: usize,
    h: usize,
    left: i32,
    top: i32,
    alpha: Vec<u8>,
    advance: f32,
}

impl ChineseFace {
    /// The font of the TrueType or OpenType file `bytes`.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn new(bytes: Vec<u8>) -> Result<Self, crate::FontError> {
        let face = FontVec::try_from_vec(bytes).map_err(|_| crate::FontError::Face)?;
        Ok(Self { face, glyphs: Mutex::new(FxHashMap::default()) })
    }

    /// The advance of `ch` in pixels of the original, at the em of the text.
    fn advance(&self, ch: char) -> f32 {
        self.advance_at(ch, EM)
    }

    /// The advance of `ch` in pixels of the original, at an em of `em` pixels of the original.
    fn advance_at(&self, ch: char, em: f32) -> f32 {
        let units = self.face.units_per_em().unwrap_or(1000.0);
        self.face.h_advance_unscaled(self.face.glyph_id(ch)) * em / units
    }

    /// The glyph of `ch` at `scale`, at the em of the text, drawn on first use.
    fn glyph(&self, ch: char, scale: usize) -> Arc<Glyph> {
        self.glyph_at(ch, scale, EM)
    }

    /// The glyph of `ch` at `scale` and an em of `em` pixels of the original, drawn on first use.
    fn glyph_at(&self, ch: char, scale: usize, em: f32) -> Arc<Glyph> {
        let tenths = (em * 10.0).round() as u16;
        let em = f32::from(tenths) / 10.0;
        let mut glyphs = self.glyphs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = glyphs.entry((ch, scale, tenths));
        Arc::clone(entry.or_insert_with(|| Arc::new(self.rasterise(ch, scale, em))))
    }

    fn rasterise(&self, ch: char, scale: usize, em: f32) -> Glyph {
        let advance = self.advance_at(ch, em);
        let empty = Glyph { w: 0, h: 0, left: 0, top: 0, alpha: Vec::new(), advance };
        let Some(units) = self.face.units_per_em() else { return empty };
        let em = em * scale as f32;
        // `PxScale` is the font's height (ascent to descent) in pixels, not its em
        let height = em * self.face.height_unscaled() / units;
        let glyph = self
            .face
            .glyph_id(ch)
            .with_scale_and_position(PxScale::from(height), point(0.0, ASCENT * em));
        let Some(outlined) = self.face.outline_glyph(glyph) else { return empty };
        let bounds = outlined.px_bounds();
        let (w, h) = (bounds.width().ceil() as usize, bounds.height().ceil() as usize);
        if w == 0 || h == 0 {
            return empty;
        }
        let mut alpha = vec![0_u8; w * h];
        outlined.draw(|x, y, coverage| {
            if let Some(cell) = alpha.get_mut(y as usize * w + x as usize) {
                *cell = (coverage.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        });
        // The strokes widened across by a quarter of an original pixel: the weight of the game's letters
        let reach = scale / 4;
        let wide = w + reach;
        let bold: Vec<u8> = (0..h * wide)
            .map(|at| {
                let (y, x) = (at / wide, at % wide);
                (x.saturating_sub(reach)..=x.min(w.saturating_sub(1)))
                    .map(|from| alpha[y * w + from])
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let (left, top) = (bounds.min.x as i32, bounds.min.y as i32);
        Glyph { w: wide, h, left, top, alpha: bold, advance }
    }
}

/// What the overlay draws from: the translation and the font.
#[derive(Debug)]
pub struct Overlay {
    table: Arc<Translation>,
    face: Arc<ChineseFace>,
    /// The names the game writes straight into its tile maps, as the screen spells them, and their translations
    /// (`ssd_catalogue::engine::screen_names`).
    names: HashMap<String, String>,
}

impl Overlay {
    #[must_use]
    pub fn new(table: Arc<Translation>, face: Arc<ChineseFace>) -> Self {
        Self { table, face, names: HashMap::new() }
    }

    /// The translation of the name `english` spells on screen: the name, or the one name it is the start of
    /// (a list cuts a long name at its column: `Piccol`) when it has four letters or more. A start the name goes on
    /// from with a lower-case word, as a sentence does, is a word of its own: the card prompt's `Attack` is not the
    /// `Attack without` of the number of fighters (but `Zeni` is the money window's `Zeni     P`, `Galick` the
    /// `Galick Ho` cut at a column).
    fn name(&self, english: &str) -> Option<&str> {
        if let Some(name) = self.names.get(english) {
            return Some(name);
        }
        // A name with blanks inside that the game writes letters into (the Stats page's `EX    UP.`, its `LV` in them)
        let mut filled = self.names.iter().filter(|(name, _)| fills(name, english));
        if let Some((_, first)) = filled.next() {
            return filled.all(|(_, other)| other == first).then_some(first.as_str());
        }
        if english.chars().count() < 4 {
            return None;
        }
        let cut = |name: &str| {
            let rest = &name[english.len()..];
            !(rest.starts_with(' ') && rest.trim_start().starts_with(|ch: char| ch.is_lowercase()))
        };
        let mut starting =
            self.names.iter().filter(|(name, _)| name.starts_with(english) && cut(name));
        let (_, first) = starting.next()?;
        starting.all(|(_, other)| other == first).then_some(first.as_str())
    }

    /// The overlay drawing the translations of `names` over the runs of letters that spell them.
    #[must_use]
    pub fn with_names(mut self, names: HashMap<String, String>) -> Self {
        self.names = names;
        self
    }

    /// Whether the translation turns the card tooltip off ([`CARD_TOOLTIP_KEY`] is `off`).
    pub(crate) fn card_tooltip_off(&self) -> bool {
        self.table.text(CARD_TOOLTIP_KEY).is_some_and(|text| text.trim() == "off")
    }

    /// The text of the page `screen` shows, its slots filled in; `None` when the translation lacks the page.
    fn text(&self, screen: &OnScreen) -> Option<String> {
        let text = self.table.text(&screen.key)?;
        let plain = |text: &str| render(text, |_| String::new()).replace('\n', "");
        Some(render(text, |slot| match slot {
            Slot::Name(n) => screen
                .names
                .get(usize::from(n).wrapping_sub(1))
                .and_then(Option::as_ref)
                .map(|name| self.table.text(&name.key).map_or_else(|| name.english.clone(), plain))
                .unwrap_or_default(),
            Slot::Number(n) => {
                screen.numbers.get(usize::from(n).wrapping_sub(1)).cloned().unwrap_or_default()
            }
            Slot::Phrase(n) => {
                self.table.text(&format!("phrase:{n}")).map(plain).unwrap_or_default()
            }
        }))
    }
}

/// The page being drawn: its text and where and how far the engine is.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    key: String,
    text: String,
    table: u8,
    /// `message_table` and `message_string`.
    string: (u8, u16),
    /// The tile-map cell of the text's first cell.
    cell: (i32, i32),
    portrait: bool,
    /// The original page's lines, and its longest line in cells.
    lines: usize,
    longest: usize,
    /// The cells of each of its lines (a line may run past the window: the battle's `power level of 710BP!`).
    widths: Vec<usize>,
    progress: f32,
    /// The engine has written some of the page: its letters show where the window's text is.
    settled: bool,
    /// The page was written at once (`message_state` bit 5 or 6): it stays while its window has letters.
    instant: bool,
    /// The engine has written all of the page and waits (for A, or a timed wait: `message_state` 2, 3 or 6): it
    /// stays while its window has letters, as a page written at once (the shop's `"What do you want?"`, whose
    /// window stays up over the list once the engine names nothing).
    finished: bool,
    /// The engine waits for A after the page (`message_state` 2 or 3) as it last named it: the original shows the
    /// `▼` after its last cell ([`continue_mark`]), also on the frames the page is kept while the engine names the
    /// next one and the screen still shows this one.
    waits: bool,
    /// The engine writes the page's name slot first (`message_state` 5 before the page's first cell, the battle's
    /// `{name} attacks`): its letters show, but not yet where the page's text starts. Until the engine writes a
    /// cell of the page they are hidden in their ground colour, and nothing is drawn.
    naming: bool,
    /// The page is a finished one the engine names again from its start: it shows while its window has letters.
    again: bool,
    /// The page's translation is centred in its window ([`centres`], [`centred`]).
    centre: bool,
    /// The page as the engine named it, for a page written at once whose English can be compared with the screen:
    /// whether it is drawn, and where, is its [`presence`] on each frame ([`draw`]), not its window's letters.
    measured: Option<OnScreen>,
    /// The page as the engine named it, for a page the engine writes cell by cell (`message_state` 1 or 5) whose
    /// English can be compared with the screen: whether it is drawn, where, and how much of its translation, is its
    /// [`typed_presence`] on each frame ([`draw`]), not its window's letters nor the engine's count.
    typed: Option<OnScreen>,
    /// The page's English shows no letter to compare: every cell of it is a tile the HD font has no glyph for (`…`)
    /// or a space, or it has none (a portrait and nothing else); not a page with no `english` at all (the intro's and
    /// the ending's). It is placed by its window's blank ground and typed as the Japanese ROM types it ([`draw`],
    /// [`blank_ground`]).
    blank: bool,
}

/// What the overlay keeps from one frame to the next.
#[derive(Debug, Default, Clone)]
pub(crate) struct Kept {
    shown: Option<Shown>,
    /// Frames since the engine last named the page shown.
    age: u32,
    /// The window of the page shown has shown letters: only then does an empty window mean it was cleared (a page
    /// written at once is named before its window's letters are drawn: the Item screen's help as it opens).
    seen: bool,
    /// Where the window's text is on the screen and its colours, as last seen in its letters.
    place: Option<Place>,
    colours: Option<([u8; 3], [u8; 3])>,
    /// The letters (letter and digit glyphs, sorted) of the window of the page shown when the engine last named it.
    /// See [`draw`].
    glyphs: Vec<u8>,
    /// A page kept behind the page the engine names now ([`draw_pages`]): it stays only while its window holds all the
    /// letters it held once it settled behind ([`KEEP`] frames after the engine last named it), also when it was
    /// written at once.
    strict: bool,
    /// The letters (as [`Kept::glyphs`]) of the window of a page written at once, once it settled ([`KEEP`] frames
    /// after the engine named it): fewer later is another window over it. See [`draw`].
    held: Vec<u8>,
    /// The letters (as [`Kept::glyphs`]) of the window of a finished page the engine still names, once it settled
    /// ([`KEEP`] frames named finished, `done_frames`): fewer while the engine still names it is another window drawn
    /// in its place (the battle's status box with its HP, KI and BP after a KI attack's message), and the page is
    /// `gone` until the engine writes a page again. See [`draw`].
    done: Vec<u8>,
    done_frames: u32,
    gone: bool,
    /// The letters (cell and glyph) the pages of a window wrote where they cover (a letter that came or changed there
    /// while the window was up: another window's letters already there, the money box under the Item help, stay out),
    /// and the window (its table and the tile-map cell of its text): a page of the same window that covers fewer cells
    /// leaves the others to the original. See [`draw`].
    covered: Vec<(i32, i32, u8)>,
    covered_in: Option<(u8, (i32, i32))>,
    /// The letters of the screen on the last frame a page was drawn.
    before: Vec<(i32, i32, u8)>,
    /// Of those, the ones a later page left to the original, and the ground they take: they keep it while no page is
    /// drawn (the frames between two pages of the window).
    left: Vec<(i32, i32, u8, [u8; 3])>,
    /// The cells a page's English lines wrote outside its window ([`spill_of`]), with a print of the original's pixels
    /// there ([`cell_print`]): while the original shows a cell as it was, the cell takes the picture of the cells
    /// beside it ([`paint_spilled`]), on the pages after (`k?` of Gohan's `"Vegeta! You're on Namek?"` and its `▼`,
    /// left of the window) and on the frames between them.
    spilled: Vec<(i32, i32, u64)>,
    /// Of those, the cells another window drew over (the original no longer shows them as they were): that window's
    /// from then on, while a letter of the text font is there ([`paint_spilled`]).
    taken: Vec<(i32, i32)>,
    /// The paper painted on the frame before ([`draw`]); while the engine has written no cell of a window's next page,
    /// the paper of the page before it, which the next page takes too.
    painted: Option<Painted>,
    /// The letters (cell and glyph) of another window in the place of a page that went: from the frame its window was
    /// cleared, another window was drawn over it, or the page was `gone`, those of the screen on each frame the engine
    /// names nothing or still names the page gone. The engine writes a page's cells only while it names the page, so
    /// these are no page's. A page the engine names while its window holds none but these is written where the screen
    /// does not show it (the next target of a KI attack on a group, under the battle's status box): nothing is drawn.
    /// See [`draw`].
    under: Option<Vec<(i32, i32, u8)>>,
    /// A page written at once kept behind that went when another window was drawn over its place ([`WentUnder`]).
    /// One [`Kept`] holds it at a time: [`draw_pages`] hands it to the page shown when the page behind ends, and to the
    /// page behind when the page of its key goes behind again.
    went_under: Option<WentUnder>,
    /// A page written at once the engine named whose English the screen does not show yet ([`shown_now`]), and the
    /// frames since the engine last named it. Until then the page shown before stays.
    pending: Option<(OnScreen, u32)>,
    /// Of a page whose English shows no letter ([`Shown::blank`]): whether it was drawn (its window's ground gone
    /// after that, it ends).
    blank_drawn: bool,
    /// Of a page whose English shows no letter: the rows its window's blank ground runs to above and under its first
    /// line, on the last frame the engine named it ([`ground_rows`]).
    blank_rows: Option<(i32, i32)>,
}

/// A page written at once kept behind ([`Kept::strict`]) whose window another window was drawn over, and its letters
/// (letters, digits and marks, cell and glyph) on its own lines on the last frame its window held them all: the
/// enemies' HP window of Porunga's third wish over most of the battle's Item window, `Le` and `3` of `Lets you make 3
/// wishes!` left of it. The page goes, as a page behind does ([`draw_pages`]); while some of those letters still show as
/// they were and some do not, they take their ground ([`paint_went_under`]), and a page of its key the engine names is
/// not drawn ([`draw`]). Forgotten once they all show again (that window closed) or none does (another window took the
/// place).
#[derive(Debug, Clone)]
struct WentUnder {
    key: String,
    letters: Vec<(i32, i32, u8)>,
}

/// Paints the letters of the page that went under another window ([`WentUnder`]) still on the screen as they were,
/// with the ground of their cells ([`paint_beside_window`]), while some of them are not: forgets the page once they
/// all are, or none is.
fn paint_went_under(
    went_under: &mut Option<WentUnder>,
    target: &mut Target<'_>,
    cells: &[TextCell],
    frame: &[Color],
    frame_width: usize,
    unlettered: Option<&[u8]>,
) {
    let Some(under) = went_under.as_ref() else { return };
    let showing: Vec<&TextCell> =
        cells.iter().filter(|cell| under.letters.contains(&(cell.x, cell.y, cell.glyph))).collect();
    if showing.is_empty() || showing.len() >= under.letters.len() {
        *went_under = None;
        return;
    }
    for cell in showing {
        paint_beside_window(target, frame, frame_width, cell, unlettered);
    }
}

/// Paints a letter of a page under another window, still showing beside that window, with the ground of its cell,
/// and the pixel round its cell where the 1x frame `frame` (`frame_width` pixels a row) is that ground: the HD text
/// draws a letter's ink past its cell only over its ground (`font`), so the other pixels there are not its. The
/// lowest row of the party list's lower frame of Porunga's second wish, right above the wishes' third line, keeps its
/// picture. With the picture as it was before the HD text drew the letters (`unlettered`), such a pixel round the cell
/// takes it only where it is the letter's ink blended over it ([`Target::unblend_pixel`]): the HD text draws the
/// cell beside again too, and may blend its ink into a 1x pixel of the ground (the edge of the frame of the enemy
/// window of Grandpa's and Yajirobe's items, right of the Item help's `a` under it), which keeps its picture.
fn paint_beside_window(
    target: &mut Target<'_>,
    frame: &[Color],
    frame_width: usize,
    cell: &TextCell,
    unlettered: Option<&[u8]>,
) {
    target.fill((cell.x, cell.y), (cell.x + 8, cell.y + 8), cell.bg);
    let before = unlettered.filter(|before| before.len() == target.rgba.len());
    let height = (frame.len() / frame_width.max(1)) as i32;
    for y in cell.y - 1..cell.y + 9 {
        for x in cell.x - 1..cell.x + 9 {
            let inside = (cell.x..cell.x + 8).contains(&x) && (cell.y..cell.y + 8).contains(&y);
            if inside || !(0..frame_width as i32).contains(&x) || !(0..height).contains(&y) {
                continue;
            }
            let colour = frame[y as usize * frame_width + x as usize];
            if [colour.r, colour.g, colour.b] == cell.bg {
                match before {
                    Some(before) => target.unblend_pixel(before, (x, y), cell.fg),
                    None => target.fill((x, y), (x + 1, y + 1), cell.bg),
                }
            }
        }
    }
}

/// Whether the cell at `x`, `y` is on one of the lines of `shown` as wide as the original writes them, the text's
/// first line at `x0`, `y0` and the lines `pitch` apart (a portrait's first line hangs a cell to the left).
fn on_its_lines(shown: &Shown, (x0, y0): (i32, i32), pitch: i32, (x, y): (i32, i32)) -> bool {
    let hang = if shown.portrait { 8 } else { 0 };
    shown.widths.iter().enumerate().any(|(row, &width)| {
        let line = y0 + pitch * row as i32;
        (line - 4..=line + 4).contains(&y) && (x0 - hang..x0 + 8 * width as i32 + 8).contains(&x)
    })
}

/// The letters (cell and glyph) of the screen's text, as [`Kept::under`] keeps them.
fn screen_letters(cells: &[TextCell]) -> Vec<(i32, i32, u8)> {
    cells
        .iter()
        .filter(|cell| cell.glyph != BLANK && cell.glyph < CARD_BASE)
        .map(|cell| (cell.x, cell.y, cell.glyph))
        .collect()
}

/// Paints the letters `left` that are still on the screen as they were with their ground, and forgets the others.
fn paint_left(
    left: &mut Vec<(i32, i32, u8, [u8; 3])>,
    target: &mut Target<'_>,
    cells: &[TextCell],
) {
    left.retain(|&(x, y, glyph, _)| {
        cells.iter().any(|cell| (cell.x, cell.y, cell.glyph) == (x, y, glyph))
    });
    for &(x, y, _, ground) in left.iter() {
        target.fill((x, y - 1), (x + 8, y + 9), ground);
    }
}

/// A print of the 1x pixels of the cell (8 pixels square) at `x`, `y` of `frame`; `None` for a cell not wholly on it.
fn cell_print(frame: &[Color], frame_width: usize, (x, y): (i32, i32)) -> Option<u64> {
    let height = frame.len() / frame_width.max(1);
    let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
    if x + 8 > frame_width || y + 8 > height {
        return None;
    }
    let mut print = 0xCBF2_9CE4_8422_2325_u64;
    for row in y..y + 8 {
        for colour in &frame[row * frame_width + x..row * frame_width + x + 8] {
            for byte in [colour.r, colour.g, colour.b] {
                print = (print ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3);
            }
        }
    }
    Some(print)
}

/// The cell whose picture a spilled cell at `at` takes: the one above or under it (or two above or under), never one
/// of `spilled`. A piece of a window right beside it (a text tile: the frame's side, the paper) whose column goes on
/// the same past it on the original comes first (not the corner at the end of a side); else the first one on the
/// picture, the one above (the screen's picture goes on from there).
fn spill_source(
    frame: &[Color],
    frame_width: usize,
    cells: &[TextCell],
    spilled: &[(i32, i32)],
    (x, y): (i32, i32),
) -> Option<(i32, i32)> {
    let sources: Vec<(i32, i32)> = [-8, 8, -16, 16]
        .into_iter()
        .map(|dy| (x, y + dy))
        .filter(|&at| !spilled.contains(&at) && cell_print(frame, frame_width, at).is_some())
        .collect();
    sources
        .iter()
        .copied()
        .find(|&(_, from)| {
            let beyond = (x, from + (from - y).signum() * 8);
            (from - y).abs() == 8
                && cells.iter().any(|cell| (cell.x, cell.y) == (x, from))
                && !spilled.contains(&beyond)
                && cell_print(frame, frame_width, beyond)
                    == cell_print(frame, frame_width, (x, from))
        })
        .or_else(|| sources.first().copied())
}

/// The picture of the frame drawn without the BG layers of its text, at the target's scale, and where the art drew
/// one of its pictures (a portrait, a face) in it: what the cells an English line spilled over a portrait hide.
#[derive(Debug, Clone)]
pub(crate) struct Beneath {
    pub rgba: Vec<u8>,
    pub width: usize,
    /// Per pixel of the 1x frame (`frame_width` pixels a row), whether the art drew a picture there.
    pub pictures: Vec<bool>,
    pub frame_width: usize,
}

impl Beneath {
    /// Whether the art drew a picture in the 1x cell at `(x, y)`.
    fn pictured(&self, (x, y): (i32, i32)) -> bool {
        let width = self.frame_width as i32;
        (y..y + 8).any(|row| {
            (x..x + 8).any(|column| {
                (0..width).contains(&column)
                    && row >= 0
                    && self.pictures.get((row * width + column) as usize).copied().unwrap_or(false)
            })
        })
    }
}

/// How far a pixel may be outside the blends of a picture's colour and a letter's ink and still be one
/// ([`Target::unink_pixel`]): the rounding of the anti-aliasing. With `d` the summed channel differences,
/// `d(picture, pixel) + d(pixel, ink)` equals `d(picture, ink)` exactly when each channel of the pixel lies between the
/// picture's and the ink's (a box of colours with those two at opposite corners, not the line between them), and
/// exceeds it by twice the channels' overshoot past that box; this is the most that sum may exceed it by.
const BLEND_SLACK: u32 = 24;

/// What the overlay asks of the renderer for the cells an English line spilled out of its window: the picture
/// beneath them ([`Beneath`]), drawn when it is first asked for (`None`: none, the cells take the picture beside
/// them).
pub(crate) type BeneathOf<'a> = dyn FnMut(&[(i32, i32)]) -> Option<Arc<Beneath>> + 'a;

/// Paints the cells `spilled` that the original still shows as they were with the picture beside them
/// ([`spill_source`]), and forgets the others. A cell over one of the art's pictures (the bottom row of a portrait
/// beside the window, Vegeta's `summoned the dragon!`) takes that picture as it is drawn without the text
/// ([`Beneath`]): the cell beside it is another part of the portrait. The cells it forgets are `taken`: another
/// window's, while a letter of the text font is there.
#[allow(clippy::too_many_arguments)]
fn paint_spilled(
    spilled: &mut Vec<(i32, i32, u64)>,
    taken: &mut Vec<(i32, i32)>,
    target: &mut Target<'_>,
    cells: &[TextCell],
    frame: &[Color],
    frame_width: usize,
    beneath: &mut BeneathOf<'_>,
    unlettered: Option<&[u8]>,
) {
    spilled.retain(|&(x, y, print)| {
        let kept = cell_print(frame, frame_width, (x, y)) == Some(print);
        if !kept && !taken.contains(&(x, y)) {
            taken.push((x, y));
        }
        kept
    });
    taken.retain(|&(x, y)| {
        cells
            .iter()
            .any(|cell| (cell.x, cell.y) == (x, y) && cell.glyph != BLANK && cell.glyph < CARD_BASE)
    });
    spilled.sort_by_key(|&(x, y, _)| (y, x));
    let places: Vec<(i32, i32)> = spilled.iter().map(|&(x, y, _)| (x, y)).collect();
    // The ink the HD text drew around a spilled letter, past its cell (font::MARGIN: a descender, an overhang), goes
    // with it: a pixel there of the letter's ink over the picture as it was before the HD text drew the letters
    // (`unlettered`), or of a blend of the two (its anti-aliased edge), takes that picture; the others are the HD
    // text's own picture of the cells beside (a frame's edge it drew again, a colour that is no such blend) and stay. A pixel of a spilled cell is that cell's (battle:243's `g` of `dragon!` left its
    // descender as a `_` beside the frame)
    if let Some(before) = unlettered.filter(|before| before.len() == target.rgba.len()) {
        let margin = crate::font::MARGIN as i32;
        let inside = |x: i32, y: i32| {
            places.iter().any(|&(cx, cy)| (cx..cx + 8).contains(&x) && (cy..cy + 8).contains(&y))
        };
        for &(cx, cy) in &places {
            let Some(ink) =
                cells.iter().find(|cell| (cell.x, cell.y) == (cx, cy)).map(|cell| cell.fg)
            else {
                continue;
            };
            for y in cy - margin..cy + 8 + margin {
                for x in cx - margin..cx + 8 + margin {
                    if !inside(x, y) {
                        target.unink_pixel(before, (x, y), ink);
                    }
                }
            }
        }
    }
    let under = if places.is_empty() { None } else { beneath(&places) };
    for &at in &places {
        // (a picture beneath of another size than the target is none: the cell takes the cell beside it)
        if let Some(under) = under.as_deref().filter(|under| {
            under.width == target.width
                && under.rgba.len() == target.rgba.len()
                && under.pictured(at)
        }) {
            target.paste_cell(&under.rgba, at);
            continue;
        }
        if let Some(from) = spill_source(frame, frame_width, cells, &places, at) {
            target.copy_cell(from, at);
        }
    }
}

/// Whether drawing the page `screen` names (or none) over `kept` and the page `behind` may paint cells an English line
/// spilled out of its window ([`paint_spilled`]): cells kept from the frames before, or a page shown (one kept, or the
/// one named now: [`draw`] finds new spilled cells only for a page it shows), or a page that went under another
/// window, whose letters beside it take their ground ([`paint_went_under`]) with that picture.
fn may_spill(kept: &Kept, behind: Option<&Kept>, screen: Option<&OnScreen>) -> bool {
    // (a page that went under another window: [`paint_went_under`] on every frame while it is kept)
    let any =
        |kept: &Kept| !kept.spilled.is_empty() || kept.shown.is_some() || kept.went_under.is_some();
    screen.is_some() || any(kept) || behind.is_some_and(any)
}

/// What [`spill_of`] walks: the page's English lines as the engine writes them.
struct SpillWalk<'a> {
    cells: &'a [TextCell],
    /// The cells of each line on the screen, from the first.
    widths: &'a [usize],
    /// The first line's first cell, and the pixels from one line to the next.
    at: (i32, i32),
    pitch: i32,
    /// How far left of the text a portrait's first line starts.
    hang: i32,
    /// The window's right edge, and the screen's pixel of the tile map's first column.
    right: i32,
    map_left: i32,
    /// The rows of the window the overlay paints over (its paper).
    painted: (i32, i32),
    /// The page waits for A: the `▼` after its last line.
    waiting: bool,
    /// The cells of the frame's tile map that hold the `▼` ([`mark_cells`]).
    marks: &'a [MarkCell],
}

/// The cell of a `▼` tile ([`mark_cells`]) in the column `x`, at the row `y` or a few pixels off it (the battle's lines
/// closer than written).
fn mark_at(marks: &[MarkCell], (x, y): (i32, i32)) -> Option<(i32, i32)> {
    marks
        .iter()
        .find(|mark| mark.at.0 == x && (y - 4..=y + 4).contains(&mark.at.1))
        .map(|mark| mark.at)
}

/// The cells English lines write outside their window: those from the window's right edge on, and those past the
/// tile map's 32 columns, which the engine writes at the start of the tile-map row under the line (`01:CF3A` adds the
/// cell to the row's offset, 32 tile words a row). Of each, a letter of the text font, and a cell with no text tile
/// right after one (a mark the font has no glyph for, the `…` of `but…"`); after the last line of a page that waits
/// for A, the `▼` there, also while it blinks off: only while the tile map holds a `▼` tile there ([`mark_at`]). Until
/// the engine writes the `▼` that cell shows what was there (a portrait beside the window, Vegeta's ear), and when A
/// turns the page first it never does: the cell is not the page's. A cell of the line with no letter (a space, never
/// written) ends the run of marks, and a cell the window's paper takes anyway is the window's.
fn spill_of(walk: &SpillWalk<'_>) -> Vec<(i32, i32)> {
    let SpillWalk {
        cells,
        widths,
        at: (x0, y0),
        pitch,
        hang,
        right,
        map_left,
        painted,
        waiting,
        marks,
    } = *walk;
    let tiled = |x: i32, y: i32| cells.iter().find(|cell| (cell.x, cell.y) == (x, y));
    let letter = |cell: &TextCell| cell.glyph != BLANK && cell.glyph < CARD_BASE;
    let mut spill = Vec::new();
    for (row, &width) in widths.iter().enumerate() {
        let line = y0 + pitch * row as i32;
        let first = if row == 0 { -(hang / 8) } else { 0 };
        let last = width as i32 + i32::from(waiting && row + 1 == widths.len());
        let mut after_letter = false;
        for n in first..last {
            let along = x0 + 8 * n;
            let wrapped = along >= map_left + 256;
            let (x, y) = if wrapped { (along - 256, line + 8) } else { (along, line) };
            let outside = (wrapped || along >= right)
                && !((x0 - hang..right).contains(&x) && (painted.0..painted.1).contains(&y));
            // (the cell after the last line: the `▼`'s, the page's only while the tile map holds it)
            if n == width as i32 {
                if let Some(at) = mark_at(marks, (x, y)).filter(|_| after_letter && outside)
                    && !spill.contains(&at)
                {
                    spill.push(at);
                }
                continue;
            }
            // (a letter of the row, maybe a few pixels off the line: the battle's lines closer than written)
            let found = cells
                .iter()
                .find(|cell| letter(cell) && cell.x == x && (y - 4..=y + 4).contains(&cell.y));
            let spilled = match found {
                Some(cell) => {
                    after_letter = true;
                    Some((cell.x, cell.y))
                }
                None if after_letter && tiled(x, y).is_none() => Some((x, y)),
                None => {
                    after_letter = false;
                    None
                }
            };
            if let Some(at) = spilled.filter(|_| outside)
                && !spill.contains(&at)
            {
                spill.push(at);
            }
        }
    }
    spill
}

/// Where a string's text is on the screen (a window may scroll from one string to the next): the string
/// (`message_table`, `message_string`), the screen's pixel of its first line's first cell, and the pixels from one
/// line to the next; and the tile-map cell the engine writes its first cell at (with the table, its window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    string: (u8, u16),
    at: (i32, i32),
    pitch: i32,
    cell: (i32, i32),
}

/// The paper painted over a page ([`Kept::painted`]): the tile-map cell of the page's first cell (its window) and the
/// paper's corners, in 1x pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Painted {
    cell: (i32, i32),
    from: (i32, i32),
    to: (i32, i32),
}

/// The overlay of a renderer: what it draws from, the page the engine named at the start of the frame being drawn,
/// and what it keeps between frames.
#[derive(Debug)]
pub(crate) struct Translator {
    pub overlay: Arc<Overlay>,
    pub screen: Option<OnScreen>,
    kept: Kept,
    /// The page of another window still up under the one the engine names now ([`draw_pages`]).
    behind: Option<Kept>,
}

impl Translator {
    pub(crate) fn new(overlay: Arc<Overlay>) -> Self {
        Self { overlay, screen: None, kept: Kept::default(), behind: None }
    }

    /// The text cells of the frame the HD font draws, when not all of `cells`: in the intro, all but the letters of
    /// each line the translation has ([`intro_line`]), which [`draw_intro`] draws in their place (the picture under
    /// them is the frame without the letters).
    /// Whether the frame [`Translator::draw`] draws next may have cells an English line spilled out of its window
    /// ([`may_spill`]): only then does the renderer keep the picture before the HD text draws the letters.
    pub(crate) fn may_spill(&self) -> bool {
        let screen = self.screen.as_ref().filter(|screen| screen.table != INTRO_TABLE);
        may_spill(&self.kept, self.behind.as_ref(), screen)
    }

    pub(crate) fn font_cells(&self, cells: &[TextCell]) -> Option<Vec<TextCell>> {
        let screen = self.screen.as_ref().filter(|screen| screen.table == INTRO_TABLE)?;
        Some(
            cells
                .iter()
                .filter(|cell| intro_line(&self.overlay, screen, cell).is_none())
                .copied()
                .collect(),
        )
    }

    /// The BG layers the frame is drawn without, in the intro: BG1 (lines 0 to 15) and BG2 (16 to 31) hold its text
    /// only, and a layer whose lines the translation has every one of is drawn by the translation alone. Without
    /// them the art of the pack sees no text there: it draws its pictures under text only where the frame shows none
    /// (the stripes behind the intro, whose art left the English letters' shapes in the original's darker colours
    /// under the translation). The letters are read from the frame drawn with them ([`Translator::font_cells`]).
    pub(crate) fn quiet_layers(&self) -> [bool; 4] {
        if self.screen.as_ref().is_none_or(|screen| screen.table != INTRO_TABLE) {
            return [false; 4];
        }
        let half = INTRO_LINES / 2;
        let all = |lines: std::ops::Range<usize>| {
            lines.into_iter().all(|line| self.overlay.table.text(&intro_key(line)).is_some())
        };
        [all(0..half), all(half..INTRO_LINES), false, false]
    }

    /// In the intro, takes the English letters of each line the translation has out of the 1x frame `pixels`
    /// (`width` wide), before it is scaled: the cells of the line, from column 0 as wide as its English, take the
    /// pixels of `beneath` of its layer (the frame drawn with that layer off: BG1 and BG2 hold the intro's text only),
    /// the marks the HD font has no glyph for with them (the intro's `.`, which [`Translator::font_cells`] cannot
    /// leave out). A layer the frame is drawn without ([`Translator::quiet_layers`]) has no letter there and no
    /// frame `beneath`: this is for the lines of a layer one of whose lines the translation lacks.
    pub(crate) fn clean_intro(
        &self,
        pixels: &mut [Color],
        width: usize,
        cells: &[TextCell],
        beneath: [Option<&[Color]>; 4],
        window_x: i32,
    ) {
        let Some(screen) = self.screen.as_ref().filter(|screen| screen.table == INTRO_TABLE) else {
            return;
        };
        let height = (pixels.len() / width.max(1)) as i32;
        for (line, top) in intro_rows(&self.overlay, screen, cells, height) {
            let Some(under) =
                beneath[line / (INTRO_LINES / 2)].filter(|under| under.len() == pixels.len())
            else {
                continue;
            };
            let right = window_x + 8 * screen.lines.get(line).copied().unwrap_or(0) as i32;
            // The scrolling introduction's bold SNES title glyph spans
            // both 8-pixel tiles of its 16-pixel text row. Clearing only
            // the upper tile leaves English visible behind the translation.
            for y in top.max(0)..(top + INTRO_EM as i32).min(height) {
                for x in window_x.max(0)..right.min(width as i32) {
                    let at = y as usize * width + x as usize;
                    pixels[at] = Color { a: pixels[at].a, ..under[at] };
                }
            }
        }
    }

    /// Draws the frame's translation: the names written into the tile maps ([`draw_names`]), the chase's damage
    /// label of the tile maps of `log` ([`draw_chase_damage`]), then the intro's lines ([`draw_intro`]) or the
    /// message engine's page ([`draw`]). `character` spells a glyph of the cells.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        target: &mut Target<'_>,
        cells: &[TextCell],
        frame: &crate::Frame,
        log: &FrameLog,
        window_x: i32,
        character: &dyn Fn(u8) -> Option<char>,
        beneath: &mut BeneathOf<'_>,
        unlettered: Option<&[u8]>,
    ) {
        let rows = tile_rows(log);
        // (the original's `▼`, as the picture shows it before anything is drawn over it: [`continue_mark`])
        let marks = mark_cells(&rows, window_x, target);
        draw_names(&self.overlay, target, cells, &frame.pixels, frame.width, character);
        draw_rules(target, cells, character);
        if self.overlay.table.text(CHASE_DAMAGE_KEY).is_some() {
            draw_chase_damage(&self.overlay, target, cells, &rows, window_x);
        }
        let intro = self.screen.as_ref().filter(|screen| screen.table == INTRO_TABLE);
        if let Some(screen) = intro {
            draw_intro(&self.overlay, screen, target, cells, window_x);
        }
        let screen = self.screen.as_ref().filter(|screen| screen.table != INTRO_TABLE);
        draw_pages(
            &self.overlay,
            &mut self.kept,
            &mut self.behind,
            screen,
            target,
            cells,
            &frame.pixels,
            frame.width,
            window_x,
            character,
            beneath,
            unlettered,
            &marks,
        );
    }
}

/// The em of the intro's lines, in pixels of the original: a line takes the row of its English letters and the
/// blank row under them, as the Japanese ROM's lines take theirs (a row of kana and the row of their voiced marks).
const INTRO_EM: f32 = 16.0;
/// The column the intro's lines start at, where the Japanese ROM's do (its lines are 20 cells, columns 6 to 25).
const INTRO_COLUMN: i32 = 6;
/// How far from where the scroll the engine read puts a line its letters may be: the frame drawn can show the
/// scroll a row off the one read at the end of the frame before (it moves a row about every four frames). Lines are
/// 16 rows apart.
const INTRO_SLACK: i32 = 2;

/// The line (0 to [`INTRO_LINES`] - 1) of the intro's text that `cell` is a letter of, when the translation has the
/// line: a glyph of the title set on BG1 (lines 0 to 15) or BG2 (16 to 31) whose row is where the scroll of `screen`
/// puts that line ([`intro_line_top`]), give or take [`INTRO_SLACK`].
fn intro_line(overlay: &Overlay, screen: &OnScreen, cell: &TextCell) -> Option<usize> {
    if !cell.bold || cell.glyph == BLANK || cell.glyph >= CARD_BASE || cell.layer > 1 {
        return None;
    }
    let half = INTRO_LINES / 2;
    let lines = usize::from(cell.layer) * half..(usize::from(cell.layer) + 1) * half;
    lines.into_iter().find(|&line| {
        intro_line_top(screen, line).is_some_and(|top| (cell.y - top).abs() <= INTRO_SLACK)
            && overlay.table.text(&intro_key(line)).is_some()
    })
}

/// The intro's lines the translation has whose place is on a frame `height` rows high, and the row each starts on:
/// the row of its English letters in `cells` (the most of them), or where the scroll of `screen` puts it when none of
/// them shows (the line's [`INTRO_EM`] rows reach the frame while its letters are above it).
fn intro_rows(
    overlay: &Overlay,
    screen: &OnScreen,
    cells: &[TextCell],
    height: i32,
) -> Vec<(usize, i32)> {
    let letters: Vec<(usize, i32)> = cells
        .iter()
        .filter_map(|cell| Some((intro_line(overlay, screen, cell)?, cell.y)))
        .collect();
    (0..INTRO_LINES)
        .filter(|&line| overlay.table.text(&intro_key(line)).is_some())
        .filter_map(|line| {
            let seen = most(letters.iter().filter(|(of, _)| *of == line).map(|&(_, y)| y));
            Some((line, seen.or_else(|| intro_line_top(screen, line))?))
        })
        .filter(|&(_, top)| top > -(INTRO_EM as i32) && top < height)
        .collect()
}

/// Draws the intro's lines the translation has over the screen of `screen`
/// ([`ssd_catalogue::engine::intro_on_screen`]): each on the row of its English letters (where the scroll puts the
/// line when none of them shows), [`INTRO_EM`] from [`INTRO_COLUMN`], in the colour of the English letters on the
/// screen (none there: nothing is drawn). The English letters of those lines are not drawn
/// ([`Translator::font_cells`]); a line the translation lacks keeps its English.
fn draw_intro(
    overlay: &Overlay,
    screen: &OnScreen,
    target: &mut Target<'_>,
    cells: &[TextCell],
    window_x: i32,
) {
    let ink = most(
        cells.iter().filter(|cell| intro_line(overlay, screen, cell).is_some()).map(|cell| cell.fg),
    );
    let Some(ink) = ink else { return };
    let scale = target.scale;
    let face = &overlay.face;
    for (line, top) in intro_rows(overlay, screen, cells, (target.height() / scale) as i32) {
        let Some(text) = overlay.table.text(&intro_key(line)) else { continue };
        let mut pen = 0.0;
        for ch in render(text, |_| String::new()).chars().filter(|&ch| ch != '\n') {
            let glyph = face.glyph_at(ch, scale, INTRO_EM);
            if !ch.is_whitespace() {
                let x = ((window_x + 8 * INTRO_COLUMN) as f32 + pen) * scale as f32;
                target.glyph(&glyph, (x.round() as i32, top * scale as i32), ink);
            }
            pen += glyph.advance;
        }
    }
}

/// The tile-map cells a page's window text takes: columns `from..to`, rows `top..bottom` (two rows a line).
fn page_cells(table: u8, cell: (i32, i32), longest: usize, lines: usize) -> (i32, i32, i32, i32) {
    let wide = window_cells(table, longest) as i32;
    (cell.0, cell.0 + wide, cell.1, cell.1 + 2 * lines.max(1) as i32)
}

fn overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.1 && b.0 < a.1 && a.2 < b.3 && b.2 < a.3
}

fn screen_cells(screen: &OnScreen) -> (i32, i32, i32, i32) {
    let (column, row) = screen.base_cell();
    let longest = screen.lines.iter().copied().max().unwrap_or(0);
    page_cells(screen.table, (i32::from(column), i32::from(row)), longest, screen.lines.len())
}

fn shown_cells(shown: &Shown) -> (i32, i32, i32, i32) {
    page_cells(shown.table, shown.cell, shown.longest, shown.lines)
}

/// Draws the page the engine names ([`draw`]) and, under it, the page of another window still up: a page written at
/// once or finished (Porunga's help in a battle's Item window) whose window the engine leaves as it is while it names
/// a page of another window (the wishes, upstream 0.3.4 on, drawn by the game) used to go, its English back. Such a
/// page goes `behind` when the engine names a page of a window apart from its own, and stays while its window holds
/// all of its letters (another window over the place has letters of its own: [`Kept::strict`]); a page the engine
/// names over its place, or the page itself named again, ends it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_pages(
    overlay: &Overlay,
    kept: &mut Kept,
    behind: &mut Option<Kept>,
    screen: Option<&OnScreen>,
    target: &mut Target<'_>,
    cells: &[TextCell],
    frame: &[Color],
    frame_width: usize,
    window_x: i32,
    character: &dyn Fn(u8) -> Option<char>,
    beneath: &mut BeneathOf<'_>,
    unlettered: Option<&[u8]>,
    marks: &[MarkCell],
) {
    let screen = screen.filter(|screen| !past_its_lines(screen));
    let size = (frame_width as i32, (frame.len() / frame_width.max(1)) as i32);
    let named = shown_now(kept, screen, cells, character, window_x, size);
    let screen = named.as_ref();
    if let (Some(screen), Some(shown)) = (screen, kept.shown.as_ref()) {
        let lasting = kept.seen && (shown.instant || shown.finished);
        if shown.key != screen.key
            && lasting
            && !kept.gone
            && !overlap(shown_cells(shown), screen_cells(screen))
        {
            // (the page that went under another window goes behind with the page of its key: [`Kept::went_under`])
            let went_under = kept.went_under.take_if(|under| under.key == shown.key);
            // (a page written at once that settled goes behind with the letters it held then: one waiting under
            // another window drawn over its own, the cards of Porunga's first wish or the party list of his second,
            // took that window's letters for its own and was drawn over it, [`Kept::held`])
            let glyphs =
                if shown.instant && kept.age > KEEP { kept.held.clone() } else { Vec::new() };
            *behind =
                Some(Kept { strict: true, glyphs, went_under, pending: None, ..kept.clone() });
        }
    }
    // (a page behind that ends hands the page that went under another window to the page shown)
    let end_behind = |behind: &mut Option<Kept>, kept: &mut Kept| {
        if let Some(under) = behind.take().and_then(|back| back.went_under) {
            kept.went_under = Some(under);
        }
    };
    if let (Some(screen), Some(back)) =
        (screen, behind.as_ref().and_then(|kept| kept.shown.as_ref()))
        && (back.key == screen.key || overlap(shown_cells(back), screen_cells(screen)))
    {
        end_behind(behind, kept);
    }
    if let Some(back) = behind.as_mut() {
        draw(
            overlay,
            back,
            None,
            target,
            cells,
            frame,
            frame_width,
            window_x,
            character,
            beneath,
            unlettered,
            marks,
        );
        if back.shown.is_none() {
            end_behind(behind, kept);
        }
    }
    draw(
        overlay,
        kept,
        screen,
        target,
        cells,
        frame,
        frame_width,
        window_x,
        character,
        beneath,
        unlettered,
        marks,
    );
}

/// Whether the engine names a page written at once (`message_state` bit 5 or 6) on a line past the page's lines: the
/// page is not on the screen, and [`draw_pages`] takes the frame as one that names no page. A message written at once
/// that the engine has not begun (state 0) may have only its `message_table` set; its string, window and the engine's
/// line and cell are still those of the string before (Porunga's third wish in a battle: as the Item window comes back
/// the game starts the help again, `D000`, two frames before the engine writes it, and table 0 with the wishes' string
/// 0, window and line 2 names `help:0`, Kami's two-line help, at the wishes; the original's wishes stay as they
/// were, but Kami's help was drawn in their window and kept behind the help for 17 frames).
fn past_its_lines(screen: &OnScreen) -> bool {
    screen.state == 0 && usize::from(screen.line) >= screen.lines.len()
}

/// Whether the sorted glyphs `all` hold every one of the sorted glyphs `some`, as many times as `some` does.
fn contains_all(all: &[u8], some: &[u8]) -> bool {
    let mut rest = all.iter().peekable();
    some.iter().all(|glyph| {
        while rest.next_if(|other| *other < glyph).is_some() {}
        rest.next_if_eq(&glyph).is_some()
    })
}

/// Draws the column rules of the Status page and the save screens as the rules they are. The original draws them with
/// tile `$7B` (a one-pixel line down the cell's fourth column), which the English patch made its comma: the HD text
/// pass then draws a comma in every cell of the rule. A comma with another one right above or below it in the same
/// column is such a rule (text lines are two tile rows apart, so no sentence's comma has one), and so is every other
/// comma of that column on the screen: a window over the rule (the Continue's Yes or No, a portrait) can leave a
/// single cell of it showing between two windows. A rule's cell is filled with its ground and the line drawn down
/// its fourth column in its ink, as the original's tile shows it.
fn draw_rules(target: &mut Target<'_>, cells: &[TextCell], character: &dyn Fn(u8) -> Option<char>) {
    let comma = |cell: &TextCell| !cell.bold && character(cell.glyph) == Some(',');
    let commas: Vec<&TextCell> = cells.iter().filter(|cell| comma(cell)).collect();
    let stacked = |cell: &TextCell| {
        commas.iter().any(|other| other.x == cell.x && (other.y - cell.y).abs() == 8)
    };
    let columns: Vec<i32> = commas.iter().filter(|cell| stacked(cell)).map(|cell| cell.x).collect();
    for cell in &commas {
        if columns.contains(&cell.x) {
            target.fill((cell.x, cell.y), (cell.x + 8, cell.y + 8), cell.bg);
            target.fill(
                (cell.x + RULE_COLUMN, cell.y),
                (cell.x + RULE_COLUMN + 1, cell.y + 8),
                cell.fg,
            );
        }
    }
}

/// The column of the line in the original's rule tile (`$7B` of the Japanese UI font: `111#1111` in every row).
const RULE_COLUMN: i32 = 3;

/// The cells of the table's window, across (`docs/re/data/text-engine.md`): measured for the battle (1), dialogue
/// (2), story (3) and wish (4) windows, the page's longest line for the others; the Item help (0) at least 12 (its
/// window shows 17 from column 12: an English help page of one blank cell, Saibaman's, still takes a line of eight
/// characters); the ending's captions ([`ENDING_TABLE`]) all [`ENDING_CELLS`].
fn window_cells(table: u8, longest: usize) -> usize {
    match table {
        0 => longest.max(HELP_CELLS),
        1 => 17,
        2 => 20,
        3 => 25,
        4 => WISH_CELLS,
        ENDING_TABLE => ENDING_CELLS,
        _ => longest.max(4),
    }
}

/// Where the text of a page whose English shows no letter ([`Shown::blank`]) is, from its window's blank ground on
/// the engine's first line (`left`, `top`: the 1x pixel of the engine's first cell, the window `cells_wide` cells
/// across): the row and grid of the blank cells there (the layer's scroll moves them from their tile-map place, as it
/// does the letters) and their colour, when at least half the window's cells, but a portrait's hanging cell, are
/// blank cells of one colour on that row; `None` while the window is not up (the field or the fight shows there,
/// or the window opening).
fn blank_ground(
    cells: &[TextCell],
    (left, top): (i32, i32),
    cells_wide: i32,
) -> Option<((i32, i32), [u8; 3])> {
    let row: Vec<&TextCell> = cells
        .iter()
        .filter(|cell| {
            cell.glyph == BLANK
                && (left - 4..left + 8 * cells_wide).contains(&cell.x)
                && (top - 4..=top + 4).contains(&cell.y)
        })
        .collect();
    let y = most(row.iter().map(|cell| cell.y))?;
    let paper = most(row.iter().filter(|cell| cell.y == y).map(|cell| cell.bg))?;
    let ground: Vec<&&TextCell> =
        row.iter().filter(|cell| cell.y == y && cell.bg == paper).collect();
    if 2 * (ground.len() as i32) < cells_wide - 1 {
        return None;
    }
    let dx = most(ground.iter().map(|cell| off_grid(cell.x, left, 8)))?;
    Some(((left + dx, y), paper))
}

/// The rows of the screen (1x pixels) the blank ground of a window runs to, above and under the cell `at` (the first
/// line's first cell of a page whose English shows no letter, [`blank_ground`]): the blank cells in its column, a
/// row of cells apart, from it on (the cell itself may hold the page's `…` or `▼`).
fn ground_rows(cells: &[TextCell], (x, y): (i32, i32)) -> (i32, i32) {
    let blank: std::collections::HashSet<i32> =
        cells.iter().filter(|cell| cell.glyph == BLANK && cell.x == x).map(|cell| cell.y).collect();
    let (mut top, mut bottom) = (y, y);
    while blank.contains(&(top - 8)) {
        top -= 8;
    }
    while blank.contains(&(bottom + 8)) {
        bottom += 8;
    }
    (top, bottom)
}

/// The ink of the letters of the battle (1), dialogue (2) and story (3) windows, measured with the HD pack (the
/// letters of `battle:88`, of the story page `str3:0x03` in scene 3C and of the dialogue pages there):
/// a page whose English shows no letter ([`blank_ground`]) takes it where no page of its window was seen before.
/// `None` for the other windows: such a page is not drawn there.
fn blank_ink(table: u8) -> Option<[u8; 3]> {
    match table {
        1..=3 => Some([0, 0, 49]),
        _ => None,
    }
}

/// The cells across the wish window (`$3C`, Porunga's and Shenron's scenes): its text starts at column 17 (1x x 194
/// on the 372-pixel screen) and its paper ends at x 286, where its right frame starts (measured, with and without the
/// HD pack): 11 cells, 88 pixels. The English lines run past the frame (`You have found` / `Dragon Balls.`, 15
/// cells); the longest of them made the paper and the Chinese lines run over the frame and past it.
const WISH_CELLS: usize = 11;

/// The tile-map column of the left frame of the dialogue (2) and story (3) windows: the story box is columns 2 to 29
/// (`docs/re/data/text-engine.md`), and the dialogue window's frame is there too (measured on the card shop, 1x x 16
/// to 24 of the picture). Right of it, left of the text, is the window's own: a portrait and the name under it.
const WINDOW_LEFT_FRAME: i32 = 2;

/// The window paper of the battle (1), dialogue (2) and story (3) tables, from the screen row of the engine's first
/// line: where it starts and how tall it is. The battle's message window is tile rows 22 to 29 with the text from
/// row 24, window `$08` (table 2) rows 18 to 29 with the text from row 20, `$09` (table 3) rows 21 to 28 with the
/// text from row 23; the paper starts 4 pixels into the frame's top row and ends 3 pixels into its bottom row
/// (measured, with and without the HD pack: 1x y 180 to 235, 148 to 235 and 172 to 227). The other tables are not
/// centred: the Item help (0) shares its window with the game's own rows, the wishes (4) and the system lines (7)
/// with its cursor rows, and the ending's captions and the intro have their own places.
fn window_paper(table: u8) -> Option<(i32, i32)> {
    match table {
        1 | 3 => Some((-12, 55)),
        2 => Some((-12, 87)),
        _ => None,
    }
}

/// Whether a page of `table` whose translation is `text` (with its tokens) is centred in its window ([`centred`]):
/// the dialogue (2) and story (3) pages, but not one whose window closes by itself (`FD`, the translation's
/// `{auto}`): the game goes on with a choice or a list (the shop's 買 / 賣 and 是 / 否, a yes-or-no question, the
/// gravity training's level), its cursor on the page's own rows, on which its translation is laid out. Every page of
/// the battle's message window (1): the fighters' talk (`battle:260/p2` 「那傢伙看到丹丹…」) and the battle's own
/// messages (an attack, its damage, the results, the training's rules, a line of narration), whether they close by
/// themselves or not: three lines on the English's rows left 11 pixels of paper above them and none under them
/// (`battle:0`, 「樂平／對拉克特／發動攻擊！」). In that window `{auto}` is the timed wait of the
/// battle's messages, not a choice: its choices are the pages whose translation sets its words in the columns of the
/// game's cursor with ideographic spaces ([`CHOICE_COLUMNS`]), and they keep the English's rows.
fn centres(table: u8, text: &str) -> bool {
    match table {
        2 | 3 => !text.contains("{auto}"),
        1 => !text.contains(CHOICE_COLUMNS),
        _ => false,
    }
}

/// What a battle page's translation sets its words in the cursor's columns with (U+3000): `battle:317` (the
/// English `Will you  Yes!` / `continue? No!`, 「下一戰　　　　開打／要打嗎？　　　不打」), the training's
/// `battle:208` (星 / 數) and `battle:210` (好啊 / 不要), and `battle:220`.
const CHOICE_COLUMNS: char = '\u{3000}';

/// Where the translation's first line goes (the screen row of its letters' line, as the English's: the glyphs start
/// [`RISE`] above it) so that its `lines` are centred in the paper of the table's window, the window's paper
/// starting at `english` (the engine's first line) plus [`window_paper`]'s offset: blank lines before and after the
/// text take no room, and an odd pixel goes above (the glyphs' ink sits a pixel or two above the em's bottom, the
/// font's ascent [`ASCENT`]: Krillin's five lines show 6 pixels of paper above and 6.5 under, not 5 and 7.5). A
/// text taller than the paper starts at its top. `None` for a table that is not centred.
fn centred(table: u8, english: i32, lines: &[Vec<char>], pitch: i32) -> Option<i32> {
    let (from, height) = window_paper(table)?;
    let inked = |line: &Vec<char>| line.iter().any(|ch| !ch.is_whitespace());
    let first = lines.iter().position(inked)? as i32;
    let last = lines.iter().rposition(inked)? as i32;
    let block = pitch * (last - first) + EM as i32;
    Some(english + from + ((height - block + 1) / 2).max(0) + RISE - pitch * first)
}

/// The cells across the Item help takes at least (eight characters of the 12-pixel em).
const HELP_CELLS: usize = 12;

/// The cells across the ending's captions: from column 4, the longest line of its texts (27 cells, `xtask
/// text-dump`), eighteen characters of the 12-pixel em, on the black under the picture.
const ENDING_CELLS: usize = 27;

/// Wraps `text` (lines at its `\n`) to `width` pixels of the original, each character `advance` wide: a line breaks
/// before the character that does not fit, except a mark that never starts a line (it stays at the end, past the
/// width); an opening mark that would end a line, and a number or Latin word that would be cut, go down to the
/// next line. Spaces that would start a wrapped line are dropped.
#[must_use]
pub fn wrap(text: &str, width: f32, advance: impl Fn(char) -> f32) -> Vec<Vec<char>> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line: Vec<char> = Vec::new();
        let mut used = 0.0;
        for ch in paragraph.chars() {
            let wide = advance(ch);
            if used + wide > width + 0.01 && !line.is_empty() && !NO_START.contains(ch) {
                let mut next = Vec::new();
                let latin = |c: &char| c.is_ascii_alphanumeric();
                if latin(&ch) && line.last().is_some_and(latin) {
                    // A number or a Latin word is not cut: it goes down whole, when the line has more
                    let start = line.iter().rposition(|c| !latin(c)).map_or(0, |at| at + 1);
                    if start > 0 {
                        next = line.split_off(start);
                    }
                } else if line.len() > 1 && line.last().is_some_and(|&last| NO_END.contains(last)) {
                    next.extend(line.pop());
                }
                lines.push(std::mem::take(&mut line));
                line = next;
                used = line.iter().map(|&c| advance(c)).sum();
                if ch.is_whitespace() && line.is_empty() {
                    continue;
                }
            }
            line.push(ch);
            used += wide;
        }
        lines.push(line);
    }
    lines
}

/// The most frequent of `values`.
fn most<T: Copy + Eq + std::hash::Hash>(values: impl Iterator<Item = T>) -> Option<T> {
    let mut counts: FxHashMap<T, usize> = FxHashMap::default();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts.into_iter().max_by_key(|&(_, count)| count).map(|(value, _)| value)
}

/// The offset of a cell from the grid of `origin` with a pitch of `pitch` pixels, from `-pitch / 2` up.
fn off_grid(at: i32, origin: i32, pitch: i32) -> i32 {
    let off = (at - origin).rem_euclid(pitch);
    if off >= pitch / 2 { off - pitch } else { off }
}

/// Whether a page's English is on the screen ([`presence`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    /// Some of its letters show where it is, and nothing else does there: blank cells of its window at most (the
    /// cells the engine has not written yet, or written and not yet on the screen).
    In,
    /// Some of its letters show, and in some of its cells something else: another window drawn over part of it.
    Covered,
    /// None of its letters shows, its cells are blank cells of its window: the window, not written yet.
    Empty,
    /// None of its letters shows, and not its window either: another window, a picture, a black screen.
    Out,
    /// The page has no cell to compare (no letter, or only tiles the HD font has no glyph for): it says nothing.
    NoCells,
}

/// What a cell of a page that is not its English shows instead ([`Seen::misses`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Miss {
    /// Another letter, digit or mark of the HD font.
    Letter(char),
    /// A tile of the card art.
    Card,
    /// A blank cell of another ground than the window's paper.
    Ground,
    /// No text cell: a frame, a picture, a tile the HD font has no glyph for, a black screen.
    Nothing,
}

/// A page's English compared with the screen's text cells ([`presence`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    pub presence: Presence,
    /// The cells compared that show the page's English, a blank cell of its window, or something else.
    pub own: usize,
    pub blank: usize,
    pub other: usize,
    /// The cells not compared: `None` in the page's English, the spaces that end a line or show no blank cell of the
    /// window, or off the frame.
    pub skipped: usize,
    /// The cells compared that are not spaces (the most `own` can be).
    pub letters: usize,
    /// The frame's pixel (1x) of the first line's first cell (every line starts in its column, a portrait's page
    /// too) and the pixels from one line to the next; `matched` when a run of the page's characters shows them, else
    /// they are the hint's or where the engine writes.
    pub at: (i32, i32),
    pub pitch: i32,
    pub matched: bool,
    /// The first cells that showed something else: line, cell, the page's English there, what the screen shows.
    pub misses: Vec<(usize, usize, char, Miss)>,
    /// The text cells that show the page's English (`own` of them).
    pub shows: Vec<TextCell>,
}

/// How far from where the engine writes (the tile-map cell of `message_text_base`) a page's text may show, in 1x
/// pixels: the layer's scroll, and the battle's windows that show their lines closer than the engine writes them.
const PRESENCE_DX: std::ops::RangeInclusive<i32> = -8..=8;
const PRESENCE_DY: std::ops::RangeInclusive<i32> = -2 * PITCH - 4..=PITCH;
/// The pixels from one line to the next a window may show (the engine writes them [`PITCH`] apart).
const PRESENCE_PITCHES: std::ops::RangeInclusive<i32> = 10..=PITCH;
/// The characters of a page (letters, digits, marks) side by side on a line that place it on the screen where they all
/// show (fewer when the page has no run that long): characters of it scattered over another window's text do not.
const PRESENCE_RUN: usize = 3;
/// The misses [`Seen`] keeps.
const PRESENCE_MISSES: usize = 6;
/// Where a page's text shows ([`presence`]): the 1x pixel of its first line's first cell, and the pixels between lines.
type TextAt = ((i32, i32), i32);

/// Whether the page `screen` names is on the screen, from its English cell by cell ([`OnScreen::english`]) and the
/// frame's text `cells` (1x, the original's window at `window_x`, the frame `size` wide and high), `character`
/// spelling their glyphs. Its place: the shift from where the engine writes and the pitch between its lines at which
/// most of its characters show, among those where [`PRESENCE_RUN`] of them side by side do (one at `hint`, the place
/// it was seen last, is enough); without one, `hint` or where the engine writes. There each cell of the page is the
/// page's English, a blank cell of its window (on `paper`, else on the ground of its letters that show) or something
/// else; cells off the frame and those not compared do not count. See [`Presence`] for what the counts say.
pub(crate) fn presence(
    screen: &OnScreen,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    window_x: i32,
    size: (i32, i32),
    paper: Option<[u8; 3]>,
    hint: Option<TextAt>,
) -> Seen {
    presence_of(screen, cells, character, window_x, size, paper, hint, false)
}

/// [`presence`]; of a page the engine writes cell by cell (`typed`), also placed by the start of its English alone,
/// when no run of [`PRESENCE_RUN`] of its characters shows nor one at `hint`: the screen shows such a page from its
/// first cell on, a frame or more after the engine wrote them, so the first characters it shows, and none after them,
/// are where its first ones are (`he` of `hello`, a row up from where the engine writes as the layer scrolls). The
/// place of the most of them, nearest where the engine writes. Its window's blank cells are on the ground its letters
/// show on, `paper` only while none of them shows.
#[allow(clippy::too_many_arguments)]
fn presence_of(
    screen: &OnScreen,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    window_x: i32,
    size: (i32, i32),
    paper: Option<[u8; 3]>,
    hint: Option<TextAt>,
    typed: bool,
) -> Seen {
    let mut by_place: FxHashMap<(i32, i32), Vec<&TextCell>> = FxHashMap::default();
    let mut by_letter: FxHashMap<char, Vec<(i32, i32)>> = FxHashMap::default();
    for cell in cells {
        by_place.entry((cell.x, cell.y)).or_default().push(cell);
        if cell.glyph != BLANK
            && cell.glyph < CARD_BASE
            && let Some(ch) = character(cell.glyph).filter(|&ch| ch != ' ')
        {
            by_letter.entry(ch).or_default().push((cell.x, cell.y));
        }
    }
    let place = |(x, y): (i32, i32), pitch: i32, line: usize, cell: usize| {
        (x + 8 * cell as i32, y + pitch * line as i32)
    };
    let shows = |(x, y): (i32, i32), ch: char| {
        by_place.get(&(x, y)).is_some_and(|here| {
            here.iter().any(|cell| {
                cell.glyph != BLANK && cell.glyph < CARD_BASE && character(cell.glyph) == Some(ch)
            })
        })
    };
    let english: Vec<(usize, usize, char)> = screen
        .english
        .iter()
        .enumerate()
        .flat_map(|(line, cells)| {
            cells.iter().enumerate().filter_map(move |(cell, ch)| Some((line, cell, (*ch)?)))
        })
        .collect();
    let anchors: Vec<(usize, usize, char)> =
        english.iter().copied().filter(|&(_, _, ch)| ch != ' ').collect();
    // (the longest run of the anchors side by side on a line for which `shown` holds)
    let longest = |shown: &dyn Fn(usize, usize, char) -> bool| {
        let (mut longest, mut run, mut last) = (0, 0, None);
        for &(line, cell, ch) in &anchors {
            if !shown(line, cell, ch) {
                last = None;
                continue;
            }
            run = if cell > 0 && last == Some((line, cell - 1)) { run + 1 } else { 1 };
            last = Some((line, cell));
            longest = longest.max(run);
        }
        longest
    };
    let need = PRESENCE_RUN.min(longest(&|_, _, _| true)).max(1);
    // The place: where most of the page's characters show
    let (column, row) = screen.base_cell();
    let nominal = (window_x + 8 * i32::from(column), 8 * i32::from(row));
    let near = |(x, y): (i32, i32)| {
        PRESENCE_DX.contains(&(x - nominal.0)) && PRESENCE_DY.contains(&(y - nominal.1))
    };
    let mut tried: FxHashSet<TextAt> = FxHashSet::default();
    let mut best: Option<(usize, TextAt)> = None;
    // (the most first characters of a page being written, alone: `typed`)
    let mut start: Option<(usize, TextAt)> = None;
    // (the most letters; then the place nearest where the engine writes, and the engine's pitch)
    let rank = |(score, (origin, pitch)): (usize, TextAt)| {
        let off = (origin.0 - nominal.0).abs() + (origin.1 - nominal.1).abs();
        (score, std::cmp::Reverse(off), pitch)
    };
    for &(line, cell, ch) in &anchors {
        for &(x, y) in by_letter.get(&ch).into_iter().flatten() {
            for pitch in PRESENCE_PITCHES.rev() {
                let origin = (x - 8 * cell as i32, y - pitch * line as i32);
                if !near(origin) || !tried.insert((origin, pitch)) {
                    continue;
                }
                let hit = |line, cell, ch| shows(place(origin, pitch, line, cell), ch);
                if longest(&hit) < need {
                    // (the page's first characters and none after them)
                    let shown =
                        anchors.iter().take_while(|&&(line, cell, ch)| hit(line, cell, ch)).count();
                    let after =
                        anchors[shown..].iter().any(|&(line, cell, ch)| hit(line, cell, ch));
                    if typed
                        && shown > 0
                        && !after
                        && start.is_none_or(|start| rank((shown, (origin, pitch))) > rank(start))
                    {
                        start = Some((shown, (origin, pitch)));
                    }
                    continue;
                }
                let score = anchors.iter().filter(|&&(line, cell, ch)| hit(line, cell, ch)).count();
                if best.is_none_or(|best| rank((score, (origin, pitch))) > rank(best)) {
                    best = Some((score, (origin, pitch)));
                }
            }
        }
    }
    let found = best.or_else(|| {
        // (one letter at the place the page was seen last)
        let (origin, pitch) = hint?;
        let score = anchors
            .iter()
            .filter(|&&(line, cell, ch)| shows(place(origin, pitch, line, cell), ch))
            .count();
        (score >= 1).then_some((score, (origin, pitch)))
    });
    let found = found.or(start);
    let matched = found.is_some();
    let (at, pitch) = found.map(|(_, found)| found).or(hint).unwrap_or((nominal, PITCH));
    // The page's cells there
    let on_frame = |(x, y): (i32, i32)| x >= 0 && y >= 0 && x + 8 <= size.0 && y + 8 <= size.1;
    // (the window's ground: `paper`, else that of its letters that show; of a page being written, that of its letters
    // that show first, `paper` while none does: the screen may fade as the engine writes, `str4:0x02` as Porunga is
    // summoned, and the cells not written yet are on the ground its letters are on now)
    let shown_ground = || {
        most(english.iter().filter_map(|&(line, cell, ch)| {
            let here = by_place.get(&place(at, pitch, line, cell))?;
            here.iter()
                .find(|c| c.glyph != BLANK && c.glyph < CARD_BASE && character(c.glyph) == Some(ch))
                .map(|c| c.bg)
        }))
    };
    let ground = if typed { shown_ground().or(paper) } else { paper.or_else(shown_ground) };
    let mut seen = Seen {
        presence: Presence::NoCells,
        own: 0,
        blank: 0,
        other: 0,
        skipped: 0,
        letters: 0,
        at,
        pitch,
        matched,
        misses: Vec::new(),
        shows: Vec::new(),
    };
    for (line, cells_of) in screen.english.iter().enumerate() {
        // (the spaces that end a line are no text: the engine writes no space, and they may reach past the window)
        let text = cells_of.iter().rposition(|&ch| ch != Some(' ')).map_or(0, |last| last + 1);
        for (cell, &ch) in cells_of.iter().enumerate() {
            let xy = place(at, pitch, line, cell);
            let Some(ch) = ch.filter(|_| cell < text && on_frame(xy)) else {
                seen.skipped += 1;
                continue;
            };
            seen.letters += usize::from(ch != ' ');
            let here = by_place.get(&xy).map_or(&[][..], Vec::as_slice);
            let own = here.iter().find(|c| {
                ch != ' '
                    && c.glyph != BLANK
                    && c.glyph < CARD_BASE
                    && character(c.glyph) == Some(ch)
            });
            let miss = if let Some(&&own) = own {
                seen.own += 1;
                seen.shows.push(own);
                continue;
            } else if let Some(card) = here.iter().find(|c| c.glyph != BLANK) {
                if card.glyph >= CARD_BASE {
                    Miss::Card
                } else {
                    character(card.glyph).map_or(Miss::Nothing, Miss::Letter)
                }
            } else if let Some(blank) = here.first() {
                if ground.is_none_or(|ground| blank.bg == ground) {
                    seen.blank += 1;
                    continue;
                }
                Miss::Ground
            } else {
                Miss::Nothing
            };
            // (a space the window's blank cell does not show says nothing: the engine writes no space, and the
            // cells under one may be another window's, the frame under the shop's `                BUY`)
            if ch == ' ' {
                seen.skipped += 1;
                continue;
            }
            seen.other += 1;
            if seen.misses.len() < PRESENCE_MISSES {
                seen.misses.push((line, cell, ch, miss));
            }
        }
    }
    seen.presence = match (seen.own, seen.blank, seen.other) {
        _ if seen.own + seen.blank + seen.other == 0 || english.iter().all(|e| e.2 == ' ') => {
            Presence::NoCells
        }
        (1.., _, 0) => Presence::In,
        (1.., _, _) => Presence::Covered,
        (0, 1.., 0) => Presence::Empty,
        _ => Presence::Out,
    };
    seen
}

/// [`presence`] of the page `screen` names with the cells of its lines past its window not compared: a line of the
/// original longer than its window runs on past the frame (Porunga's `str4:0x03` and `0x04` in his 11-cell window,
/// the gravity training's `x Gravity` past the story window's 25 cells), where the screen shows the frame, the field
/// or another window, not those letters. Such a page was `Covered` though all of it that shows in its window is its
/// own. (The cells past the frame keep the picture beside them as before: [`spill_of`].)
pub(crate) fn presence_in_window(
    screen: &OnScreen,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    window_x: i32,
    size: (i32, i32),
    paper: Option<[u8; 3]>,
    hint: Option<TextAt>,
) -> Seen {
    in_window(screen, cells, character, window_x, size, paper, hint, false)
}

/// [`presence_in_window`] of a page the engine writes cell by cell (`message_state` 1 or 5): also placed by the first
/// characters of its English alone ([`presence_of`]), the start of the page as the screen shows it.
pub(crate) fn typed_presence(
    screen: &OnScreen,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    window_x: i32,
    size: (i32, i32),
    paper: Option<[u8; 3]>,
    hint: Option<TextAt>,
) -> Seen {
    in_window(screen, cells, character, window_x, size, paper, hint, true)
}

/// [`presence_in_window`], `typed` as [`presence_of`] takes it.
#[allow(clippy::too_many_arguments)]
fn in_window(
    screen: &OnScreen,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    window_x: i32,
    size: (i32, i32),
    paper: Option<[u8; 3]>,
    hint: Option<TextAt>,
    typed: bool,
) -> Seen {
    let judge = |page: &OnScreen| {
        if typed {
            presence_of(page, cells, character, window_x, size, paper, hint, true)
        } else {
            presence(page, cells, character, window_x, size, paper, hint)
        }
    };
    let wide = window_cells(screen.table, screen.lines.iter().copied().max().unwrap_or(0));
    if screen.english.iter().all(|line| line.len() <= wide) {
        return judge(screen);
    }
    let mut inside = screen.clone();
    for line in &mut inside.english {
        for ch in line.iter_mut().skip(wide) {
            *ch = None;
        }
    }
    judge(&inside)
}

/// The frames a page written at once the engine named waits for its English to show ([`Kept::pending`]): the screen
/// shows a page the engine wrote 2 to 6 frames later (the wish list 4, the Item help 6), and the engine names it for
/// a frame or three only. Past them, the page never showed where the engine wrote it (registers of the last message,
/// `message_state` A0/C0 before the engine writes: `battle:89` over the cards), and is forgotten.
const PENDING: u32 = 2 * KEEP;

/// The page a frame is drawn as naming, from the page the engine names `screen` and [`Kept::pending`] (the
/// pages written at once). The engine names a page written at once (`message_state` bit 5 or 6) before
/// the screen shows it, its registers still the last message's or its tile map not on the screen yet; meanwhile the
/// page shown before stays, its window still up. So such a page is named on the frame its English shows where the
/// engine writes it ([`Presence::In`], [`presence_in_window`]): when the engine names it, or a frame after while it is
/// pending; until then it waits ([`PENDING`] frames at most) and the frame names no page. One page a place: the page
/// the engine names (or the one pending) takes the place once it shows, also where the page before shows the same
/// English (the Item help of the seven Dragon Balls, `help:33` to `help:39`); a page only partly there is not named
/// (`Covered`: its letters there may be the page before's, `help:1` over `help:0` share `Restores`). A finished page
/// the engine names that is not on the screen at all ([`Presence::Out`]: a black screen, frame 10099 of
/// `secret-nail-porunga`) names no page either. A page the engine writes cell by cell (state 1 or 5)
/// is named once its window shows it ([`typed_presence`]: its first characters, or its window blank); while its
/// place shows something else (the page before, not cleared yet: `str2:0x0FF/p2` named at its first cell 6 frames
/// before the game clears the window; another window, the window not open yet: `str7:0x07` over the cards) the frame
/// names no page, and the page before stays as long as it shows. (Not when the page the overlay shows is that page:
/// its frames go as [`draw`] finds it.) Other pages, and pages with no cell to compare, are named as the engine names
/// them; a page the engine names that is not written at once ends a page pending.
#[allow(clippy::too_many_arguments)]
fn shown_now(
    kept: &mut Kept,
    screen: Option<&OnScreen>,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    window_x: i32,
    size: (i32, i32),
) -> Option<OnScreen> {
    let hint = |kept: &Kept, page: &OnScreen| {
        kept.place
            .filter(|place| place.string == (page.table, page.string))
            .map(|place| (place.at, place.pitch))
    };
    let presence_of = |kept: &Kept, page: &OnScreen| {
        presence_in_window(page, cells, character, window_x, size, None, hint(kept, page)).presence
    };
    let typed_of = |kept: &Kept, page: &OnScreen| {
        typed_presence(page, cells, character, window_x, size, None, hint(kept, page)).presence
    };
    match screen {
        // (a page a stopped string left, [`OnScreen::stopped`]: the engine names it as long as the bytes wait, whatever
        // the window shows; it is the frame's page only where its English shows, not over a blank window)
        Some(page) if page.stopped => {
            kept.pending = None;
            (presence_of(kept, page) == Presence::In).then(|| page.clone())
        }
        Some(page) if page.state == 0 => match presence_of(kept, page) {
            Presence::In | Presence::NoCells => {
                kept.pending = None;
                Some(page.clone())
            }
            _ => {
                kept.pending = Some((page.clone(), 0));
                None
            }
        },
        Some(page)
            if matches!(page.state, 2 | 3 | 6) && presence_of(kept, page) == Presence::Out =>
        {
            kept.pending = None;
            None
        }
        Some(page)
            if matches!(page.state, 1 | 5)
                && kept.shown.as_ref().is_none_or(|shown| shown.key != page.key)
                && matches!(typed_of(kept, page), Presence::Covered | Presence::Out) =>
        {
            kept.pending = None;
            None
        }
        Some(page) => {
            kept.pending = None;
            Some(page.clone())
        }
        None => {
            let (page, frames) = kept.pending.take()?;
            if presence_of(kept, &page) == Presence::In {
                return Some(page);
            }
            if frames < PENDING {
                kept.pending = Some((page, frames + 1));
            }
            None
        }
    }
}

/// The picture being drawn into: `width` pixels of `scale` times the 1x frame.
pub(crate) struct Target<'a> {
    pub rgba: &'a mut [u8],
    pub width: usize,
    pub scale: usize,
}

impl Target<'_> {
    fn height(&self) -> usize {
        self.rgba.len() / 4 / self.width
    }

    /// Fills the 1x pixels `x0..x1`, `y0..y1` with `colour`.
    fn fill(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), colour: [u8; 3]) {
        let s = self.scale as i32;
        let (w, h) = (self.width as i32, self.height() as i32);
        for y in (y0 * s).max(0)..(y1 * s).min(h) {
            for x in (x0 * s).max(0)..(x1 * s).min(w) {
                let at = (y as usize * self.width + x as usize) * 4;
                self.rgba[at..at + 3].copy_from_slice(&colour);
            }
        }
    }

    /// Fills the 1x pixels `x0..x1`, `y0..y1` with `colour`, but for the 1x cells (8 pixels square) whose top left
    /// corners `keep` lists: they keep the picture.
    fn fill_but(
        &mut self,
        (x0, y0): (i32, i32),
        (x1, y1): (i32, i32),
        colour: [u8; 3],
        keep: &[(i32, i32)],
    ) {
        let s = self.scale as i32;
        let (w, h) = (self.width as i32, self.height() as i32);
        let kept = |x: i32, y: i32| {
            keep.iter().any(|&(kx, ky)| {
                (kx * s..(kx + 8) * s).contains(&x) && (ky * s..(ky + 8) * s).contains(&y)
            })
        };
        for y in (y0 * s).max(0)..(y1 * s).min(h) {
            for x in (x0 * s).max(0)..(x1 * s).min(w) {
                if !kept(x, y) {
                    let at = (y as usize * self.width + x as usize) * 4;
                    self.rgba[at..at + 3].copy_from_slice(&colour);
                }
            }
        }
    }

    /// Copies the picture of the 1x cell (8 pixels square) at `from` over the one at `to`.
    fn copy_cell(&mut self, from: (i32, i32), to: (i32, i32)) {
        let s = self.scale as i32;
        let (w, h) = (self.width as i32, self.height() as i32);
        for dy in 0..8 * s {
            let (sy, ty) = (from.1 * s + dy, to.1 * s + dy);
            if !(0..h).contains(&sy) || !(0..h).contains(&ty) {
                continue;
            }
            for dx in 0..8 * s {
                let (sx, tx) = (from.0 * s + dx, to.0 * s + dx);
                if !(0..w).contains(&sx) || !(0..w).contains(&tx) {
                    continue;
                }
                let (src, dst) = ((sy * w + sx) as usize * 4, (ty * w + tx) as usize * 4);
                self.rgba.copy_within(src..src + 3, dst);
            }
        }
    }

    /// Moves the picture of the 1x cell (8 pixels square) at `at` up `lift` pixels; the `lift` rows it leaves at its
    /// bottom take `paper`.
    fn lift_cell(&mut self, (x, y): (i32, i32), lift: i32, paper: [u8; 3]) {
        self.copy_cell((x, y), (x, y - lift));
        self.fill((x, y + 8 - lift), (x + 8, y + 8), paper);
    }

    /// Of the 1x pixel at `at`, the picture's pixels that are `ink` over the picture `from` (as wide and high as this
    /// one), or a blend of the two, take `from`'s.
    fn unink_pixel(&mut self, from: &[u8], (column, line): (i32, i32), ink: [u8; 3]) {
        let scale = self.scale as i32;
        let (width, height) = (self.width as i32, self.height() as i32);
        if from.len() != self.rgba.len()
            || column < 0
            || line < 0
            || (column + 1) * scale > width
            || (line + 1) * scale > height
        {
            return;
        }
        let distance = |one: &[u8], other: &[u8]| -> u32 {
            one.iter().zip(other).map(|(one, other)| u32::from(one.abs_diff(*other))).sum()
        };
        for row in line * scale..(line + 1) * scale {
            for x in column * scale..(column + 1) * scale {
                let at = (row * width + x) as usize * 4;
                let (now, was) = (&self.rgba[at..at + 3], &from[at..at + 3]);
                let span = distance(was, &ink);
                if now != was
                    && span > 0
                    && distance(was, now) + distance(now, &ink) <= span + BLEND_SLACK
                {
                    let was = [from[at], from[at + 1], from[at + 2]];
                    self.rgba[at..at + 3].copy_from_slice(&was);
                }
            }
        }
    }

    /// Of the 1x pixel at `at`, the picture's pixels that are `ink` blended over the picture `from` (as wide and high
    /// as this one) as the HD text blends a letter's ink in, one coverage for the three channels (`font`), take
    /// `from`'s. Stricter than [`Target::unink_pixel`]: a pixel the HD text drew in another ink (the edge of a frame
    /// tile it drew again, a blend of the frame's colour and the paper) is not this ink's, even where each of its
    /// channels lies between the picture's and the ink's.
    fn unblend_pixel(&mut self, from: &[u8], (column, line): (i32, i32), ink: [u8; 3]) {
        let scale = self.scale as i32;
        let (width, height) = (self.width as i32, self.height() as i32);
        if from.len() != self.rgba.len()
            || column < 0
            || line < 0
            || (column + 1) * scale > width
            || (line + 1) * scale > height
        {
            return;
        }
        for row in line * scale..(line + 1) * scale {
            for x in column * scale..(column + 1) * scale {
                let at = (row * width + x) as usize * 4;
                let (now, was) = (&self.rgba[at..at + 3], &from[at..at + 3]);
                if now == was {
                    continue;
                }
                // (the blend of `font::Font::draw_with`; a channel may be a step off: two glyphs' edges of one ink)
                let inked = (1..=255_u32).any(|coverage| {
                    (0..3).all(|channel| {
                        let blended = (u32::from(was[channel]) * (255 - coverage)
                            + u32::from(ink[channel]) * coverage
                            + 127)
                            / 255;
                        blended.abs_diff(u32::from(now[channel])) <= 1
                    })
                });
                if inked {
                    let was = [from[at], from[at + 1], from[at + 2]];
                    self.rgba[at..at + 3].copy_from_slice(&was);
                }
            }
        }
    }

    /// Copies the 1x cell at `at` of the picture `from` (as wide and high as this one) over this one's.
    fn paste_cell(&mut self, from: &[u8], at: (i32, i32)) {
        let s = self.scale as i32;
        let (w, h) = (self.width as i32, self.height() as i32);
        if from.len() != self.rgba.len() {
            return;
        }
        for y in (at.1 * s).max(0)..((at.1 + 8) * s).min(h) {
            let (x0, x1) = ((at.0 * s).max(0), ((at.0 + 8) * s).min(w));
            if x0 >= x1 {
                continue;
            }
            let span = (y * w + x0) as usize * 4..(y * w + x1) as usize * 4;
            self.rgba[span.clone()].copy_from_slice(&from[span]);
        }
    }

    /// Blends `glyph` in `ink` with its em box's top left at the picture's pixel (`x`, `y`).
    fn glyph(&mut self, glyph: &Glyph, (x, y): (i32, i32), ink: [u8; 3]) {
        let (w, h) = (self.width as i32, self.height() as i32);
        for gy in 0..glyph.h {
            let py = y + glyph.top + gy as i32;
            if !(0..h).contains(&py) {
                continue;
            }
            for gx in 0..glyph.w {
                let px = x + glyph.left + gx as i32;
                let alpha = u32::from(glyph.alpha[gy * glyph.w + gx]);
                if alpha == 0 || !(0..w).contains(&px) {
                    continue;
                }
                let at = (py as usize * self.width + px as usize) * 4;
                for (channel, &ink) in ink.iter().enumerate() {
                    let under = u32::from(self.rgba[at + channel]);
                    self.rgba[at + channel] =
                        ((under * (255 - alpha) + u32::from(ink) * alpha + 127) / 255) as u8;
                }
            }
        }
    }
}

/// The em of a name drawn over a run of letters, at most: a line of the tile map is 8 pixels high.
const NAME_EM: f32 = 10.0;

/// The battle's commands, the window each turn of a fight starts with: the words of each of its blocks, one a row
/// (`block:0x35`; `block:0x42` and `block:0x4E`, three rows of the same column placed from two tile rows lower, so
/// their last row is the four rows' last).
const BATTLE_COMMANDS: [&[&str]; 3] =
    [&["Atk.", "Item", "Run", "Menu"], &["Atk.", "Item", "Menu"], &["Atk.", "Flee", "Menu"]];

/// How far up the names of the battle's commands and their cursor go. The window's rows are [`PITCH`] apart, each word
/// on the lower tile row of its pair, the row above it blank: the four pairs fill the window's paper, so the words sit
/// 4 pixels under its middle (measured at 3x with the HD pack: 10 pixels of paper above the first name's ink, 2.7
/// under the last's). Drawn 4 pixels up, a name is in the middle of its pair, and the four in the middle of the
/// paper: 6 pixels above, 6.7 under.
const COMMAND_LIFT: i32 = 4;

/// Where the battle's commands are among `parts` ([`name_parts`]): a column of [`BATTLE_COMMANDS`], one a row
/// [`PITCH`] apart from the same x: the left of its first word, its row, the ground of its cells and its rows. Other
/// columns of names (the field's commands, `block:0x00`; flying, `block:0x02`) are not it.
fn battle_commands(parts: &[(String, Vec<&TextCell>)]) -> Option<(i32, i32, [u8; 3], i32)> {
    // Each part's word with the place of its first cell, looked up rather than searched for: a screen of garbage
    // tiles (the Continue's while the engine writes VRAM) has thousands of parts, and a search for each of them took
    // a second a frame
    let starting: FxHashSet<(&str, i32, i32)> = parts
        .iter()
        .filter_map(|(english, part)| part.first().map(|cell| (english.trim(), cell.x, cell.y)))
        .collect();
    BATTLE_COMMANDS.iter().find_map(|words| {
        parts.iter().filter(|(_, part)| !part.is_empty()).find_map(|(_, first)| {
            let (x, y) = (first[0].x, first[0].y);
            let column = words
                .iter()
                .zip(0..)
                .all(|(word, row)| starting.contains(&(*word, x, y + PITCH * row)));
            column.then_some((x, y, first[0].bg, words.len() as i32))
        })
    })
}

/// Whether `english` spells the name `name` with letters written into its blanks: the name has two blanks in a row
/// inside it (the Stats page's `EX    UP.`, the page writes its `LV` into them), `english` is as long, has the name's
/// letters where the name has letters, and no figure where the name has blanks.
fn fills(name: &str, english: &str) -> bool {
    name.trim().contains("  ")
        && name != english
        && name.chars().count() == english.chars().count()
        && name.chars().zip(english.chars()).all(|(a, b)| a == b || a == ' ' && !b.is_ascii_digit())
}

/// Draws, over each part of `cells` (spelled by `character`, see [`name_parts`]) that is a name of the overlay's,
/// its translation: the part's cells take their ground colour and the name is drawn across them in their ink, its
/// em shrunk to fit the part. The letters of names whose first letter is cut take their ground, round their cells
/// where the 1x frame `frame` (`frame_width` pixels a row) is that ground ([`draw_covered_column`]).
fn draw_names(
    overlay: &Overlay,
    target: &mut Target<'_>,
    cells: &[TextCell],
    frame: &[Color],
    frame_width: usize,
    character: &dyn Fn(u8) -> Option<char>,
) {
    if overlay.names.is_empty() {
        return;
    }
    let mut unnamed = Vec::new();
    let parts = name_parts(cells, character);
    let commands = battle_commands(&parts);
    // (the places of the parts that are names, gathered once when a part needs them: see `hide_cut_words`)
    let named_starts = std::cell::OnceCell::new();
    for (english, part) in parts {
        if let Some(name) = overlay.name(english.trim_end()) {
            // (a row of words or figures set in columns, `1 2 3 4 5` of the text speed, keeps to its cells; so does a
            // name found by its start, which may be a name being written into the blanks after it)
            let whole = overlay.names.contains_key(english.trim_end());
            let room = if !whole || english.trim().contains(' ') {
                0
            } else {
                room_after(cells, &part, character)
            };
            let command = commands.is_some_and(|(x, y, _, rows)| {
                part[0].x == x && (y..y + PITCH * rows).contains(&part[0].y)
            });
            let lift = if command { COMMAND_LIFT } else { 0 };
            draw_name_reaching(&overlay.face, target, &part, name, 1, room, lift);
            continue;
        }
        // A word framed by marks right beside it (the dashes of the text speed window's `--Speed--`): the word's
        // name, over the word's cells; the marks keep their letters
        let framed = |cell: &&&TextCell| {
            character(cell.glyph).is_some_and(|ch| ch != ' ' && !ch.is_alphanumeric())
        };
        let lead = part.iter().take_while(framed).count();
        let trail = part.iter().rev().take_while(framed).count();
        if (lead > 0 || trail > 0) && lead + trail < part.len() {
            let core = &part[lead..part.len() - trail];
            let word: String = core.iter().filter_map(|cell| character(cell.glyph)).collect();
            if let Some(name) = overlay.name(word.trim()) {
                draw_name(&overlay.face, target, core, name);
                // A dash right after the name with nothing after it (the Stats page's `Krillin-`, `STATS` a row
                // below): the English's hyphen, which the Chinese has not; it takes its ground
                let end = &part[part.len() - 1..];
                if lead == 0 && trail == 1 && character(end[0].glyph) == Some('-') {
                    draw_name_reaching(&overlay.face, target, end, "", 0, 0, 0);
                }
                continue;
            }
        }
        // Words a cell apart with no tile between them: a sentence's (the opening's scroll), or two windows' with a
        // frame's tile between them (`Order` of the Menu and `Save?` of the window over it). They are names when
        // each of them is one; a sentence is left alone.
        let words: Vec<&[&TextCell]> = part.chunk_by(|a, b| b.x != a.x + 16).collect();
        let spell = |word: &[&TextCell]| -> String {
            word.iter().filter_map(|cell| character(cell.glyph)).collect()
        };
        // A lone mark such a gap keeps apart at either end is another window's (the slot screen's dotted rule a
        // frame's tile left of the Continue's `Load game? Yes`): the name is the rest
        let mark = |word: &[&TextCell]| !spell(word).chars().any(char::is_alphanumeric);
        let start = words.iter().take_while(|word| mark(word)).count();
        let end = words.len() - words[start..].iter().rev().take_while(|word| mark(word)).count();
        if (start > 0 || end < words.len()) && start < end {
            let core = &words[start..end];
            let english =
                core.iter().map(|word| spell(word).trim().to_owned()).collect::<Vec<_>>().join(" ");
            if let Some(name) = overlay.name(&english) {
                let cells: Vec<&TextCell> =
                    core.iter().flat_map(|word| word.iter().copied()).collect();
                draw_name(&overlay.face, target, &cells, name);
                continue;
            }
        }
        // Words a tile without a letter sets apart (`KI-USE`: the English patch's dash, tile $43, between): each
        // that is a name is drawn, the others keep their letters (`KI消耗`: a word after the mark takes the mark's
        // cell, below). The HD font spells that tile, a dash (`-`): the two words and the dash are then one part, the
        // dash a cell of it between them
        let letters = |word: &&[&TextCell]| {
            !word.is_empty()
                && word
                    .iter()
                    .all(|cell| character(cell.glyph).is_some_and(|ch| ch.is_ascii_alphabetic()))
        };
        let dashed: Vec<&[&TextCell]> =
            part.split(|cell| character(cell.glyph) == Some('-')).collect();
        let by_dash = dashed.len() == 2 && dashed.iter().all(letters);
        let marked = if by_dash {
            Some(dashed)
        } else {
            (words.len() > 1
                && words.windows(2).all(|pair| {
                    let last = pair[0][pair[0].len() - 1];
                    cells.iter().any(|cell| {
                        cell.x == last.x + 8
                            && cell.y == last.y
                            && cell.glyph != BLANK
                            && character(cell.glyph)
                                .is_none_or(|ch| ch != ' ' && !ch.is_alphanumeric())
                    })
                }))
            .then(|| words.clone())
        };
        if let Some(marked) = marked {
            let names: Vec<(usize, &[&TextCell], &str)> = marked
                .iter()
                .enumerate()
                .filter_map(|(at, word)| Some((at, *word, overlay.name(spell(word).trim_end())?)))
                .collect();
            if !names.is_empty() {
                for (at, word, name) in names {
                    // The word after the HD font's dash (`USE` of `KI-USE`, `消耗`): the dash goes with its name, which
                    // starts on the dash's cell, right after the word before it (`KI消耗`, one word as the HD Stats
                    // page's `ＫＩ消耗`); in the word's colours, its ground not reaching into the letters left of the
                    // dash. A tile the font has no letter for keeps its picture: it may be a frame between two windows
                    // (the Menu's `Order` and the `Is this OK?` window over it, a name and the `LV` window beside it)
                    let mark = cells
                        .iter()
                        .filter(|_| by_dash && at > 0)
                        .find(|cell| (cell.x, cell.y) == (word[0].x - 8, word[0].y))
                        .map(|mark| TextCell { fg: word[0].fg, bg: word[0].bg, ..*mark });
                    match mark {
                        Some(mark) => {
                            let with: Vec<&TextCell> =
                                std::iter::once(&mark).chain(word.iter().copied()).collect();
                            draw_name_reaching(&overlay.face, target, &with, name, 0, 0, 0);
                        }
                        None => draw_name(&overlay.face, target, word, name),
                    }
                }
                continue;
            }
        }
        let named: Option<Vec<&str>> =
            words.iter().map(|word| overlay.name(spell(word).trim_end())).collect();
        if let Some(named) = named.filter(|_| words.len() > 1) {
            for (word, name) in words.iter().zip(named) {
                draw_name(&overlay.face, target, word, name);
            }
            continue;
        }
        // Of words a cell apart that are not all names, a name in a column of names (a name starting at its x a row
        // above or below it) is a list's, beside letters of something else: the Item screen's `AID` under `EXT`, the
        // original's `RSa` spilled out of the list right before it and the cursor between. It is drawn; the other
        // words keep their letters
        let column: Vec<(&[&TextCell], &str)> = if words.len() > 1 {
            words
                .iter()
                .filter(|word| {
                    let (x, y) = (word[0].x, word[0].y);
                    let starts = named_starts.get_or_init(|| {
                        name_parts(cells, character)
                            .into_iter()
                            .filter(|(english, part)| {
                                !part.is_empty() && overlay.name(english.trim_end()).is_some()
                            })
                            .map(|(_, part)| (part[0].x, part[0].y))
                            .collect::<FxHashSet<(i32, i32)>>()
                    });
                    starts.contains(&(x, y - PITCH)) || starts.contains(&(x, y + PITCH))
                })
                .filter_map(|word| Some((*word, overlay.name(spell(word).trim_end())?)))
                .collect()
        } else {
            Vec::new()
        };
        if !column.is_empty() {
            for (word, name) in column {
                draw_name(&overlay.face, target, word, name);
            }
            continue;
        }
        unnamed.push((english, part));
    }
    // The cursor's cell left of each of the battle's commands (a cell with no tile: the picture's) goes up with them
    if let Some((x, y, paper, rows)) = commands {
        for row in 0..rows {
            target.lift_cell((x - 8, y + PITCH * row), COMMAND_LIFT, paper);
        }
    }
    draw_covered_column(overlay, target, cells, frame, frame_width, character, &unnamed);
    hide_cut_words(overlay, target, cells, character, &unnamed);
}

/// Hides the letters of the words a window over a list cuts at their end (the Item screen's `INT` / `EXT` / `AID`
/// under the "can't use" message, its frame over the `ID`: `A` shows): a part of `unnamed` shorter than four letters
/// (a longer one is named by its start, see [`Overlay::name`]) that a longer name starts with, with no letter right
/// after it on its row (the frame's tile) and a name right above it in its column (the part starting at its x within
/// the 16 pixels above). No translation fits in what shows, so its cells take their ground: no English letter shows.
fn hide_cut_words(
    overlay: &Overlay,
    target: &mut Target<'_>,
    cells: &[TextCell],
    character: &dyn Fn(u8) -> Option<char>,
    unnamed: &[(String, Vec<&TextCell>)],
) {
    // (the places of the cells with letters, gathered once when a part needs them: a screen of garbage tiles has
    // thousands of short parts, and a search of its cells for each of them took tens of milliseconds a frame)
    let places = std::cell::OnceCell::new();
    let lettered = |x: i32, y: i32| {
        places
            .get_or_init(|| {
                cells
                    .iter()
                    .filter(|cell| character(cell.glyph).is_some())
                    .map(|cell| (cell.x, cell.y))
                    .collect::<FxHashSet<(i32, i32)>>()
            })
            .contains(&(x, y))
    };
    let named: Vec<(i32, i32)> = name_parts(cells, character)
        .into_iter()
        .filter(|(english, part)| !part.is_empty() && overlay.name(english.trim_end()).is_some())
        .map(|(_, part)| (part[0].x, part[0].y))
        .collect();
    for (english, part) in unnamed {
        let english = english.trim();
        let letters = english.chars().count();
        if part.is_empty()
            || letters == 0
            || letters >= 4
            || !english.chars().any(char::is_alphanumeric)
        {
            continue;
        }
        let (first, last) = (part[0], part[part.len() - 1]);
        let cut = !lettered(last.x + 8, last.y);
        let above = named.iter().any(|&(x, y)| x == first.x && y < first.y && y >= first.y - 16);
        let starts = overlay
            .names
            .keys()
            .any(|name| name.len() > english.len() && name.starts_with(english));
        if cut && above && starts {
            draw_name_reaching(&overlay.face, target, part, "", 1, 0, 0);
        }
    }
}

/// A row of a column of names whose first letters are cut ([`draw_covered_column`]): the part's letters after the cut
/// one, its name, and whether it is a listed row (its name drawn) or a cut one (its letters hidden).
type CutRow<'a> = (&'a [&'a TextCell], &'a str, bool);

/// Hides the letters of a column of names whose first letters are cut: two parts or more of `unnamed` (parts no name
/// was drawn over) whose letters start at the same column, each, with one letter in front, spelling a name. A letter
/// is cut where
/// - the frame of a window over the column covers it (the save's `Yes` / `No` under the slot window: `es` / `o`
///   show): the cell left of the part is a tile the HD font has no letter for (the frame's), or has no text tile and
///   the one left of that has one (the window's);
/// - the game writes a blank over it right after the name's place (the Order window as it closes: `1Piccolo` /
///   `2Goku` show as `1 iccolo` / `2 oku`): the part is a number, one blank and the letters.
///
/// No translation fits in what shows, so the letters take their ground ([`paint_beside_window`], from the 1x frame
/// `frame`, `frame_width` pixels a row): no English letter shows, as for a word a window cuts at its end
/// ([`hide_cut_words`]).
///
/// A list whose cursor is written over the first letter of a row is another thing: see `listed` below, whose rows
/// show their names.
fn draw_covered_column(
    overlay: &Overlay,
    target: &mut Target<'_>,
    cells: &[TextCell],
    frame: &[Color],
    frame_width: usize,
    character: &dyn Fn(u8) -> Option<char>,
    unnamed: &[(String, Vec<&TextCell>)],
) {
    let tiled: FxHashMap<(i32, i32), &TextCell> =
        cells.iter().map(|cell| ((cell.x, cell.y), cell)).collect();
    let digit = |cell: &TextCell| character(cell.glyph).is_some_and(|ch| ch.is_ascii_digit());
    // The letters of `part` a cut first letter would be in front of, when it is cut
    let cut = |part: &[&TextCell]| -> Option<usize> {
        let places = part.iter().take_while(|cell| digit(cell)).count();
        if places > 0 {
            let (blank, letters) = (part.get(places)?, part.get(places + 1)?);
            let placed = blank.glyph == BLANK
                && blank.x == part[places - 1].x + 8
                && letters.x == blank.x + 8
                && part[places + 1..]
                    .iter()
                    .all(|cell| character(cell.glyph).is_some_and(|ch| ch.is_ascii_alphabetic()));
            return placed.then_some(places + 1);
        }
        let framed = match tiled.get(&(part[0].x - 8, part[0].y)) {
            Some(left) => character(left.glyph).is_none(),
            None => tiled.contains_key(&(part[0].x - 16, part[0].y)),
        };
        framed.then_some(0)
    };
    // The one name a part is the end of, a letter in front
    let name = |english: &str| {
        let len = english.chars().count() + 1;
        let mut ending = overlay
            .names
            .iter()
            .filter(|(name, _)| name.chars().count() == len && name.ends_with(english));
        let (_, first) = ending.next()?;
        ending.all(|(_, other)| other == first).then_some(first.as_str())
    };
    // A list whose cursor is written over the first letter of the row it is on (the shop's SELL list: `▶ulma` for
    // Bulma, the `B` blanked and the cursor a sprite over it; a row of 龍珠雷達 below it starting in the blanked
    // letter's column): one part alone is that row's name when a name of the list starts in the cut letter's column
    // within two rows of it, or a name of the list's next column is on its row (the last row left: `▶urtle  龍珠雷達`),
    // the cell of the cut letter holding no letter
    let starts: Vec<(i32, i32)> = name_parts(cells, character)
        .into_iter()
        .filter(|(english, part)| !part.is_empty() && overlay.name(english.trim_end()).is_some())
        .map(|(_, part)| (part[0].x, part[0].y))
        .collect();
    let listed = |part: &[&TextCell]| {
        let cut = tiled
            .get(&(part[0].x - 8, part[0].y))
            .is_none_or(|cell| character(cell.glyph).is_none_or(|ch| !ch.is_alphanumeric()));
        cut && starts.iter().any(|&(x, y)| {
            (x == part[0].x - 8 && y != part[0].y && (y - part[0].y).abs() <= 32)
                || (y == part[0].y && x > part[0].x)
        })
    };
    let mut columns: FxHashMap<i32, Vec<CutRow<'_>>> = FxHashMap::default();
    for (english, part) in unnamed {
        if part.is_empty() || english.trim().is_empty() {
            continue;
        }
        let listed = listed(part);
        let Some(from) = (if listed { Some(0) } else { cut(part) }) else { continue };
        let letters = &part[from..];
        let spelled: String = letters.iter().filter_map(|cell| character(cell.glyph)).collect();
        if let Some(name) = name(spelled.trim()) {
            columns.entry(letters[0].x).or_default().push((letters, name, listed));
        }
    }
    for (x, column) in columns {
        if column.len() < 2 && !column.iter().all(|&(_, _, listed)| listed) {
            continue;
        }
        let right = column.iter().map(|(part, _, _)| part[part.len() - 1].x).max().unwrap_or(x);
        for (part, name, listed) in column {
            if !listed {
                for cell in part {
                    paint_beside_window(target, frame, frame_width, cell, None);
                }
                continue;
            }
            let y = part[0].y;
            let mut wide: Vec<&TextCell> = part.to_vec();
            let mut at = part[part.len() - 1].x + 8;
            while at <= right {
                match tiled.get(&(at, y)) {
                    Some(cell) if cell.glyph == BLANK => wide.push(cell),
                    _ => break,
                }
                at += 8;
            }
            // (the frame's tile left of the part keeps its pixels)
            draw_name_reaching(&overlay.face, target, &wide, name, 0, 0, 0);
        }
    }
}

/// The parts of `cells` (spelled by `character`) a name could be, with their cells. A run is the letters of one
/// row, 8 pixels apart, or 16 where a space has no tile (prose: the opening's scroll); a part is the whole run, or
/// a piece of it between two or more blank cells (a list's columns). So a word of a sentence is never a part.
fn name_parts<'a>(
    cells: &'a [TextCell],
    character: &dyn Fn(u8) -> Option<char>,
) -> Vec<(String, Vec<&'a TextCell>)> {
    let mut row: Vec<(&TextCell, char)> = cells
        .iter()
        .filter(|cell| cell.glyph < CARD_BASE)
        .filter_map(|cell| Some((cell, character(cell.glyph)?)))
        .collect();
    row.sort_unstable_by_key(|(cell, _)| (cell.y, cell.x));
    // the row's letters, with `None` for a space that has no tile
    let mut slots: Vec<Option<(&TextCell, char)>> = Vec::new();
    let mut parts = Vec::new();
    for at in 0..=row.len() {
        if at > 0 {
            let (prev, before) = row[at - 1];
            let next = row.get(at).copied();
            let same_row = next.is_some_and(|(next, _)| next.y == prev.y);
            // (a number beside a name is its own: a box's line or a frame's tile is between them, or nothing at all
            // in the battle's status box: `Piccolo77`, the HP right after a seven-letter name; a digit after a mark
            // stays in the name: `DB'1`)
            let digit = before.is_ascii_digit() || next.is_some_and(|(_, ch)| ch.is_ascii_digit());
            let to_number =
                before.is_ascii_alphabetic() && next.is_some_and(|(_, ch)| ch.is_ascii_digit());
            // (and a name right after a number is its own: the Order screen's `1Goku`, the place before the name)
            let to_name =
                before.is_ascii_digit() && next.is_some_and(|(_, ch)| ch.is_ascii_alphabetic());
            match next {
                Some((next, _)) if same_row && !to_number && !to_name && next.x == prev.x + 8 => {}
                Some((next, _)) if same_row && !digit && next.x == prev.x + 16 => slots.push(None),
                _ => {
                    run_parts(&slots, &mut parts);
                    slots.clear();
                }
            }
        }
        if let Some(&letter) = row.get(at) {
            slots.push(Some(letter));
        }
    }
    parts
}

/// The parts of one run's `slots` (see [`name_parts`]), added to `parts`.
fn run_parts<'a>(
    slots: &[Option<(&'a TextCell, char)>],
    parts: &mut Vec<(String, Vec<&'a TextCell>)>,
) {
    let blank = |at: usize| slots.get(at).is_none_or(|slot| slot.is_none_or(|(_, ch)| ch == ' '));
    let ch = |at: usize| slots.get(at).copied().flatten().map(|(_, ch)| ch);
    let mut at = 0;
    while at < slots.len() {
        if blank(at) {
            at += 1;
            continue;
        }
        // a part: from a letter to before two blanks in a row, or a blank between a letter or a mark and a digit (a
        // number a column after a name: `Piccolo 77` on the slot screen, `PLA. 04` on the Stats page), or the run's
        // end
        let mut end = at;
        while end < slots.len()
            && !(blank(end)
                && (blank(end + 1)
                    || end > at
                        && ch(end - 1).is_some_and(|ch| {
                            ch.is_ascii_alphabetic() || ch.is_ascii_punctuation()
                        })
                        && ch(end + 1).is_some_and(|ch| ch.is_ascii_digit())))
        {
            end += 1;
        }
        let english = slots[at..end].iter().map(|slot| slot.map_or(' ', |(_, ch)| ch)).collect();
        parts.push((english, slots[at..end].iter().flatten().map(|&(cell, _)| cell).collect()));
        at = end;
    }
}

/// Draws `name` over the cells `part`, in their colours.
fn draw_name(face: &ChineseFace, target: &mut Target<'_>, part: &[&TextCell], name: &str) {
    draw_name_reaching(face, target, part, name, 1, 0, 0);
}

/// The pixels right of `part` a name drawn over it may take as well: the blank cells (spaces) right after it on its
/// row, all but one when a letter follows them (the gap to the next column stays). A name wider than its English
/// takes them before it is shrunk (`Fly ` of the field's commands: 舞空術 at its full size, not 8 of 10 pixels).
fn room_after(
    cells: &[TextCell],
    part: &[&TextCell],
    character: &dyn Fn(u8) -> Option<char>,
) -> i32 {
    let last = part[part.len() - 1];
    let at = |x: i32| {
        cells
            .iter()
            .find(|cell| cell.y == last.y && cell.x == x && cell.glyph < CARD_BASE)
            .and_then(|cell| character(cell.glyph))
    };
    let mut blanks = 0;
    while at(last.x + 8 * (blanks + 1)) == Some(' ') {
        blanks += 1;
    }
    if blanks > 0 && at(last.x + 8 * (blanks + 1)).is_some() {
        blanks -= 1;
    }
    8 * blanks
}

/// Draws `name` over the cells `part`, in their colours, their ground reaching `reach` pixels left of the first; a
/// name wider than the part takes up to `room` pixels right of it before its em is shrunk, and is drawn `lift` pixels
/// above the part's row ([`COMMAND_LIFT`]).
fn draw_name_reaching(
    face: &ChineseFace,
    target: &mut Target<'_>,
    part: &[&TextCell],
    name: &str,
    reach: i32,
    room: i32,
    lift: i32,
) {
    let (first, last) = (part[0], part[part.len() - 1]);
    let (ink, paper) = (first.fg, first.bg);
    let width = (last.x + 8 - first.x + room) as f32;
    let natural: f32 = name.chars().map(|ch| face.advance_at(ch, NAME_EM)).sum();
    let em = if natural > width { NAME_EM * width / natural } else { NAME_EM };
    // The part's letters take their ground, a pixel more around (the HD letters reach a little past their cells); a
    // cell with no tile between them keeps the picture (the cursor before the Continue's `Yes`)
    for run in part.chunk_by(|a, b| b.x <= a.x + 8) {
        let (from, to) = (run[0].x, run[run.len() - 1].x + 8);
        let left = if from == first.x { from - reach } else { from };
        let right = if to == last.x + 8 { to + 1 } else { to };
        target.fill((left, first.y - 1), (right, first.y + 9), paper);
    }
    let scale = target.scale;
    let top = (first.y - lift) as f32 + (8.0 - em) / 2.0;
    let mut pen = first.x as f32;
    for ch in name.chars() {
        let glyph = face.glyph_at(ch, scale, em);
        if !ch.is_whitespace() {
            let at = ((pen * scale as f32).round() as i32, (top * scale as f32).round() as i32);
            target.glyph(&glyph, at, ink);
        }
        pen += glyph.advance;
    }
}

/// The key of the translation of the chase's damage label (see [`draw_chase_damage`]).
pub(crate) const CHASE_DAMAGE_KEY: &str = "screen:chase_damage";

/// The key of the translation that turns the Enhanced card tooltip (`crate::tooltip`) off: with the text `off`, a
/// language whose cards the player reads as they are (the kanji of the zh-tw pack) draws no tooltip. Any other text,
/// or none, leaves the tooltip as it is.
pub(crate) const CARD_TOOLTIP_KEY: &str = "screen:card_tooltip";
/// The tiles of the chase's damage label, as the Japanese writes it and the English patch left it: `タメーシ……`
/// (`00:C46D`, the table `00:C413` the game copies to its tile map at `00:C3B6`). The patch drew Latin letters over
/// most of these tiles, so the screen shows `s゛X－ミ゛……` (the HD text takes `$53` for an `s`).
const CHASE_DAMAGE: [u16; 6] = [0x53, 0x65, 0x43, 0x4F, 0x7D, 0x7D];
/// The tile of `゛`, and the tiles of the label it is over in the row above (`タ` and `シ`, `00:C455`): `ダメージ`.
const DAKUTEN: u16 = 0x40;
const CHASE_DAMAGE_MARKS: [usize; 2] = [0, 3];

/// A row of 8 by 8 tiles of a 2-bit BG, as the screen shows it: where its first tile's top left corner is (1x pixels
/// of the original's 256), and the tile numbers (the map entries' low 10 bits) of its tiles from there and of the
/// tiles of the row above them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileRow {
    pub x: i32,
    pub y: i32,
    pub tiles: Vec<u16>,
    pub above: Vec<u16>,
}

/// The tile rows of the 2-bit BGs (of modes 0 and 1, 8 by 8 tiles, on the main screen) of the frame `log` shows:
/// one for each line of the frame that is the top line of a tile row of a BG, its 33 tiles from the left edge. The
/// line of scanline `n` is row `n - 1` of the frame and shows the BG's line `n` plus its vertical scroll, as the
/// renderer draws it (`bg.rs`) and the text cells are noted: a row's `y` is its top line, where the cells of its
/// letters are.
pub(crate) fn tile_rows(log: &FrameLog) -> Vec<TileRow> {
    let mut rows = Vec::new();
    for entry in log.entries.iter().filter(|entry| entry.from_pixel == 0) {
        let regs = &entry.regs;
        if regs.forced_blanking || regs.bg_mode > 1 {
            continue;
        }
        let y = i32::from(entry.scanline) - 1;
        for bg in 0..4 {
            if !regs.main_bg[bg]
                || regs.bg_large_tiles[bg]
                || crate::bg::bg_bitplanes(regs.bg_mode, bg) != 2
                || (bg == 3 && regs.bg_mode != 0)
            {
                continue;
            }
            let v = entry.scanline.wrapping_add(regs.bg_v_scroll[bg]);
            if v % 8 != 0 {
                continue;
            }
            let h = regs.bg_h_scroll[bg];
            let row = |v: u16| -> Vec<u16> {
                (0..33_u16)
                    .map(|column| {
                        let x = h.wrapping_sub(h % 8).wrapping_add(8 * column);
                        crate::bg::map_entry(&log.vram, regs, bg, x, v) & 0x3FF
                    })
                    .collect()
            };
            rows.push(TileRow {
                x: -i32::from(h % 8),
                y,
                tiles: row(v),
                above: row(v.wrapping_sub(8)),
            });
        }
    }
    rows
}

/// The column of `row` where the chase's damage label starts: its six tiles in a run, a `゛` over the first and the
/// fourth of them in the row above.
fn chase_damage_at(row: &TileRow) -> Option<usize> {
    (0..row.tiles.len()).find(|&at| {
        row.tiles[at..].starts_with(&CHASE_DAMAGE)
            && CHASE_DAMAGE_MARKS.iter().all(|&mark| row.above.get(at + mark) == Some(&DAKUTEN))
    })
}

/// Draws the translation of the chase's damage label (`screen:chase_damage`) where `rows` show it (Namek, the
/// flight after Dende is saved: the label of the window under the race, before the damage's two digits). The game
/// writes it straight into its tile map, so the message engine never names it, and the English patch left the
/// Japanese tiles, which show as garbage over its Latin letters. Its six cells and the two `゛` over them take the
/// ground of the text cells the font noted on the label's row (its `s`, the digits after it), and the translation
/// is drawn across the six cells in their ink, as a name is ([`draw_name_reaching`]). The digits after it are the
/// game's, where they are. A frame with none of those cells is left alone, as is every frame when the translation
/// lacks the key.
fn draw_chase_damage(
    overlay: &Overlay,
    target: &mut Target<'_>,
    cells: &[TextCell],
    rows: &[TileRow],
    window_x: i32,
) {
    let Some(text) = overlay.table.text(CHASE_DAMAGE_KEY) else { return };
    let face = &overlay.face;
    for row in rows {
        let Some(at) = chase_damage_at(row) else { continue };
        let (x0, y) = (window_x + row.x + 8 * at as i32, row.y);
        let width = 8 * CHASE_DAMAGE.len() as i32;
        let on_row: Vec<&TextCell> = cells
            .iter()
            .filter(|cell| {
                cell.y == y
                    && cell.glyph != BLANK
                    && cell.glyph < CARD_BASE
                    && (x0..x0 + width + 16).contains(&cell.x)
            })
            .collect();
        let (Some(ink), Some(paper)) =
            (most(on_row.iter().map(|cell| cell.fg)), most(on_row.iter().map(|cell| cell.bg)))
        else {
            continue;
        };
        target.fill((x0 - 1, y - 1), (x0 + width, y + 9), paper);
        for mark in CHASE_DAMAGE_MARKS {
            let x = x0 + 8 * mark as i32;
            target.fill((x, y - 8), (x + 8, y), paper);
        }
        let natural: f32 = text.chars().map(|ch| face.advance_at(ch, NAME_EM)).sum();
        let em = if natural > width as f32 { NAME_EM * width as f32 / natural } else { NAME_EM };
        let scale = target.scale;
        let top = y as f32 + (8.0 - em) / 2.0;
        let mut pen = x0 as f32;
        for ch in text.chars() {
            let glyph = face.glyph_at(ch, scale, em);
            if !ch.is_whitespace() {
                let at = ((pen * scale as f32).round() as i32, (top * scale as f32).round() as i32);
                target.glyph(&glyph, at, ink);
            }
            pen += glyph.advance;
        }
    }
}

/// Draws the translation of the page `screen` names (the engine's bytes of the frame; `None` when they name none)
/// over the picture `target`, whose 1x frame is `frame` (`frame_width` pixels a row) with the original's window at
/// `window_x` and the text `cells` the font noted in it. `kept` carries the page from one frame to the next.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    overlay: &Overlay,
    kept: &mut Kept,
    screen: Option<&OnScreen>,
    target: &mut Target<'_>,
    cells: &[TextCell],
    frame: &[Color],
    frame_width: usize,
    window_x: i32,
    character: &dyn Fn(u8) -> Option<char>,
    beneath: &mut BeneathOf<'_>,
    unlettered: Option<&[u8]>,
    marks: &[MarkCell],
) {
    // (the letters a page left to the original, also on the frames that draw no page)
    paint_left(&mut kept.left, target, cells);
    // (the letters of a page written at once that went under another window, beside that window)
    paint_went_under(&mut kept.went_under, target, cells, frame, frame_width, unlettered);
    // (the cells English lines wrote outside their window, also after their page)
    paint_spilled(
        &mut kept.spilled,
        &mut kept.taken,
        target,
        cells,
        frame,
        frame_width,
        beneath,
        unlettered,
    );
    match screen {
        Some(screen) => {
            let before = kept.shown.take();
            let settled = screen.line > 0 || screen.cell > 0 || !matches!(screen.state, 1 | 5);
            // (the engine goes back to state 1 for a few frames between the name and the page's first cell)
            let naming = !settled
                && (screen.state == 5
                    || before
                        .as_ref()
                        .is_some_and(|shown| shown.key == screen.key && shown.naming));
            // A finished page the engine names again from its start while its window waits (the shop's opening page
            // while the player picks `BUY` or `SELL`: state 1, line 0, cell 0) is still the page as written
            let again = !settled
                && before.as_ref().is_some_and(|shown| shown.key == screen.key && shown.finished);
            kept.shown = overlay.text(screen).map(|text| Shown {
                key: screen.key.clone(),
                text,
                table: screen.table,
                string: (screen.table, screen.string),
                cell: {
                    let (column, row) = screen.base_cell();
                    (i32::from(column), i32::from(row))
                },
                portrait: screen.portrait,
                lines: screen.lines.len(),
                widths: screen.lines.clone(),
                longest: screen.lines.iter().copied().max().unwrap_or(0),
                progress: if again { 1.0 } else { screen.progress },
                settled: settled || again,
                instant: screen.state == 0,
                finished: again || matches!(screen.state, 2 | 3 | 6),
                waits: matches!(screen.state, 2 | 3),
                again,
                naming,
                centre: overlay
                    .table
                    .text(&screen.key)
                    .is_some_and(|text| centres(screen.table, text)),
                measured: (screen.state == 0 && !screen.english.is_empty()).then(|| screen.clone()),
                typed: (matches!(screen.state, 1 | 5) && !screen.english.is_empty())
                    .then(|| screen.clone()),
                // (each line's cells its English cells: a name slot the engine does not read counts cells in
                // `lines` and gives no English cell, and the game writes the name there)
                blank: !screen.english.is_empty()
                    && screen.lines.len() == screen.english.len()
                    && screen
                        .lines
                        .iter()
                        .zip(&screen.english)
                        .all(|(&cells, line)| cells == line.len())
                    && screen.english.iter().flatten().all(|cell| cell.is_none_or(|ch| ch == ' ')),
            });
            if kept.shown.as_ref().map(|shown| &shown.key)
                != before.as_ref().map(|shown| &shown.key)
            {
                kept.seen = false;
                kept.blank_drawn = false;
                kept.blank_rows = None;
            }
            // (a page the engine writes, or another page, starts again what its window held when it was finished; so
            // does the same page with other numbers in its slots: the gravity training's choice writes the level it
            // shows into its window, `ssd_catalogue::engine`, and the letters there change with the number)
            if kept.shown.as_ref().map(|shown| &shown.key)
                != before.as_ref().map(|shown| &shown.key)
                || !kept.shown.as_ref().is_some_and(|shown| shown.finished)
                || kept.shown.as_ref().map(|shown| &shown.text)
                    != before.as_ref().map(|shown| &shown.text)
            {
                kept.done.clear();
                kept.done_frames = 0;
                kept.gone = false;
            }
            kept.age = 0;
        }
        None => kept.age += 1,
    }
    let lasting =
        kept.seen && kept.shown.as_ref().is_some_and(|shown| shown.instant || shown.finished);
    if kept.age > KEEP && !lasting {
        kept.shown = None;
    }
    // (a frame that paints no paper ends the paper before: [`Kept::painted`])
    let painted = kept.painted.take();
    let Some(shown) = kept.shown.clone() else {
        // (letters that come while the engine names nothing are another window's; a page named is being written)
        if screen.is_some() {
            kept.under = None;
        } else if let Some(under) = kept.under.as_mut() {
            *under = screen_letters(cells);
        }
        return;
    };
    if kept.gone {
        kept.under = Some(screen_letters(cells));
        return;
    }
    let frame_height = (frame.len() / frame_width.max(1)) as i32;
    let cells_wide = window_cells(shown.table, shown.longest) as i32;
    let left = window_x + 8 * shown.cell.0;
    let top = 8 * shown.cell.1;
    let face = &overlay.face;
    let lines = wrap(&shown.text, (8 * cells_wide) as f32, |ch| face.advance(ch));
    let rows = shown.lines.max(lines.len()) as i32;
    // The window's letters (a blank cell is the window's ground, whose colour 3 may be its frame's): where its text
    // is on the screen (the layer's scroll moves it from its tile-map place, and the battle's window shows its
    // lines closer than the engine writes them), and its colours
    let letters: Vec<&TextCell> = cells
        .iter()
        .filter(|cell| {
            cell.glyph != BLANK
                && cell.glyph < CARD_BASE
                && (left - 8..left + 8 * cells_wide + 8).contains(&cell.x)
                && (top - 2 * PITCH - 4..top + PITCH * (rows + 1)).contains(&cell.y)
        })
        .collect();
    // A page written at once ([`Shown::measured`]) is where its English shows, as its [`presence_in_window`] says on
    // each frame, not as its window's letters say (the checks below, `held`, `went_under`, the
    // place from the window's letters): whole, it is drawn at the place and pitch its letters show at, in their
    // colours; partly, another window drawn over the rest (the cards of a wish of Porunga over the wishes, the HP
    // window over his help, the yes/no window over the Item help), it waits under that window and its letters still
    // showing beside it take their ground ([`paint_beside_window`]); not at all, or its window blank, it went (the
    // window closed, or the next page's). A page with no cell to compare goes as before.
    let measured = match shown.measured.as_ref() {
        Some(page) => {
            let own = kept.place.filter(|place| place.string == shown.string);
            let hint = own.map(|place| (place.at, place.pitch));
            let paper = kept.colours.map(|(_, paper)| paper).filter(|_| own.is_some() && kept.seen);
            let size = (frame_width as i32, frame_height);
            let seen = presence_in_window(page, cells, character, window_x, size, paper, hint);
            match seen.presence {
                Presence::In => {
                    kept.seen = true;
                    kept.place = Some(Place {
                        string: shown.string,
                        at: seen.at,
                        pitch: seen.pitch,
                        cell: shown.cell,
                    });
                    let fg = most(seen.shows.iter().map(|cell| cell.fg));
                    let bg = most(seen.shows.iter().map(|cell| cell.bg));
                    if let Some(colours) = fg.zip(bg) {
                        kept.colours = Some(colours);
                    }
                    true
                }
                Presence::Covered => {
                    kept.seen = true;
                    for cell in &seen.shows {
                        paint_beside_window(target, frame, frame_width, cell, unlettered);
                    }
                    return;
                }
                Presence::Empty | Presence::Out => {
                    kept.shown = None;
                    kept.under = Some(screen_letters(cells));
                    return;
                }
                Presence::NoCells => false,
            }
        }
        None => false,
    };
    // A page the engine writes cell by cell ([`Shown::typed`]) is where its English shows too,
    // and its translation shows as much as the screen shows of its English, not as much as the engine counts written
    // (the engine counts a name or number slot written at once, state 5, as the screen shows it a letter at a time;
    // and the screen shows the cells it wrote a frame or more later). Its letters there: drawn at their place and
    // pitch, in their colours; its window blank (nothing of it shown yet): its paper alone, where its window is.
    // Another window over part of it, it waits under it as a page written at once does, its letters beside it on their
    // ground; not on the screen, it went. A page with no cell to compare goes as before
    let mut progress = shown.progress;
    let measured = measured
        || match shown.typed.as_ref() {
            Some(page) => {
                let own = kept.place.filter(|place| place.string == shown.string);
                let hint = own.map(|place| (place.at, place.pitch));
                let paper =
                    kept.colours.map(|(_, paper)| paper).filter(|_| own.is_some() && kept.seen);
                let size = (frame_width as i32, frame_height);
                let seen = typed_presence(page, cells, character, window_x, size, paper, hint);
                match seen.presence {
                    Presence::In | Presence::Empty => {
                        if seen.presence == Presence::In {
                            kept.seen = true;
                            let fg = most(seen.shows.iter().map(|cell| cell.fg));
                            let bg = most(seen.shows.iter().map(|cell| cell.bg));
                            if let Some(colours) = fg.zip(bg) {
                                kept.colours = Some(colours);
                            }
                        }
                        kept.place = Some(Place {
                            string: shown.string,
                            at: seen.at,
                            pitch: seen.pitch,
                            cell: shown.cell,
                        });
                        progress = seen.own as f32 / seen.letters.max(1) as f32;
                        true
                    }
                    Presence::Covered => {
                        kept.seen = true;
                        for cell in &seen.shows {
                            paint_beside_window(target, frame, frame_width, cell, unlettered);
                        }
                        return;
                    }
                    Presence::Out => {
                        kept.shown = None;
                        kept.under = Some(screen_letters(cells));
                        return;
                    }
                    Presence::NoCells => false,
                }
            }
            None => false,
        };
    if !measured && letters.is_empty() && kept.age > 0 && kept.seen {
        // The window was cleared or closed while the engine named nothing
        kept.shown = None;
        kept.under = Some(Vec::new());
        return;
    }
    // A page the engine names (not one written at once, whose letters come after it names it) while its window holds
    // only the letters of another window drawn in its place after the page before went ([`Kept::under`]): the engine
    // writes it where the screen does not show it (Krillin's `battle:23` again for the next Saibaman, the status box
    // on the screen, seen in a play-test video). Nothing is drawn: no paper, and the other window's letters keep
    // their picture. A letter of its own (the window drawn again) shows the page
    if !measured && kept.age == 0 && !shown.instant && !kept.strict && !letters.is_empty() {
        if let Some(under) = &kept.under
            && letters.iter().all(|cell| under.contains(&(cell.x, cell.y, cell.glyph)))
        {
            kept.gone = true;
            return;
        }
        kept.under = None;
    }
    // The page's letters: those of its window when the engine last named the page. A page the engine wrote out and
    // finished, kept while the engine names nothing, is on the screen only while its window still holds all of them:
    // a window with fewer is another window drawn over the place (the command window and the hand after a finished
    // talk, the battle's status boxes after a KI attack's message, each with letters of its own there), and the page
    // goes. (Letters and digits only: a blinking mark that waits for A comes and goes. A page written at once is
    // named before its window is drawn again, the last page's letters still there: its letters come later.)
    if !measured && !letters.is_empty() && (!shown.instant || kept.strict) {
        let mut glyphs: Vec<u8> = letters
            .iter()
            .map(|cell| cell.glyph)
            .filter(|&glyph| character(glyph).is_some_and(char::is_alphanumeric))
            .collect();
        glyphs.sort_unstable();
        // (a page behind takes the letters of its window as they are once it settled, [`KEEP`] frames after the engine
        // last named it: a window over its place that closes in those frames leaves the page its own letters, not
        // that window's. The enemies' HP window of Porunga's third wish is still up when the engine writes the help
        // again and names the wishes, and closes a few frames later: the help went, its English back)
        // A finished page the engine still names: a window with fewer of its letters is another window drawn in
        // its place while the engine names it (the battle's status box, HP / KI / BP and their numbers, over a KI
        // attack's message): the page is gone, and nothing is drawn over that window. Its letters are those of its
        // window until it settled ([`KEEP`] frames): a window that takes its place in those frames (the status box
        // when an A cuts the timed wait of Frieza's `battle:49` short) has fewer of the letters
        // the page ended with, and the page goes too, not taking that window's letters for its own
        if kept.age == 0 && shown.finished && !kept.strict {
            if !contains_all(&glyphs, &kept.done) {
                kept.gone = true;
                kept.under = Some(screen_letters(cells));
                return;
            }
            if kept.done_frames <= KEEP {
                kept.done.clone_from(&glyphs);
            }
            kept.done_frames += 1;
        }
        if kept.age == 0 || (kept.strict && (kept.glyphs.is_empty() || kept.age <= KEEP)) {
            kept.glyphs = glyphs;
        } else if !contains_all(&glyphs, &kept.glyphs) {
            // (a page written at once kept behind: its letters still showing beside the other window take their
            // ground, [`WentUnder`]; those of its own lines on the last frame it was drawn, all of them there)
            if kept.strict
                && shown.instant
                && let Some(place) = kept.place.filter(|place| place.string == shown.string)
            {
                let letters = kept
                    .before
                    .iter()
                    .copied()
                    .filter(|&(x, y, glyph)| {
                        on_its_lines(&shown, place.at, place.pitch, (x, y))
                            && character(glyph)
                                .is_some_and(|ch| ch.is_alphanumeric() || ch.is_ascii_punctuation())
                    })
                    .collect();
                kept.went_under = Some(WentUnder { key: shown.key.clone(), letters });
            }
            kept.shown = None;
            kept.under = Some(screen_letters(cells));
            return;
        }
    }
    // A page written at once that went under another window still up over its place ([`WentUnder`]), named again
    // (Porunga's help, written again for 15 frames while the enemies' HP window of his third wish is still up, then
    // behind the wishes until it closes): its window holds that window's letters, not its own, and the page waits under
    // it as it does once it settled (below): nothing is drawn, and its letters beside that window keep their ground
    // ([`paint_went_under`]). Its letters are not those of its window then. When that window closes, it is drawn
    if !measured
        && shown.instant
        && kept.went_under.as_ref().is_some_and(|under| under.key == shown.key)
    {
        kept.seen |= !letters.is_empty();
        kept.held.clear();
        return;
    }
    // A page written at once whose window settled, and now holds fewer of its letters: another window is drawn over
    // it (the cards a wish of Porunga changes, over the wishes, upstream 0.3.4 on). The page waits under it: nothing
    // is drawn, and those of its letters still showing beside the other window take their ground (`1'Ma`, `2'Le`
    // left of the cards). When its letters are back, it is drawn again.
    if !measured && shown.instant && !kept.strict && !letters.is_empty() {
        let mut glyphs: Vec<u8> = letters
            .iter()
            .map(|cell| cell.glyph)
            .filter(|&glyph| character(glyph).is_some_and(char::is_alphanumeric))
            .collect();
        glyphs.sort_unstable();
        if kept.age <= KEEP {
            kept.held = glyphs;
        } else if !kept.held.is_empty() && !contains_all(&glyphs, &kept.held) {
            // (its letters and digits, and the marks between them: the `'` of `1'Ma`; not the cursor before them. Those
            // on its own lines where they were on the last frame it was drawn, as [`WentUnder`] keeps them: not the
            // letters of the window over it that are the same glyphs, the names, `HP`, `KI`, numbers and frame of
            // the party list of Porunga's second wish)
            let place = kept.place.filter(|place| place.string == shown.string);
            let its = |cell: &TextCell| {
                place.is_some_and(|place| {
                    on_its_lines(&shown, place.at, place.pitch, (cell.x, cell.y))
                }) && kept.before.contains(&(cell.x, cell.y, cell.glyph))
                    && character(cell.glyph)
                        .is_some_and(|ch| ch.is_alphanumeric() || ch.is_ascii_punctuation())
            };
            for cell in letters.iter().filter(|cell| its(cell)) {
                paint_beside_window(target, frame, frame_width, cell, unlettered);
            }
            return;
        }
    }
    kept.seen |= !letters.is_empty();
    // (a page with no letter has none to show again: the choice after `str3:0x5D`, its window holding the cursor
    // alone, below)
    if shown.again && letters.is_empty() && !shown.blank {
        return;
    }
    if !measured && shown.naming && kept.place.is_none_or(|place| place.string != shown.string) {
        // The name the engine writes before the page's first cell: its letters, on the engine's first line, go
        for cell in letters.iter().filter(|cell| (top - PITCH..top + PITCH).contains(&cell.y)) {
            target.fill((cell.x - 1, cell.y - 1), (cell.x + 9, cell.y + 9), cell.bg);
        }
        return;
    }
    // (only once the engine has written some of the page: a window that opens may still scroll the last text. Not a
    // page with no letter: letters there are not its own, below)
    let first = letters
        .iter()
        .map(|cell| cell.y)
        .min()
        .filter(|_| shown.settled && !measured && !shown.blank);
    if let Some(first) = first {
        let dx = most(letters.iter().map(|cell| off_grid(cell.x, left, 8))).unwrap_or(0);
        let mut ys: Vec<i32> = letters.iter().map(|cell| cell.y).collect();
        ys.sort_unstable();
        ys.dedup();
        let known = kept.place.filter(|place| place.string == shown.string);
        let pitch = ys
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .filter(|gap| (10..=2 * PITCH).contains(gap))
            .min()
            .or(known.map(|place| place.pitch))
            .unwrap_or(PITCH);
        // The text's first line: the row of letters nearest the engine's first, and the rows a line apart above it
        // (a window above the text, its letters a line or more off that run, is not the page's)
        let mut first = ys.iter().copied().min_by_key(|&y| ((y - top).abs(), y)).unwrap_or(first);
        while ys.binary_search(&(first - pitch)).is_ok() {
            first -= pitch;
        }
        // A page whose English starts with blank lines (Gohan's `str3:0x75`, `\nYou have found\nthe Dragonballs!`)
        // has no letter there: its first line is that many lines above its first letters, but never above the
        // engine's first line
        let blank = shown.widths.iter().take_while(|&&cells| cells == 0).count() as i32;
        first -= pitch * blank.min((first - top + pitch / 2).div_euclid(pitch)).max(0);
        kept.place =
            Some(Place { string: shown.string, at: (left + dx, first), pitch, cell: shown.cell });
        let fg = most(letters.iter().map(|cell| cell.fg));
        let bg = most(letters.iter().map(|cell| cell.bg));
        kept.colours = fg.zip(bg);
    }
    // A page whose English shows no letter ([`Shown::blank`]: the English patch left `str2:0x113` and `str2:0x13D`
    // their portrait and nothing else, `F4 24 FF FF`, and `str3:0x5D` a `…` the HD font has no glyph for) never shows
    // where its window's text is, though its window opens, waits for A and closes as the Japanese ROM's does, which
    // writes the page there. Once the engine has written it, and while its window is up (at least half the cells of
    // the engine's first line are the window's blank ground, of one colour, [`blank_ground`]; a closed window shows
    // the field or the fight there), it is drawn on that ground's row and grid, the engine's pitch apart, on paper of
    // the ground's colour, in the ink of the window's last page (else the ink measured in that window,
    // [`blank_ink`]): also as the first page of its message, no page of its window seen before. Its translation is whole
    // from the first frame it is drawn (the English ROM lets A turn such a page about 10 frames
    // after its window opens, and a page typed a character every 4 frames, as the Japanese ROM types it, lost what was
    // not typed yet; the Japanese ROM's A hurries the typing, it skips nothing), and its `▼` comes with the original's
    // (after its last character, [`draw`]'s mark). Its window's ground gone, it ends; so does it
    // once the engine names it no more and another window takes its place, its ground running to other rows
    // ([`ground_rows`]: the taller dialogue window of `str2:0x0F1` where the story window of `str3:0x5E` was)
    let mut blank_whole = false;
    if shown.blank && shown.settled {
        let ground = blank_ground(cells, (left, top), cells_wide);
        let rows = ground.map(|(at, _)| ground_rows(cells, at));
        let another = kept.age > 0 && kept.blank_rows.is_some_and(|before| Some(before) != rows);
        let Some((at, paper)) = ground.filter(|_| !another) else {
            if kept.blank_drawn {
                kept.shown = None;
            }
            return;
        };
        if kept.age == 0 {
            kept.blank_rows = rows;
        }
        let window =
            kept.place.filter(|place| place.string.0 == shown.table && place.cell == shown.cell);
        let Some(ink) =
            window.and(kept.colours).map(|(ink, _)| ink).or_else(|| blank_ink(shown.table))
        else {
            return;
        };
        kept.place = Some(Place { string: shown.string, at, pitch: PITCH, cell: shown.cell });
        kept.colours = Some((ink, paper));
        kept.blank_drawn = true;
        blank_whole = true;
    }
    // No letter of this string was seen yet: the original shows none of it either. A window seen before (the place of
    // another string is known) may still hold that string's letters while the engine starts this one (the shop's
    // `"Anything else?"` named again from its start): they take their ground, so no English shows meanwhile. (A
    // window never seen is left alone: one that opens may still show the last text, scrolling.)
    let Some(place) = kept.place.filter(|place| place.string == shown.string) else {
        if kept.place.is_some() {
            for cell in &letters {
                target.fill((cell.x - 1, cell.y - 1), (cell.x + 9, cell.y + 9), cell.bg);
            }
        }
        return;
    };
    let Some((ink, paper)) = kept.colours else { return };
    let ((x0, y0), pitch) = (place.at, place.pitch);
    // The translation's first line: centred in its window's paper ([`centres`], [`centred`]), else on the
    // English's first line
    let centring = shown.centre.then(|| centred(shown.table, y0, &lines, pitch)).flatten();
    let zh_y0 = centring.unwrap_or(y0);
    // The original's letters go: the text's lines, from the hanging cell of a portrait's first line to the
    // window's width; and the translation's lines take the window's paper. The paper reaches the original's last
    // line (its HD letters' cells, a pixel over and under the tile row) or the translation's (an em, from [`RISE`]
    // above its row), whichever is lower: not an em under as many lines as the longer of the two (a page whose
    // English has a blank first line, or more lines than the translation, ran the paper over the window's frame)
    let fit = |y: i32| ((frame_height - y + pitch - 1) / pitch).max(0);
    let english_rows = (shown.lines as i32).min(fit(y0));
    let zh_rows = (lines.len() as i32).min(fit(zh_y0));
    let paper_top = y0.min(zh_y0) - RISE;
    let paper_bottom =
        (y0 + pitch * (english_rows - 1) + 9).max(zh_y0 + pitch * (zh_rows - 1) + EM as i32 - RISE);
    let hang = if shown.portrait { 8 } else { 0 };
    let right = (x0 + 8 * cells_wide).min(frame_width as i32);
    // A cell of the window with no text tile, a blank cell left of it and a letter right of it, is the cursor before
    // a choice (the shop's `▶YES` under `"Anything else?"`, `▶BUY`): it keeps the picture, as the cursor of a name
    // does ([`draw_name`]). A cell with no text tile right after a letter is a mark of the text the font has no
    // glyph for (the `…` of `way…"`, the third line of Vegeta's retreat), not a cursor: it takes the ground too
    let blanks: std::collections::HashSet<(i32, i32)> =
        cells.iter().filter(|cell| cell.glyph == BLANK).map(|cell| (cell.x, cell.y)).collect();
    let tiled: std::collections::HashSet<(i32, i32)> =
        cells.iter().map(|cell| (cell.x, cell.y)).collect();
    // A page centred in its window has no choice (its window does not close by itself, [`centres`]): a cell with no
    // tile after a blank one there is a mark of its text the font has no glyph for (the opening `"` of Gohan's
    // `str2:0x10E/p3` after the portrait's blank cell), whose line the translation no longer covers. A page of the
    // battle's window that is not centred is a choice ([`centres`]): there a cell with no tile before a letter is the
    // cursor whatever is left of it (`battle:317`'s `continue?▶No!`, the cursor right after the `?`: its `No!` lost
    // the cursor)
    let battle_choice = shown.table == 1 && !shown.centre;
    let mut cursors: Vec<(i32, i32)> = letters
        .iter()
        .filter(|_| centring.is_none() && !shown.blank)
        .map(|cell| (cell.x - 8, cell.y))
        .filter(|&(x, y)| {
            !tiled.contains(&(x, y)) && (battle_choice || blanks.contains(&(x - 8, y)))
        })
        .collect();
    // A page with no letter has none for a cursor to come before: a cell of its paper with no text tile, a blank cell
    // left of it, that is not a cell of its English lines (the `…` of `str3:0x5D`) nor the cell after one (its `▼`),
    // is the cursor of the choice after it (`str3:0x5D`'s `▶` before 是 / 否, as the game shows it with no word)
    if shown.blank && centring.is_none() {
        let english = |(x, y): (i32, i32)| {
            shown.widths.iter().enumerate().any(|(row, &width)| {
                y == y0 + pitch * row as i32 && x < x0 + 8 * (width as i32 + 1)
            })
        };
        for y in (0..).map(|row| y0 + 8 * row).take_while(|&y| y < paper_bottom) {
            for x in (0..cells_wide).map(|column| x0 + 8 * column) {
                if !tiled.contains(&(x, y)) && blanks.contains(&(x - 8, y)) && !english((x, y)) {
                    cursors.push((x, y));
                }
            }
        }
    }
    // (the `▼` the original shows after the page's last letter while it waits for A, before the paper takes its cell:
    // it goes after the translation, [`continue_mark`])
    let mark = continue_mark(
        shown.waits,
        &MarkAt {
            widths: &shown.widths,
            at: (x0, y0),
            pitch,
            map_left: x0 - 8 * shown.cell.0,
            marks,
        },
    );
    target.fill_but((x0 - hang, paper_top), (right, paper_bottom), paper, &cursors);
    // The engine names a window's next page frames before the game clears the window, which still shows the page
    // before (Vegeta's `str2:0x0BC/p4` after `/p3`, frames 2498 to 2503 of `secret-zarbon-vegeta-joins`): the paper
    // of the next page alone (one line, centred) left the lowest rows of `/p3`'s third English line, `Thanks for
    // your`, under it. Until the engine writes the next page's first cell, the paper of the page before stays (not its text)
    let before = painted.filter(|before| !shown.settled && before.cell == shown.cell);
    if let Some(before) = &before {
        target.fill_but(before.from, before.to, paper, &cursors);
    }
    kept.painted = Some(before.unwrap_or(Painted {
        cell: shown.cell,
        from: (x0 - hang, paper_top),
        to: (right, paper_bottom),
    }));
    // A line of the original longer than the window runs on past its frame (the battle's `power level of 710BP!`
    // under the portrait beside it), and a line that runs past the tile map's 32 columns goes on at the start of the
    // tile-map row under it (Gohan's `"Vegeta! You're on Namek?"`: `k?"` and the `▼` left of the window). Those cells
    // take the picture beside them (the screen, the frame's side, the window's paper), so no English shows there,
    // and keep it on the pages after while the original still shows them as they were
    let past_frame = spill_of(&SpillWalk {
        cells,
        widths: &shown.widths[..shown.widths.len().min(english_rows as usize)],
        at: (x0, y0),
        pitch,
        hang,
        right,
        map_left: x0 - 8 * shown.cell.0,
        painted: (paper_top, paper_bottom),
        waiting: shown.finished,
        marks,
    });
    for &at in &past_frame {
        let Some(print) = cell_print(frame, frame_width, at) else { continue };
        match kept.spilled.iter_mut().find(|spilled| (spilled.0, spilled.1) == at) {
            Some(spilled) => spilled.2 = print,
            None => kept.spilled.push((at.0, at.1, print)),
        }
    }
    // A letter of the text font on the rows of a dialogue or story page (its paper's), left of its window or from its
    // right edge on, is an English line's spill though no frame of the page that wrote it was drawn: the cells above
    // find it only when that page was drawn (Gohan's `"Vegeta! You're on Namek?"`, `str2:0x0BC/p2`, leaves `k?` left of
    // the window for `/p3`; a frame of `/p2` that never reached the overlay left it there). Not a page whose window
    // closes by itself (`{auto}`: the shop's list, a choice beside it) nor another table's (the battle's status box),
    // nor a cell another window drew over (`taken`). Left of the window is left of its frame or on it
    // ([`WINDOW_LEFT_FRAME`]): the card shop's name under the shopkeeper's portrait, right of the frame, is the
    // window's and keeps its letters. The wish window's pages too (table 4, Porunga's: `und` of `You have found` and
    // `s.` of `Dragon Balls.` right of the window for the pages after `str4:0x03`); left of it, from the same column
    // on, is the portrait's window, which keeps its picture. A `▼` tile right after such a letter ([`mark_at`]) is
    // the `▼` of that line, outside the window's paper
    if (matches!(shown.table, 2 | 3) && shown.centre) || shown.table == 4 {
        let left_of_window = (x0 - hang).min(x0 - 8 * shown.cell.0 + 8 * (WINDOW_LEFT_FRAME + 1));
        let letters: Vec<(i32, i32)> = cells
            .iter()
            .filter(|cell| {
                !kept.taken.contains(&(cell.x, cell.y))
                    && cell.glyph != BLANK
                    && cell.glyph < CARD_BASE
                    && (paper_top..paper_bottom).contains(&cell.y)
                    && (cell.x < left_of_window || cell.x >= right)
            })
            .map(|cell| (cell.x, cell.y))
            .collect();
        let after = letters
            .iter()
            .filter_map(|&(x, y)| mark_at(marks, (x + 8, y)))
            .filter(|at| !kept.taken.contains(at))
            .filter(|&(x, y)| {
                !((x0 - hang..right).contains(&x) && (paper_top..paper_bottom).contains(&y))
            });
        let found: Vec<(i32, i32)> = letters.iter().copied().chain(after).collect();
        for at in found {
            let Some(print) = cell_print(frame, frame_width, at) else { continue };
            match kept.spilled.iter_mut().find(|spilled| (spilled.0, spilled.1) == at) {
                Some(spilled) => spilled.2 = print,
                None => kept.spilled.push((at.0, at.1, print)),
            }
        }
    }
    paint_spilled(
        &mut kept.spilled,
        &mut kept.taken,
        target,
        cells,
        frame,
        frame_width,
        beneath,
        unlettered,
    );
    // A page's English line that ran past the window's frame is not cleared by the next pages, which write no letter
    // there (Porunga's `You have found` / `Dragon Balls.`, wider than his window, leave `und` and `s.` beside the
    // frame for the narrower pages after it). The letters a page of this window covered that this page does not, and
    // that are still on the screen as they were, take the ground the original shows above them (the screen beside
    // the frame, or the window's paper inside it)
    let covers = |x: i32, y: i32| {
        ((x0 - hang..right).contains(&x)
            && (y0.min(zh_y0) - pitch / 2
                ..(y0 + pitch * english_rows).max(zh_y0 + pitch * zh_rows))
                .contains(&y))
            || past_frame.contains(&(x, y))
    };
    let window = (shown.table, shown.cell);
    let same_window = kept.covered_in == Some(window);
    if !same_window {
        kept.covered.clear();
    }
    // (a cell written over with another letter is another window's from then on, never a page's left letter again; a
    // blank one, the `▼` blinking, stays)
    kept.covered.retain(|&(x, y, glyph)| {
        covers(x, y)
            || !cells.iter().any(|cell| {
                (cell.x, cell.y) == (x, y) && cell.glyph != BLANK && cell.glyph != glyph
            })
    });
    let mut left = Vec::new();
    for &(x, y, glyph) in &kept.covered {
        // (a cell an English line spilled out of its window takes the picture beside it: [`paint_spilled`])
        if covers(x, y)
            || !cells.iter().any(|cell| (cell.x, cell.y, cell.glyph) == (x, y, glyph))
            || kept.spilled.iter().any(|spilled| (spilled.0, spilled.1) == (x, y))
        {
            continue;
        }
        let above = usize::try_from(y - 4)
            .ok()
            .zip(usize::try_from(x + 4).ok())
            .and_then(|(row, column)| frame.get(row * frame_width + column));
        let Some(ground) = above.map(|colour| [colour.r, colour.g, colour.b]) else { continue };
        target.fill((x, y - 1), (x + 8, y + 9), ground);
        left.push((x, y, glyph, ground));
    }
    // (a letter that came or changed on the screen since the last frame, on the cells of the page's own lines as wide
    // as the original writes them; any tile of the text with no letter too; the window's first frame only says what
    // was there before it)
    let on_its_lines = |x: i32, y: i32| on_its_lines(&shown, (x0, y0), pitch, (x, y));
    let now: Vec<(i32, i32, u8)> = cells
        .iter()
        .filter(|cell| cell.glyph != BLANK)
        .map(|cell| (cell.x, cell.y, cell.glyph))
        .collect();
    if same_window {
        for &(x, y, glyph) in now.iter().filter(|&&(x, y, glyph)| {
            covers(x, y) && on_its_lines(x, y) && !kept.before.contains(&(x, y, glyph))
        }) {
            match kept.covered.iter_mut().find(|seen| (seen.0, seen.1) == (x, y)) {
                Some(seen) => seen.2 = glyph,
                None => kept.covered.push((x, y, glyph)),
            }
        }
    }
    kept.before = now;
    kept.covered_in = Some(window);
    kept.left = left;
    // The translation, as far as the engine has written (as the screen shows the English of a page written cell by
    // cell). A number shows whole: the digits after the last one drawn come with it (the gravity training's `10x`
    // shows a digit at a time; its share of the English reached the middle of the translation's `10`, and `1` alone
    // showed where the original shows `10`)
    let total = lines.iter().flatten().filter(|ch| !ch.is_whitespace()).count();
    // (a page with no letter: whole)
    let mut left_to_draw =
        if blank_whole { total } else { (progress * total as f32).ceil() as usize };
    let scale = target.scale;
    let mut last = None;
    'text: for (row, line) in lines.iter().enumerate().take(zh_rows as usize) {
        let mut pen = 0.0;
        for &ch in line {
            if left_to_draw == 0
                && !(ch.is_ascii_digit() && last.is_some_and(|last: char| last.is_ascii_digit()))
            {
                break 'text;
            }
            let glyph = face.glyph(ch, scale);
            if !ch.is_whitespace() {
                let x = ((x0 as f32 + pen) * scale as f32).round() as i32;
                let y = (zh_y0 - RISE + pitch * row as i32) * scale as i32;
                target.glyph(&glyph, (x, y), ink);
                left_to_draw = left_to_draw.saturating_sub(1);
                last = Some(ch);
            }
            pen += glyph.advance;
        }
    }
    if let Some(mark) = mark
        && let Some(at) = after_translation(
            face,
            &lines[..lines.len().min(zh_rows as usize)],
            x0,
            zh_y0,
            pitch,
            scale,
        )
    {
        paint_continue_mark(target, &mark, at);
    }
}

/// What [`continue_mark`] needs of a page [`draw`] draws: the cells of each of the page's English lines, the screen's
/// pixel of its first line's first cell, the pixels from one line to the next, the screen's pixel of the tile map's
/// first column, and the cells of the frame that show the original's `▼` ([`mark_cells`]).
struct MarkAt<'a> {
    widths: &'a [usize],
    at: (i32, i32),
    pitch: i32,
    map_left: i32,
    marks: &'a [MarkCell],
}

/// The picture of the `▼` the original shows after the last cell of a page that waits for A, while the engine names it
/// (`message_state` 2 or 3: `01:CDB5` and `01:CDDB` call `01:D1DC` every frame, which on every eighth frame writes the
/// `▼` in one of its two colours, map entry `$292A` or `$2929` ([`MARK_TILES`]), at `message_page_end`, the cell after
/// the page's last character; past the tile map's 32 columns that cell is at the start of the tile-map row under the
/// line, [`spill_of`]), and on the frames after while the page is kept and the screen still shows it (the engine names
/// the next page before the game clears the window: `waits`, as the engine last named the page, [`Shown::waits`]).
/// Only a cell of the frame's tile map that holds the `▼` is one ([`mark_cells`]): `None` where it holds anything else
/// (the screen beside the window or a portrait's corner, where an English line ran past the window, before the engine
/// writes the `▼`; a blank tile; a letter), for a page being written (state 1), written at once (the Item screen's
/// help, state 0) or timed (state 6, `01:CF11`) when the engine last named it. The picture is the cell's as drawn
/// before the overlay painted anything (the HD art's `▼`: the overlay paints a cell an English line spilled out of
/// its window, [`paint_spilled`], before the page), `8 * scale` pixels square, 3 bytes a pixel.
fn continue_mark(waits: bool, page: &MarkAt<'_>) -> Option<Vec<u8>> {
    if !waits {
        return None;
    }
    let MarkAt { widths, at: (x0, y0), pitch, map_left, marks } = *page;
    let row = widths.len().checked_sub(1)?;
    let along = x0 + 8 * widths[row] as i32;
    let line = y0 + pitch * row as i32;
    let (x, y) = if along >= map_left + 256 { (along - 256, line + 8) } else { (along, line) };
    // (a few pixels off the line too: the battle's lines closer than written)
    let mark = marks.iter().find(|mark| mark.at.0 == x && (y - 4..=y + 4).contains(&mark.at.1))?;
    Some(mark.picture.clone())
}

/// The picture's pixel where the `▼` goes after the translation `lines` drawn from (`x0`, `zh_y0`), `pitch` pixels a
/// line, at `scale`: the top left of its cell right after the last character of the last line with one, the cell's
/// bottom on that line's em's. (The original writes the `▼` in the cell after its last character whatever is there,
/// the window's frame too; a line of the translation is never wider than its window, short of the tile map's last
/// column, so it is never moved to another line.)
fn after_translation(
    face: &ChineseFace,
    lines: &[Vec<char>],
    x0: i32,
    zh_y0: i32,
    pitch: i32,
    scale: usize,
) -> Option<(i32, i32)> {
    let (row, line) = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.iter().any(|ch| !ch.is_whitespace()))?;
    let end = line.iter().rposition(|ch| !ch.is_whitespace())?;
    let pen: f32 = line[..=end].iter().map(|&ch| face.glyph(ch, scale).advance).sum();
    let x = ((x0 as f32 + pen) * scale as f32).round() as i32;
    let em_bottom = zh_y0 - RISE + pitch * row as i32 + EM as i32;
    Some((x, (em_bottom - 8) * scale as i32))
}

/// A cell of the original's `▼` ([`continue_mark`]): where the frame shows it (1x pixels) and its picture before the
/// overlay draws anything, `8 * scale` pixels square, 3 bytes a pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkCell {
    pub at: (i32, i32),
    pub picture: Vec<u8>,
}

/// The tiles of the `▼` `01:D1DC` writes after a page that waits for A: map entries `$292A` and `$2929` (the same
/// mark in two colours, each eight frames), tiles `$12A` and `$129`.
const MARK_TILES: [u16; 2] = [0x12A, 0x129];

/// The cells of the tile `rows` (the screen's, the original's window at `window_x`) that hold the `▼`
/// ([`MARK_TILES`]), with their pictures in `target` as it is now.
pub(crate) fn mark_cells(rows: &[TileRow], window_x: i32, target: &Target<'_>) -> Vec<MarkCell> {
    let mut marks = Vec::new();
    for row in rows {
        for (column, tile) in row.tiles.iter().enumerate() {
            if !MARK_TILES.contains(tile) {
                continue;
            }
            let at = (window_x + row.x + 8 * column as i32, row.y);
            if let Some(picture) = cell_picture(target, at) {
                marks.push(MarkCell { at, picture });
            }
        }
    }
    marks
}

/// The picture of the 1x cell whose top left is `(x, y)`, `8 * scale` pixels square, 3 bytes a pixel; `None` for a
/// cell not wholly in the picture.
fn cell_picture(target: &Target<'_>, (x, y): (i32, i32)) -> Option<Vec<u8>> {
    let s = target.scale as i32;
    let (width, height) = (target.width as i32, target.height() as i32);
    if x < 0 || y < 0 || (x + 8) * s > width || (y + 8) * s > height {
        return None;
    }
    let mut pixels = Vec::with_capacity((8 * s * 8 * s * 3) as usize);
    for row in y * s..(y + 8) * s {
        for column in x * s..(x + 8) * s {
            let at = (row * width + column) as usize * 4;
            pixels.extend_from_slice(&target.rgba[at..at + 3]);
        }
    }
    Some(pixels)
}

/// Puts the picture of the `▼` ([`continue_mark`]) with its top left at the picture's pixel `at`.
fn paint_continue_mark(target: &mut Target<'_>, mark: &[u8], (x, y): (i32, i32)) {
    let side = 8 * target.scale as i32;
    let (width, height) = (target.width as i32, target.height() as i32);
    for row in 0..side {
        for column in 0..side {
            let (px, py) = (x + column, y + row);
            if !(0..width).contains(&px) || !(0..height).contains(&py) {
                continue;
            }
            let from = ((row * side + column) * 3) as usize;
            let to = (py * width + px) as usize * 4;
            target.rgba[to..to + 3].copy_from_slice(&mark[from..from + 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(wrapped: &[Vec<char>]) -> Vec<String> {
        wrapped.iter().map(|line| line.iter().collect()).collect()
    }

    #[test]
    fn a_line_wraps_at_the_width_and_keeps_its_own_breaks() {
        let wrapped = wrap("一二三四五\n六", 36.0, |_| 12.0);
        assert_eq!(lines(&wrapped), ["一二三", "四五", "六"]);
    }

    #[test]
    fn a_closing_mark_never_starts_a_line_and_an_opening_one_never_ends_one() {
        assert_eq!(lines(&wrap("一二三。四", 36.0, |_| 12.0)), ["一二三。", "四"]);
        assert_eq!(lines(&wrap("一二「三四", 36.0, |_| 12.0)), ["一二", "「三四"]);
        assert_eq!(lines(&wrap("一二！？三", 24.0, |_| 12.0)), ["一二！？", "三"]);
    }

    #[test]
    fn narrow_letters_take_their_own_width_and_a_wrapped_line_drops_its_leading_space() {
        let advance = |ch: char| if ch.is_ascii() { 6.0 } else { 12.0 };
        assert_eq!(lines(&wrap("得到120點", 36.0, advance)), ["得到", "120點"]);
        assert_eq!(lines(&wrap("12345678", 24.0, advance)), ["1234", "5678"], "too long: cut");
        assert_eq!(lines(&wrap("ab cd", 12.0, advance)), ["ab", "cd"]);
    }

    #[test]
    fn the_offset_from_the_grid_is_the_nearest_one() {
        assert_eq!(off_grid(185, 184, 16), 1);
        assert_eq!(off_grid(199, 184, 16), -1);
        assert_eq!(off_grid(184 + 32, 184, 16), 0);
    }

    const PAPER: [u8; 3] = [0xFF, 0xEF, 0xCE];
    const INK: [u8; 3] = [0x00, 0x00, 0x31];

    /// An overlay that draws the made-up page `str3:0x00` ("AB" / "C") with the Latin font of the HD text (the tests
    /// have no Chinese font).
    fn overlay() -> Overlay {
        let table = Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        Overlay::new(Arc::new(table), Arc::new(face))
    }

    /// The story window's first page of string 0, `progress` written.
    fn screen(progress: f32) -> OnScreen {
        OnScreen {
            table: 3,
            string: 0,
            page: 0,
            key: "str3:0x00".to_owned(),
            state: 1,
            stopped: false,
            line: 0,
            cell: 1,
            base: 0x05C8,
            portrait: false,
            lines: vec![9, 4],
            english: Vec::new(),
            progress,
            names: [None, None, None],
            numbers: Default::default(),
        }
    }

    /// A letter of the original at the text's first cell (4, 23).
    fn letter() -> TextCell {
        TextCell { x: 32, y: 184, glyph: 1, fg: INK, bg: PAPER, mask: !0, ..TextCell::default() }
    }

    /// Draws a frame at scale 2 over a grey picture and a 1x frame of paper.
    fn drawn(
        overlay: &Overlay,
        kept: &mut Kept,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
    ) -> Vec<u8> {
        drawn_on(overlay, kept, screen, cells, &vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224])
    }

    /// Draws a frame as [`drawn`] does, over the 1x frame `frame` (256 pixels a row).
    fn drawn_on(
        overlay: &Overlay,
        kept: &mut Kept,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
        frame: &[Color],
    ) -> Vec<u8> {
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw(
            overlay,
            kept,
            screen,
            &mut target,
            cells,
            frame,
            256,
            0,
            &|glyph| (glyph != BLANK).then_some('a'),
            &mut |_| None,
            None,
            &[],
        );
        rgba
    }

    fn pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 3] {
        let at = (y * 512 + x) * 4;
        [rgba[at], rgba[at + 1], rgba[at + 2]]
    }

    /// Whether a pixel of the text's first line holds ink.
    fn inked(rgba: &[u8]) -> bool {
        (64..128).any(|x| (366..390).any(|y| pixel(rgba, x, y)[2] < 0x80))
    }

    #[test]
    fn a_page_the_original_has_no_letter_of_is_drawn_on_its_windows_blank_ground() {
        // `str2:0x13D`: the English patch left the page its portrait and nothing else (`F4 09 FF FF`), so no letter
        // of it ever shows; the page before it (`str2:0x13C`) was written in the same window
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr3:0x01\tAB{auto}\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let lettered = OnScreen { state: 2, ..screen(1.0) };
        let empty = OnScreen {
            string: 1,
            key: "str3:0x01".to_owned(),
            lines: vec![0],
            english: vec![Vec::new()],
            ..lettered.clone()
        };
        // The window's blank ground across its first line
        let ground: Vec<TextCell> =
            (0..25).map(|n| TextCell { x: 32 + 8 * n, glyph: BLANK, ..letter() }).collect();
        // (a glyph's pixels on the first line: neither the picture's grey nor the paper)
        let written = |rgba: &[u8]| {
            (64..128).any(|x| (366..390).any(|y| ![[7, 7, 7], PAPER].contains(&pixel(rgba, x, y))))
        };
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&lettered), &[letter()]);
        let rgba = drawn(&overlay, &mut kept, Some(&empty), &ground);
        assert!(written(&rgba), "drawn on the first line of the page before");
        // Its window gone (the field under it, no blank cell of the window): nothing is drawn
        let rgba = drawn(&overlay, &mut kept, Some(&empty), &[]);
        assert!(!written(&rgba), "not over a closed window");
        assert_eq!(pixel(&rgba, 2 * 40, 2 * 186), [7, 7, 7], "the picture is left alone");
        // Another window (the engine's first cell elsewhere) does not take this window's place
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&lettered), &[letter()]);
        let elsewhere = OnScreen { base: 0x0548, ..empty.clone() };
        let rgba = drawn(&overlay, &mut kept, Some(&elsewhere), &ground);
        assert!(!written(&rgba), "another window's place");
        // A window no page was seen in (the page is the first of its message, `str2:0x113` in scene 3C): its blank
        // ground places it, and it is drawn there
        let mut fresh = Kept::default();
        let rgba = drawn(&overlay, &mut fresh, Some(&empty), &ground);
        assert!(written(&rgba), "no window seen: drawn on its blank ground");
    }

    /// The overlay of the blank pages: `str3:0x01` "AB" / "C" (a page whose English shows no letter; a choice may come
    /// after it, `{auto}`, as after `str3:0x5D`: not centred, on the English's rows).
    fn blank_overlay() -> Overlay {
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr3:0x01\tAB\\nC{auto}\n")
                .unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        Overlay::new(Arc::new(table), Arc::new(face))
    }

    /// `str3:0x01` as the engine names it, written (its first cell, a portrait's or `…`): its English `english`, one
    /// line of `cells` cells.
    fn blank_page(state: u8, english: Vec<Option<char>>) -> OnScreen {
        OnScreen {
            string: 1,
            key: "str3:0x01".to_owned(),
            state,
            lines: vec![english.len()],
            english: vec![english],
            ..screen(1.0)
        }
    }

    /// The story window's blank ground on the text's first line (from the text's first cell, 25 cells; `from` cells
    /// left out: a `…` or a `▼` there, no text tile) and on its next two lines.
    fn blank_ground(from: i32) -> Vec<TextCell> {
        let mut cells = Vec::new();
        for (row, first) in [(184, from), (200, 0), (216, 0)] {
            cells.extend((first..25).map(|n| TextCell {
                x: 32 + 8 * n,
                y: row,
                glyph: BLANK,
                ..letter()
            }));
        }
        cells
    }

    /// The ink pixels (blue under 0x80; not the grey picture of [`drawn`] nor the cells' of [`drawn_over`], blue 99) of
    /// the 2x picture `rgba` on the translation's first and second lines.
    fn line_ink(rgba: &[u8]) -> (usize, usize) {
        let count = |rows: std::ops::Range<usize>| {
            rows.flat_map(|y| (64..400).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let [_, _, b] = pixel(rgba, x, y);
                    b < 0x80 && b != 7 && b != 99
                })
                .count()
        };
        (count(366..390), count(398..422))
    }

    #[test]
    fn a_page_with_no_letter_is_drawn_whole_from_its_first_frame() {
        // The English ROM shows nothing to type and lets A turn the page about 10 frames after its window opens: the
        // translation is whole on the first frame it is drawn, both lines (not a character every four frames, as
        // the Japanese ROM types it), as the engine writes it (state 1, part of the way) and
        // as it waits for A (state 3), and stays so
        let overlay = blank_overlay();
        for page in
            [OnScreen { progress: 0.2, ..blank_page(1, Vec::new()) }, blank_page(3, Vec::new())]
        {
            let mut kept = Kept::default();
            let inks: Vec<(usize, usize)> = (0..12)
                .map(|_| line_ink(&drawn(&overlay, &mut kept, Some(&page), &blank_ground(0))))
                .collect();
            let (a, c) = inks[0];
            assert!(a > 0 && c > 0, "state {}: `AB` and `C` on the first frame", page.state);
            assert!(
                inks.iter().all(|&ink| ink == inks[0]),
                "state {}: the same on every frame",
                page.state
            );
        }
    }

    #[test]
    fn a_page_with_no_letter_is_not_drawn_before_its_window_is_up_or_the_engine_wrote_it() {
        let overlay = blank_overlay();
        // The window still opening: fewer than half of its first line's cells are its blank ground
        let mut kept = Kept::default();
        let few: Vec<TextCell> =
            blank_ground(0).into_iter().filter(|cell| cell.x < 32 + 8 * 10).collect();
        let rgba = drawn(&overlay, &mut kept, Some(&blank_page(3, Vec::new())), &few);
        assert_eq!(line_ink(&rgba), (0, 0), "a window not up");
        assert_eq!(pixel(&rgba, 2 * 40, 2 * 186), [7, 7, 7], "the picture is left alone");
        // The engine has not written its first cell (state 1, line 0, cell 0)
        let mut kept = Kept::default();
        let unwritten = OnScreen { state: 1, cell: 0, ..blank_page(3, Vec::new()) };
        let rgba = drawn(&overlay, &mut kept, Some(&unwritten), &blank_ground(0));
        assert_eq!(line_ink(&rgba), (0, 0), "not written");
        // The intro and the ending have no English to compare (`english` empty): they are not such pages
        let mut kept = Kept::default();
        let ending = OnScreen { english: Vec::new(), ..blank_page(3, Vec::new()) };
        let rgba = drawn(&overlay, &mut kept, Some(&ending), &blank_ground(0));
        assert_eq!(line_ink(&rgba), (0, 0), "not the ending's pages");
    }

    #[test]
    fn a_page_of_a_mark_the_font_has_no_glyph_for_is_a_page_with_no_letter() {
        // `str3:0x5D`, `…` (no glyph in the HD font: `None`) and nothing else; and a page of spaces
        let overlay = blank_overlay();
        // (as the engine writes it, state 1, and as it waits for A, state 3)
        for english in [vec![None], vec![Some(' '), Some(' ')]] {
            for state in [1, 3] {
                let mut kept = Kept::default();
                let page = blank_page(state, english.clone());
                let rgba =
                    drawn(&overlay, &mut kept, Some(&page), &blank_ground(english.len() as i32));
                assert!(line_ink(&rgba).0 > 0, "{english:?} state {state}: drawn");
                assert_eq!(
                    pixel(&rgba, 2 * 32 + 3, 2 * 184 + 3),
                    PAPER,
                    "{english:?} state {state}: its cell takes the paper"
                );
            }
        }
        // A letter of the HD font is compared: not such a page
        let mut kept = Kept::default();
        let rgba =
            drawn(&overlay, &mut kept, Some(&blank_page(3, vec![Some('a')])), &blank_ground(1));
        assert_eq!(line_ink(&rgba), (0, 0), "a page with a letter");
    }

    #[test]
    fn a_page_whose_name_slot_was_not_read_is_not_a_page_with_no_letter() {
        // `battle:79`, `{name:1}` and nothing else: a name slot `ssd_catalogue::engine` does not read (a type it
        // does not decode) counts its `SLOT_CELLS` cells in `lines` but gives no English cell. The game writes the
        // name there: such a page is not one whose English shows no letter (the name would go out of the Chinese,
        // drawn over the game's letters); a line whose cells are its English cells is
        let overlay = blank_overlay();
        for (lines, english) in [(vec![6], vec![Vec::new()]), (vec![7], vec![vec![None]])] {
            let mut kept = Kept::default();
            let page = OnScreen { lines: lines.clone(), english, ..blank_page(3, Vec::new()) };
            let rgba = drawn(&overlay, &mut kept, Some(&page), &blank_ground(0));
            assert_eq!(line_ink(&rgba), (0, 0), "{lines:?}: a name slot not read");
            assert!(!kept.shown.as_ref().is_some_and(|shown| shown.blank), "{lines:?}: not blank");
        }
        // A portrait alone (no cell), `…` (a cell, no glyph) and spaces: pages with no letter, as before
        for english in [Vec::new(), vec![None], vec![Some(' '), Some(' ')]] {
            let mut kept = Kept::default();
            let page = blank_page(3, english.clone());
            let rgba = drawn(&overlay, &mut kept, Some(&page), &blank_ground(english.len() as i32));
            assert!(line_ink(&rgba).0 > 0, "{english:?}: drawn");
            assert!(kept.shown.as_ref().is_some_and(|shown| shown.blank), "{english:?}: blank");
        }
    }

    #[test]
    fn a_page_with_no_letter_takes_its_windows_ink_else_the_measured_one_and_its_grounds_paper() {
        let overlay = blank_overlay();
        let has = |rgba: &[u8], colour: [u8; 3]| {
            (64..400).any(|x| (366..390).any(|y| pixel(rgba, x, y) == colour))
        };
        // No page of the window seen: the ink measured on the story window's letters, on the ground's colour
        let ground = [0x10, 0x80, 0x10];
        let cells: Vec<TextCell> =
            blank_ground(0).into_iter().map(|cell| TextCell { bg: ground, ..cell }).collect();
        let mut kept = Kept::default();
        let rgba = drawn(&overlay, &mut kept, Some(&blank_page(3, Vec::new())), &cells);
        assert!(has(&rgba, [0, 0, 49]), "the measured ink");
        assert_eq!(pixel(&rgba, 2 * 32 + 3, 2 * 184 + 3), ground, "the ground's paper");
        // A page of the same window before it, in another ink: that ink
        let red = [0xC0, 0, 0];
        let mut kept = Kept::default();
        let lettered = OnScreen { state: 2, ..screen(1.0) };
        drawn(&overlay, &mut kept, Some(&lettered), &[TextCell { fg: red, ..letter() }]);
        let rgba = drawn(&overlay, &mut kept, Some(&blank_page(3, Vec::new())), &blank_ground(0));
        assert!(has(&rgba, red), "the window's ink");
    }

    #[test]
    fn a_page_with_no_letter_ends_when_its_window_goes_or_the_engine_names_it_no_more() {
        let overlay = blank_overlay();
        let page = blank_page(3, Vec::new());
        // Its first line's ground gone (the window closed): it ends, also on the frames after
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&page), &blank_ground(0));
        let rgba = drawn(&overlay, &mut kept, Some(&page), &[]);
        assert_eq!(line_ink(&rgba), (0, 0), "not over the closed window");
        assert_eq!(pixel(&rgba, 2 * 40, 2 * 186), [7, 7, 7], "the picture is left alone");
        assert!(kept.shown.is_none(), "it ended");
        // The engine names nothing: it stays `KEEP` frames, then ends
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&page), &blank_ground(0));
        for frame in 1..=KEEP {
            let rgba = drawn(&overlay, &mut kept, None, &blank_ground(0));
            assert!(line_ink(&rgba).0 > 0, "frame {frame} after: still drawn");
        }
        let rgba = drawn(&overlay, &mut kept, None, &blank_ground(0));
        assert_eq!(line_ink(&rgba), (0, 0), "{} frames after: ended", KEEP + 1);
    }

    #[test]
    fn a_page_with_no_letter_ends_when_another_window_takes_its_place_once_the_engine_names_it_no_more()
     {
        // `str3:0x5E` in scene 15b: after A the engine names nothing for 8 frames, and the game draws the taller
        // dialogue window of `str2:0x0F1` where the story window was; its ground is blank on the page's first line
        // too, but its window is not the page's (its ground runs to other rows): the page ends
        let overlay = blank_overlay();
        let page = blank_page(3, Vec::new());
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&page), &blank_ground(0));
        let rgba = drawn(&overlay, &mut kept, None, &blank_ground(0));
        assert!(line_ink(&rgba).0 > 0, "its window still up: drawn");
        let mut taller = blank_ground(0);
        for row in [168, 176, 192, 208, 224] {
            taller.extend((0..25).map(|n| TextCell {
                x: 32 + 8 * n,
                y: row,
                glyph: BLANK,
                ..letter()
            }));
        }
        let rgba = drawn(&overlay, &mut kept, None, &taller);
        assert_eq!(line_ink(&rgba), (0, 0), "another window: ended");
        assert!(kept.shown.is_none(), "it ended");
        // While the engine names it, its window is its own: a window that grows as it opens does not end it
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&page), &blank_ground(0));
        let rgba = drawn(&overlay, &mut kept, Some(&page), &taller);
        assert!(line_ink(&rgba).0 > 0, "named: still drawn");
    }

    #[test]
    fn the_continue_mark_of_a_page_with_no_letter_comes_after_the_translation_with_the_originals() {
        // The original's `▼` in the page's first cell (its English has none): the translation is whole from the first
        // frame, and the mark comes after its last character, `C`, on the frames the original shows its own (not
        // before: the frames the original shows none, there is none)
        let overlay = blank_overlay();
        let mark = cell_colour(32, 184);
        let page = blank_page(3, Vec::new());
        let lit = frame_with_mark((32, 184));
        let unlit = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut kept = Kept::default();
        for frame in 0..4 {
            let drawn =
                drawn_over_marked(&overlay, &mut kept, Some(&page), &blank_ground(1), &unlit, &[]);
            assert_eq!(
                colour_box(&drawn, mark).0,
                0,
                "frame {frame}: no mark in the original, none"
            );
        }
        let drawn = drawn_over_marked(
            &overlay,
            &mut kept,
            Some(&page),
            &blank_ground(1),
            &lit,
            &[(32, 184)],
        );
        let (count, place) = colour_box(&drawn, mark);
        assert_eq!(count, 16 * 16, "the original's mark: its cell, whole");
        let (x0, y0, _, _) = place.unwrap();
        assert!((2 * 199..2 * 211).contains(&y0), "on `C`'s line ({y0})");
        assert!(x0 > 2 * 32, "after `C` ({x0})");
        assert_ne!(cell_pixel(&drawn, 32, 184), mark, "not where the English had it");
        // From the first frame the page is drawn, with the original's mark
        let mut kept = Kept::default();
        let drawn = drawn_over_marked(
            &overlay,
            &mut kept,
            Some(&page),
            &blank_ground(1),
            &lit,
            &[(32, 184)],
        );
        assert_eq!(colour_box(&drawn, mark).0, 16 * 16, "the first frame: the mark after `C`");
    }

    #[test]
    fn the_cursor_of_a_choice_after_a_page_with_no_letter_keeps_its_picture() {
        // `str3:0x5D`: `…` waits for A; then the game's choice shows its cursor `▶` (no text tile, the window's blank
        // ground left of it) on the page's second line, and the engine names the page again from its start (state 1,
        // line 0, cell 0): the whole translation shows, and the cursor keeps its picture
        let overlay = blank_overlay();
        let waiting = blank_page(3, vec![None]);
        let choice = OnScreen { state: 1, cell: 0, ..waiting.clone() };
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut kept = Kept::default();
        drawn_over(&overlay, &mut kept, Some(&waiting), &blank_ground(1), &frame);
        let cursor = (32 + 8 * 15, 200);
        let cells: Vec<TextCell> =
            blank_ground(1).into_iter().filter(|cell| (cell.x, cell.y) != cursor).collect();
        let drawn = drawn_over(&overlay, &mut kept, Some(&choice), &cells, &frame);
        assert_eq!(
            cell_pixel(&drawn, 32 + 8 * 15, 200),
            cell_colour(32 + 8 * 15, 200),
            "the cursor keeps its picture"
        );
        assert_eq!(
            cell_pixel(&drawn, 32 + 8 * 17, 200),
            PAPER,
            "the ground beside it takes the paper"
        );
        assert_ne!(
            cell_pixel(&drawn, 32, 184),
            cell_colour(32, 184),
            "the `…` goes under the paper"
        );
        let (a, c) = line_ink(&drawn);
        assert!(a > 0 && c > 0, "the whole translation");
    }

    #[test]
    fn the_window_is_painted_over_and_the_text_drawn_as_far_as_the_engine_wrote() {
        let overlay = overlay();
        let mut kept = Kept::default();
        let rgba = drawn(&overlay, &mut kept, Some(&screen(0.0)), &[letter()]);
        assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "the original's letter is gone");
        assert_eq!(pixel(&rgba, 2 * 200, 2 * 200), PAPER, "the second line too");
        assert_eq!(pixel(&rgba, 2 * 240, 2 * 186), [7, 7, 7], "right of the 25 cells: untouched");
        assert!(!inked(&rgba), "nothing is written yet");
        let rgba = drawn(&overlay, &mut kept, Some(&screen(1.0)), &[letter()]);
        assert!(inked(&rgba));
    }

    #[test]
    fn the_text_stays_while_the_window_keeps_letters_and_goes_when_it_is_cleared() {
        let overlay = overlay();
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&screen(1.0)), &[letter()]);
        assert!(inked(&drawn(&overlay, &mut kept, None, &[letter()])), "held between pages");
        let cleared = drawn(&overlay, &mut kept, None, &[]);
        assert_eq!(pixel(&cleared, 2 * 32 + 2, 2 * 184 + 2), [7, 7, 7], "nothing drawn");
        assert!(kept.shown.is_none());
    }

    #[test]
    fn a_page_written_at_once_stays_while_its_window_keeps_letters() {
        let overlay = overlay();
        // A page that was being written goes some frames after the engine stops naming it
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&screen(1.0)), &[letter()]);
        for _ in 0..KEEP {
            drawn(&overlay, &mut kept, None, &[letter()]);
        }
        let gone = drawn(&overlay, &mut kept, None, &[letter()]);
        assert_eq!(
            pixel(&gone, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "gone after the frames it is kept"
        );
        // A page written at once (state 0: the favourite card's message) stays while its window is up
        let mut kept = Kept::default();
        let instant = OnScreen { state: 0, ..screen(1.0) };
        drawn(&overlay, &mut kept, Some(&instant), &[letter()]);
        for _ in 0..4 * KEEP {
            drawn(&overlay, &mut kept, None, &[letter()]);
        }
        let up = drawn(&overlay, &mut kept, None, &[letter()]);
        assert_eq!(
            pixel(&up, 2 * 32 + 2, 2 * 184 + 2),
            PAPER,
            "still painted while the window is up"
        );
        assert!(inked(&up), "and its text drawn");
        let cleared = drawn(&overlay, &mut kept, None, &[]);
        assert_eq!(
            pixel(&cleared, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "nothing drawn once it is cleared"
        );
        assert!(kept.shown.is_none());
    }

    /// Draws a frame as [`drawn`] does, through [`draw_pages`] (the page named and the one kept behind it).
    fn drawn_pages(
        overlay: &Overlay,
        kept: &mut Kept,
        behind: &mut Option<Kept>,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
    ) -> Vec<u8> {
        let paper = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        drawn_pages_on(overlay, kept, behind, screen, cells, &paper)
    }

    /// Draws a frame as [`drawn_pages`] does, over the 1x frame `frame` (256 pixels a row).
    fn drawn_pages_on(
        overlay: &Overlay,
        kept: &mut Kept,
        behind: &mut Option<Kept>,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
        frame: &[Color],
    ) -> Vec<u8> {
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        let character = |glyph| (glyph != BLANK).then_some('a');
        draw_pages(
            overlay,
            kept,
            behind,
            screen,
            &mut target,
            cells,
            frame,
            256,
            0,
            &character,
            &mut |_| None,
            None,
            &[],
        );
        rgba
    }

    #[test]
    fn a_page_written_at_once_stays_under_a_page_of_another_window_while_its_window_keeps_its_letters()
     {
        // Porunga in a battle (upstream 0.3.4 on): his help is written at once in the Item window at the bottom, then
        // the engine names the wishes, a page of a window at the top, while the help's window stays as it is
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr7:0x00\tDE\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let help = OnScreen { state: 0, ..screen(1.0) };
        let wishes = OnScreen {
            table: 7,
            key: "str7:0x00".to_owned(),
            base: 2 * 64 + 8,
            lines: vec![9],
            ..screen(1.0)
        };
        let wish_letter = TextCell { y: 16, ..letter() };
        let (mut kept, mut behind) = (Kept::default(), None);
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&help), &[letter()]);
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&help), &[letter()]);
        let both =
            drawn_pages(&overlay, &mut kept, &mut behind, Some(&wishes), &[letter(), wish_letter]);
        assert_eq!(
            pixel(&both, 2 * 32 + 2, 2 * 184 + 2),
            PAPER,
            "the help's English stays painted over"
        );
        assert!(inked(&both), "and its Chinese drawn");
        assert_ne!(pixel(&both, 2 * 32 + 2, 2 * 16 + 2), [7, 7, 7], "the wishes are drawn too");
        for _ in 0..4 * KEEP {
            drawn_pages(&overlay, &mut kept, &mut behind, Some(&wishes), &[letter(), wish_letter]);
        }
        let held =
            drawn_pages(&overlay, &mut kept, &mut behind, Some(&wishes), &[letter(), wish_letter]);
        assert!(inked(&held), "the help stays while its window keeps its letters");
        // Another window over the help's place, with letters of its own: the help goes
        let other = TextCell { glyph: 2, ..letter() };
        let gone =
            drawn_pages(&overlay, &mut kept, &mut behind, Some(&wishes), &[other, wish_letter]);
        assert_eq!(
            pixel(&gone, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "the other window is left alone"
        );
        assert!(behind.is_none());
        // A page of the same window named over it ends the page behind at once (no page is kept twice)
        let (mut kept, mut behind) = (Kept::default(), None);
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&help), &[letter()]);
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&help), &[letter()]);
        let next = OnScreen { key: "str7:0x00".to_owned(), ..help.clone() };
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&next), &[letter()]);
        assert!(behind.is_none(), "the window's own next page");
    }

    #[test]
    fn a_page_behind_keeps_its_window_when_a_window_over_it_closes_as_it_goes_behind() {
        // Porunga's third wish (the enemies' HP halved) in a battle: the game shows the enemies' HP window over the
        // Item window, writes the help again and names the wishes while that window is still up, and closes it a few
        // frames later. The help's own letters come back where the enemy's name was: the help stays behind the
        // wishes, in Chinese (it went, its English back: `Lets you make 3 wishes!` under the wishes)
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr7:0x00\tDE\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let help = OnScreen { state: 0, ..screen(1.0) };
        let wishes = OnScreen {
            table: 7,
            key: "str7:0x00".to_owned(),
            base: 2 * 64 + 8,
            lines: vec![9],
            ..screen(1.0)
        };
        let wish_letter = TextCell { y: 16, ..letter() };
        // (the enemy's name over the help's second cell, then the help's own letter there)
        let enemy = TextCell { x: 40, glyph: 2, ..letter() };
        let own = TextCell { x: 40, glyph: 3, ..letter() };
        let (mut kept, mut behind) = (Kept::default(), None);
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&help), &[letter(), enemy]);
        drawn_pages(&overlay, &mut kept, &mut behind, Some(&help), &[letter(), enemy]);
        drawn_pages(
            &overlay,
            &mut kept,
            &mut behind,
            Some(&wishes),
            &[letter(), enemy, wish_letter],
        );
        for _ in 0..2 {
            drawn_pages(&overlay, &mut kept, &mut behind, None, &[letter(), enemy, wish_letter]);
        }
        for _ in 0..4 * KEEP {
            drawn_pages(&overlay, &mut kept, &mut behind, None, &[letter(), own, wish_letter]);
        }
        let after =
            drawn_pages(&overlay, &mut kept, &mut behind, None, &[letter(), own, wish_letter]);
        assert!(behind.is_some(), "the help stays behind the wishes");
        assert_eq!(
            pixel(&after, 2 * 32 + 2, 2 * 184 + 2),
            PAPER,
            "the help's English stays painted over"
        );
        // Once its window settled, another window over the help's place with letters of its own still ends it
        let other = TextCell { glyph: 4, ..letter() };
        drawn_pages(&overlay, &mut kept, &mut behind, None, &[other, own, wish_letter]);
        assert!(behind.is_none(), "another window over the settled help");
    }

    /// Porunga's help (a page written at once, `AB\nC`) settled behind the wishes, its window holding its letter and
    /// `own` beside it.
    fn porunga_help_behind_the_wishes(
        overlay: &Overlay,
        own: TextCell,
        wish_letter: TextCell,
    ) -> (Kept, Option<Kept>, OnScreen, OnScreen) {
        let help = OnScreen { state: 0, ..screen(1.0) };
        let wishes = OnScreen {
            table: 7,
            key: "str7:0x00".to_owned(),
            base: 2 * 64 + 8,
            lines: vec![9],
            ..screen(1.0)
        };
        let (mut kept, mut behind) = (Kept::default(), None);
        drawn_pages(overlay, &mut kept, &mut behind, Some(&help), &[letter(), own]);
        drawn_pages(overlay, &mut kept, &mut behind, Some(&help), &[letter(), own]);
        drawn_pages(overlay, &mut kept, &mut behind, Some(&wishes), &[letter(), own, wish_letter]);
        for _ in 0..4 * KEEP {
            drawn_pages(overlay, &mut kept, &mut behind, None, &[letter(), own, wish_letter]);
        }
        (kept, behind, help, wishes)
    }

    #[test]
    fn a_page_behind_leaves_its_letters_beside_a_window_drawn_over_it_their_ground() {
        // Porunga's third wish (the enemies' HP halved) in a battle: the game draws the enemies' HP window over most
        // of the Item window, the help behind the wishes. The help goes (its window holds the enemy's name), and the
        // English it still showed left of the HP window, `Le` and `3`, stayed for the 175 frames the window is up
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr7:0x00\tDE\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let wish_letter = TextCell { y: 16, ..letter() };
        let own = TextCell { x: 40, glyph: 3, ..letter() };
        let (mut kept, mut behind, _, _) =
            porunga_help_behind_the_wishes(&overlay, own, wish_letter);
        // (the enemy's name over the help's second cell; its first cell still shows the help's letter)
        let enemy = TextCell { x: 40, glyph: 2, ..letter() };
        for frame in 0..3 * KEEP {
            let covered = drawn_pages(
                &overlay,
                &mut kept,
                &mut behind,
                None,
                &[letter(), enemy, wish_letter],
            );
            assert_eq!(
                pixel(&covered, 2 * 32 + 2, 2 * 184 + 2),
                PAPER,
                "the help's letter left of the HP window takes its ground (frame {frame})"
            );
            assert_eq!(
                pixel(&covered, 2 * 40 + 2, 2 * 184 + 2),
                [7, 7, 7],
                "the HP window is left alone (frame {frame})"
            );
        }
        // Another letter in that cell (another window over it too) is that window's: left alone
        let another = TextCell { glyph: 5, ..letter() };
        let taken =
            drawn_pages(&overlay, &mut kept, &mut behind, None, &[another, enemy, wish_letter]);
        assert_eq!(pixel(&taken, 2 * 32 + 2, 2 * 184 + 2), [7, 7, 7], "another window's letter");
    }

    #[test]
    fn a_page_named_again_while_a_window_is_drawn_over_it_waits_under_that_window() {
        // Porunga's third wish: while the enemies' HP window is still up over the Item window, the engine writes the
        // help again (15 frames) and names the wishes (the help behind them), and closes the HP window 4 frames later.
        // The help's Chinese and its paper were drawn over the HP window, over the enemy's name and HP bar, for those
        // 19 frames. It waits under the HP window (its letter beside it keeps its ground) and shows once it closed
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr7:0x00\tDE\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let wish_letter = TextCell { y: 16, ..letter() };
        let own = TextCell { x: 40, glyph: 3, ..letter() };
        let (mut kept, mut behind, help, wishes) =
            porunga_help_behind_the_wishes(&overlay, own, wish_letter);
        let enemy = TextCell { x: 40, glyph: 2, ..letter() };
        let covered = [letter(), enemy, wish_letter];
        for _ in 0..KEEP {
            drawn_pages(&overlay, &mut kept, &mut behind, None, &covered);
        }
        let mut frames = vec![Some(&help); 15];
        frames.extend([Some(&wishes), Some(&wishes), None, None]);
        for (frame, screen) in frames.into_iter().enumerate() {
            let rgba = drawn_pages(&overlay, &mut kept, &mut behind, screen, &covered);
            assert_eq!(
                pixel(&rgba, 2 * 40 + 2, 2 * 184 + 2),
                [7, 7, 7],
                "nothing drawn over the HP window (frame {frame})"
            );
            assert!(
                (2 * 41..128).all(|x| (366..390).all(|y| pixel(&rgba, x, y) == [7, 7, 7])),
                "no paper or Chinese over the HP window (frame {frame})"
            );
            assert_eq!(
                pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2),
                PAPER,
                "the help's letter left of it keeps its ground (frame {frame})"
            );
        }
        // The HP window closes: the help's letters are back, and it shows behind the wishes, in Chinese
        for _ in 0..4 * KEEP {
            drawn_pages(&overlay, &mut kept, &mut behind, None, &[letter(), own, wish_letter]);
        }
        let after =
            drawn_pages(&overlay, &mut kept, &mut behind, None, &[letter(), own, wish_letter]);
        assert!(behind.is_some(), "the help stays behind the wishes");
        assert_ne!(
            pixel(&after, 2 * 40 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "the help drawn where the HP window was"
        );
        assert!(inked(&after), "and its Chinese drawn");
    }

    #[test]
    fn a_page_named_at_once_past_its_lines_is_not_drawn_over_the_window_the_engine_was_in() {
        // Porunga's third wish in a battle, as the Item window comes back: the game starts the help again (`D000`,
        // `message_state` $C0) and for two frames, before the engine writes it, only `message_table` is the help's
        // (0): the string (0), the window (the wishes', `$0108`) and the engine's line and cell (2, 18) are still those
        // of the wishes, `str7:0x00`. That names `help:0` (Kami's help, two lines) at the wishes, past its lines;
        // the engine never writes it there. The original's wishes stay as they were, so the frames are as with no page
        // named: the wishes' Chinese stays, not Kami's help kept behind the help in their window (17 frames)
        let table =
            Translation::parse("key\tzh\nstr7:0x00\tDE\\nF\\nG\nhelp:0\tXY\\nZ\nhelp:15\tAB\n")
                .unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let wishes = OnScreen {
            table: 7,
            key: "str7:0x00".to_owned(),
            state: 0,
            line: 2,
            cell: 18,
            base: 0x0108,
            lines: vec![26, 10, 18],
            ..screen(1.0)
        };
        let stale =
            OnScreen { table: 0, key: "help:0".to_owned(), lines: vec![8, 8], ..wishes.clone() };
        let help = OnScreen {
            table: 0,
            string: 15,
            key: "help:15".to_owned(),
            state: 0,
            line: 1,
            cell: 9,
            lines: vec![13, 9],
            ..screen(1.0)
        };
        // (the wishes' three lines at (4, 2), the help's line at (4, 23))
        let wish_letters: Vec<TextCell> =
            [16, 32, 48].into_iter().map(|y| TextCell { y, ..letter() }).collect();
        let with_help: Vec<TextCell> = wish_letters.iter().copied().chain([letter()]).collect();
        let mut frames: Vec<(Option<&OnScreen>, &[TextCell])> =
            vec![(Some(&wishes), &wish_letters[..]); 2];
        frames.extend(vec![(None, &wish_letters[..]); 3 * KEEP as usize]);
        let named_at = frames.len();
        frames.extend([(Some(&stale), &wish_letters[..]), (Some(&stale), &wish_letters[..])]);
        frames.extend([(Some(&help), &with_help[..]), (Some(&help), &with_help[..])]);
        frames.extend(vec![(None, &with_help[..]); 17]);
        let run = |frames: &[(Option<&OnScreen>, &[TextCell])]| -> Vec<Vec<u8>> {
            let (mut kept, mut behind) = (Kept::default(), None);
            frames
                .iter()
                .map(|&(screen, cells)| {
                    drawn_pages(&overlay, &mut kept, &mut behind, screen, cells)
                })
                .collect()
        };
        let drawn = run(&frames);
        frames[named_at].0 = None;
        frames[named_at + 1].0 = None;
        let unnamed = run(&frames);
        let wish_window = |rgba: &[u8]| -> Vec<[u8; 3]> {
            (2 * 16..2 * 64)
                .flat_map(|y| (2 * 32..2 * 240).map(move |x| (x, y)))
                .map(|(x, y)| pixel(rgba, x, y))
                .collect()
        };
        for (frame, (drawn, unnamed)) in drawn.iter().zip(&unnamed).enumerate().skip(named_at) {
            assert!(
                wish_window(drawn) == wish_window(unnamed),
                "the wishes' window as with no page named (frame {})",
                frame - named_at
            );
        }
        let last = drawn.last().unwrap();
        assert_ne!(pixel(last, 2 * 32 + 2, 2 * 16 + 2), [7, 7, 7], "the wishes are drawn");
        assert_ne!(pixel(last, 2 * 32 + 2, 2 * 184 + 2), [7, 7, 7], "and the help under them");
    }

    #[test]
    fn a_page_written_at_once_waits_under_another_window_drawn_over_its_own() {
        // Porunga's wishes (a page written at once), then the cards a wish changes drawn over most of their window:
        // the wishes' Chinese is not drawn over the cards, and their letters left beside the cards take their ground
        let overlay = overlay();
        let mut kept = Kept::default();
        let wishes = OnScreen { state: 0, ..screen(1.0) };
        let beside = TextCell { x: 40, glyph: 3, ..letter() };
        drawn(&overlay, &mut kept, Some(&wishes), &[letter(), beside]);
        for _ in 0..=KEEP {
            drawn(&overlay, &mut kept, None, &[letter(), beside]);
        }
        assert!(
            inked(&drawn(&overlay, &mut kept, None, &[letter(), beside])),
            "drawn while its window is up"
        );
        let card = TextCell { glyph: CARD_BASE, ..letter() };
        let covered = drawn(&overlay, &mut kept, None, &[card, beside]);
        assert_eq!(
            pixel(&covered, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "nothing drawn over the cards"
        );
        assert_eq!(
            pixel(&covered, 2 * 40 + 2, 2 * 184 + 2),
            PAPER,
            "its letter beside the cards takes its ground"
        );
        assert!(kept.shown.is_some(), "the page waits");
        assert!(
            inked(&drawn(&overlay, &mut kept, None, &[letter(), beside])),
            "drawn again once its letters are back"
        );
    }

    #[test]
    fn a_page_waiting_under_another_window_leaves_that_windows_own_letters_alone() {
        // Porunga's second wish in a battle: the party list to pick from is drawn over the wishes, with letters of its
        // own (the names, `HP`, `KI`, the numbers, its frame). Those that were the same glyphs as some of the wishes'
        // took their ground as if they were the wishes' letters beside it: the list lost most of its names, its
        // labels, the top of its numbers and its lower frame for the 110 frames it is up. Only the wishes' letters
        // where they were, still showing beside it, take their ground
        let overlay = overlay();
        let mut kept = Kept::default();
        let wishes = OnScreen { state: 0, ..screen(1.0) };
        let beside = TextCell { x: 40, glyph: 3, ..letter() };
        drawn(&overlay, &mut kept, Some(&wishes), &[letter(), beside]);
        for _ in 0..=KEEP {
            drawn(&overlay, &mut kept, None, &[letter(), beside]);
        }
        // (the list's letter where the wishes' first letter was is the glyph of the wishes' letter beside it)
        let party = TextCell { glyph: 3, ..letter() };
        let covered = drawn(&overlay, &mut kept, None, &[party, beside]);
        assert_eq!(
            pixel(&covered, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "the list's own letter is left alone"
        );
        assert_eq!(
            pixel(&covered, 2 * 40 + 2, 2 * 184 + 2),
            PAPER,
            "the wishes' letter beside it takes its ground"
        );
        assert!(kept.shown.is_some(), "the page waits");
    }

    /// A 1x frame of paper whose row 183, the one above the text's first line, is a window's lower frame.
    fn paper_under_a_frame() -> Vec<Color> {
        let mut frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        for pixel in &mut frame[183 * 256..184 * 256] {
            *pixel = Color::rgb(0x84, 0x00, 0x00);
        }
        frame
    }

    #[test]
    fn a_page_waiting_under_another_window_leaves_the_frame_above_its_letters_alone() {
        // Porunga's second wish in a battle: the party list drawn over the wishes ends right above the wishes' third
        // line, whose letters still show under it. They took their ground a pixel past their cells all round (the
        // ink the HD text draws past a cell, over the ground), and with it the lowest row of the list's lower frame
        // (5186..5276). The HD text draws no ink past a cell but over its ground: a pixel beside the cell that is not
        // the ground on the 1x frame keeps its picture
        let overlay = overlay();
        let mut kept = Kept::default();
        let wishes = OnScreen { state: 0, ..screen(1.0) };
        let beside = TextCell { x: 40, glyph: 3, ..letter() };
        drawn(&overlay, &mut kept, Some(&wishes), &[letter(), beside]);
        for _ in 0..=KEEP {
            drawn(&overlay, &mut kept, None, &[letter(), beside]);
        }
        let party = TextCell { glyph: 2, ..letter() };
        let covered = drawn_on(&overlay, &mut kept, None, &[party, beside], &paper_under_a_frame());
        assert_eq!(
            pixel(&covered, 2 * 40 + 2, 2 * 184 + 2),
            PAPER,
            "the wishes' letter beside the list takes its ground"
        );
        for y in [2 * 183, 2 * 183 + 1] {
            assert_eq!(
                pixel(&covered, 2 * 40 + 2, y),
                [7, 7, 7],
                "the list's lower frame above it is left alone (row {y})"
            );
        }
        assert_eq!(
            pixel(&covered, 2 * 48, 2 * 184 + 2),
            PAPER,
            "the ground right of its cell, where the HD text's ink reaches, goes with it"
        );
        assert!(kept.shown.is_some(), "the page waits");
    }

    #[test]
    fn a_page_behind_leaves_the_frame_above_its_letters_beside_a_window_drawn_over_it_alone() {
        // Porunga's second wish in a battle, as the party list's last frames go behind the wishes (5277..5295): the
        // wishes went under the list ([`WentUnder`]), and their letters still showing under it took their ground a
        // pixel past their cells, the lowest row of the list's lower frame with them
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr7:0x00\tDE\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let wish_letter = TextCell { y: 16, ..letter() };
        let own = TextCell { x: 40, glyph: 3, ..letter() };
        let (mut kept, mut behind, _, _) =
            porunga_help_behind_the_wishes(&overlay, own, wish_letter);
        let enemy = TextCell { x: 40, glyph: 2, ..letter() };
        let framed = paper_under_a_frame();
        for frame in 0..3 * KEEP {
            let covered = drawn_pages_on(
                &overlay,
                &mut kept,
                &mut behind,
                None,
                &[letter(), enemy, wish_letter],
                &framed,
            );
            assert_eq!(
                pixel(&covered, 2 * 32 + 2, 2 * 184 + 2),
                PAPER,
                "its letter beside the window takes its ground (frame {frame})"
            );
            for y in [2 * 183, 2 * 183 + 1] {
                assert_eq!(
                    pixel(&covered, 2 * 32 + 2, y),
                    [7, 7, 7],
                    "the frame above it is left alone (frame {frame}, row {y})"
                );
            }
            assert_eq!(
                pixel(&covered, 2 * 31, 2 * 184 + 2),
                PAPER,
                "the ground left of its cell goes with it (frame {frame})"
            );
        }
    }

    #[test]
    fn a_page_waiting_under_another_window_keeps_waiting_behind_a_page_of_a_third_window_and_named_again()
     {
        // Porunga's first wish in a battle (the cards it changes, over the wishes) and his second (the party list to
        // pick from, letters of its own): while that window is up the game writes the help again in the Item window
        // (another window: the wishes go behind it) and names the wishes again. The wishes' Chinese was drawn over the
        // cards (frames 5699 to 5724) and over the party list (5277 to 5299): the page behind took the letters of the
        // window over it for its own, and so did the page named again. It waits under that window until it closes
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr7:0x00\tDE\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        // (the wishes, written at once, at (4, 23); the help a window at the top)
        let wishes = OnScreen { state: 0, ..screen(1.0) };
        let help = OnScreen {
            table: 7,
            key: "str7:0x00".to_owned(),
            state: 0,
            base: 2 * 64 + 8,
            lines: vec![9],
            ..screen(1.0)
        };
        let beside = TextCell { x: 40, glyph: 3, ..letter() };
        let help_letter = TextCell { y: 16, ..letter() };
        let cards = TextCell { glyph: CARD_BASE, ..letter() };
        let party = TextCell { glyph: 2, ..letter() };
        for over in [cards, party] {
            let (mut kept, mut behind) = (Kept::default(), None);
            let up = [letter(), beside, help_letter];
            let covered = [over, beside, help_letter];
            drawn_pages(&overlay, &mut kept, &mut behind, Some(&wishes), &up);
            for _ in 0..=KEEP {
                drawn_pages(&overlay, &mut kept, &mut behind, None, &up);
            }
            let mut frames = vec![None; 3 * KEEP as usize];
            frames.extend([Some(&help), Some(&help)]);
            frames.extend([None; 5]);
            frames.extend([Some(&help); 8]);
            frames.extend([Some(&wishes), Some(&wishes)]);
            frames.extend([None; 3 * KEEP as usize]);
            for (frame, screen) in frames.into_iter().enumerate() {
                let rgba = drawn_pages(&overlay, &mut kept, &mut behind, screen, &covered);
                assert!(
                    (2 * 31..2 * 39).all(|x| (366..384).all(|y| pixel(&rgba, x, y) == [7, 7, 7])),
                    "nothing drawn over the window over the wishes ({over:?}, frame {frame})"
                );
                assert_eq!(
                    pixel(&rgba, 2 * 40 + 2, 2 * 184 + 2),
                    PAPER,
                    "the wishes' letter beside it keeps its ground ({over:?}, frame {frame})"
                );
            }
            // That window closes: the wishes' letters are back, and they are drawn
            for _ in 0..KEEP {
                drawn_pages(&overlay, &mut kept, &mut behind, None, &up);
            }
            let back = drawn_pages(&overlay, &mut kept, &mut behind, None, &up);
            assert!(inked(&back), "the wishes drawn again once their letters are back ({over:?})");
        }
    }

    #[test]
    fn a_page_the_engine_finished_writing_stays_while_its_window_keeps_letters() {
        // The shop's `"What do you want?"`: written cell by cell, then a timed wait (state 6), then the engine names
        // nothing while the window stays up over the list
        let overlay = overlay();
        let mut kept = Kept::default();
        let waiting = OnScreen { state: 6, ..screen(1.0) };
        drawn(&overlay, &mut kept, Some(&waiting), &[letter()]);
        for _ in 0..4 * KEEP {
            drawn(&overlay, &mut kept, None, &[letter()]);
        }
        let up = drawn(&overlay, &mut kept, None, &[letter()]);
        assert_eq!(
            pixel(&up, 2 * 32 + 2, 2 * 184 + 2),
            PAPER,
            "still painted while the window is up"
        );
        assert!(inked(&up), "and its text drawn");
        let cleared = drawn(&overlay, &mut kept, None, &[]);
        assert_eq!(
            pixel(&cleared, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "nothing once it is cleared"
        );
        assert!(kept.shown.is_none());
    }

    #[test]
    fn a_finished_page_goes_once_another_windows_letters_take_its_place() {
        // A talk written and finished (state 6), then the engine names nothing: the window keeps the page's letters
        // (glyphs 1, 2, 3) and a choice's letter beside them (4), and then the command window drawn over the place
        // has letters of its own (glyphs 5, 6, and one of the page's)
        let overlay = overlay();
        let mut kept = Kept::default();
        let at = |x: i32, glyph: u8| TextCell { x, glyph, ..letter() };
        let page = [at(32, 1), at(40, 2), at(48, 3)];
        let waiting = OnScreen { state: 6, ..screen(1.0) };
        drawn(&overlay, &mut kept, Some(&waiting), &page);
        for _ in 0..2 * KEEP {
            drawn(&overlay, &mut kept, None, &page);
        }
        let with_choice = [page.as_slice(), &[at(64, 4)]].concat();
        let up = drawn(&overlay, &mut kept, None, &with_choice);
        assert!(inked(&up), "the page stays while its letters do, a choice's added beside them");
        let other = [at(32, 5), at(40, 6), at(48, 3)];
        let replaced = drawn(&overlay, &mut kept, None, &other);
        assert_eq!(
            pixel(&replaced, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "the other window is not painted over"
        );
        assert!(kept.shown.is_none(), "the page goes");
        // The same while the engine names the page: its letters are taken as they are
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&waiting), &page);
        let named = drawn(&overlay, &mut kept, Some(&waiting), &other);
        assert!(inked(&named), "a page the engine names is drawn whatever its window holds");
        // A page written at once (the battle's Item help) is named while its window still holds the last help: its
        // own letters come after, with the engine naming nothing
        let mut kept = Kept::default();
        let at_once = OnScreen { state: 0, ..screen(1.0) };
        drawn(&overlay, &mut kept, Some(&at_once), &page);
        drawn(&overlay, &mut kept, None, &other);
        assert!(
            kept.shown.is_some(),
            "a page written at once stays when its window's letters change"
        );
    }

    #[test]
    fn a_finished_page_the_engine_still_names_goes_once_another_window_takes_its_place() {
        // A KI attack's message (`Goku KI attack!`, battle:6) written and finished (state 6), named for many frames:
        // its window holds the page's letters (glyphs 1, 2, 3). The battle's status box then takes the window's place
        // (HP, KI, BP and their numbers: glyphs 5, 6 and one of the page's) while the engine still names the page.
        // The original shows no text there: nothing is drawn over the box, its letters keep their picture
        let overlay = overlay();
        let mut kept = Kept::default();
        let at = |x: i32, glyph: u8| TextCell { x, glyph, ..letter() };
        let page = [at(32, 1), at(40, 2), at(48, 3)];
        let finished = OnScreen { state: 6, ..screen(1.0) };
        for _ in 0..=KEEP + 1 {
            let up = drawn(&overlay, &mut kept, Some(&finished), &page);
            assert!(inked(&up), "the finished page is drawn while its window holds it");
        }
        // (the picture under the window's first line as it was: the grey of `drawn`, neither ground nor ink)
        let untouched =
            |rgba: &[u8]| (64..128).all(|x| (366..390).all(|y| pixel(rgba, x, y) == [7, 7, 7]));
        let status = [at(32, 5), at(40, 6), at(48, 3)];
        let replaced = drawn(&overlay, &mut kept, Some(&finished), &status);
        assert!(untouched(&replaced), "nothing is drawn or painted over the status box");
        // still gone on the next frames, named or not, the page's letters back or not
        assert!(
            untouched(&drawn(&overlay, &mut kept, Some(&finished), &page)),
            "gone while the engine names it"
        );
        assert!(untouched(&drawn(&overlay, &mut kept, None, &status)), "and when it names nothing");
        // A page the engine writes again (the next KI attack) is drawn again
        let writing = OnScreen { state: 1, cell: 3, ..screen(0.5) };
        assert!(
            inked(&drawn(&overlay, &mut kept, Some(&writing), &page)),
            "a page written again shows"
        );
        // A window that adds letters to the finished page's (a choice's) keeps it
        let mut kept = Kept::default();
        for _ in 0..=KEEP + 1 {
            drawn(&overlay, &mut kept, Some(&finished), &page);
        }
        let with_choice = [page.as_slice(), &[at(64, 4)]].concat();
        assert!(
            inked(&drawn(&overlay, &mut kept, Some(&finished), &with_choice)),
            "a choice beside the page"
        );
    }

    #[test]
    fn a_finished_page_whose_window_another_takes_as_soon_as_it_ends_goes() {
        // Frieza's `battle:49` (`{name:1}的\n念動力！{auto}`): the message finished, and an A
        // cuts its timed wait short: the status box takes the message window's place within [`KEEP`] frames of the
        // page's end (its numbers, other letters than the page's), while the engine still names the page. The page
        // is gone from then on: nothing is drawn over the box
        let overlay = overlay();
        let mut kept = Kept::default();
        let at = |n: i32, glyph: u8| TextCell { x: 32 + 8 * n, glyph, ..letter() };
        let finished = OnScreen { state: 6, ..screen(1.0) };
        let page: Vec<TextCell> = (0..6).map(|n| at(n, 1 + n as u8)).collect();
        assert!(inked(&drawn(&overlay, &mut kept, Some(&finished), &page)), "the message");
        drawn(&overlay, &mut kept, Some(&finished), &page);
        let status_box: Vec<TextCell> = (0..4).map(|n| at(n, 20 + n as u8)).collect();
        for frame in 0..2 * KEEP {
            let up = drawn(&overlay, &mut kept, Some(&finished), &status_box);
            for x in [32, 64, 120] {
                assert_eq!(
                    pixel(&up, 2 * x + 2, 2 * 184 + 2),
                    [7, 7, 7],
                    "nothing over the box at {x}, frame {frame}"
                );
            }
        }
        let after = drawn(&overlay, &mut kept, None, &status_box);
        assert_eq!(
            pixel(&after, 2 * 64 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "nor once the engine names nothing"
        );
    }

    /// An overlay with two pages of the story window: `str3:0x00` ("AB" / "C") and `str3:0x01` ("DE" / "F").
    fn two_pages() -> Overlay {
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\\nC{auto}\nstr3:0x01\tDE\\nF{auto}\n")
                .unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        Overlay::new(Arc::new(table), Arc::new(face))
    }

    /// The battle's status box where the message window is (HP, KI and BP with their numbers: a column of letters left
    /// of the text's first cell, a row of them one tile apart, two letters in the text's span above its first line, the
    /// BP's digits under its second line, and a digit on its first line).
    fn status_box() -> Vec<TextCell> {
        let at = |x: i32, y: i32, glyph: u8| TextCell { x, y, glyph, ..letter() };
        vec![
            at(24, 176, 20),
            at(24, 184, 21),
            at(24, 192, 22),
            at(24, 200, 23),
            at(24, 208, 24),
            at(120, 176, 25),
            at(128, 176, 26),
            at(40, 184, 27),
            at(40, 216, 28),
            at(48, 216, 29),
        ]
    }

    /// Whether the cells of `cells` show the picture under them as it was (the grey of `drawn`): neither ground nor ink.
    fn left_alone(rgba: &[u8], cells: &[TextCell]) -> bool {
        cells.iter().all(|cell| {
            (0..8).all(|dx| {
                (0..8).all(|dy| {
                    pixel(rgba, (2 * (cell.x + dx)) as usize, (2 * (cell.y + dy)) as usize)
                        == [7, 7, 7]
                })
            })
        })
    }

    /// The engine writing a page from its start to its end, frame by frame: named (state 1, nothing written), the
    /// name of its slot (state 5), its cells (state 1), the wait for A (state 3), the timed wait (state 6).
    fn written(page: &OnScreen) -> Vec<OnScreen> {
        let mut frames = vec![
            OnScreen { state: 1, line: 0, cell: 0, progress: 0.0, ..page.clone() },
            OnScreen { state: 5, line: 0, cell: 0, progress: 0.3, ..page.clone() },
            OnScreen { state: 1, line: 0, cell: 3, progress: 0.3, ..page.clone() },
            OnScreen { state: 1, line: 1, cell: 2, progress: 0.8, ..page.clone() },
            OnScreen { state: 3, line: 1, cell: 4, progress: 1.0, ..page.clone() },
        ];
        frames.extend((0..=KEEP + 2).map(|_| OnScreen {
            state: 6,
            line: 1,
            cell: 4,
            progress: 1.0,
            ..page.clone()
        }));
        frames
    }

    #[test]
    fn a_page_the_engine_writes_under_the_status_box_is_not_drawn() {
        // Krillin's Kakusadan (`battle:23`, `{name:1}的\n擴散能量波！`) on four Saibamen (seen in a play-test video):
        // after the first target's `battle:103` the game clears the message window and shows the status box (HP, KI
        // and BP) in its place while the engine names nothing; then, for the next target, the engine writes
        // `battle:23` again from its start into the window's tile map, which the screen does not show: the status
        // box stays, with no text over it in the original (frames 4639 to 4725 of the four-Saibamen run). The
        // letters that came while the engine named nothing are no page's: nothing is drawn while the window holds
        // none but those
        let overlay = two_pages();
        let mut kept = Kept::default();
        let at = |x: i32, glyph: u8| TextCell { x, glyph, ..letter() };
        let before = OnScreen { string: 1, key: "str3:0x01".to_owned(), ..screen(1.0) };
        let page = [at(32, 1), at(40, 2), at(48, 3)];
        for frame in written(&before) {
            drawn(&overlay, &mut kept, Some(&frame), &page);
        }
        assert!(
            inked(&drawn(&overlay, &mut kept, None, &page)),
            "the page before, while its window holds it"
        );
        drawn(&overlay, &mut kept, None, &[]);
        let status = status_box();
        for _ in 0..4 {
            assert!(
                left_alone(&drawn(&overlay, &mut kept, None, &status), &status),
                "the status box"
            );
        }
        for (n, frame) in written(&screen(0.0)).iter().enumerate() {
            let up = drawn(&overlay, &mut kept, Some(frame), &status);
            assert!(
                left_alone(&up, &status),
                "nothing over the status box's letters, frame {n} of the page"
            );
            assert!(
                (64..128).all(|x| (366..390).all(|y| pixel(&up, x, y) == [7, 7, 7])),
                "nothing drawn where the window's text is, frame {n} of the page"
            );
        }
        for _ in 0..2 * KEEP {
            assert!(
                left_alone(&drawn(&overlay, &mut kept, None, &status), &status),
                "nor once it names nothing"
            );
        }
        // The next page written where the screen shows it (the game draws the message window again): drawn
        let mut kept = Kept::default();
        for frame in written(&before) {
            drawn(&overlay, &mut kept, Some(&frame), &page);
        }
        drawn(&overlay, &mut kept, None, &[]);
        for _ in 0..4 {
            drawn(&overlay, &mut kept, None, &status);
        }
        let frames = written(&screen(0.0));
        drawn(&overlay, &mut kept, Some(&frames[0]), &[]);
        drawn(&overlay, &mut kept, Some(&frames[1]), &[at(32, 4)]);
        let shown = drawn(&overlay, &mut kept, Some(&frames[2]), &[at(32, 4), at(40, 5)]);
        assert_eq!(
            pixel(&shown, 2 * 32 + 2, 2 * 184 + 2),
            PAPER,
            "a page its window shows: its paper"
        );
        assert!(inked(&shown), "and its text");
    }

    #[test]
    fn a_page_named_after_one_gone_under_the_status_box_is_not_drawn_while_it_stays() {
        // A KI attack's message gone under the status box while the engine named it (battle:6, battle:49), then the
        // next page of the window named while the box is still there: the box's letters are those it had while the
        // page before was gone, and the page is under the box too
        let overlay = two_pages();
        let mut kept = Kept::default();
        let at = |x: i32, glyph: u8| TextCell { x, glyph, ..letter() };
        let before = OnScreen { string: 1, key: "str3:0x01".to_owned(), ..screen(1.0) };
        let page = [at(32, 1), at(40, 2), at(48, 3)];
        for frame in written(&before) {
            drawn(&overlay, &mut kept, Some(&frame), &page);
        }
        let status = status_box();
        let finished = OnScreen { state: 6, line: 1, cell: 4, ..before.clone() };
        assert!(
            left_alone(&drawn(&overlay, &mut kept, Some(&finished), &status), &status),
            "gone under the box"
        );
        for _ in 0..4 {
            drawn(&overlay, &mut kept, None, &status);
        }
        for (n, frame) in written(&screen(0.0)).iter().enumerate() {
            let up = drawn(&overlay, &mut kept, Some(frame), &status);
            assert!(
                left_alone(&up, &status),
                "nothing over the status box, frame {n} of the next page"
            );
        }
    }

    #[test]
    fn a_finished_page_whose_number_the_game_writes_again_in_its_window_stays() {
        // The gravity training's choice (`str3:0x46`, `{num:1}x Gravity`): the page finished, named again while the
        // handler runs, its window holding the page's letters (glyphs 1, 2, 3). Down writes the next level into the
        // window, over a letter of the page (`Gravity` to `Grav 20`: glyph 3 becomes 7), and into the number slot
        let table = Translation::parse("key\tzh\nstr3:0x00\tAB\\n{num:1}C{auto}\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let mut kept = Kept::default();
        let at = |x: i32, glyph: u8| TextCell { x, glyph, ..letter() };
        let page = [at(32, 1), at(40, 2), at(48, 3)];
        let level = |n: &str| OnScreen {
            state: 6,
            numbers: [n.to_owned(), String::new(), String::new()],
            ..screen(1.0)
        };
        // (the picture under the window's first line as it was: the grey of `drawn`, neither ground nor ink)
        let untouched =
            |rgba: &[u8]| (64..128).all(|x| (366..390).all(|y| pixel(rgba, x, y) == [7, 7, 7]));
        let shows = |rgba: &[u8]| inked(rgba) && !untouched(rgba);
        for _ in 0..=KEEP + 1 {
            assert!(
                shows(&drawn(&overlay, &mut kept, Some(&level("10")), &page)),
                "the choice at 10x"
            );
        }
        let written = [at(32, 1), at(40, 2), at(48, 7)];
        for frame in 0..=2 * KEEP {
            let up = drawn(&overlay, &mut kept, Some(&level("20")), &written);
            assert!(shows(&up), "the choice at 20x, frame {frame}");
        }
        assert_eq!(
            kept.shown.as_ref().map(|shown| shown.text.as_str()),
            Some("AB\n20C"),
            "the slot's number"
        );
        // The page's letters gone while the number stays: another window in its place, as before
        let other = [at(32, 1), at(40, 2), at(48, 8)];
        assert!(
            untouched(&drawn(&overlay, &mut kept, Some(&level("20")), &other)),
            "another window"
        );
    }

    #[test]
    fn letters_a_wider_page_left_past_the_next_pages_reach_take_the_ground_above_them() {
        // A window as wide as its page's longest line (table 7; Porunga's, table 4, was one before it took its
        // measured width): a page of 16 cells, then a page of 6 the engine writes over it, three letters of the
        // first still beside it. They take the colour of the original above them (the test's frame is paper all
        // over); a letter there that is not the one the first page left (another window) keeps its picture
        let table = Translation::parse("key\tzh\nstr7:0x00\tAB\nstr7:0x00/p2\tC\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let mut kept = Kept::default();
        let at = |n: i32, glyph: u8| TextCell { x: 32 + 8 * n, glyph, ..letter() };
        let wide = OnScreen {
            table: 7,
            key: "str7:0x00".to_owned(),
            state: 2,
            lines: vec![16],
            ..screen(1.0)
        };
        // (the engine writes the page cell by cell; a letter of another window, the money box, is there all along)
        let page: Vec<TextCell> =
            (0..16).filter(|&n| n != 11).map(|n| at(n, 1 + (n % 5) as u8)).collect();
        let money = at(11, 8);
        for written in 1..=page.len() {
            let frame: Vec<TextCell> = page[..written].iter().copied().chain([money]).collect();
            drawn(&overlay, &mut kept, Some(&wide), &frame);
        }
        let narrow =
            OnScreen { page: 1, key: "str7:0x00/p2".to_owned(), lines: vec![6], ..wide.clone() };
        let written: Vec<TextCell> = (0..6).map(|n| at(n, 2)).collect();
        let left: Vec<TextCell> = written
            .iter()
            .copied()
            .chain((13..16).map(|n| at(n, 1 + (n % 5) as u8)))
            .chain([money])
            .collect();
        let (x, y) = (2 * (32 + 8 * 14) + 3, 2 * 184 + 3);
        let rgba = drawn(&overlay, &mut kept, Some(&narrow), &left);
        assert_eq!(pixel(&rgba, x, y), [0xFF, 0xEF, 0xCE], "the letter left beside the window");
        assert_eq!(
            pixel(&rgba, 2 * (32 + 8 * 11) + 3, y),
            [7, 7, 7],
            "the other window's letter keeps its picture"
        );
        let rgba = drawn(&overlay, &mut kept, Some(&narrow), &left);
        assert_eq!(pixel(&rgba, x, y), [0xFF, 0xEF, 0xCE], "and on the next frames");
        let between = vec![at(14, 1 + (14 % 5) as u8)];
        let rgba = drawn(&overlay, &mut kept, None, &between);
        assert_eq!(pixel(&rgba, x, y), [0xFF, 0xEF, 0xCE], "and while no page is named");
        let other: Vec<TextCell> = written.iter().copied().chain([at(14, 9)]).collect();
        let rgba = drawn(&overlay, &mut kept, Some(&narrow), &other);
        assert_eq!(pixel(&rgba, x, y), [7, 7, 7], "another letter there keeps its picture");
        let rgba = drawn(&overlay, &mut kept, Some(&narrow), &left);
        assert_eq!(pixel(&rgba, x, y), [7, 7, 7], "and the cell is another window's from then on");
    }

    #[test]
    fn the_next_page_keeps_the_paper_of_the_page_before_until_the_engine_writes_its_first_cell() {
        // Vegeta's `str2:0x0BC/p3` (four lines of the translation, centred) and `/p4` (one): the engine names `/p4`
        // (state 1, line 0, cell 0) frames before the game clears the window, which still shows the English of
        // `/p3`. The paper of `/p4` alone, centred, ends at 1x y 197, in the third English line (191 to 199): the
        // lowest rows of `Thanks for your` showed under it (`secret-zarbon-vegeta-joins`, frames 2498 to 2503)
        let table =
            Translation::parse("key\tzh\nstr2:0xBC/p3\tAB\\nCD\\nEF\\nGH\nstr2:0xBC/p4\tI\n")
                .unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        // (the dialogue window: the text's first cell at tile-map column 9, row 20)
        let at = |n: i32, row: i32| TextCell {
            x: 72 + 8 * n,
            y: 159 + 16 * row,
            glyph: 1 + (n % 5) as u8,
            ..letter()
        };
        let three: Vec<TextCell> =
            (0..3).flat_map(|row| (0..17).map(move |n| at(n, row))).collect();
        let before = OnScreen {
            table: 2,
            string: 0xBC,
            page: 2,
            key: "str2:0xBC/p3".to_owned(),
            state: 2,
            base: 1298,
            portrait: false,
            lines: vec![17, 17, 17],
            ..screen(1.0)
        };
        let next = OnScreen {
            page: 3,
            key: "str2:0xBC/p4".to_owned(),
            state: 1,
            line: 0,
            cell: 0,
            lines: vec![5],
            progress: 0.0,
            ..before.clone()
        };
        let third = (2 * (72 + 8 * 5) + 3, 2 * 198);
        let below = (2 * (72 + 8 * 5) + 3, 2 * 212);
        let mut kept = Kept::default();
        let rgba = drawn(&overlay, &mut kept, Some(&before), &three);
        assert_eq!(pixel(&rgba, third.0, third.1), PAPER, "the page before takes its third line");
        let rgba = drawn(&overlay, &mut kept, Some(&next), &three);
        assert_eq!(
            pixel(&rgba, third.0, third.1),
            PAPER,
            "the next page named, its window not cleared yet"
        );
        assert_eq!(
            pixel(&rgba, below.0, below.1),
            PAPER,
            "the paper of the page before, all of it"
        );
        let rgba = drawn(&overlay, &mut kept, None, &three);
        assert_eq!(
            pixel(&rgba, third.0, third.1),
            PAPER,
            "and on the frames the engine names no page"
        );
        // The engine writes the page's first cell (the window cleared): its own paper only
        let written = OnScreen { cell: 1, progress: 0.2, ..next.clone() };
        let rgba = drawn(&overlay, &mut kept, Some(&written), &[at(0, 0)]);
        assert_eq!(
            pixel(&rgba, below.0, below.1),
            [7, 7, 7],
            "the page written, the paper before goes"
        );
        // Another window (the next page's first cell elsewhere) does not take this window's paper
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&before), &three);
        let elsewhere = OnScreen { base: 1298 + 2, ..next.clone() };
        let rgba = drawn(&overlay, &mut kept, Some(&elsewhere), &three);
        assert_eq!(pixel(&rgba, below.0, below.1), [7, 7, 7], "another window's page");
        // A frame that drew no page between them (the window cleared and closed) ends the paper before
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&before), &three);
        drawn(&overlay, &mut kept, None, &[]);
        drawn(&overlay, &mut kept, None, &[]);
        let rgba = drawn(&overlay, &mut kept, Some(&next), &three);
        assert_eq!(
            pixel(&rgba, below.0, below.1),
            [7, 7, 7],
            "not after a frame with no page drawn"
        );
    }

    /// Draws a frame at scale 2 over a picture of a colour per 1x cell ([`cell_colour`]) and the 1x frame `frame`.
    fn drawn_over(
        overlay: &Overlay,
        kept: &mut Kept,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
        frame: &[Color],
    ) -> Vec<u8> {
        drawn_over_with(overlay, kept, screen, cells, frame, &mut |_| None)
    }

    /// Draws a frame as [`drawn_over`] does, the picture beneath the spilled cells from `beneath`.
    fn drawn_over_with(
        overlay: &Overlay,
        kept: &mut Kept,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
        frame: &[Color],
        beneath: &mut BeneathOf<'_>,
    ) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(512 * 448 * 4);
        for y in 0..448 {
            for x in 0..512 {
                let [r, g, b] = cell_colour(x / 2, y / 2);
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw(
            overlay,
            kept,
            screen,
            &mut target,
            cells,
            frame,
            256,
            0,
            &|glyph| (glyph != BLANK).then_some('a'),
            beneath,
            None,
            &[],
        );
        rgba
    }

    /// Draws a frame as [`drawn_over`] does, the original's `▼` in the 1x cells `marks` (the tile map's `▼` tiles,
    /// [`mark_cells`]: their pictures taken before anything is drawn).
    fn drawn_over_marked(
        overlay: &Overlay,
        kept: &mut Kept,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
        frame: &[Color],
        marks: &[(i32, i32)],
    ) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(512 * 448 * 4);
        for y in 0..448 {
            for x in 0..512 {
                let [r, g, b] = cell_colour(x / 2, y / 2);
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        let marks: Vec<MarkCell> = marks
            .iter()
            .map(|&at| MarkCell { at, picture: cell_picture(&target, at).unwrap() })
            .collect();
        draw(
            overlay,
            kept,
            screen,
            &mut target,
            cells,
            frame,
            256,
            0,
            &|glyph| (glyph != BLANK).then_some('a'),
            &mut |_| None,
            None,
            &marks,
        );
        rgba
    }

    /// The colour of the test picture in the 1x cell of the pixel `x`, `y`.
    fn cell_colour(x: usize, y: usize) -> [u8; 3] {
        [(y / 8) as u8 * 8 + 1, (x / 8) as u8 * 4 + 1, 99]
    }

    /// The colour of the test picture in the 1x cell at `x`, `y`, at the middle of that cell drawn at scale 2.
    fn cell_pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 3] {
        pixel(rgba, 2 * x + 7, 2 * y + 7)
    }

    #[test]
    fn the_letters_of_a_line_longer_than_the_window_take_the_picture_beside_them_past_its_frame() {
        // A story page (window 25 cells from x 32) whose first line has 27 cells: its letters at x 232 and 240 run
        // past the window's 25 cells (x 32 to 232) and take the picture of the cell above them (the frame's side, the
        // screen), not their ground (the window's paper, a block of it beside the frame); a letter of the second line
        // (4 cells) out there is another window's, and so is one past the line's end
        let overlay = overlay();
        let mut kept = Kept::default();
        let long = OnScreen { lines: vec![27, 4], ..screen(1.0) };
        let past = |x: i32, y: i32| TextCell { x, y, bg: PAPER, ..letter() };
        let cells = [letter(), past(232, 184), past(240, 184), past(248, 184), past(232, 200)];
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let drawn = drawn_over(&overlay, &mut kept, Some(&long), &cells, &frame);
        assert_eq!(
            cell_pixel(&drawn, 232, 184),
            cell_colour(232, 176),
            "the line's letter past the frame"
        );
        assert_eq!(cell_pixel(&drawn, 240, 184), cell_colour(240, 176), "and the next");
        assert_eq!(
            cell_pixel(&drawn, 248, 184),
            cell_colour(248, 184),
            "past the line's end: another window's"
        );
        assert_eq!(
            cell_pixel(&drawn, 232, 200),
            cell_colour(232, 200),
            "the short line's row past the frame"
        );
    }

    #[test]
    fn a_line_past_the_tile_maps_columns_goes_on_left_of_the_window_and_its_cells_take_the_picture_beside_them()
     {
        // Gohan's `"Vegeta! You're on Namek?"`, 26 cells from column 9 (the dialogue window, 20 cells): the engine
        // writes the cells past column 31 at the start of the tile-map row under the line, `k?"` left of the window
        // and the `▼` after them. In the story window here (25 cells from column 4, x 32), a line of 30 cells: its
        // 29th and 30th letters at x 0 and 8 of the row under it (y 192), the `▼` at x 16 (a `▼` tile of the tile map,
        // no text tile). They take the picture of the cell above them; the cell after the `▼` is not the page's
        let overlay = overlay();
        let mut kept = Kept::default();
        let waiting = OnScreen { state: 2, lines: vec![30], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let mut cells: Vec<TextCell> = (0..28).map(|n| at(32 + 8 * n, 184)).collect();
        cells.extend([at(0, 192), at(8, 192)]);
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mark = [(16, 192)];
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&waiting), &cells, &frame, &mark);
        assert_eq!(
            cell_pixel(&drawn, 0, 192),
            cell_colour(0, 184),
            "the letter past column 31, left of the window"
        );
        assert_eq!(cell_pixel(&drawn, 8, 192), cell_colour(8, 184), "the next");
        assert_eq!(cell_pixel(&drawn, 16, 192), cell_colour(16, 184), "the `▼` after them");
        assert_eq!(
            cell_pixel(&drawn, 24, 192),
            cell_colour(24, 192),
            "the cell after the `▼`: not the page's"
        );
        assert_eq!(
            cell_pixel(&drawn, 248, 184),
            cell_colour(248, 176),
            "the letters past the frame before them"
        );
        // The `▼` blinking off (its other tile, a blank cell to the font) takes it too
        cells.push(TextCell { glyph: BLANK, ..at(16, 192) });
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&waiting), &cells, &frame, &mark);
        assert_eq!(cell_pixel(&drawn, 16, 192), cell_colour(16, 184), "the `▼` blinking off");
        // A page still being written has no `▼`: the cell after its last letter is left as it is
        let mut kept = Kept::default();
        let writing = OnScreen { state: 1, lines: vec![30], ..screen(1.0) };
        let drawn = drawn_over(&overlay, &mut kept, Some(&writing), &cells[..30], &frame);
        assert_eq!(
            cell_pixel(&drawn, 16, 192),
            cell_colour(16, 192),
            "no `▼` while the page is written"
        );
    }

    #[test]
    fn a_name_under_a_dialogue_portrait_keeps_its_letters_and_letters_left_of_the_frame_go() {
        // The card shop: the shopkeeper's name under her portrait, inside the dialogue window (columns 3 to 7, right
        // of its left frame at column 2) and on the page's rows, is the window's and keeps its letters (its name is
        // drawn there); `k?` left of the frame (columns 0 and 1, the start of the tile-map row) takes the picture
        let table = Translation::parse("key\tzh\nstr2:0x000\tAB\\nCD\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let page = OnScreen {
            table: 2,
            key: "str2:0x000".to_owned(),
            state: 2,
            base: 0x0512,
            portrait: true,
            lines: vec![4, 4],
            ..screen(1.0)
        };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let name = [at(24, 176), at(32, 176), at(40, 176), at(48, 176), at(56, 176)];
        let cells: Vec<TextCell> = (0..4)
            .flat_map(|n| [at(72 + 8 * n, 160), at(72 + 8 * n, 176)])
            .chain(name)
            .chain([at(0, 168), at(8, 168)])
            .collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut kept = Kept::default();
        let drawn = drawn_over(&overlay, &mut kept, Some(&page), &cells, &frame);
        for cell in name {
            let (x, y) = (cell.x as usize, cell.y as usize);
            assert_eq!(cell_pixel(&drawn, x, y), cell_colour(x, y), "the name's letter at x {x}");
        }
        assert_eq!(cell_pixel(&drawn, 0, 168), cell_colour(0, 160), "`k` left of the frame");
        assert_eq!(cell_pixel(&drawn, 8, 168), cell_colour(8, 160), "`?` beside it");
    }

    #[test]
    fn letters_beside_a_story_window_on_its_rows_take_the_picture_with_no_page_drawn_before() {
        // `str2:0x0BC/p3`: the `k?` Gohan's `/p2` wrote left of the window (the start of the tile-map row under its
        // line) is still on the screen, and no frame of `/p2` reached the overlay (nothing kept). On the page's rows,
        // a letter left of the window and one from its right edge on take the picture of the cell above them; a
        // letter left of the window on another row keeps its own
        let table = Translation::parse(
            "key\tzh\nstr3:0x00\tAB\\nCD\nstr3:0x01\tAB\\nCD{auto}\nbattle:0\t「AB\\nCD\n",
        )
        .unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let page = OnScreen { state: 2, lines: vec![4, 4], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = (0..4)
            .flat_map(|n| [at(32 + 8 * n, 184), at(32 + 8 * n, 200)])
            .chain([at(0, 192), at(8, 192), at(240, 200), at(0, 120)])
            .collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut kept = Kept::default();
        let drawn = drawn_over(&overlay, &mut kept, Some(&page), &cells, &frame);
        assert_eq!(cell_pixel(&drawn, 0, 192), cell_colour(0, 184), "`k` left of the window");
        assert_eq!(cell_pixel(&drawn, 8, 192), cell_colour(8, 184), "`?` beside it");
        assert_eq!(cell_pixel(&drawn, 240, 200), cell_colour(240, 192), "right of the window");
        assert_eq!(cell_pixel(&drawn, 0, 120), cell_colour(0, 120), "another row: kept");
        // A page whose window closes by itself (the shop's list beside it) and another table's (the battle's status
        // box beside its window) leave them as they are
        let auto = OnScreen { string: 1, key: "str3:0x01".to_owned(), ..page.clone() };
        let battle = OnScreen { table: 1, key: "battle:0".to_owned(), ..page.clone() };
        for (other, what) in [(auto, "a page that closes by itself"), (battle, "a battle page")] {
            let mut kept = Kept::default();
            let drawn = drawn_over(&overlay, &mut kept, Some(&other), &cells, &frame);
            assert_eq!(cell_pixel(&drawn, 0, 192), cell_colour(0, 192), "{what}: kept");
        }
    }

    /// A page of the wish window (table 4: 11 cells from column 17, x 136 here; the portrait's window left of it),
    /// four lines written, waiting for A, and the overlay that draws it.
    fn wish_page() -> (Overlay, OnScreen) {
        let table = Translation::parse("key\tzh\nstr4:0x04\tAB\\nCD\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let page = OnScreen {
            table: 4,
            string: 4,
            key: "str4:0x04".to_owned(),
            state: 2,
            line: 3,
            base: 0x0122,
            lines: vec![10, 10, 10, 10],
            ..screen(1.0)
        };
        (Overlay::new(Arc::new(table), Arc::new(face)), page)
    }

    /// The letters of [`wish_page`] on the screen: ten cells on each of its four lines.
    fn wish_letters() -> Vec<TextCell> {
        (0..4)
            .flat_map(|row| {
                (0..10).map(move |n| TextCell { x: 136 + 8 * n, y: 32 + 16 * row, ..letter() })
            })
            .collect()
    }

    #[test]
    fn letters_right_of_the_wish_window_take_the_picture_with_no_page_drawn_before() {
        // Porunga's `You have found` / `Dragon Balls.` run past the wish window (`und`, `s.` and its `▼`) and stay
        // there for the pages after it; no frame of that page reached the overlay (nothing kept: a state loaded, the
        // language switched). On the page's rows, the letters from the window's right edge (x 224) on and the `▼`
        // right after them (a `▼` tile of the tile map) take the picture of the cell above them; a letter on the
        // portrait's side, left of the window, keeps its own
        let (overlay, page) = wish_page();
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = wish_letters()
            .into_iter()
            .chain([at(224, 32), at(232, 32), at(240, 32), at(224, 64), at(232, 64), at(96, 48)])
            .collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut kept = Kept::default();
        let drawn =
            drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &frame, &[(240, 64)]);
        for x in [224, 232, 240] {
            assert_eq!(
                cell_pixel(&drawn, x, 32),
                cell_colour(x, 24),
                "the first line's letter at x {x}"
            );
        }
        assert_eq!(cell_pixel(&drawn, 224, 64), cell_colour(224, 56), "`s` of the third line");
        assert_eq!(cell_pixel(&drawn, 232, 64), cell_colour(232, 56), "`.` after it");
        assert_eq!(cell_pixel(&drawn, 240, 64), cell_colour(240, 56), "the `▼` after them");
        assert_eq!(cell_pixel(&drawn, 96, 48), cell_colour(96, 48), "the portrait's side: kept");
        // A cell after them that holds no `▼` tile (the screen beside the window) is not the line's
        let mut kept = Kept::default();
        let drawn = drawn_over(&overlay, &mut kept, Some(&page), &cells, &frame);
        assert_eq!(cell_pixel(&drawn, 240, 64), cell_colour(240, 64), "no `▼` tile there: kept");
    }

    #[test]
    fn the_cell_after_a_waiting_page_past_its_window_is_its_mark_only_while_the_tile_map_holds_one()
    {
        // Vegeta's page whose last line runs past the dialogue window onto his portrait: until the engine writes the
        // `▼` after its last letter, the cell there shows the portrait, which stays on the frames after the page when
        // the engine never writes it (A pressed at once). That cell is not the page's: it keeps its picture, on that
        // frame and on the frames after. Once the tile map holds the `▼` there, it is the page's
        let overlay = overlay();
        let waiting = OnScreen { state: 2, lines: vec![27], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = (0..27).map(|n| at(32 + 8 * n, 184)).collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut kept = Kept::default();
        let drawn = drawn_over(&overlay, &mut kept, Some(&waiting), &cells, &frame);
        assert_eq!(
            cell_pixel(&drawn, 248, 184),
            cell_colour(248, 184),
            "no `▼` tile: the portrait"
        );
        assert_eq!(
            cell_pixel(&drawn, 240, 184),
            cell_colour(240, 176),
            "the letter before it goes"
        );
        let drawn = drawn_over(&overlay, &mut kept, None, &cells[25..], &frame);
        assert_eq!(cell_pixel(&drawn, 248, 184), cell_colour(248, 184), "nor on the frames after");
        let mut kept = Kept::default();
        let drawn =
            drawn_over_marked(&overlay, &mut kept, Some(&waiting), &cells, &frame, &[(248, 184)]);
        assert_eq!(cell_pixel(&drawn, 248, 184), cell_colour(248, 176), "the `▼` tile: the page's");
    }

    #[test]
    fn a_spilled_cell_on_the_frames_side_takes_the_side_beside_it_not_the_corner() {
        // A letter written over the window's right side (x 232) on the first line, under the frame's corner (y 176):
        // it takes the side under it (y 192, the same as at y 200 on the original), not the corner
        let overlay = overlay();
        let mut kept = Kept::default();
        let long = OnScreen { lines: vec![27], ..screen(1.0) };
        let at = |x: i32, y: i32, glyph: u8| TextCell { x, y, glyph, ..letter() };
        let cells =
            [letter(), at(232, 184, 1), at(232, 176, 9), at(232, 192, 10), at(232, 200, 10)];
        let mut frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        for y in 176..208 {
            for x in 232..240 {
                frame[y * 256 + x] =
                    if y < 184 { Color::rgb(1, 2, 3) } else { Color::rgb(4, 5, 6) };
            }
        }
        for y in 184..192 {
            frame[y * 256 + 235] = Color::rgb(0, 0, 0x31);
        }
        let drawn = drawn_over(&overlay, &mut kept, Some(&long), &cells, &frame);
        assert_eq!(cell_pixel(&drawn, 232, 184), cell_colour(232, 192), "the side under it");
    }

    #[test]
    fn the_cells_a_page_spilled_out_of_its_window_keep_the_picture_on_the_next_pages_while_the_original_shows_them()
     {
        // The `k?` and the `▼` of Gohan's line stay left of the window for the pages after it (`"I see you found`,
        // shorter), on the frames between the pages too, until another window draws over the place
        let table = Translation::parse("key\tzh\nstr3:0x00\tAB\nstr3:0x00/p2\tC\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let mut kept = Kept::default();
        let waiting = OnScreen { state: 2, lines: vec![30], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let mut cells: Vec<TextCell> = (0..28).map(|n| at(32 + 8 * n, 184)).collect();
        cells.extend([at(0, 192), at(8, 192)]);
        let mut frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        // (the original's pixels of the spilled cells, as the engine wrote them)
        for y in 192..200 {
            for x in 0..24 {
                frame[y * 256 + x] = Color::rgb(0x10, 0x20, 0x30 + x as u8);
            }
        }
        // (the engine writes the page in two frames; the letters past the frame, x 232 to 248, stay on the next page)
        drawn_over(&overlay, &mut kept, Some(&waiting), &cells[..20], &frame);
        drawn_over_marked(&overlay, &mut kept, Some(&waiting), &cells, &frame, &[(16, 192)]);
        let next =
            OnScreen { page: 1, key: "str3:0x00/p2".to_owned(), lines: vec![4], ..waiting.clone() };
        let shorter: Vec<TextCell> = (0..4)
            .map(|n| at(32 + 8 * n, 184))
            .chain([at(232, 184), at(240, 184), at(248, 184), at(0, 192), at(8, 192)])
            .collect();
        let drawn = drawn_over(&overlay, &mut kept, Some(&next), &shorter, &frame);
        assert_eq!(
            cell_pixel(&drawn, 0, 192),
            cell_colour(0, 184),
            "a letter left of the window, the next page"
        );
        assert_eq!(cell_pixel(&drawn, 16, 192), cell_colour(16, 184), "the `▼` left there");
        assert_eq!(
            cell_pixel(&drawn, 248, 184),
            cell_colour(248, 176),
            "a letter past the frame: the picture, too"
        );
        // (between two pages the engine clears the window: only the cells outside it are left)
        let drawn = drawn_over(&overlay, &mut kept, None, &shorter[7..], &frame);
        assert!(kept.shown.is_none(), "the cleared window ends the page");
        assert_eq!(cell_pixel(&drawn, 8, 192), cell_colour(8, 184), "and while no page is named");
        // Another window drawn over one of them: that cell is the other window's from then on
        for y in 192..200 {
            for x in 8..16 {
                frame[y * 256 + x] = Color::rgb(0x77, 0x77, 0x77);
            }
        }
        let drawn = drawn_over(&overlay, &mut kept, Some(&next), &shorter, &frame);
        assert_eq!(
            cell_pixel(&drawn, 8, 192),
            cell_colour(8, 192),
            "the other window's cell keeps its picture"
        );
        assert_eq!(
            cell_pixel(&drawn, 0, 192),
            cell_colour(0, 184),
            "the one beside it still takes the picture"
        );
    }

    #[test]
    fn a_cell_spilled_over_a_portrait_takes_the_portrait_beneath_it_not_the_cell_above() {
        // Vegeta's `"Oh no! You summoned the dragon!` (battle:243) runs past its window's frame into the bottom row of
        // his portrait beside it: those cells take the portrait as the art draws it without the text's layer (the
        // picture beneath), not the cell above them (the portrait's row of eyes, drawn a second time under the
        // first). A spilled cell with no picture beneath (the frame's side, the screen) takes the cell beside it.
        let overlay = overlay();
        let mut kept = Kept::default();
        let long = OnScreen { lines: vec![27], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = (0..27).map(|n| at(32 + 8 * n, 184)).collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        // The picture beneath at scale 2, one colour; the art drew a picture in the cell at x 240 only
        let mut pictures = vec![false; 256 * 224];
        for y in 184..192 {
            for x in 240..248 {
                pictures[y * 256 + x] = true;
            }
        }
        let under = Arc::new(Beneath {
            rgba: [1_u8, 200, 3, 255].repeat(512 * 448),
            width: 512,
            pictures,
            frame_width: 256,
        });
        let mut asked: Vec<(i32, i32)> = Vec::new();
        let mut beneath = |spilled: &[(i32, i32)]| {
            asked.extend_from_slice(spilled);
            Some(Arc::clone(&under))
        };
        let drawn = drawn_over_with(&overlay, &mut kept, Some(&long), &cells, &frame, &mut beneath);
        assert_eq!(
            cell_pixel(&drawn, 240, 184),
            [1, 200, 3],
            "the portrait beneath the letter, not the cell above"
        );
        assert_eq!(
            cell_pixel(&drawn, 232, 184),
            cell_colour(232, 176),
            "no picture beneath: the cell above"
        );
        assert!(asked.contains(&(240, 184)) && asked.contains(&(232, 184)), "{asked:?}");
        // Without a picture beneath, the cell above, as before
        let mut kept = Kept::default();
        let drawn = drawn_over(&overlay, &mut kept, Some(&long), &cells, &frame);
        assert_eq!(cell_pixel(&drawn, 240, 184), cell_colour(240, 176));
    }

    #[test]
    fn a_picture_beneath_of_another_size_leaves_the_spilled_cell_to_the_cell_beside_it() {
        // A picture beneath as wide as the target but not as high (another frame's height): no cell of it is pasted,
        // and the spilled cell over its picture still takes the cell above it (the English letter does not stay)
        let overlay = overlay();
        let mut kept = Kept::default();
        let long = OnScreen { lines: vec![27], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = (0..27).map(|n| at(32 + 8 * n, 184)).collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let under = Arc::new(Beneath {
            rgba: [1_u8, 200, 3, 255].repeat(512 * 440),
            width: 512,
            pictures: vec![true; 256 * 224],
            frame_width: 256,
        });
        let drawn = drawn_over_with(&overlay, &mut kept, Some(&long), &cells, &frame, &mut |_| {
            Some(Arc::clone(&under))
        });
        assert_eq!(
            cell_pixel(&drawn, 240, 184),
            cell_colour(240, 176),
            "the cell above, as without a picture beneath"
        );
    }

    #[test]
    fn a_frame_may_spill_only_with_a_page_named_or_shown_or_cells_spilled_before() {
        let overlay = overlay();
        let long = OnScreen { lines: vec![27], ..screen(1.0) };
        assert!(
            !may_spill(&Kept::default(), None, None),
            "no page, nothing kept: the picture is not kept"
        );
        assert!(may_spill(&Kept::default(), None, Some(&long)), "a page named now");
        // A page whose first line ran past the window: its cells stay spilled after the engine names no page
        let mut kept = Kept::default();
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = (0..27).map(|n| at(32 + 8 * n, 184)).collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        drawn_over(&overlay, &mut kept, Some(&long), &cells, &frame);
        assert_ne!(kept.spilled, Vec::new(), "the page spilled cells");
        assert!(may_spill(&kept, None, None), "cells spilled before");
        let gone = Kept { shown: None, ..kept.clone() };
        assert!(may_spill(&Kept::default(), Some(&gone), None), "the page behind's cells");
        let spent = Kept { spilled: Vec::new(), ..kept };
        assert!(may_spill(&spent, None, None), "a page kept shown");
        // A page written at once that went under another window, the page shown and the page behind gone (Porunga's
        // third wish, `help:15` under the enemies' HP window): its letters beside that window take their ground on
        // every frame (`paint_went_under`), with the picture before the HD text drew the letters
        let under = WentUnder { key: "help:15".to_owned(), letters: vec![(32, 184, 0x30)] };
        let went = Kept { went_under: Some(under), ..Kept::default() };
        assert!(may_spill(&went, None, None), "a page that went under another window");
        assert!(may_spill(&Kept::default(), Some(&went), None), "the page behind's");
    }

    #[test]
    fn the_ink_a_spilled_letter_drew_around_its_cell_goes_with_it() {
        // battle:243's `g` of `dragon!` is written in the column of the window's right side (x 232 here, past the
        // window's 25 cells): the cell takes the cell above it, and the descender the HD text drew under the cell,
        // over the side's next cell, goes too (it took the picture as it was before the HD text drew the letters),
        // not left as a `_` beside the frame. The ink of a letter of the window, under the window's paper, stays the
        // overlay's to paint.
        let overlay = overlay();
        let mut kept = Kept::default();
        let long = OnScreen { lines: vec![27], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells: Vec<TextCell> = (0..27).map(|n| at(32 + 8 * n, 184)).collect();
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut before = Vec::with_capacity(512 * 448 * 4);
        for y in 0..448 {
            for x in 0..512 {
                let [r, g, b] = cell_colour(x / 2, y / 2);
                before.extend_from_slice(&[r, g, b, 255]);
            }
        }
        // The HD text's descender: rows 192 and 193 (1x) under the letter at x 232, over the cell under it
        let mut rgba = before.clone();
        for y in 2 * 192..2 * 194 {
            for x in 2 * 233..2 * 238 {
                rgba[(y * 512 + x) * 4..(y * 512 + x) * 4 + 3].copy_from_slice(&INK);
            }
        }
        // (and a pixel of the cell beside the HD text drew again in its own colour, the frame's edge: not ink)
        let edge = 4 * ((2 * 193) * 512 + 2 * 239);
        rgba[edge..edge + 3].copy_from_slice(&[0x84, 0x00, 0x00]);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        let character = |glyph| (glyph != BLANK).then_some('a');
        draw(
            &overlay,
            &mut kept,
            Some(&long),
            &mut target,
            &cells,
            &frame,
            256,
            0,
            &character,
            &mut |_| None,
            Some(&before),
            &[],
        );
        assert_eq!(
            pixel(&rgba, 2 * 235, 2 * 192 + 1),
            cell_colour(235, 192),
            "no descender under the spilled letter"
        );
        assert_eq!(
            pixel(&rgba, 2 * 239, 2 * 193),
            [0x84, 0x00, 0x00],
            "the HD text's own picture beside it stays"
        );
        assert_eq!(
            cell_pixel(&rgba, 232, 184),
            cell_colour(232, 176),
            "the cell itself takes the cell above it"
        );
    }

    #[test]
    fn a_page_written_at_once_waits_for_its_windows_letters() {
        // The battle's Item list as it opens: the help of the first row is named for two frames, before the window
        // shows its letters
        let overlay = overlay();
        let mut kept = Kept::default();
        let instant = OnScreen { state: 0, ..screen(1.0) };
        drawn(&overlay, &mut kept, Some(&instant), &[]);
        drawn(&overlay, &mut kept, Some(&instant), &[]);
        drawn(&overlay, &mut kept, None, &[]);
        let shown = drawn(&overlay, &mut kept, None, &[letter()]);
        assert_eq!(pixel(&shown, 2 * 32 + 2, 2 * 184 + 2), PAPER, "the original's letter is gone");
        assert!(inked(&shown), "the help is drawn once its letters show");
        // A page whose window never shows letters goes after the frames a page is kept
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&instant), &[]);
        for _ in 0..=KEEP {
            drawn(&overlay, &mut kept, None, &[]);
        }
        assert!(kept.shown.is_none());
    }

    #[test]
    fn a_name_written_before_the_page_is_hidden_until_the_engine_writes_a_cell() {
        // The battle's `{name} attacks`: the engine writes the name slot first (state 5, no cell of the page yet,
        // then state 1 for a few frames), its letters on the window while the place of the page's text is unknown
        let overlay = overlay();
        let mut kept = Kept::default();
        let naming = OnScreen { state: 5, cell: 0, ..screen(0.2) };
        let rgba = drawn(&overlay, &mut kept, Some(&naming), &[letter()]);
        let blank = |rgba: &[u8]| {
            (2 * 31..2 * 41).all(|x| (2 * 183..2 * 193).all(|y| pixel(rgba, x, y) == PAPER))
        };
        assert!(blank(&rgba), "the name's letter is hidden, nothing drawn over it");
        assert_eq!(
            pixel(&rgba, 2 * 48 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "only the letters' cells are touched"
        );
        let between = OnScreen { state: 1, cell: 0, ..screen(0.0) };
        let rgba = drawn(&overlay, &mut kept, Some(&between), &[letter()]);
        assert!(blank(&rgba), "still hidden back in state 1");
        let rgba = drawn(&overlay, &mut kept, Some(&screen(1.0)), &[letter()]);
        assert!(inked(&rgba), "the translation once the engine writes a cell");
    }

    #[test]
    fn a_page_not_begun_in_a_window_never_seen_is_left_alone() {
        // (a window that opens may still show the last text, scrolling)
        let overlay = overlay();
        let mut kept = Kept::default();
        let fresh = OnScreen { cell: 0, ..screen(0.0) };
        let rgba = drawn(&overlay, &mut kept, Some(&fresh), &[letter()]);
        assert!(rgba.chunks(4).all(|p| p[..3] == [7, 7, 7]));
        drawn(&overlay, &mut kept, Some(&screen(0.5)), &[letter()]);
        let rgba = drawn(&overlay, &mut kept, Some(&fresh), &[letter()]);
        assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "its place is known now");
    }

    #[test]
    fn a_page_not_begun_in_a_window_seen_before_hides_the_letters_it_still_holds() {
        // The shop's `"Anything else?"` named again from its start (state 1, cell 0) after another string of the same
        // window was seen there: the window still holds the last letters, which take their ground
        let overlay = overlay();
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&screen(0.5)), &[letter()]);
        let other = OnScreen { string: 1, cell: 0, ..screen(0.0) };
        let rgba = drawn(&overlay, &mut kept, Some(&other), &[letter()]);
        assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "the letter takes its ground");
        assert!(
            !inked(&rgba) || pixel(&rgba, 2 * 32 + 4, 2 * 184 + 4) == PAPER,
            "and no text is drawn"
        );
    }

    #[test]
    fn a_page_the_translation_lacks_keeps_the_originals_text() {
        let overlay = overlay();
        let mut kept = Kept::default();
        let other = OnScreen { key: "str3:0x01".to_owned(), ..screen(1.0) };
        let rgba = drawn(&overlay, &mut kept, Some(&other), &[letter()]);
        assert!(rgba.chunks(4).all(|p| p[..3] == [7, 7, 7]));
    }

    #[test]
    fn the_page_starts_at_the_row_nearest_the_engines_and_hides_all_its_lines() {
        // The Item screen's help in battle: the window over it shows letters two lines above the text, and the
        // original's page has more lines than the translation
        let overlay = overlay();
        let mut kept = Kept::default();
        let help = OnScreen { lines: vec![12, 13, 13], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let cells = [at(48, 151), at(32, 183), at(32, 199), at(32, 215)];
        let rgba = drawn(&overlay, &mut kept, Some(&help), &cells);
        assert_eq!(kept.place.map(|place| place.at), Some((32, 183)), "the text's first line");
        for y in [183, 199, 215] {
            assert_eq!(
                pixel(&rgba, 2 * 32 + 2, 2 * y + 2),
                PAPER,
                "the original's line at {y} is painted over"
            );
        }
        assert_eq!(
            pixel(&rgba, 2 * 48 + 2, 2 * 151 + 2),
            [7, 7, 7],
            "the window above is left alone"
        );
        // A window scrolled a line up: its first line is the row a line above the one nearest the engine's
        let mut kept = Kept::default();
        let rgba =
            drawn(&overlay, &mut kept, Some(&help), &[at(32, 168), at(32, 184), at(32, 200)]);
        assert_eq!(kept.place.map(|place| place.at), Some((32, 168)));
        assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 168 + 2), PAPER);
    }

    #[test]
    fn words_a_frame_keeps_apart_are_names_when_each_of_them_is_one() {
        const LETTERS: &str = "OSadeprv?.";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        // `text` at row 0 from x 0, a space a cell with no tile
        let row = |text: &str| -> Vec<TextCell> {
            (0..)
                .zip(text.chars())
                .filter(|&(_, ch)| ch != ' ')
                .map(|(at, ch)| {
                    let glyph = u8::try_from(LETTERS.find(ch).unwrap()).unwrap();
                    TextCell { x: 8 * at, y: 8, glyph, fg: INK, bg: PAPER, ..TextCell::default() }
                })
                .collect()
        };
        let names = [("Order", "排序"), ("Save?", "要存檔嗎？")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let painted = |cells: &[TextCell]| -> Vec<bool> {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            [0, 56].map(|x| pixel(&rgba, 2 * x + 1, 2 * 8 + 1) != [7, 7, 7]).to_vec()
        };
        assert_eq!(
            painted(&row("Order Save?")),
            [true, true],
            "the Menu's word and the question over it"
        );
        assert_eq!(painted(&row("Order Saved.")), [false, false], "a sentence is left alone");
    }

    #[test]
    fn a_mark_a_frame_keeps_apart_from_a_name_is_not_the_names() {
        // The Continue's question: the slot screen's dotted rule (spelled `,`) a frame's tile left of
        // `Load game? Yes`, and the cursor a cell with no tile before `Yes`
        const LETTERS: &str = " ,LYadegmos?";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, ch: char| TextCell {
            x,
            y: 8,
            glyph: u8::try_from(LETTERS.find(ch).unwrap()).unwrap(),
            fg: INK,
            bg: PAPER,
            ..TextCell::default()
        };
        let mut cells = vec![cell(0, ',')];
        cells.extend((16..).step_by(8).zip("Load game?".chars()).map(|(x, ch)| cell(x, ch)));
        cells.extend((104..).step_by(8).zip("Yes".chars()).map(|(x, ch)| cell(x, ch)));
        let names = [("Load game? Yes", "這樣好嗎？　是")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let painted = [0, 16, 99, 104].map(|x| pixel(&rgba, 2 * x + 1, 2 * 8 + 1) != [7, 7, 7]);
        assert_eq!(
            painted,
            [false, true, false, true],
            "the mark is left, the question is drawn, the cursor's cell keeps the picture"
        );
    }

    #[test]
    fn a_screen_of_garbage_tiles_takes_the_names_and_rules_a_time_its_cells_not_their_square() {
        // The Continue's screen while the engine writes VRAM: every tile map full of the font's tiles, thousands of
        // runs of a letter or two (f1828 of the load: 25423 cells, 8658 parts). Searched for each part, every other
        // part (the battle's commands) and every cell (the letter after a cut word) took over a second a frame;
        // looked up, milliseconds (Mac, the test profile: 6.5 ms here, 2.1 s searched). `a,` in the first two cells
        // of each four, on every row of six layers: 24576 cells, 6144 parts and as many commas, each under another
        const LETTERS: &str = " a,";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph)).filter(|&ch| ch != ' ');
        let cells: Vec<TextCell> = (0..6_u8)
            .flat_map(|layer| {
                (0..64).flat_map(move |y| (0..64).map(move |x| (layer, x * 8, y * 8)))
            })
            .map(|(layer, x, y)| TextCell {
                x,
                y,
                glyph: [1, 2, 0, 0][(x / 8 % 4) as usize],
                layer: layer & 3,
                fg: INK,
                bg: PAPER,
                ..TextCell::default()
            })
            .collect();
        let names = [("Atk.", "戰鬥"), ("Item", "卡牌"), ("Menu", "選單")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 512);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 1 };
        let start = std::time::Instant::now();
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        draw_rules(&mut target, &cells, &character);
        let took = start.elapsed();
        assert!(took < std::time::Duration::from_millis(200), "{took:?}");
    }

    #[test]
    fn the_battle_commands_and_their_cursor_are_centred_in_their_row_pairs() {
        // The battle's command window (`block:0x35`): `Atk.`, `Item`, `Run` and `Menu` 16 pixels apart, each on the
        // lower tile row of its pair; the cursor a cell with no tile left of the chosen word (`Item` here)
        const LETTERS: &str = " .AFILMRTaeklmnotuy";
        const CURSOR: [u8; 3] = [0xC0, 0x20, 0x10];
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let column = |words: &[&str]| -> Vec<TextCell> {
            words
                .iter()
                .zip((24..).step_by(16))
                .flat_map(|(word, y)| {
                    (32..).step_by(8).zip(word.chars()).map(move |(x, ch)| TextCell {
                        x,
                        y,
                        glyph: u8::try_from(LETTERS.find(ch).unwrap()).unwrap(),
                        fg: INK,
                        bg: PAPER,
                        ..TextCell::default()
                    })
                })
                .collect()
        };
        let names = [
            ("Atk.", "戰鬥"),
            ("Item", "卡牌"),
            ("Run", "逃跑"),
            ("Flee", "逃跑"),
            ("Menu", "選單"),
            ("Talk", "交談"),
            ("Look", "調查"),
            ("Fly", "舞空術"),
        ]
        .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        // The window's paper, the cursor's cell before the second word
        let drawn = |cells: &[TextCell]| -> Vec<u8> {
            let mut rgba = [PAPER[0], PAPER[1], PAPER[2], 255].repeat(512 * 256);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            target.fill((24, 40), (32, 48), CURSOR);
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            rgba
        };
        // The first and last 1x rows, from the word's own row `y`, the ink of its name takes
        let inked = |rgba: &[u8], y: i32| -> (i32, i32) {
            let rows: Vec<i32> = (-8..12)
                .filter(|&dy| {
                    (2 * 32..2 * 64).any(|x| {
                        (0..2).any(|half| {
                            pixel(rgba, x, usize::try_from(2 * (y + dy) + half).unwrap()) != PAPER
                        })
                    })
                })
                .collect();
            (rows[0], rows[rows.len() - 1])
        };
        let commands = drawn(&column(&["Atk.", "Item", "Run", "Menu"]));
        for y in [24, 40, 56, 72] {
            let (top, bottom) = inked(&commands, y);
            assert!(
                top >= -5 && bottom <= 4,
                "the name of the word at {y} is in the middle of its pair, 4 pixels up: ink {top}..={bottom}"
            );
        }
        assert_eq!(pixel(&commands, 2 * 26, 2 * 37), CURSOR, "the cursor goes up with its word");
        assert_eq!(pixel(&commands, 2 * 26, 2 * 45), PAPER, "and leaves the paper under it");
        // The three-row windows (`block:0x42`, `block:0x4E`) go up as much, their cursor too
        for words in [["Atk.", "Item", "Menu"], ["Atk.", "Flee", "Menu"]] {
            let three = drawn(&column(&words));
            for y in [24, 40, 56] {
                let (top, bottom) = inked(&three, y);
                assert!(
                    top >= -5 && bottom <= 4,
                    "{words:?}: the name at {y} is in the middle of its pair, 4 pixels up: ink {top}..={bottom}"
                );
            }
            assert_eq!(
                pixel(&three, 2 * 26, 2 * 37),
                CURSOR,
                "{words:?}: the cursor goes up with its word"
            );
            assert_eq!(
                pixel(&three, 2 * 26, 2 * 45),
                PAPER,
                "{words:?}: and leaves the paper under it"
            );
        }
        // The field's commands (`block:0x00`) are not the battle's: their names and cursor stay on their rows
        let field = drawn(&column(&["Talk", "Look", "Fly", "Item", "Menu"]));
        for y in [24, 40, 56, 72, 88] {
            let (top, bottom) = inked(&field, y);
            assert!(
                top >= -1 && bottom >= 6,
                "the field's word at {y} keeps its row: ink {top}..={bottom}"
            );
        }
        assert_eq!(pixel(&field, 2 * 26, 2 * 45), CURSOR, "the field's cursor stays");
        assert_eq!(pixel(&field, 2 * 26, 2 * 37), PAPER);
    }

    #[test]
    fn a_name_wider_than_its_english_takes_the_blank_cells_after_it_before_it_is_shrunk() {
        // `Fly`, two spaces and `Item` on a row (the field's commands are one a row; the letter keeps a gap here)
        const LETTERS: &str = "FlyItem ";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, ch: char| TextCell {
            x,
            y: 8,
            glyph: u8::try_from(LETTERS.find(ch).unwrap()).unwrap(),
            fg: INK,
            bg: PAPER,
            ..TextCell::default()
        };
        let cells: Vec<TextCell> =
            (0..).step_by(8).zip("Fly  Item".chars()).map(|(x, ch)| cell(x, ch)).collect();
        // Share Tech Mono at the name's em of 10 px: 5.4 px a letter, so five letters are 27 px and seven 37.8
        let ink_right_of = |name: &str| -> usize {
            let names =
                [("Fly", name)].map(|(english, name)| (english.to_owned(), name.to_owned()));
            let overlay = overlay().with_names(names.into_iter().collect());
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 64);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, &cells, &[], 0, &character);
            (0..2 * 40)
                .filter(|&x| (2 * 8..2 * 16).any(|y| pixel(&rgba, x, y) == INK))
                .max()
                .unwrap_or(0)
        };
        let five = ink_right_of("WWWWW");
        assert!(
            five > 2 * 24,
            "five letters reach past Fly's three cells at their full size: {five}"
        );
        let seven = ink_right_of("WWWWWWW");
        assert!(
            seven < 2 * 32,
            "seven are shrunk to Fly and one blank: the cell before Item stays blank ({seven})"
        );
    }

    #[test]
    fn a_word_framed_by_marks_beside_it_is_named_and_the_marks_kept() {
        const LETTERS: &str = "-Sped ";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cells: Vec<TextCell> = (0..)
            .step_by(8)
            .zip("--Speed--".chars())
            .map(|(x, ch)| TextCell {
                x,
                y: 8,
                glyph: u8::try_from(LETTERS.find(ch).unwrap()).unwrap(),
                fg: INK,
                bg: PAPER,
                ..TextCell::default()
            })
            .collect();
        let names =
            [("Speed", "WWWWW")].map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 64);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let painted = |x: usize| (2 * 8..2 * 16).any(|y| pixel(&rgba, 2 * x + 1, y) != [7, 7, 7]);
        assert!(painted(16) && painted(50), "the word's cells take the name");
        assert!(!painted(4) && !painted(60), "the dashes' cells keep their picture");
    }

    #[test]
    fn a_name_found_by_its_start_takes_no_room() {
        // `Saib` of `Saibaman` being written, the blanks after it waiting for the rest
        const LETTERS: &str = "Saib ";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cells: Vec<TextCell> = (0..)
            .step_by(8)
            .zip("Saib    ".chars())
            .map(|(x, ch)| TextCell {
                x,
                y: 8,
                glyph: u8::try_from(LETTERS.find(ch).unwrap()).unwrap(),
                fg: INK,
                bg: PAPER,
                ..TextCell::default()
            })
            .collect();
        let names = [("Saibaman", "WWWWWWWWWW")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 64);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let right = (0..2 * 64)
            .filter(|&x| (2 * 8..2 * 16).any(|y| pixel(&rgba, x, y) == INK))
            .max()
            .unwrap_or(0);
        assert!(right < 2 * 32, "the name keeps to the four cells written so far ({right})");
    }

    #[test]
    fn a_row_set_in_columns_keeps_to_its_cells() {
        const LETTERS: &str = "12345 ";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cells: Vec<TextCell> = (0..)
            .step_by(8)
            .zip("1 2 3 4 5   ".chars())
            .map(|(x, ch)| TextCell {
                x,
                y: 8,
                glyph: u8::try_from(LETTERS.find(ch).unwrap()).unwrap(),
                fg: INK,
                bg: PAPER,
                ..TextCell::default()
            })
            .collect();
        let names = [("1 2 3 4 5", "WWWWWWWWWWWWWWW")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 64);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let right = (0..2 * 96)
            .filter(|&x| (2 * 8..2 * 16).any(|y| pixel(&rgba, x, y) == INK))
            .max()
            .unwrap();
        assert!(
            right < 2 * 72,
            "the figures stay over their nine cells, not the blanks after them ({right})"
        );
    }

    #[test]
    fn a_start_that_ends_a_word_of_a_longer_name_is_not_that_name() {
        let names = [
            ("Attack without", "參加戰鬥的人數"),
            ("Piccolo", "比克"),
            ("Galick Ho", "魔閃光"),
            ("Zeni     P", "金錢　　Ｐ"),
        ]
        .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        assert_eq!(overlay.name("Attack"), None, "the card prompt's word");
        assert_eq!(overlay.name("Attack with"), Some("參加戰鬥的人數"), "cut inside a word");
        assert_eq!(overlay.name("Piccol"), Some("比克"));
        assert_eq!(overlay.name("Galick"), Some("魔閃光"), "a word of a name's own");
        assert_eq!(overlay.name("Zeni"), Some("金錢　　Ｐ"), "the money window's word");
    }

    #[test]
    fn a_name_is_found_whole_or_by_the_one_name_a_cut_column_starts() {
        let names =
            [("Piccolo", "笛子魔童"), ("Goku", "孫悟空"), ("Gohan", "孫悟飯"), ("Nappa", "立巴")]
                .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        assert_eq!(overlay.name("Goku"), Some("孫悟空"));
        assert_eq!(overlay.name("Piccol"), Some("笛子魔童"));
        assert_eq!(overlay.name("Go"), None, "too short to stand for a name");
        assert_eq!(overlay.name("Napp"), Some("立巴"));
        assert_eq!(overlay.name("Talk"), None);
    }

    #[test]
    fn a_word_of_a_sentence_is_no_part_even_where_its_spaces_have_no_tiles() {
        // `text` at row `y`: a letter is a cell (its glyph its place in `LETTERS`), a space none (the opening's
        // scroll draws no tile for it)
        const LETTERS: &str = "ABDEFGHIKLMNOPRSYabcehiklnoprsu.01279'\" ";
        let row = |text: &str, y: i32| -> Vec<TextCell> {
            (0..)
                .zip(text.chars())
                .filter(|&(_, ch)| ch != ' ')
                .map(|(at, ch)| {
                    let glyph = u8::try_from(LETTERS.find(ch).unwrap()).unwrap();
                    TextCell { x: 8 * at, y, glyph, ..TextCell::default() }
                })
                .collect()
        };
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let english = |cells: &[TextCell]| -> Vec<String> {
            name_parts(cells, &character).into_iter().map(|(english, _)| english).collect()
        };
        assert_eq!(
            english(&row("A FROG HIDES IN A PILE OF REEDS.", 0)),
            ["A FROG HIDES IN A PILE OF REEDS."]
        );
        assert_eq!(english(&row("FOR", 0)), ["FOR"], "a word alone is still a part");
        assert_eq!(english(&row("Goku   Piccolo", 0)), ["Goku", "Piccolo"], "a list's columns");
        assert_eq!(
            english(&row("Piccol 01", 0)),
            ["Piccol", "01"],
            "a frame's tile before the level"
        );
        assert_eq!(
            english(&row("Piccolo01", 0)),
            ["Piccolo", "01"],
            "the status box's HP right after the name"
        );
        // (a space of the slot screen is a blank tile, a cell)
        let spaced = |text: &str, y: i32| -> Vec<TextCell> {
            (0..)
                .zip(text.chars())
                .map(|(at, ch)| {
                    let glyph = u8::try_from(LETTERS.find(ch).unwrap()).unwrap();
                    TextCell { x: 8 * at, y, glyph, ..TextCell::default() }
                })
                .collect()
        };
        let english_spaced = |cells: &[TextCell]| -> Vec<String> {
            name_parts(cells, &character).into_iter().map(|(english, _)| english).collect()
        };
        assert_eq!(
            english_spaced(&spaced("Piccolo 77  12", 0)),
            ["Piccolo", "77", "12"],
            "the slot screen's HP a blank after a seven-letter name"
        );
        assert_eq!(english_spaced(&spaced("Goku    97", 0)), ["Goku", "97"]);
        assert_eq!(english_spaced(&spaced("Mr. Popo", 0)), ["Mr. Popo"], "a name's own blank");
        assert_eq!(
            english(&row("1Goku", 0)),
            ["1", "Goku"],
            "the Order screen's place before the name"
        );
        assert_eq!(english(&row("DB'1", 0)), ["DB'1"], "a dragon ball's number is its name's");
        assert_eq!(english(&row("\"DB'1\"", 0)), ["\"DB'1\""]);
        let two_rows = [row("Goku", 0), row("Gohan", 8)].concat();
        assert_eq!(english(&two_rows), ["Goku", "Gohan"]);
        let parts = name_parts(&two_rows, &character);
        assert_eq!(parts[1].1.len(), 5, "a part keeps its cells");
    }

    #[test]
    fn the_measured_windows_have_their_widths_and_the_others_their_longest_line() {
        assert_eq!(window_cells(3, 9), 25);
        assert_eq!(window_cells(1, 30), 17);
        assert_eq!(window_cells(7, 13), 13);
        assert_eq!(window_cells(7, 1), 4);
        assert_eq!(
            window_cells(0, 1),
            12,
            "the Item help of a blank English page still has a line of 8 characters"
        );
        assert_eq!(window_cells(0, 15), 15);
        assert_eq!(
            window_cells(4, 15),
            11,
            "the wish window to its right frame, not its longest English line"
        );
    }

    /// Draws `screen`'s page with the translation `table` over a 1x frame of paper 240 rows tall (the dialogue
    /// window's paper reaches row 235), at scale 2 over a grey picture.
    fn drawn_in(table: &str, screen: &OnScreen, cells: &[TextCell]) -> (Kept, Vec<u8>) {
        let table = Translation::parse(&format!("key\tzh\n{table}\n")).unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let mut kept = Kept::default();
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 480);
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 240];
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw(
            &overlay,
            &mut kept,
            Some(screen),
            &mut target,
            cells,
            &frame,
            256,
            0,
            &|glyph| (glyph != BLANK).then_some('a'),
            &mut |_| None,
            None,
            &[],
        );
        (kept, rgba)
    }

    /// The 1x rows with ink between the 1x columns `from..to`, the first and the last.
    fn ink_rows(rgba: &[u8], from: usize, to: usize) -> Option<(usize, usize)> {
        let inked = |y: usize| {
            (2 * from..2 * to)
                .any(|x| pixel(rgba, x, y) != [7, 7, 7] && pixel(rgba, x, y)[2] < 0x80)
        };
        let first = (0..480).find(|&y| inked(y))?;
        let last = (0..480).rev().find(|&y| inked(y))?;
        Some((first / 2, last / 2))
    }

    #[test]
    fn a_page_whose_english_starts_with_a_blank_line_starts_at_the_engines_first_line() {
        // Gohan's `str3:0x75` (`\nYou have found\nthe Dragonballs!`, two lines of Chinese) in the story window, here
        // a row higher (its first line at 176): its first letters are on the second line, its last line the window's
        // last row. The page starts a line above its letters, and the paper ends under the English's last row (its
        // HD letters' cells, to a pixel under the tile row), not an em under three lines from its letters: the frame
        // under it keeps its picture
        let screen = OnScreen { base: 0x0588, lines: vec![0, 14, 16], ..screen(1.0) };
        let cells = [TextCell { y: 192, ..letter() }, TextCell { y: 208, ..letter() }];
        let (kept, rgba) = drawn_in("str3:0x00\tAB\\nC", &screen, &cells);
        assert_eq!(kept.place.map(|place| place.at), Some((32, 176)), "the engine's first line");
        assert_eq!(
            pixel(&rgba, 2 * 40 + 3, 2 * 216 + 1),
            PAPER,
            "the English's last letters' cells are painted"
        );
        assert_eq!(
            pixel(&rgba, 2 * 40 + 3, 2 * 217),
            [7, 7, 7],
            "the row under them keeps its picture"
        );
        let (top, bottom) = ink_rows(&rgba, 32, 200).unwrap();
        assert!(
            top >= 164 && bottom < 219,
            "the Chinese in the window's paper: rows {top} to {bottom}"
        );
    }

    #[test]
    fn a_page_with_more_english_lines_than_chinese_hides_the_englishs_last_line_whole() {
        // The battle's `battle:88` (one line of Chinese, two of English): the paper takes the HD letters' cells of
        // the English's last line, to a pixel under its tile row
        let battle = OnScreen {
            table: 1,
            key: "battle:0".to_owned(),
            base: 0x0610,
            lines: vec![8, 4],
            ..screen(1.0)
        };
        let cells =
            [TextCell { x: 64, y: 192, ..letter() }, TextCell { x: 64, y: 208, ..letter() }];
        let (_, rgba) = drawn_in("battle:0\tAB", &battle, &cells);
        assert_eq!(pixel(&rgba, 2 * 72 + 3, 2 * 216 + 1), PAPER, "the cells' last pixel row");
        assert_eq!(pixel(&rgba, 2 * 72 + 3, 2 * 217), [7, 7, 7]);
    }

    #[test]
    fn the_wish_windows_paper_ends_at_its_right_frame() {
        // Porunga's `str4:0x04`, its English lines 15 cells wide from column 17: the paper and the Chinese keep to
        // the 11 cells left of the window's right frame
        let screen = OnScreen {
            table: 4,
            key: "str4:0x04".to_owned(),
            base: 0x0122,
            lines: vec![15, 15],
            ..screen(1.0)
        };
        let first = TextCell { x: 136, y: 32, ..letter() };
        let (_, rgba) = drawn_in("str4:0x04\t一二三四五六七八九十", &screen, &[first]);
        assert_eq!(pixel(&rgba, 2 * 223 + 1, 2 * 33), PAPER, "the window's last cell");
        assert_eq!(
            pixel(&rgba, 2 * 224, 2 * 33),
            [7, 7, 7],
            "the frame right of it keeps its picture"
        );
    }

    #[test]
    fn the_translation_is_centred_in_the_dialogue_and_story_windows_paper() {
        let lines = |n: usize| vec![vec!['A']; n];
        // The story window's paper, from 12 above the engine's first line, 55 tall: two lines (28 from the first's
        // em to the last's) start 14 down (27 to share, the odd one above), three (44) 6 down; the row is RISE under
        // the em's top
        assert_eq!(centred(3, 184, &lines(2), 16), Some(172 + 14 + RISE));
        assert_eq!(centred(3, 184, &lines(3), 16), Some(172 + 6 + RISE));
        // The dialogue window's, 87 tall: Krillin's five lines (76) start 6 down
        assert_eq!(centred(2, 160, &lines(5), 16), Some(148 + 6 + RISE));
        // Blank lines before and after the text take no room; a text taller than the paper starts at its top
        let padded = [vec![' '], vec!['A'], vec!['A'], vec![' ']];
        assert_eq!(centred(3, 184, &padded, 16), Some(172 + 14 + RISE - 16));
        assert_eq!(centred(3, 184, &lines(4), 16), Some(172 + RISE));
        // The battle's message window: its paper as the story window's, from 12 above its first line (1x y 180 to
        // 235): one line (12) starts 22 down, two (28) 14, three (44) 6 (the English's first line, 192, left 11 above
        // and none under); four (60) are taller than the paper and start at its top
        assert_eq!(centred(1, 192, &lines(1), 16), Some(180 + 22 + RISE));
        assert_eq!(centred(1, 192, &lines(2), 16), Some(180 + 14 + RISE));
        assert_eq!(centred(1, 192, &lines(3), 16), Some(180 + 6 + RISE));
        assert_eq!(centred(1, 192, &lines(4), 16), Some(180 + RISE));
        for table in [0, 4, 7, ENDING_TABLE] {
            assert_eq!(
                centred(table, 184, &lines(2), 16),
                None,
                "table {table} keeps the English's rows"
            );
        }
    }

    #[test]
    fn a_portraits_page_is_centred_right_of_the_portrait() {
        // Krillin's `str2:0x0D0`: five lines of Chinese in the dialogue window (text from column 9, row 20), its
        // English two lines here. The text's ink has as much paper above as under it (the paper is 148 to 235), and
        // the portrait left of the text's hanging cell keeps its picture
        let screen = OnScreen {
            table: 2,
            key: "str2:0x0D0".to_owned(),
            base: 0x0512,
            portrait: true,
            lines: vec![18, 18],
            ..screen(1.0)
        };
        let cells =
            [TextCell { x: 72, y: 160, ..letter() }, TextCell { x: 72, y: 176, ..letter() }];
        let (_, rgba) = drawn_in("str2:0x0D0\t{portrait:0A}A\\nA\\nA\\nA\\nA", &screen, &cells);
        let (top, bottom) = ink_rows(&rgba, 64, 232).unwrap();
        let (above, under) = (top as i32 - 148, 235 - 1 - bottom as i32);
        assert!(above > 0 && under > 0 && (above - under).abs() <= 2, "ink rows {top} to {bottom}");
        for y in [150, 180, 230] {
            assert_eq!(pixel(&rgba, 2 * 60, 2 * y), [7, 7, 7], "the portrait at row {y}");
        }
    }

    #[test]
    fn a_page_the_games_cursor_goes_on_keeps_the_englishs_rows_and_the_battles_messages_are_centred()
     {
        // A yes-or-no question (`{auto}`: the window closes by itself, the game's choice goes on the page's rows):
        // its first line is the English's. In the battle's message window the talk and the battle's own messages,
        // those that close by themselves too, are centred; a choice there (its words in the cursor's columns) is not
        let first_ink = |table: &str, screen: &OnScreen, cell: TextCell| {
            ink_rows(&drawn_in(table, screen, &[cell]).1, 0, 256).map(|(top, _)| top)
        };
        let question = OnScreen { lines: vec![9, 4], ..screen(1.0) };
        let story = TextCell { x: 32, y: 184, ..letter() };
        assert!(
            first_ink("str3:0x00\tAB\\nC{auto}", &question, story).is_some_and(|top| top < 186)
        );
        assert!(
            first_ink("str3:0x00\tAB\\nC", &question, story).is_some_and(|top| top >= 186),
            "centred"
        );
        let battle = OnScreen {
            table: 1,
            key: "battle:0".to_owned(),
            base: 0x0610,
            lines: vec![9, 9, 9],
            ..screen(1.0)
        };
        let message = TextCell { x: 64, y: 192, ..letter() };
        let talk = first_ink("battle:0\t「AB\\nC」", &battle, message);
        assert!(talk.is_some_and(|top| top >= 194), "the fighters' talk is centred: {talk:?}");
        for text in ["AB", "AB\\nC{auto}", "「AB\\nC」{auto}"] {
            let top = first_ink(&format!("battle:0\t{text}"), &battle, message);
            assert!(top.is_some_and(|top| top >= 194), "{text}: {top:?}");
        }
        let choice = first_ink("battle:0\tAB　　C\\nD　　E{auto}", &battle, message);
        assert!(
            choice.is_some_and(|top| top < 194),
            "a choice keeps the English's rows: {choice:?}"
        );
    }

    #[test]
    fn the_cursor_before_the_second_choice_of_the_battles_window_shows_after_a_letter() {
        // `battle:317`, `Will you  Yes!` / `continue? No!`, the cursor on `No!` (「不打」 had no cursor):
        // the cursor's cell has no text tile, right after the `?` and before the `N`; on the first line the cell
        // before `Yes!` is a blank tile. The cursor keeps the picture, the `?` and the blank take the paper (the
        // translation's words, a choice's ideographic space after them, end left of the cursor's column)
        let battle = OnScreen {
            table: 1,
            key: "battle:317".to_owned(),
            base: 0x0610,
            lines: vec![14, 13],
            ..screen(1.0)
        };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let blank = |x: i32, y: i32| TextCell { x, y, glyph: BLANK, ..letter() };
        let cells = [at(64, 192), blank(128, 192), blank(136, 192), at(144, 192)]
            .into_iter()
            .chain([at(64, 208), at(128, 208), at(144, 208)])
            .collect::<Vec<_>>();
        let (_, rgba) = drawn_in("battle:317\tAB　\\nCD　{auto}", &battle, &cells);
        assert_eq!(
            pixel(&rgba, 2 * 136 + 3, 2 * 208 + 3),
            [7, 7, 7],
            "the cursor before `No!` shows"
        );
        assert_eq!(
            pixel(&rgba, 2 * 128 + 3, 2 * 208 + 3),
            PAPER,
            "the `?` left of it takes the paper"
        );
        assert_eq!(pixel(&rgba, 2 * 136 + 3, 2 * 192 + 3), PAPER, "the blank before `Yes!` too");
    }

    #[test]
    fn the_battles_three_line_message_has_as_much_paper_above_as_under() {
        // `battle:0` (`{name:1}\n對{name:2}\n發動攻擊！`, three lines): on the English's rows (from 192)
        // its ink reached the paper's bottom (235) and left 11 pixels above it (from 180); centred, the two are within
        // two pixels of each other
        let battle = OnScreen {
            table: 1,
            key: "battle:0".to_owned(),
            base: 0x0610,
            lines: vec![9, 9, 9],
            ..screen(1.0)
        };
        let cells = [16, 32, 0].map(|dy| TextCell { x: 64, y: 192 + dy, ..letter() });
        let (_, rgba) = drawn_in("battle:0\tAB\\nC\\nDE{auto}", &battle, &cells);
        let (top, bottom) = ink_rows(&rgba, 64, 200).unwrap();
        let (above, under) = (top as i32 - 180, 235 - 1 - bottom as i32);
        assert!(above > 0 && under > 0 && (above - under).abs() <= 2, "ink rows {top} to {bottom}");
    }

    #[test]
    fn a_mark_with_no_glyph_after_a_blank_cell_of_a_centred_page_is_no_cursor() {
        // Gohan's `str2:0x10E/p3` in the dialogue window: its English first line starts with a `"` the font has no
        // glyph for (no text tile) after a blank cell, as a choice's cursor would. The page is centred, its Chinese
        // off that line: the mark's cell takes the paper; on a page of a choice (`{auto}`) it is the cursor
        let screen = OnScreen {
            table: 2,
            key: "str2:0x0D0".to_owned(),
            base: 0x0512,
            lines: vec![12],
            ..screen(1.0)
        };
        let blank = TextCell { x: 72, y: 160, glyph: BLANK, ..letter() };
        let cells = [blank, TextCell { x: 88, y: 160, ..letter() }];
        let (_, rgba) = drawn_in("str2:0x0D0\t\\nAB", &screen, &cells);
        assert_eq!(pixel(&rgba, 2 * 80 + 3, 2 * 160 + 3), PAPER, "the mark takes the paper");
        let (_, rgba) = drawn_in("str2:0x0D0\t\\nAB{auto}", &screen, &cells);
        assert_eq!(
            pixel(&rgba, 2 * 80 + 3, 2 * 160 + 3),
            [7, 7, 7],
            "the cursor before a choice keeps the picture"
        );
    }

    #[test]
    fn the_battles_talk_and_messages_are_centred_and_its_choices_are_not() {
        assert!(centres(1, "「那傢伙看到丹丹\\n在治療我們了！」"));
        assert!(centres(1, "{portrait:0F:80}「現在我就要去\\n偷襲飛里沙。」"));
        assert!(centres(1, "敵人出現了！"), "a line of the battle");
        assert!(centres(1, "殺死了那美克人…\\n這下他們不會再\\n協助我們了…。"), "narration");
        assert!(centres(1, "{name:1}\\n對{name:2}\\n發動攻擊！{auto}"), "an attack");
        assert!(centres(1, "{name:2}\\n受到{num:1}點\\n傷害！{auto}"), "an attack's damage");
        assert!(
            centres(1, "「放手，卡洛特！\\n你也別想好過！」{auto}"),
            "talk that closes by itself"
        );
        assert!(
            !centres(1, "下一戰　　　　開打\\n要打嗎？　　　不打{auto}"),
            "battle:317, a choice"
        );
        assert!(
            !centres(1, "不論成功或失敗，\\n都會扣ＫＩ，好嗎？\\n　　　好啊　　　不要"),
            "battle:210"
        );
        assert!(centres(3, "終於！集齊七顆龍珠了！"));
        assert!(!centres(3, "要產生幾倍的\\n重力呢？{num:1}倍{auto}"), "a choice");
        for table in [0, 4, 7, ENDING_TABLE] {
            assert!(!centres(table, "「AB」"), "table {table}");
        }
    }

    /// The intro with BG1's line 0 on the frame's row 40 (scroll 512 - 41) and BG2's line 16 on row 100.
    fn intro_screen() -> OnScreen {
        OnScreen {
            table: INTRO_TABLE,
            string: 512 - 101,
            page: 0,
            key: "intro".to_owned(),
            state: 2,
            stopped: false,
            line: 0,
            cell: 0,
            base: 512 - 41,
            portrait: false,
            lines: vec![32; INTRO_LINES],
            english: Vec::new(),
            progress: 1.0,
            names: [None, None, None],
            numbers: Default::default(),
        }
    }

    /// A translation of the intro's lines 0 and 16 only.
    fn intro_overlay() -> Overlay {
        let table = Translation::parse("key\tzh\nintro:0\tAB\nintro:16\tC\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        Overlay::new(Arc::new(table), Arc::new(face))
    }

    /// A white letter of the title set at column `column`, row `y` of layer `layer` (BG1 0, BG2 1).
    fn title_letter(column: i32, y: i32, layer: u8) -> TextCell {
        let white = [0xF8, 0xF8, 0xF8];
        let cell =
            TextCell { x: 8 * column, y, glyph: 5, bold: true, layer, ..TextCell::default() };
        TextCell { fg: white, bg: white, mask: 0xFF, ..cell }
    }

    #[test]
    fn in_the_intro_the_font_leaves_out_the_letters_of_the_lines_the_translation_has() {
        let mut translator = Translator::new(Arc::new(intro_overlay()));
        let line0 = title_letter(0, 40, 0);
        let line1 = title_letter(0, 56, 0);
        let line16 = title_letter(3, 100, 1);
        let late = title_letter(1, 41, 0);
        let not_title = TextCell { bold: false, ..title_letter(2, 40, 0) };
        let bg2_on_line0 = title_letter(4, 40, 1);
        let off = title_letter(5, 44, 0);
        let cells = [line0, line1, line16, late, not_title, bg2_on_line0, off];
        assert_eq!(translator.font_cells(&cells), None, "not the intro: the font draws every cell");
        translator.screen = Some(screen(1.0));
        assert_eq!(
            translator.font_cells(&cells),
            None,
            "a message page: the font draws every cell"
        );
        translator.screen = Some(intro_screen());
        // Lines 0 and 16 (a frame drawn a row off the scroll read too) go; line 1, which the translation lacks, the
        // UI font's letter, BG2's cell on BG1's line and a row four off stay
        assert_eq!(translator.font_cells(&cells), Some(vec![line1, not_title, bg2_on_line0, off]));
    }

    #[test]
    fn in_the_intro_a_layer_all_of_whose_lines_the_translation_has_is_left_out_of_the_frame() {
        let face = Arc::new(ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap());
        let translator = |lines: std::ops::Range<usize>, screen: Option<OnScreen>| {
            let rows = lines.fold(String::new(), |rows, line| rows + &intro_key(line) + "\tA\n");
            let table = Translation::parse(&format!("key\tzh\n{rows}")).unwrap();
            let mut translator =
                Translator::new(Arc::new(Overlay::new(Arc::new(table), Arc::clone(&face))));
            translator.screen = screen;
            translator
        };
        let intro = || Some(intro_screen());
        assert_eq!(translator(0..32, intro()).quiet_layers(), [true, true, false, false]);
        assert_eq!(
            translator(0..16, intro()).quiet_layers(),
            [true, false, false, false],
            "BG2's lines lacking"
        );
        assert_eq!(
            translator(1..32, intro()).quiet_layers(),
            [false, true, false, false],
            "line 0 lacking"
        );
        assert_eq!(
            translator(0..32, Some(screen(1.0))).quiet_layers(),
            [false; 4],
            "a message page"
        );
        assert_eq!(translator(0..32, None).quiet_layers(), [false; 4]);
    }

    #[test]
    fn in_the_intro_the_cells_of_a_translated_line_take_the_picture_without_its_layer() {
        // A 1x frame of white, BG1 off showing blue: line 0 (4 cells of English, on row 40) takes the blue, its `.` at
        // column 3 with it (no text cell there: the HD font does not know the intro's dot); line 1 (row 56), which
        // the translation lacks, and the cells right of line 0 keep the white
        let mut translator = Translator::new(Arc::new(intro_overlay()));
        let mut screen = intro_screen();
        screen.lines[0] = 4;
        screen.lines[1] = 4;
        let (white, blue) = (Color::rgb(0xF8, 0xF8, 0xF8), Color::rgb(0, 0, 0x80));
        let without_bg1 = vec![blue; 256 * 224];
        let cells = [title_letter(0, 40, 0), title_letter(1, 40, 0), title_letter(0, 56, 0)];
        let cleaned = |translator: &Translator, beneath: [Option<&[Color]>; 4]| {
            let mut pixels = vec![white; 256 * 224];
            translator.clean_intro(&mut pixels, 256, &cells, beneath, 0);
            pixels
        };
        let beneath = [Some(&without_bg1[..]), None, None, None];
        assert_eq!(
            cleaned(&translator, beneath),
            vec![white; 256 * 224],
            "not the intro: untouched"
        );
        translator.screen = Some(screen);
        let pixels = cleaned(&translator, beneath);
        let at = |x: usize, y: usize| pixels[y * 256 + x];
        assert_eq!(
            (at(0, 40), at(26, 44), at(31, 47), at(0, 48), at(31, 55)),
            (blue, blue, blue, blue, blue),
            "both 8-pixel halves of the original English must disappear"
        );
        assert_eq!((at(32, 40), at(0, 39), at(0, 56)), (white, white, white));
        assert_eq!(pixels.iter().filter(|&&pixel| pixel == blue).count(), 32 * 16);
        // Without the picture under BG1, nothing is taken out
        assert_eq!(cleaned(&translator, [None; 4]), vec![white; 256 * 224]);
    }

    #[test]
    fn the_intros_lines_are_drawn_from_column_six_on_the_rows_of_their_english_letters() {
        let overlay = intro_overlay();
        let screen = intro_screen();
        let draw = |cells: &[TextCell]| {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_intro(&overlay, &screen, &mut target, cells, 0);
            rgba
        };
        let inked = |rgba: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>| {
            xs.clone().any(|x| ys.clone().any(|y| pixel(rgba, x, y) != [7, 7, 7]))
        };
        let rgba = draw(&[title_letter(0, 40, 0), title_letter(3, 100, 1)]);
        assert!(inked(&rgba, 96..160, 80..112), "line 0: AB from column 6, rows 40 to 56");
        assert!(inked(&rgba, 96..130, 200..232), "line 16: C on BG2's row 100");
        assert!(!inked(&rgba, 0..96, 0..448), "nothing left of column 6");
        assert!(!inked(&rgba, 0..512, 112..200), "line 1 has no row: nothing drawn for it");
        // No English letter of the intro on the screen: nothing to draw over
        let rgba = draw(&[TextCell { bold: false, ..title_letter(0, 40, 0) }]);
        assert!(!inked(&rgba, 0..512, 0..448));
    }

    #[test]
    fn the_endings_captions_take_eighteen_characters_on_every_page() {
        // The ending's pages are as wide as its longest English line (27 cells) whatever their own: a page whose
        // English is one short line (`"!...vegeta!"`) still has lines of 18 characters of the 12-pixel em, the
        // longest of the zh-tw table's ending rows
        assert_eq!(window_cells(ENDING_TABLE, 13), ENDING_CELLS);
        assert_eq!(window_cells(ENDING_TABLE, 27), 27);
        let width = (8 * window_cells(ENDING_TABLE, 13)) as f32;
        assert_eq!(wrap(&"長".repeat(18), width, |_| EM).len(), 1);
        assert_eq!(wrap(&"長".repeat(19), width, |_| EM).len(), 2);
    }

    #[test]
    fn the_cursor_before_a_choice_keeps_the_picture() {
        // The shop's `"Anything else?"` / `▶YES` / `NO`: the cursor's cell has no text tile, a blank cell of the
        // window left of it and the choice's letter right of it
        let overlay = overlay();
        let mut kept = Kept::default();
        let blank = |x: i32| TextCell { x, y: 200, glyph: BLANK, ..letter() };
        let choice = TextCell { x: 56, y: 200, ..letter() };
        let end = TextCell { x: 112, y: 200, ..blank(112) };
        let cells = [letter(), blank(40), choice, end];
        let rgba = drawn(&overlay, &mut kept, Some(&OnScreen { state: 6, ..screen(1.0) }), &cells);
        assert_eq!(pixel(&rgba, 2 * 48 + 3, 2 * 200 + 3), [7, 7, 7], "the cursor shows");
        assert_eq!(
            pixel(&rgba, 2 * 48 + 3, 2 * 200 - 6),
            PAPER,
            "the window's ground above it is painted"
        );
        assert_eq!(pixel(&rgba, 2 * 40 + 3, 2 * 200 + 3), PAPER, "the blank cell left of it too");
        assert_eq!(pixel(&rgba, 2 * 56 + 3, 2 * 200 + 3), PAPER, "and the choice's English letter");
        assert_eq!(
            pixel(&rgba, 2 * 120 + 3, 2 * 200 + 3),
            PAPER,
            "a cell with no tile and no letter after it"
        );
    }

    #[test]
    fn a_mark_with_no_glyph_after_a_letter_is_no_cursor() {
        // Vegeta's retreat (`str2:0x076`), its third line ` way…"`: the `…` has no glyph of the font, so its cell has
        // no text tile, between the `y` and the closing `"`. It is the English text, not the cursor before a choice:
        // it takes the window's ground with the rest of the page
        let overlay = overlay();
        let mut kept = Kept::default();
        let y = TextCell { x: 40, y: 200, ..letter() };
        let quote = TextCell { x: 56, y: 200, ..letter() };
        let cells = [letter(), y, quote];
        let rgba = drawn(&overlay, &mut kept, Some(&OnScreen { state: 6, ..screen(1.0) }), &cells);
        assert_eq!(pixel(&rgba, 2 * 48 + 3, 2 * 200 + 3), PAPER, "the mark's cell is painted");
        assert_eq!(pixel(&rgba, 2 * 56 + 3, 2 * 200 + 3), PAPER, "and the quote after it");
    }

    #[test]
    fn a_finished_page_the_engine_names_again_from_its_start_stays_written() {
        // The shop's opening page while the player picks `BUY` or `SELL`: written, a timed wait (state 6), then the
        // engine names it again at its start (state 1, line 0, cell 0) while the window keeps its letters
        let overlay = overlay();
        let start = OnScreen { cell: 0, ..screen(0.0) };
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&OnScreen { state: 6, ..screen(1.0) }), &[letter()]);
        drawn(&overlay, &mut kept, None, &[letter()]);
        for _ in 0..4 * KEEP {
            let again = drawn(&overlay, &mut kept, Some(&start), &[letter()]);
            assert!(inked(&again), "the page as written");
        }
        let cleared = drawn(&overlay, &mut kept, Some(&start), &[]);
        assert_eq!(
            pixel(&cleared, 2 * 32 + 2, 2 * 184 + 2),
            [7, 7, 7],
            "nothing in a cleared window"
        );
        // A page not finished before is not written yet at its start
        let mut kept = Kept::default();
        drawn(&overlay, &mut kept, Some(&screen(0.5)), &[letter()]);
        assert!(!inked(&drawn(&overlay, &mut kept, Some(&start), &[letter()])));
    }

    #[test]
    fn a_column_of_words_a_frames_tile_cuts_the_first_letter_of_takes_its_ground() {
        // The save's `Yes` / `No` under the slot window: a tile of its frame (x 24) over the `Y` and the `N`, a blank
        // cell of the slot window left of it (x 16). The frame's tile is no text tile, or a tile the HD font has no
        // letter for (glyph 9: the save screen, the frame's tile 228)
        const LETTERS: &str = " eosYN";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, y: i32, ch: char| {
            let glyph = u8::try_from(LETTERS.find(ch).unwrap()).unwrap();
            TextCell { x, y, glyph, fg: INK, bg: PAPER, ..TextCell::default() }
        };
        // `frame`: 0 no tile at x 24, 1 a tile without a letter, 2 a blank cell (no frame)
        let row = |y: i32, frame: u8, text: &str| -> Vec<TextCell> {
            let mut row = vec![cell(16, y, ' ')];
            match frame {
                1 => row.push(TextCell {
                    x: 24,
                    y,
                    glyph: 9,
                    fg: INK,
                    bg: PAPER,
                    ..TextCell::default()
                }),
                2 => row.push(cell(24, y, ' ')),
                _ => {}
            }
            row.extend((0..).zip(text.chars()).map(|(at, ch)| cell(32 + 8 * at, y, ch)));
            row
        };
        let names = [("Yes", "OK"), ("No", "NG")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        // Of the cells of `es` (x 32, 40, y 8) and `o` (x 32, y 24): whether each is all ground; and whether the blank
        // cell after `o` (x 40, y 24) holds no ink
        let painted = |cells: &[TextCell]| -> (Vec<bool>, bool) {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            assert_eq!(
                pixel(&rgba, 2 * 24 + 15, 2 * 8 + 3),
                [7, 7, 7],
                "the frame's tile keeps its pixels"
            );
            let ground = |x: usize, y: usize| {
                (2 * x..2 * x + 16)
                    .all(|px| (2 * y..2 * y + 16).all(|py| pixel(&rgba, px, py) == PAPER))
            };
            let after = (2 * 40..2 * 48)
                .all(|px| (2 * 24..2 * 32).all(|py| pixel(&rgba, px, py) == [7, 7, 7]));
            ([(32, 8), (40, 8), (32, 24)].map(|(x, y)| ground(x, y)).to_vec(), after)
        };
        for frame in [0, 1] {
            let both = [row(8, frame, "es "), row(24, frame, "o  ")].concat();
            assert_eq!(
                painted(&both),
                (vec![true, true, true], true),
                "what shows of each takes its ground, no name drawn (frame {frame})"
            );
            assert_eq!(
                painted(&row(8, frame, "es ")),
                (vec![false, false, false], true),
                "a word alone is left alone (frame {frame})"
            );
        }
        let open = [row(8, 2, "es "), row(24, 2, "o  ")].concat();
        assert_eq!(
            painted(&open),
            (vec![false, false, false], true),
            "no frame's tile before them: words of their own"
        );
    }

    #[test]
    fn a_column_of_names_the_game_blanks_the_first_letter_of_after_their_place_takes_its_ground() {
        // The Order window as it closes: `1Piccolo` / `2Goku`, the game writes a blank over each name's first letter
        // (`1 iccolo` / `2 oku`)
        const LETTERS: &str = " 12Pickolgu";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let names = [("Piccolo", "QQ"), ("Goku", "RR")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let grounds = |cells: &[TextCell]| -> [bool; 4] {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            let ground = |x: usize, y: usize| {
                (2 * x..2 * x + 16)
                    .all(|px| (2 * y..2 * y + 16).all(|py| pixel(&rgba, px, py) == PAPER))
            };
            let kept = |x: usize, y: usize| {
                (2 * x..2 * x + 16)
                    .all(|px| (2 * y..2 * y + 16).all(|py| pixel(&rgba, px, py) == [7, 7, 7]))
            };
            // `iccolo`'s first and last cells, `oku`'s first, the place `1` kept
            [ground(48, 8), ground(88, 8), ground(48, 24), kept(32, 8)]
        };
        let cut = [spelled(LETTERS, 32, 8, "1 iccolo"), spelled(LETTERS, 32, 24, "2 oku")].concat();
        assert_eq!(grounds(&cut), [true, true, true, true], "the names' letters take their ground");
        assert_eq!(
            grounds(&spelled(LETTERS, 32, 8, "1 iccolo")),
            [false, false, false, true],
            "a row alone is left alone"
        );
        let unplaced =
            [spelled(LETTERS, 32, 8, "  iccolo"), spelled(LETTERS, 32, 24, "  oku")].concat();
        assert_eq!(
            grounds(&unplaced),
            [false, false, false, true],
            "no place right before the blank: words of their own"
        );
    }

    #[test]
    fn a_cut_word_takes_the_ground_of_its_cell_and_round_it_only_where_the_frame_is_ground() {
        // The save's `es` / `o` with the slot window's frame (a tile without a letter) at x 24; the 1x frame has a
        // line of the frame's colour right of `s` (x 48) and its ground elsewhere
        const LETTERS: &str = " eosYN";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let frame_tile =
            |y: i32| TextCell { x: 24, y, glyph: 9, fg: INK, bg: PAPER, ..TextCell::default() };
        let cells = [
            vec![frame_tile(8)],
            spelled(LETTERS, 32, 8, "es"),
            vec![frame_tile(24)],
            spelled(LETTERS, 32, 24, "o"),
        ]
        .concat();
        let names = [("Yes", "OK"), ("No", "NG")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let mut frame = vec![Color::rgb(PAPER[0], PAPER[1], PAPER[2]); 256 * 224];
        for y in 0..224 {
            frame[y * 256 + 48] = Color::rgb(0xF8, 0x58, 0x00);
        }
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &frame, 256, &character);
        assert_eq!(
            pixel(&rgba, 2 * 48, 2 * 10),
            [7, 7, 7],
            "the frame's line right of the cell keeps its picture"
        );
        assert_eq!(
            pixel(&rgba, 2 * 40, 2 * 7),
            PAPER,
            "a ground pixel above the cell takes the ground"
        );
        assert_eq!(pixel(&rgba, 2 * 40, 2 * 10), PAPER, "the cell takes the ground");
    }

    #[test]
    fn a_name_in_a_column_of_names_is_drawn_beside_a_word_that_is_none() {
        // The Item screen's labels with object 27 in the list: `EXT` (x 32, y 8), and a row below the original's
        // `RSa` spilled out of the list (x 0-16), the cursor (no text tile, x 24) and `AID` (x 32)
        const LETTERS: &str = " AIDEXTRSa";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let names = [("EXT", "QQQ"), ("AID", "RRR")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let painted = |cells: &[TextCell]| -> [bool; 2] {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            [8, 32].map(|x| pixel(&rgba, 2 * x + 1, 2 * 24 + 3) != [7, 7, 7])
        };
        let row = [spelled(LETTERS, 0, 24, "RSa"), spelled(LETTERS, 32, 24, "AID")].concat();
        assert_eq!(
            painted(&[spelled(LETTERS, 32, 8, "EXT"), row.clone()].concat()),
            [false, true],
            "AID named under EXT, RSa kept"
        );
        assert_eq!(
            painted(&row),
            [false, false],
            "no name in its column: words of a sentence, left alone"
        );
        assert_eq!(
            painted(&[spelled(LETTERS, 40, 8, "EXT"), row].concat()),
            [false, false],
            "a name above in another column"
        );
    }

    #[test]
    fn a_word_a_window_cuts_at_its_end_under_a_name_of_its_column_is_hidden() {
        // The Item screen's `EXT` (x 32, y 8) and, a row below, the `A` of `AID` with the frame of the "can't use"
        // message right after it (no text tile at x 40)
        const LETTERS: &str = " ADEINTXQ";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, y: i32, ch: char| {
            let glyph = u8::try_from(LETTERS.find(ch).unwrap()).unwrap();
            TextCell { x, y, glyph, fg: INK, bg: PAPER, ..TextCell::default() }
        };
        let word = |y: i32, text: &str| -> Vec<TextCell> {
            (0..).zip(text.chars()).map(|(at, ch)| cell(32 + 8 * at, y, ch)).collect()
        };
        let names = [("EXT", "MAP"), ("AID", "ETC"), ("INT", "WAR")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        // Whether the `A` cell (x 32, y 24) took the ground and holds no ink
        let hidden = |cells: &[TextCell]| -> bool {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            (2 * 32..2 * 40).all(|x| (2 * 24..2 * 32).all(|y| pixel(&rgba, x, y) == PAPER))
        };
        assert!(hidden(&[word(8, "EXT"), word(24, "A")].concat()), "the cut word is hidden");
        assert!(
            !hidden(&[word(8, "EXT"), word(24, "A ")].concat()),
            "a blank after it: a word of its own"
        );
        assert!(!hidden(&[word(8, "QQQ"), word(24, "A")].concat()), "no name above it");
        assert!(!hidden(&[word(8, "EXT"), word(24, "Q")].concat()), "no name starts with it");
        assert!(
            !hidden(&[word(0, "EXT"), word(24, "A")].concat()),
            "the name above is farther than 16 pixels"
        );
        let shifted: Vec<TextCell> =
            word(24, "A").into_iter().map(|c| TextCell { x: c.x + 8, ..c }).collect();
        assert!(
            !(2 * 40..2 * 48).all(|x| (2 * 24..2 * 32).all(|y| {
                let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
                let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
                draw_names(
                    &overlay,
                    &mut target,
                    &[word(8, "EXT"), shifted.clone()].concat(),
                    &[],
                    0,
                    &character,
                );
                pixel(&rgba, x, y) == PAPER
            })),
            "another column"
        );
    }

    #[test]
    fn words_a_tile_without_a_letter_sets_apart_are_named_each_and_the_others_kept() {
        // `KI-USE`: `KI` at x 32, the dash (a tile the HD font has no letter for, glyph 9) at x 48, `USE` at x 56
        const LETTERS: &str = " KIUSE";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, glyph: u8| TextCell {
            x,
            y: 8,
            glyph,
            fg: INK,
            bg: PAPER,
            ..TextCell::default()
        };
        let names = [("USE", "QQQ")].map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let painted = |cells: &[TextCell]| -> [bool; 2] {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            [32, 56].map(|x| pixel(&rgba, 2 * x + 1, 2 * 8 + 3) != [7, 7, 7])
        };
        let word = [cell(32, 1), cell(40, 2), cell(56, 3), cell(64, 4), cell(72, 5)];
        let marked = [word.as_slice(), &[cell(48, 9)]].concat();
        assert_eq!(painted(&marked), [false, true], "USE named, KI kept");
        assert_eq!(
            painted(&word),
            [false, false],
            "no tile between them: words of a sentence, left alone"
        );
    }

    /// The cells of `text` from x `x` on row `y`, each letter the glyph of its place in `letters`.
    fn spelled(letters: &str, x: i32, y: i32, text: &str) -> Vec<TextCell> {
        (0..)
            .zip(text.chars())
            .map(|(at, ch)| TextCell {
                x: x + 8 * at,
                y,
                glyph: u8::try_from(letters.find(ch).unwrap()).unwrap(),
                fg: INK,
                bg: PAPER,
                ..TextCell::default()
            })
            .collect()
    }

    #[test]
    fn the_words_either_side_of_a_dash_the_hd_font_spells_are_named_each_and_the_others_kept() {
        // `KI-USE` as the game shows it (the battle's technique list, the Stats page 2): the HD font spells the
        // English patch's tile $43 between the words, a dash (`-`), so the three are one run of letters
        const LETTERS: &str = " KIUSE-0248";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let names = [("USE", "QQQ")].map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let cells = spelled(LETTERS, 32, 8, "KI-USE    008");
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let painted = |x: usize| pixel(&rgba, 2 * x + 8, 2 * 8 + 3) != [7, 7, 7];
        assert_eq!(
            [32, 40, 48, 56, 64, 72, 112].map(painted),
            [false, false, true, true, true, true, false],
            "USE named, the dash with it; KI and the figure kept"
        );
    }

    #[test]
    fn the_mark_between_two_words_goes_with_the_right_one_when_it_is_named() {
        // `KI-USE` with `USE` named (`KI-消耗`): the HD font's dash (tile $43) between the words takes its ground and
        // the name starts on its cell, right after `KI` (`KI消耗`, one word as the HD Stats page's `ＫＩ消耗`); with
        // only `KI` named, the dash stays. A tile the font has no letter for between them stays too: it may be a
        // window's frame between two windows' words (the Menu's `Order` and the window over it)
        const LETTERS: &str = " KIUSE-";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, glyph: u8| TextCell {
            x,
            y: 8,
            glyph,
            fg: INK,
            bg: PAPER,
            ..TextCell::default()
        };
        let word = [cell(32, 1), cell(40, 2), cell(56, 3), cell(64, 4), cell(72, 5)];
        let dashed = [word.as_slice(), &[cell(48, 6)]].concat();
        let marked = [word.as_slice(), &[cell(48, 9)]].concat();
        let drawn = |names: &[(&str, &str)], cells: &[TextCell]| {
            let names = names.iter().map(|&(english, name)| (english.to_owned(), name.to_owned()));
            let overlay = overlay().with_names(names.collect());
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            rgba
        };
        // (the 2x pixels of a 1x cell of the row: the grey of the picture, the paper, the name's ink)
        let shows = |rgba: &[u8], x: usize, colour: [u8; 3]| {
            (2 * 8..2 * 16).any(|y| (2 * x..2 * x + 16).any(|x| pixel(rgba, x, y) == colour))
        };
        let rgba = drawn(&[("USE", "QQQ")], &dashed);
        assert!(!shows(&rgba, 48, [7, 7, 7]), "the dash's cell takes its ground");
        assert!(shows(&rgba, 48, INK), "the name starts on the dash's cell");
        assert!([32, 40].iter().all(|&x| !shows(&rgba, x, PAPER)), "KI keeps its letters");
        let rgba = drawn(&[("USE", "QQQ")], &marked);
        assert!(
            (2 * 8..2 * 16).all(|y| (2 * 48..2 * 55).all(|x| pixel(&rgba, x, y) == [7, 7, 7])),
            "a tile with no letter keeps its picture (its last column: the name's ground reaching a pixel left)"
        );
        assert!(shows(&rgba, 56, INK), "a tile with no letter: the name on USE's cells");
        for (cells, what) in [(&dashed, "the HD font's dash"), (&marked, "a tile with no letter")] {
            // (a name's ground reaches a pixel right of its last cell: the mark's first column)
            let rgba = drawn(&[("KI", "QQ")], cells);
            assert!(
                (2 * 8..2 * 16).all(|y| (2 * 49..2 * 56).all(|x| pixel(&rgba, x, y) == [7, 7, 7])),
                "{what}: KI named alone, the mark stays"
            );
        }
    }

    #[test]
    fn the_stats_pages_labels_are_named_beside_their_figures() {
        // The original's Stats page 2 (`docs/re/data/menus.md`): `PLA.` a blank cell before its figure; the
        // string `EX    UP.` with the page's `LV` written into its blanks; `EX` and its figure, no name
        const LETTERS: &str = " PLA.04EXVU2735";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let names = [("PLA.", "QQ"), ("EX    UP.", "RRRRRR")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let cells = [
            spelled(LETTERS, 32, 8, "PLA. 04  "),
            spelled(LETTERS, 32, 24, "EX 0022733"),
            spelled(LETTERS, 32, 40, "EX LV UP. "),
        ]
        .concat();
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let painted = |x: usize, y: usize| pixel(&rgba, 2 * x + 8, 2 * y + 3) != [7, 7, 7];
        assert_eq!([32, 56, 72].map(|x| painted(x, 8)), [true, true, false], "PLA. named, 04 kept");
        assert_eq!(
            [32, 56, 72, 96].map(|x| painted(x, 40)),
            [true, true, true, true],
            "EX LV UP. named whole, its LV too"
        );
        assert_eq!(
            [32, 56, 80].map(|x| painted(x, 24)),
            [false, false, false],
            "a name's blanks hold letters, not a figure: EX and its figure kept"
        );
    }

    #[test]
    fn a_dash_right_after_a_name_with_nothing_after_it_goes_with_the_name() {
        // The Stats page 2's title: the member's name and the string `<43>` after it (`Krillin-`, `STATS` a row
        // below); the Chinese has no dash between the two
        const LETTERS: &str = " Kriln-";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let names =
            [("Krillin", "QQ")].map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let cells = spelled(LETTERS, 32, 8, "Krillin- ");
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_names(&overlay, &mut target, &cells, &[], 0, &character);
        let painted = |x: usize| pixel(&rgba, 2 * x + 8, 2 * 8 + 3) != [7, 7, 7];
        assert_eq!(
            [32, 88].map(painted),
            [true, true],
            "the name drawn, the dash's cell its ground"
        );
    }

    #[test]
    fn a_row_a_lists_cursor_cuts_the_first_letter_of_is_named_by_the_names_of_its_column() {
        // The shop's SELL list: `Bulma` at x 32 with the cursor (no text tile) over its `B`, so `ulma` from x 40, a
        // blank cell of the window at x 24; `Radar` a row below from x 32
        const LETTERS: &str = " BulmaRdr";
        let character = |glyph: u8| LETTERS.chars().nth(usize::from(glyph));
        let cell = |x: i32, y: i32, ch: char| {
            let glyph = u8::try_from(LETTERS.find(ch).unwrap()).unwrap();
            TextCell { x, y, glyph, fg: INK, bg: PAPER, ..TextCell::default() }
        };
        let word = |x: i32, y: i32, text: &str| -> Vec<TextCell> {
            (0..).zip(text.chars()).map(|(at, ch)| cell(x + 8 * at, y, ch)).collect()
        };
        let names = [("Bulma", "QQ"), ("Radar", "RR")]
            .map(|(english, name)| (english.to_owned(), name.to_owned()));
        let overlay = overlay().with_names(names.into_iter().collect());
        let painted = |cells: &[TextCell]| -> bool {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            pixel(&rgba, 2 * 40 + 1, 2 * 8 + 3) != [7, 7, 7]
        };
        let cut = [vec![cell(24, 8, ' ')], word(40, 8, "ulma")].concat();
        let listed = [cut.clone(), vec![cell(24, 24, ' ')], word(32, 24, "Radar")].concat();
        assert!(painted(&listed), "the cut row is Bulma's");
        // The game blanks the cut letter (a blank tile under the cursor's sprite)
        let blanked = [vec![cell(32, 8, ' ')], listed.clone()].concat();
        assert!(painted(&blanked), "the cut letter's cell a blank: the same");
        assert!(
            !painted(&cut),
            "alone, with no name of its list in the cut letter's column, it is left alone"
        );
        let far = [cut.clone(), word(32, 72, "Radar")].concat();
        assert!(!painted(&far), "a name more than two rows away is another list's");
        let beside = [cut.clone(), word(96, 8, "Radar")].concat();
        assert!(painted(&beside), "a name of the next column on its row: the same list");
        // `ulma` at x 88 with `Radar` left of it on its row
        let shown_at = |cells: &[TextCell], x: usize| -> bool {
            let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
            let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
            draw_names(&overlay, &mut target, cells, &[], 0, &character);
            pixel(&rgba, 2 * x + 1, 2 * 8 + 3) != [7, 7, 7]
        };
        let before = [vec![cell(72, 8, ' ')], word(88, 8, "ulma"), word(0, 8, "Radar")].concat();
        assert!(!shown_at(&before, 88), "a name left of it on its row is no column after it");
    }

    #[test]
    fn a_column_of_commas_is_drawn_as_the_rule_and_a_lone_comma_is_left_alone() {
        let ink = [10, 20, 30];
        let ground = [200, 190, 180];
        let comma = |x, y| TextCell { x, y, glyph: 7, fg: ink, bg: ground, ..TextCell::default() };
        // A rule two cells tall at x 0, and a sentence's comma at x 16
        let cells = [comma(0, 0), comma(0, 8), comma(16, 0)];
        let mut rgba = vec![0_u8; 24 * 16 * 4];
        let mut target = Target { rgba: &mut rgba, width: 24, scale: 1 };
        draw_rules(&mut target, &cells, &|glyph| (glyph == 7).then_some(','));
        let at = |x: usize, y: usize| -> [u8; 3] {
            let i = (y * 24 + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        for y in 0..16 {
            assert_eq!(at(3, y), ink, "the rule's line runs down the fourth column, row {y}");
            assert_eq!(at(0, y), ground, "the rest of the rule's cells is ground, row {y}");
            assert_eq!(at(19, y), [0, 0, 0], "the lone comma keeps its picture, row {y}");
        }
    }

    #[test]
    fn a_lone_comma_in_the_column_of_a_rule_is_a_piece_of_the_rule() {
        let ink = [10, 20, 30];
        let ground = [200, 190, 180];
        let comma = |x, y| TextCell { x, y, glyph: 7, fg: ink, bg: ground, ..TextCell::default() };
        // A rule at x 0 rows 0-15, a window over rows 16-39, one cell of the rule showing at row 40; a comma at x 16
        let cells = [comma(0, 0), comma(0, 8), comma(0, 40), comma(16, 40)];
        let mut rgba = vec![0_u8; 24 * 48 * 4];
        let mut target = Target { rgba: &mut rgba, width: 24, scale: 1 };
        draw_rules(&mut target, &cells, &|glyph| (glyph == 7).then_some(','));
        let at = |x: usize, y: usize| -> [u8; 3] {
            let i = (y * 24 + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        for y in 40..48 {
            assert_eq!(at(3, y), ink, "the lone cell of the rule is the line, row {y}");
            assert_eq!(at(0, y), ground, "and ground beside it, row {y}");
            assert_eq!(
                at(19, y),
                [0, 0, 0],
                "a comma of another column keeps its picture, row {y}"
            );
        }
        assert_eq!(
            at(3, 24),
            [0, 0, 0],
            "nothing is drawn where no comma is (the window over the rule)"
        );
    }

    #[test]
    fn commas_two_rows_apart_are_text_lines_not_a_rule() {
        let comma =
            |y| TextCell { x: 0, y, glyph: 7, fg: [1, 1, 1], bg: [2, 2, 2], ..TextCell::default() };
        let cells = [comma(0), comma(16)];
        let mut rgba = vec![0_u8; 8 * 24 * 4];
        let mut target = Target { rgba: &mut rgba, width: 8, scale: 1 };
        draw_rules(&mut target, &cells, &|_| Some(','));
        assert!(
            rgba.iter().all(|&byte| byte == 0),
            "lines of text are 16 pixels apart: nothing is drawn"
        );
    }

    /// The chase's label at column 3 of a row, in the window's frame tiles and blanks (`00:C46D`), the digits after
    /// it; the row above with `゛` over the label's first and fourth tiles when `marks` (`00:C455`).
    fn chase_row(marks: bool) -> TileRow {
        let mut tiles = vec![0x01_u16; 33];
        tiles[..14].copy_from_slice(&[
            0x35, 0x01, 0x01, 0x53, 0x65, 0x43, 0x4F, 0x7D, 0x7D, 0x80, 0x80, 0x01, 0x01, 0x36,
        ]);
        let mut above = vec![0x01_u16; 33];
        above[0] = 0x35;
        above[13] = 0x36;
        if marks {
            above[3] = DAKUTEN;
            above[6] = DAKUTEN;
        }
        TileRow { x: 0, y: 136, tiles, above }
    }

    #[test]
    fn the_chase_damage_label_is_its_six_tiles_under_their_two_marks() {
        assert_eq!(chase_damage_at(&chase_row(true)), Some(3));
    }

    #[test]
    fn the_six_tiles_without_the_marks_over_them_or_other_tiles_are_not_the_label() {
        assert_eq!(chase_damage_at(&chase_row(false)), None, "no ゛ above");
        let mut one = chase_row(true);
        one.above[6] = 0x01;
        assert_eq!(chase_damage_at(&one), None, "one ゛ only");
        let mut moved = chase_row(true);
        moved.above.swap(6, 7);
        assert_eq!(chase_damage_at(&moved), None, "the second ゛ over another tile");
        let mut other = chase_row(true);
        other.tiles[8] = 0x01;
        assert_eq!(chase_damage_at(&other), None, "the run ends early");
        let mut shifted = chase_row(true);
        shifted.tiles[4] = 0x66;
        assert_eq!(chase_damage_at(&shifted), None, "another tile in the run");
    }

    /// A frame log of 224 lines of Mode 1 with BG3 only, scrolled `h` pixels right, its tile map (at `$0400`) holding
    /// the chase's window rows of [`chase_row`] at tile rows 16 (above) and 17.
    fn chase_log(marks: bool, h: u16) -> FrameLog {
        chase_log_scrolled(marks, h, 0)
    }

    /// [`chase_log`] with the BG scrolled `v` lines up too.
    fn chase_log_scrolled(marks: bool, h: u16, v: u16) -> FrameLog {
        use ssd_oracle::{LineEntry, LineRegs, SpriteLine};
        let regs = LineRegs {
            brightness: 15,
            bg_mode: 1,
            main_bg: [false, false, true, false],
            bg_map_base: [0, 0, 0x0400, 0],
            bg_char_base: [0, 0, 0x1000, 0],
            bg_h_scroll: [0, 0, h, 0],
            bg_v_scroll: [0, 0, v, 0],
            ..LineRegs::default()
        };
        let entries = (1..=224)
            .map(|scanline| LineEntry {
                scanline,
                from_pixel: 0,
                regs,
                sprites: SpriteLine::default(),
            })
            .collect();
        let mut vram = vec![0_u16; 32768];
        let row = chase_row(marks);
        for column in 0..32 {
            vram[0x0400 + 16 * 32 + column] = 0x2800 | row.above[column];
            vram[0x0400 + 17 * 32 + column] = 0x2400 | row.tiles[column];
        }
        FrameLog {
            complete: true,
            height: 224,
            h_hi_res: false,
            v_hi_res: false,
            entries,
            cgram_start: vec![0; 256],
            cgram_writes: Vec::new(),
            vram,
            vram_writes: Vec::new(),
            oam_low: vec![0; 512],
            oam_high: vec![0; 32],
            oam_writes: Vec::new(),
        }
    }

    #[test]
    fn the_tile_rows_of_a_frame_are_read_off_its_tile_map_where_the_screen_shows_them() {
        let rows = tile_rows(&chase_log(true, 0));
        assert_eq!(rows.len(), 28, "one a tile row of BG3");
        let found: Vec<(i32, i32, usize)> = rows
            .iter()
            .filter_map(|row| chase_damage_at(row).map(|at| (row.x, row.y, at)))
            .collect();
        // the row is the BG's lines 136 to 143: scanline 136 shows its first, the frame's row 135
        assert_eq!(found, [(0, 135, 3)]);
        // scrolled 12 pixels right: the label's first tile is at x 24 - 12, the row's first tile half off the left
        let rows = tile_rows(&chase_log(true, 12));
        let found: Vec<(i32, i32, usize)> = rows
            .iter()
            .filter_map(|row| chase_damage_at(row).map(|at| (row.x, row.y, at)))
            .collect();
        assert_eq!(found, [(-4, 135, 2)]);
        // scrolled 3 lines up: the BG's line 136 is on scanline 133, the frame's row 132
        let rows = tile_rows(&chase_log_scrolled(true, 0, 3));
        let found: Vec<(i32, i32, usize)> = rows
            .iter()
            .filter_map(|row| chase_damage_at(row).map(|at| (row.x, row.y, at)))
            .collect();
        assert_eq!(found, [(0, 132, 3)]);
        assert!(tile_rows(&chase_log(false, 0)).iter().all(|row| chase_damage_at(row).is_none()));
    }

    /// The renderer is the reference: the text cells are noted from what it draws (`font.rs`, a cell's top is its
    /// pixel's row less the pixel's line in its tile). Where a tile row is said to be, the renderer draws the first
    /// line of the label's first tile on that row of the frame, and the last line of the tile above on the row
    /// above it. A row one line off (the scanline taken for the BG's line) finds no cell of the label on its row,
    /// and the chase's label is left in the original's garbage.
    #[test]
    fn the_tile_rows_are_where_the_renderer_draws_their_tiles() {
        for (h, v) in [(0_u16, 0_u16), (12, 0), (0, 3), (5, 6), (0, 0xFFF9)] {
            let log = chase_log_scrolled(true, h, v);
            let found: Vec<(TileRow, usize)> = tile_rows(&log)
                .into_iter()
                .filter_map(|row| chase_damage_at(&row).map(|at| (row, at)))
                .collect();
            assert_eq!(found.len(), 1, "the label once, scrolled {h},{v}");
            let (row, at) = &found[0];
            let x = usize::try_from(row.x + 8 * *at as i32).unwrap();
            let regs = log.entries[0].regs;
            let drawn = |frame_row: i32| {
                let mut renderer = crate::Renderer::new();
                let scanline = u16::try_from(frame_row + 1).unwrap();
                renderer.render_bg_layers(&log.vram, &regs, scanline, 0);
                let pixel = renderer.bg_pixels[2][x];
                (pixel.char, pixel.pos >> 3 & 7)
            };
            assert_eq!(
                drawn(row.y),
                (0x1000 + 8 * CHASE_DAMAGE[0], 0),
                "the label's top line, scrolled {h},{v}"
            );
            assert_eq!(
                drawn(row.y - 1),
                (0x1000 + 8 * DAKUTEN, 7),
                "the mark's bottom line, scrolled {h},{v}"
            );
        }
    }

    /// The made-up label of a test table (the tests' font is Latin) and a picture with the chase's window row of
    /// [`chase_row`]: the label's `s` and the two digits after it are text cells.
    fn chase_drawn(table: &str, rows: &[TileRow]) -> Vec<u8> {
        let table = Translation::parse(table).unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        let overlay = Overlay::new(Arc::new(table), Arc::new(face));
        let cell = |x: i32, glyph: u8| TextCell {
            x,
            y: 136,
            glyph,
            fg: INK,
            bg: PAPER,
            mask: !0,
            ..TextCell::default()
        };
        let cells = [cell(24, 2), cell(72, 3), cell(80, 3)];
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_chase_damage(&overlay, &mut target, &cells, rows, 0);
        rgba
    }

    #[test]
    fn the_chase_damage_label_is_painted_over_and_its_translation_drawn_and_the_digits_stay() {
        let rgba = chase_drawn("key\tzh\nscreen:chase_damage\tDMG..\n", &[chase_row(true)]);
        let inked = |x0: usize, x1: usize, y0: usize, y1: usize| {
            (x0..x1).any(|x| (y0..y1).any(|y| pixel(&rgba, x, y)[2] < 0x80))
        };
        assert_eq!(pixel(&rgba, 2 * 24, 2 * 136), PAPER, "the label's first cell takes its ground");
        assert_eq!(pixel(&rgba, 2 * 71 + 1, 2 * 143 + 1), PAPER, "to its sixth");
        assert_eq!(pixel(&rgba, 2 * 24 + 1, 2 * 128 + 1), PAPER, "the ゛ over the first");
        assert_eq!(pixel(&rgba, 2 * 48 + 1, 2 * 128 + 1), PAPER, "and over the fourth");
        assert_eq!(
            pixel(&rgba, 2 * 32 + 1, 2 * 128 + 1),
            [7, 7, 7],
            "the row above is left but for them"
        );
        assert!(
            inked(2 * 24, 2 * 72, 2 * 135, 2 * 145),
            "the translation is drawn over the six cells"
        );
        assert!(
            (2 * 72..2 * 88).all(|x| (2 * 136..2 * 144).all(|y| pixel(&rgba, x, y) == [7, 7, 7])),
            "the digits stay"
        );
    }

    #[test]
    fn without_the_key_or_without_the_marks_nothing_is_drawn() {
        let untouched = |rgba: &[u8]| rgba.chunks(4).all(|px| px == [7, 7, 7, 255]);
        assert!(
            untouched(&chase_drawn("key\tzh\nstr3:0x00\tAB\n", &[chase_row(true)])),
            "a table without the key"
        );
        let table = "key\tzh\nscreen:chase_damage\tDMG..\n";
        assert!(
            untouched(&chase_drawn(table, &[chase_row(false)])),
            "the tiles without their marks"
        );
    }

    /// The font of the zh-tw language pack (`translations/zh-tw/fonts/`), as the app reads it.
    fn wenkai_tc() -> ChineseFace {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../translations/zh-tw/fonts/LXGWWenKaiTC-Regular.ttf"
        );
        ChineseFace::new(std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn the_font_of_the_language_pack_draws_wei_in_the_taiwan_form_not_the_inherited_one() {
        // LXGW WenKai TC 1.530 draws U+70BA 為 and U+7232 爲 with the one glyph of the inherited form (爫 on top);
        // the pack's font has 為 in the form of Taiwan's standard (that of LXGW WenKai 1.530), 爲 left as it was.
        let face = wenkai_tc();
        let (wei, inherited) = (face.glyph('為', 4), face.glyph('爲', 4));
        assert!(wei.alpha.iter().any(|&a| a > 0), "為 has ink");
        assert!(inherited.alpha.iter().any(|&a| a > 0), "爲 has ink");
        assert!(
            (wei.w, wei.h, &wei.alpha) != (inherited.w, inherited.h, &inherited.alpha),
            "為 is not drawn as 爲"
        );
        assert_eq!(
            face.advance('為'),
            face.advance('爲'),
            "為 takes the width of a full-width character as before"
        );
    }

    // ---- presence: is a page's English on the screen, cell by cell

    /// The test font of [`presence`]'s tests: glyph `n` (1 to 26) is the letter `n` of the alphabet, 27 a comma, 28 a
    /// full stop, 29 a quote; 0 is the blank cell.
    fn abc(glyph: u8) -> Option<char> {
        match glyph {
            BLANK => Some(' '),
            1..=26 => Some(char::from(b'a' + glyph - 1)),
            27 => Some(','),
            28 => Some('.'),
            29 => Some('"'),
            _ => None,
        }
    }

    /// The cell of `ch` (a letter, `,`, `.`, `"`, or ' ' for a blank cell) at (`x`, `y`) on the ground `bg`.
    fn cell_of(x: i32, y: i32, ch: char, bg: [u8; 3]) -> TextCell {
        let glyph = match ch {
            ' ' => BLANK,
            ',' => 27,
            '.' => 28,
            '"' => 29,
            _ => ch as u8 - b'a' + 1,
        };
        TextCell { x, y, glyph, fg: INK, bg, mask: !0, ..TextCell::default() }
    }

    /// The story window's page `english` (its text at the tile-map cell (4, 23): 32, 184 on the screen), state 2.
    fn english_page(english: &[&str]) -> OnScreen {
        let english: Vec<Vec<Option<char>>> =
            english.iter().map(|line| line.chars().map(Some).collect()).collect();
        OnScreen {
            state: 2,
            lines: english.iter().map(Vec::len).collect(),
            english,
            progress: 1.0,
            ..screen(1.0)
        }
    }

    /// The cells of `lines` from (`x`, `y`), `pitch` apart, on the paper.
    fn showing(lines: &[&str], (x, y): (i32, i32), pitch: i32) -> Vec<TextCell> {
        let mut cells = Vec::new();
        for (row, line) in lines.iter().enumerate() {
            for (column, ch) in line.chars().enumerate() {
                cells.push(cell_of(x + 8 * column as i32, y + pitch * row as i32, ch, PAPER));
            }
        }
        cells
    }

    fn seen(screen: &OnScreen, cells: &[TextCell]) -> Seen {
        presence(screen, cells, &abc, 0, (256, 224), None, None)
    }

    #[test]
    fn a_page_whose_letters_all_show_where_it_is_is_on_the_screen() {
        let page = english_page(&["hello", "world"]);
        // (a scroll 3 rows up, and the window's lines 12 pixels apart: found from the letters)
        let cells = showing(&["hello", "world"], (32, 181), 12);
        let shown = seen(&page, &cells);
        assert_eq!(shown.presence, Presence::In);
        assert_eq!((shown.own, shown.other, shown.letters), (10, 0, 10));
        assert_eq!((shown.at, shown.pitch, shown.matched), ((32, 181), 12, true));
    }

    #[test]
    fn a_page_partly_written_is_on_the_screen_with_the_letters_it_shows() {
        let page = english_page(&["hello", "world"]);
        let cells = showing(&["hel  ", "     "], (32, 184), 16);
        let shown = seen(&page, &cells);
        assert_eq!(shown.presence, Presence::In);
        assert_eq!((shown.own, shown.blank, shown.other), (3, 7, 0));
    }

    #[test]
    fn a_page_with_letters_of_another_window_over_some_cells_is_covered() {
        let page = english_page(&["hello", "world"]);
        let mut cells = showing(&["hello", "wo   "], (32, 184), 16);
        cells.extend(showing(&["xyz"], (56, 200), 16));
        let shown = seen(&page, &cells);
        assert_eq!(shown.presence, Presence::Covered);
        // (`xyz` from the line's fourth cell: two of them on the page's cells)
        assert_eq!((shown.own, shown.blank, shown.other), (7, 1, 2));
        assert_eq!(shown.misses[0], (1, 3, 'l', Miss::Letter('x')), "{:?}", shown.misses);
    }

    #[test]
    fn a_page_none_of_whose_letters_show_is_not_on_the_screen() {
        let page = english_page(&["hello", "world"]);
        // Another window's letters where it is, or no text at all (a picture, a black screen)
        let other = showing(&["abcde", "fghij"], (32, 184), 16);
        assert_eq!(seen(&page, &other).presence, Presence::Out);
        assert_eq!(seen(&page, &[]).presence, Presence::Out);
        assert_eq!(seen(&page, &[]).misses[0], (0, 0, 'h', Miss::Nothing));
    }

    #[test]
    fn a_window_of_blank_paper_shows_the_page_not_written_yet() {
        let page = english_page(&["hello"]);
        let blank = showing(&["     "], (32, 184), 16);
        assert_eq!(seen(&page, &blank).presence, Presence::Empty);
        // Blank cells of another ground are not its window
        let elsewhere = presence(&page, &blank, &abc, 0, (256, 224), Some(INK), None);
        assert_eq!(elsewhere.presence, Presence::Out);
        assert_eq!(elsewhere.misses[0], (0, 0, 'h', Miss::Ground));
    }

    #[test]
    fn cells_not_compared_and_cells_off_the_screen_do_not_count() {
        let mut page = english_page(&["hello"]);
        page.english[0][4] = None;
        let mut cells = showing(&["hell"], (32, 184), 16);
        cells.push(cell_of(64, 184, 'z', PAPER));
        let shown = seen(&page, &cells);
        assert_eq!((shown.presence, shown.own, shown.skipped), (Presence::In, 4, 1));
        // A line under the frame's last row
        let low = english_page(&["hello", "world"]);
        let shown =
            presence(&low, &showing(&["hello"], (32, 184), 16), &abc, 0, (256, 200), None, None);
        assert_eq!((shown.presence, shown.skipped, shown.letters), (Presence::In, 5, 5));
    }

    #[test]
    fn a_space_is_its_windows_blank_cell_or_says_nothing() {
        // The engine writes no space: a blank cell of the window there is the window's, anything else (another
        // window's frame or letter under the shop's `                BUY`) is not compared
        let page = english_page(&["a b"]);
        let shown = seen(&page, &showing(&["a b"], (32, 184), 16));
        assert_eq!((shown.presence, shown.own, shown.blank), (Presence::In, 2, 1));
        let shown = seen(&page, &showing(&["axb"], (32, 184), 16));
        assert_eq!(
            (shown.presence, shown.own, shown.blank, shown.other, shown.skipped),
            (Presence::In, 2, 0, 0, 1)
        );
        // (a page of spaces and blank cells: its window, not written yet)
        assert_eq!(
            seen(&english_page(&["a b"]), &showing(&["   "], (32, 184), 16)).presence,
            Presence::Empty
        );
    }

    #[test]
    fn the_place_is_a_run_of_the_pages_letters_not_one_letter_alike() {
        // The page `pear`; another window above its first line shows `e` and, a line apart, `ear` at the page's
        // own columns 1 to 3 but a row off its grid: the page's letters are where `pear` reads whole
        let page = english_page(&["pear"]);
        let mut cells = showing(&[" e  "], (32, 160), 16);
        cells.extend(showing(&["pear"], (32, 182), 16));
        let shown = seen(&page, &cells);
        assert_eq!((shown.presence, shown.at, shown.own), (Presence::In, (32, 182), 4));
        // One letter of the page alone does not place it (the nominal place, where nothing of it shows)
        let lone = seen(&page, &showing(&[" e"], (32, 170), 16));
        assert!(!lone.matched);
        assert_eq!((lone.presence, lone.at), (Presence::Out, (32, 184)));
        // Letters of the page scattered over another window's text, none beside the next: no run, no place
        let scattered = seen(&english_page(&["hello"]), &showing(&["hxlxo"], (40, 180), 16));
        assert!(!scattered.matched, "{scattered:?}");
        assert_eq!(scattered.at, (32, 184));
        // but at the place the page was seen last (the hint), one letter is enough
        let hinted = presence(
            &page,
            &showing(&["p   "], (32, 170), 16),
            &abc,
            0,
            (256, 224),
            None,
            Some(((32, 170), 16)),
        );
        assert_eq!((hinted.presence, hinted.at, hinted.matched), (Presence::In, (32, 170), true));
    }

    #[test]
    fn every_line_of_a_portraits_page_starts_at_the_text_base() {
        // `str2:0x165`, `"It's the only` / ` way! We have`: the opening quote and the space under it in the same
        // column, the engine's first cell (as scans of the screen show every portrait page of the dialogue)
        let mut page = english_page(&["\"hi", " you"]);
        page.portrait = true;
        let shown = seen(&page, &showing(&["\"hi", " you"], (32, 184), 16));
        assert_eq!((shown.presence, shown.own, shown.at), (Presence::In, 6, (32, 184)));
    }

    #[test]
    fn the_spaces_that_end_a_line_are_not_compared() {
        // The shop's `str2:0x048`, `                BUY  `: the engine writes no space, and the two after `BUY` are on
        // the window's frame (no text cell there)
        let page = english_page(&["ab  "]);
        let shown = seen(&page, &showing(&["ab"], (32, 184), 16));
        assert_eq!((shown.presence, shown.own, shown.skipped), (Presence::In, 2, 2));
        // (the space between the letters is compared: blank, its window's)
        let between = seen(&english_page(&["a b "]), &showing(&["a b"], (32, 184), 16));
        assert_eq!((between.presence, between.blank, between.skipped), (Presence::In, 1, 1));
    }

    #[test]
    fn a_page_of_marks_only_is_placed_by_its_marks() {
        // `help:30`, `.` alone, shown a row up from where the engine writes; another window's `.` far off
        let page = english_page(&["."]);
        let mut cells = showing(&["."], (32, 183), 16);
        cells.push(cell_of(32, 100, ',', PAPER));
        let shown = seen(&page, &cells);
        assert_eq!((shown.presence, shown.at, shown.matched), (Presence::In, (32, 183), true));
    }

    #[test]
    fn a_page_with_no_cell_to_compare_says_nothing() {
        // `str2:0x13D`: a portrait and no letter
        let page = OnScreen { english: vec![Vec::new()], lines: vec![0], ..english_page(&["a"]) };
        assert_eq!(seen(&page, &showing(&["abc"], (32, 184), 16)).presence, Presence::NoCells);
        let marks = english_page(&["…"]);
        let marks = OnScreen { english: vec![vec![None]], ..marks };
        assert_eq!(seen(&marks, &[]).presence, Presence::NoCells);
    }

    // ---- pages written at once, drawn by their presence ([`shown_now`], [`draw`])

    /// The story window's page `english` written at once (state 0), as [`english_page`] gives it.
    fn at_once(english: &[&str]) -> OnScreen {
        OnScreen { state: 0, ..english_page(english) }
    }

    /// An overlay of the made-up pages `str3:0x00` (`AB`), `str3:0x01` (`DE`) and `str4:0x00` (`FG`).
    fn at_once_pages() -> Overlay {
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tAB\nstr3:0x01\tDE\nstr4:0x00\tFG\n").unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        Overlay::new(Arc::new(table), Arc::new(face))
    }

    /// Draws a frame as [`drawn_pages`] does, with the test font of [`presence`] ([`abc`]).
    fn judged(
        overlay: &Overlay,
        kept: &mut Kept,
        behind: &mut Option<Kept>,
        screen: Option<&OnScreen>,
        cells: &[TextCell],
    ) -> Vec<u8> {
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let mut rgba = [7_u8, 7, 7, 255].repeat(512 * 448);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        draw_pages(
            overlay,
            kept,
            behind,
            screen,
            &mut target,
            cells,
            &frame,
            256,
            0,
            &abc,
            &mut |_| None,
            None,
            &[],
        );
        rgba
    }

    /// The pixels (2x) of the story window, where the translation is drawn.
    fn first_line(rgba: &[u8]) -> Vec<[u8; 3]> {
        (2 * 168..2 * 224)
            .flat_map(|y| (2 * 24..2 * 240).map(move |x| (x, y)))
            .map(|(x, y)| pixel(rgba, x, y))
            .collect()
    }

    /// Whether [`first_line`] shows a glyph drawn: a pixel neither the picture's grey nor the paper.
    fn glyphs_in(line: &[[u8; 3]]) -> bool {
        line.iter().any(|&p| p != [7, 7, 7] && p != PAPER)
    }

    /// The first line as a page alone draws it, its English `cells` on the screen.
    fn alone(overlay: &Overlay, page: &OnScreen, cells: &[TextCell]) -> Vec<[u8; 3]> {
        let (mut kept, mut behind) = (Kept::default(), None);
        first_line(&judged(overlay, &mut kept, &mut behind, Some(page), cells))
    }

    #[test]
    fn a_page_written_at_once_is_drawn_from_the_frame_its_english_shows() {
        // The engine names a page written at once frames before the screen shows it: its registers are still the
        // last message's (the battle's `battle:89` over the cards, `message_state` A0), or it wrote the tile map and
        // the screen shows it 2 to 6 frames later (the wish list, the Item help). Nothing is drawn until its
        // English shows, then it is drawn without the engine naming it again
        let overlay = at_once_pages();
        let page = at_once(&["hello"]);
        let cards: Vec<TextCell> =
            (0..5).map(|n| TextCell { x: 32 + 8 * n, glyph: CARD_BASE, ..letter() }).collect();
        let other = showing(&["abcde"], (32, 184), 16);
        for (what, cells) in [("the cards", &cards), ("another window", &other)] {
            let (mut kept, mut behind) = (Kept::default(), None);
            for frame in 0..3 {
                let screen = (frame < 2).then_some(&page);
                let rgba = judged(&overlay, &mut kept, &mut behind, screen, cells);
                assert_eq!(
                    pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2),
                    [7, 7, 7],
                    "{what} (frame {frame})"
                );
                assert!(!glyphs_in(&first_line(&rgba)), "no Chinese over {what} (frame {frame})");
            }
            let shows = showing(&["hello"], (32, 184), 16);
            let rgba = judged(&overlay, &mut kept, &mut behind, None, &shows);
            assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "its English painted over");
            assert_eq!(first_line(&rgba), alone(&overlay, &page, &shows), "and its Chinese drawn");
        }
    }

    #[test]
    fn the_page_before_stays_until_the_next_pages_english_shows() {
        // The Item list's help as the cursor moves: the engine names the next item's help 6 frames before the screen
        // shows it, the help before still there (its letters `help` are the next help's too)
        let overlay = at_once_pages();
        let before = at_once(&["hello"]);
        let next = OnScreen { string: 1, key: "str3:0x01".to_owned(), ..at_once(&["helps"]) };
        let old = showing(&["hello"], (32, 184), 16);
        let new = showing(&["helps"], (32, 184), 16);
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&before), &old);
        let drawn_before = alone(&overlay, &before, &old);
        assert!(glyphs_in(&drawn_before), "(the help before drawn alone)");
        assert_ne!(drawn_before, alone(&overlay, &next, &old), "(the two helps' Chinese differ)");
        for frame in 0..6 {
            let screen = (frame < 2).then_some(&next);
            let rgba = judged(&overlay, &mut kept, &mut behind, screen, &old);
            assert_eq!(first_line(&rgba), drawn_before, "the help before (frame {frame})");
        }
        let rgba = judged(&overlay, &mut kept, &mut behind, None, &new);
        assert_eq!(first_line(&rgba), alone(&overlay, &next, &new), "the next help once it shows");
        assert!(kept.pending.is_none());
    }

    #[test]
    fn of_two_pages_of_the_same_english_the_one_the_engine_names_is_drawn() {
        // The Item help of the seven Dragon Balls (`help:33` to `help:39`): one English, so the page the engine names
        // shows at once where the one before was; one page a place
        let overlay = at_once_pages();
        let before = at_once(&["hello"]);
        let next = OnScreen { string: 1, key: "str3:0x01".to_owned(), ..at_once(&["hello"]) };
        let cells = showing(&["hello"], (32, 184), 16);
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&before), &cells);
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&next), &cells);
        assert_eq!(first_line(&rgba), alone(&overlay, &next, &cells));
        assert!(behind.is_none(), "the page before is not kept under it");
    }

    #[test]
    fn a_page_a_stopped_string_left_is_drawn_only_while_its_english_shows() {
        // A story transition stops the message after `str3:0x4C` ("34 days later…"), and the engine's bytes name that
        // page for about 450 frames ([`OnScreen::stopped`]): it is drawn where its English shows, also when the
        // overlay never saw it written (a state loaded there); a window of blank paper shows no page, from the first
        // frame or after the page was drawn (the window cleared while the bytes still name it)
        let overlay = at_once_pages();
        let page = OnScreen { state: 3, stopped: true, ..english_page(&["hello"]) };
        let shows = showing(&["hello"], (32, 184), 16);
        let blank = showing(&["     "], (32, 184), 16);
        let (mut kept, mut behind) = (Kept::default(), None);
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&page), &blank);
        assert!(!glyphs_in(&first_line(&rgba)), "nothing over a blank window");
        let (mut kept, mut behind) = (Kept::default(), None);
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&page), &shows);
        assert!(glyphs_in(&first_line(&rgba)), "drawn where its English shows");
        for frame in 0..3 {
            let rgba = judged(&overlay, &mut kept, &mut behind, Some(&page), &blank);
            assert!(!glyphs_in(&first_line(&rgba)), "the cleared window (frame {frame})");
        }
    }

    #[test]
    fn a_page_the_engine_named_that_never_shows_is_forgotten() {
        let overlay = at_once_pages();
        let page = at_once(&["hello"]);
        let other = showing(&["abcde"], (32, 184), 16);
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&page), &other);
        for _ in 0..=PENDING {
            judged(&overlay, &mut kept, &mut behind, None, &other);
        }
        assert!(kept.pending.is_none());
        let late =
            judged(&overlay, &mut kept, &mut behind, None, &showing(&["hello"], (32, 184), 16));
        assert!(
            !glyphs_in(&first_line(&late)),
            "its English, that came long after, is another message's"
        );
    }

    #[test]
    fn a_page_written_at_once_with_a_window_over_part_of_it_waits_its_letters_beside_on_their_ground()
     {
        // Porunga's wishes (written at once) and the cards a wish changes over most of their window: the Chinese is
        // not drawn over the cards, the wishes' letters left of them take their ground; drawn again once they close
        let overlay = at_once_pages();
        let page = at_once(&["hello"]);
        let whole = showing(&["hello"], (32, 184), 16);
        let mut covered = showing(&["he"], (32, 184), 16);
        covered.extend((2..5).map(|n| TextCell { x: 32 + 8 * n, glyph: CARD_BASE, ..letter() }));
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&page), &whole);
        for frame in 0..2 * KEEP {
            let rgba = judged(&overlay, &mut kept, &mut behind, None, &covered);
            assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "`h` on its ground ({frame})");
            assert_eq!(pixel(&rgba, 2 * 48 + 2, 2 * 184 + 2), [7, 7, 7], "the cards ({frame})");
            assert!(!glyphs_in(&first_line(&rgba)), "no Chinese ({frame})");
        }
        assert!(kept.shown.is_some(), "the page waits");
        let rgba = judged(&overlay, &mut kept, &mut behind, None, &whole);
        assert_eq!(first_line(&rgba), alone(&overlay, &page, &whole), "drawn once the cards close");
        // A window over all of it, or its window cleared: the page goes
        judged(&overlay, &mut kept, &mut behind, None, &showing(&["abcde"], (32, 184), 16));
        assert!(kept.shown.is_none());
    }

    #[test]
    fn a_page_waiting_under_another_window_leaves_the_frame_beside_its_letters_alone() {
        // The battle's Item help (written at once) under the enemy window of Grandpa's and Yajirobe's items: `Pa`,
        // `en`, `at` still show left of that window and take their ground. Right of `a`, the 1x pixel is the ground
        // (the window's frame tile starts with a column of paper), but the HD text draws that frame tile again, its
        // edge blended into the right half of that pixel (`ffe7c4`, the picture before the HD text paper there):
        // painting the whole pixel with the ground took the edge out of a column of 20 HD pixels inside the enemy
        // window's name field (4880..4995). Past its cell, only a pixel that is the letter's own ink blended over the
        // picture as it was before the HD text drew the letters (one coverage for the three channels, as the HD
        // text blends) takes that picture; the frame's edge is no such blend (its ink is the frame's) and stays
        const EDGE: [u8; 3] = [0xFF, 0xE7, 0xC4];
        let overlay = at_once_pages();
        let page = at_once(&["hello"]);
        let whole = showing(&["hello"], (32, 184), 16);
        let mut covered = showing(&["he"], (32, 184), 16);
        covered.extend((2..5).map(|n| TextCell { x: 32 + 8 * n, glyph: CARD_BASE, ..letter() }));
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&page), &whole);
        let set = |rgba: &mut [u8], x: usize, y: usize, colour: [u8; 3]| {
            rgba[(y * 512 + x) * 4..(y * 512 + x) * 4 + 3].copy_from_slice(&colour);
        };
        // The picture before the HD text: grey, and the paper of the window
        let mut before = [7_u8, 7, 7, 255].repeat(512 * 448);
        for y in 2 * 183..2 * 193 {
            for x in 2 * 31..2 * 49 {
                set(&mut before, x, y, PAPER);
            }
        }
        // The HD text: the frame's edge right of `e`'s cell, and `e`'s ink past its cell over the paper there (an
        // overhang: the ink, and its anti-aliased edge at a coverage of 128)
        let mut rgba = before.clone();
        for y in 2 * 183..2 * 193 {
            set(&mut rgba, 2 * 48 + 1, y, EDGE);
        }
        set(&mut rgba, 2 * 48, 2 * 186, INK);
        set(&mut rgba, 2 * 48, 2 * 187, [0x7F, 0x77, 0x7F]);
        let mut target = Target { rgba: &mut rgba, width: 512, scale: 2 };
        let frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        draw_pages(
            &overlay,
            &mut kept,
            &mut behind,
            None,
            &mut target,
            &covered,
            &frame,
            256,
            0,
            &abc,
            &mut |_| None,
            Some(&before),
            &[],
        );
        assert!(kept.shown.is_some(), "(the page waits under the window)");
        assert_eq!(pixel(&rgba, 2 * 40 + 2, 2 * 184 + 2), PAPER, "`e` on its ground");
        for y in 2 * 183..2 * 193 {
            assert_eq!(
                pixel(&rgba, 2 * 48 + 1, y),
                EDGE,
                "the frame's edge beside it stays (row {y})"
            );
        }
        assert_eq!(pixel(&rgba, 2 * 48, 2 * 186), PAPER, "the ink past its cell goes");
        assert_eq!(pixel(&rgba, 2 * 48, 2 * 187), PAPER, "and the blend of ink and paper");
    }

    #[test]
    fn a_page_behind_with_a_window_over_part_of_it_waits_under_it_and_shows_once_it_closes() {
        // Porunga's third wish in a battle: his help (written at once, behind the wishes) under the enemies' HP
        // window, `Le` and `3` of it left of that window; the engine writes the help again while the window is up
        let overlay = at_once_pages();
        let help = at_once(&["hello"]);
        let wishes = OnScreen {
            table: 4,
            key: "str4:0x00".to_owned(),
            base: 2 * 64 + 8,
            ..at_once(&["wish"])
        };
        let wish = showing(&["wish"], (32, 16), 16);
        let whole: Vec<TextCell> =
            showing(&["hello"], (32, 184), 16).into_iter().chain(wish.iter().copied()).collect();
        let covered: Vec<TextCell> =
            showing(&["hexyz"], (32, 184), 16).into_iter().chain(wish.iter().copied()).collect();
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&help), &whole);
        let both = judged(&overlay, &mut kept, &mut behind, Some(&wishes), &whole);
        assert!(behind.is_some(), "the help goes behind the wishes");
        assert_eq!(first_line(&both), alone(&overlay, &help, &whole), "the help drawn under them");
        assert_ne!(pixel(&both, 2 * 32 + 2, 2 * 16 + 2), [7, 7, 7], "and the wishes");
        let mut frames = vec![None; KEEP as usize];
        frames.extend([Some(&help); 3]);
        frames.extend([Some(&wishes), None]);
        for (frame, screen) in frames.into_iter().enumerate() {
            let rgba = judged(&overlay, &mut kept, &mut behind, screen, &covered);
            assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "`h` on its ground ({frame})");
            assert_eq!(pixel(&rgba, 2 * 48 + 2, 2 * 184 + 2), [7, 7, 7], "the HP window ({frame})");
            assert!(!glyphs_in(&first_line(&rgba)), "no Chinese over it ({frame})");
            assert_ne!(pixel(&rgba, 2 * 32 + 2, 2 * 16 + 2), [7, 7, 7], "the wishes ({frame})");
        }
        let after = judged(&overlay, &mut kept, &mut behind, None, &whole);
        assert!(behind.is_some(), "the help stays behind the wishes");
        assert_eq!(
            first_line(&after),
            alone(&overlay, &help, &whole),
            "drawn once the window closes"
        );
    }

    #[test]
    fn a_finished_page_the_engine_still_names_over_a_black_screen_is_not_drawn() {
        // Gohan's `str2:0x10E/p3` waiting for A as the screen goes black (frame 10099 of `secret-nail-porunga`)
        let overlay = at_once_pages();
        let page = english_page(&["hello"]);
        let cells = showing(&["hello"], (32, 184), 16);
        let (mut kept, mut behind) = (Kept::default(), None);
        assert!(glyphs_in(&first_line(&judged(
            &overlay,
            &mut kept,
            &mut behind,
            Some(&page),
            &cells
        ))));
        let black = judged(&overlay, &mut kept, &mut behind, Some(&page), &[]);
        assert_eq!(pixel(&black, 2 * 32 + 2, 2 * 184 + 2), [7, 7, 7], "no paper");
        assert!(!glyphs_in(&first_line(&black)), "no Chinese");
    }

    #[test]
    fn the_cells_of_a_line_past_its_window_are_not_compared() {
        // Porunga's `str4:0x03`: a line of the wish window (11 cells) longer than the window; past its frame the
        // screen shows the frame and the field, not the line's letters
        let page =
            OnScreen { table: 4, key: "str4:0x00".to_owned(), ..at_once(&["abcdefghijklm"]) };
        let mut cells = showing(&["abcdefghijk"], (32, 184), 16);
        cells.extend(showing(&["zz"], (32 + 8 * 11, 184), 16));
        assert_eq!(seen(&page, &cells).presence, Presence::Covered, "(presence alone)");
        let window = presence_in_window(&page, &cells, &abc, 0, (256, 224), None, None);
        assert_eq!((window.presence, window.own, window.other), (Presence::In, 11, 0));
    }

    // ---- pages the engine writes cell by cell, drawn by their presence ([`shown_now`], [`draw`])

    /// The story window's page `english` the engine is writing (state 1), `progress` of it written as the engine
    /// counts.
    fn being_written(english: &[&str], progress: f32) -> OnScreen {
        OnScreen { state: 1, line: 0, cell: 1, progress, ..english_page(english) }
    }

    /// An overlay of the made-up pages `str3:0x00` (`ABCDEFGH`), `str3:0x01` (`IJKL`) and `str3:0x02` (`AB12C`).
    fn written_pages() -> Overlay {
        let table =
            Translation::parse("key\tzh\nstr3:0x00\tABCDEFGH\nstr3:0x01\tIJKL\nstr3:0x02\tAB12C\n")
                .unwrap();
        let face = ChineseFace::new(crate::font::SHARE_TECH_MONO.to_vec()).unwrap();
        Overlay::new(Arc::new(table), Arc::new(face))
    }

    /// The pixels (2x) of the rows of the story window's first line that hold ink.
    fn ink_of(rgba: &[u8]) -> usize {
        first_line(rgba).iter().filter(|&&p| inky(p)).count()
    }

    /// Whether a pixel (2x) holds ink of a glyph drawn: dark, and not the picture's grey.
    fn inky(p: [u8; 3]) -> bool {
        p != [7, 7, 7] && p[2] < 0x80
    }

    #[test]
    fn a_page_being_written_is_placed_by_the_first_letters_it_shows() {
        // Two letters of the page's first line, a row up from where the engine writes (the layer's scroll), and its
        // window's blank cells after them: too few side by side to place a page from anywhere, but they are the
        // start of a page being written
        let page = being_written(&["hello"], 0.4);
        let cells = showing(&["he   "], (32, 183), 16);
        let still = seen(&page, &cells);
        assert!(!still.matched, "(not from a run: {still:?})");
        let shown = typed_presence(&page, &cells, &abc, 0, (256, 224), None, None);
        assert_eq!((shown.presence, shown.at, shown.matched), (Presence::In, (32, 183), true));
        assert_eq!((shown.own, shown.blank, shown.letters), (2, 3, 5));
        // Letters of the page that are not its start (another window's `l`, `o`) do not place it
        let middle = typed_presence(
            &page,
            &showing(&["  l o"], (32, 183), 16),
            &abc,
            0,
            (256, 224),
            None,
            None,
        );
        assert!(!middle.matched, "{middle:?}");
        // nor its start with a letter of it further on and the cells between not written (`o` of another window)
        let gap = typed_presence(
            &page,
            &showing(&["he  o"], (32, 183), 16),
            &abc,
            0,
            (256, 224),
            None,
            None,
        );
        assert!(!gap.matched, "{gap:?}");
    }

    #[test]
    fn the_page_before_stays_until_the_window_of_the_next_page_is_blank() {
        // A dialogue's next page: the engine names it at its first cell frames before the game clears the window,
        // which still shows the page before (`str2:0x0FF/p2`, frames 2330 to 2335 of `secret-ginyu-jeice-burter`).
        // The page before is drawn until the window is blank; then the next page, nothing of it written yet
        let overlay = written_pages();
        let before = english_page(&["hello"]);
        let next = OnScreen {
            string: 1,
            key: "str3:0x01".to_owned(),
            cell: 0,
            ..being_written(&["abcd"], 0.0)
        };
        let old = showing(&["hello"], (32, 184), 16);
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&before), &old);
        let drawn_before = alone(&overlay, &before, &old);
        for frame in 0..6 {
            let rgba = judged(&overlay, &mut kept, &mut behind, Some(&next), &old);
            assert_eq!(first_line(&rgba), drawn_before, "the page before (frame {frame})");
        }
        let blank = showing(&["     "], (32, 184), 16);
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&next), &blank);
        assert!(!glyphs_in(&first_line(&rgba)), "the window blank: no Chinese");
        let typed = showing(&["ab   "], (32, 184), 16);
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&next), &typed);
        assert_eq!(first_line(&rgba), alone(&overlay, &next, &typed), "the next page as it shows");
    }

    #[test]
    fn a_page_the_engine_starts_before_its_window_shows_draws_nothing() {
        // `str7:0x07` named at its first cell four frames before its window opens over the cards (frames 9113 to
        // 9116 of the Frieza fight), `str7:0x05` one frame before (`bm_F2`): no paper over what the screen shows
        let overlay = written_pages();
        let page = being_written(&["hello"], 0.2);
        let cards: Vec<TextCell> =
            (0..5).map(|n| TextCell { x: 32 + 8 * n, glyph: CARD_BASE, ..letter() }).collect();
        let (mut kept, mut behind) = (Kept::default(), None);
        // (a page of the same string seen before: its place is known)
        judged(
            &overlay,
            &mut kept,
            &mut behind,
            Some(&english_page(&["hello"])),
            &showing(&["hello"], (32, 184), 16),
        );
        judged(&overlay, &mut kept, &mut behind, None, &[]);
        for frame in 0..4 {
            let rgba = judged(&overlay, &mut kept, &mut behind, Some(&page), &cards);
            assert_eq!(
                pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2),
                [7, 7, 7],
                "no paper (frame {frame})"
            );
            assert!(!glyphs_in(&first_line(&rgba)), "no Chinese (frame {frame})");
        }
    }

    #[test]
    fn the_translation_shows_as_much_as_the_screen_shows_of_the_english() {
        // The engine's count runs ahead of the screen (a name slot counted written at once, state 5; the tile map on
        // the screen frames later): the Chinese follows the English letters the screen shows
        let overlay = written_pages();
        let shows = |letters: &str, progress: f32| {
            let page = being_written(&["abcdefgh"], progress);
            let cells = showing(&[&format!("{letters:<8}")], (32, 184), 16);
            ink_of(&judged(&overlay, &mut Kept::default(), &mut None, Some(&page), &cells))
        };
        assert_eq!(shows("ab", 0.75), shows("ab", 0.25), "two letters of eight show");
        assert!(shows("abcdef", 0.75) > shows("ab", 0.75));
        assert_eq!(shows("", 0.5), 0, "the window blank: nothing written yet");
    }

    #[test]
    fn a_name_the_engine_writes_first_shows_as_its_letters_do() {
        // `battle:101`, `{name:1} reached`: the engine writes the name (state 5) before the page's first cell; the
        // screen shows it letter by letter, and so is the translation drawn
        let overlay = written_pages();
        let page = OnScreen { state: 5, cell: 0, ..being_written(&["piccolo reached"], 0.3) };
        let rgba = judged(
            &overlay,
            &mut Kept::default(),
            &mut None,
            Some(&page),
            &showing(&["pic            "], (32, 184), 16),
        );
        assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "its letters painted over");
        assert!(glyphs_in(&first_line(&rgba)), "its Chinese drawn");
    }

    #[test]
    fn a_number_of_the_translation_shows_whole() {
        // The gravity training's `str3:0x46`: `10x` shows a digit at a time, and its share of the English reached the
        // middle of the translation's `10`; the number shows whole, not `1` alone
        let overlay = written_pages();
        let shows = |letters: &str| {
            let page = OnScreen {
                string: 2,
                key: "str3:0x02".to_owned(),
                ..being_written(&["abcdefghij"], 0.5)
            };
            let cells = showing(&[&format!("{letters:<10}")], (32, 184), 16);
            ink_of(&judged(&overlay, &mut Kept::default(), &mut None, Some(&page), &cells))
        };
        // (5 glyphs: 6 letters of 10 is 3 of them, `AB1`; 8 of 10 is 4, `AB12`)
        assert_eq!(shows("abcdef"), shows("abcdefgh"));
        assert!(shows("abcd") < shows("abcdef"), "(2 glyphs, `AB`)");
    }

    #[test]
    fn a_page_being_written_is_drawn_on_its_own_line_not_another_windows_above() {
        // `str7:0x01` as its first letters show (frames 4938 to 4945 of the obj 27 scene): the KI/BP row of the
        // window above, a line and a half up, was taken for the page's first line and its Chinese drawn there
        let overlay = written_pages();
        let page = being_written(&["hello", "world"], 0.2);
        let mut cells = showing(&["xyzxy"], (32, 160), 16);
        cells.extend(showing(&["he   ", "     "], (32, 184), 16));
        let rgba = judged(&overlay, &mut Kept::default(), &mut None, Some(&page), &cells);
        let picture = &rgba;
        let mut above =
            (2 * 150..2 * 176).flat_map(|y| (2 * 24..2 * 240).map(move |x| pixel(picture, x, y)));
        assert!(!above.any(inky), "nothing drawn on the window above");
        assert!(ink_of(&rgba) > 0, "its Chinese on its own first line");
    }

    #[test]
    fn a_page_being_written_whose_window_changes_colour_is_still_on_the_screen() {
        // `str4:0x02`, `Dende` / `summoned` (frames 9105 to 9116 of `secret-nail-porunga`): the screen fades as the
        // engine writes, the window's ground no longer the paper the page was seen on; its blank cells are on the
        // ground its letters are on now
        let overlay = written_pages();
        let page = being_written(&["hello", "world"], 0.3);
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(
            &overlay,
            &mut kept,
            &mut behind,
            Some(&page),
            &showing(&["he   ", "     "], (32, 184), 16),
        );
        let faded: Vec<TextCell> = showing(&["hel  ", "     "], (32, 184), 16)
            .into_iter()
            .map(|cell| TextCell { bg: [0xC0, 0xB0, 0x90], ..cell })
            .collect();
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&page), &faded);
        assert!(ink_of(&rgba) > 0, "its Chinese drawn");
    }

    #[test]
    fn a_page_being_written_with_a_window_over_part_of_it_is_not_drawn_its_letters_beside_on_their_ground()
     {
        // Another window drawn over part of a page the engine writes: no Chinese over that window; the page's
        // letters still showing beside it take their ground, as a page written at once's do
        let overlay = written_pages();
        let page = being_written(&["hello"], 0.6);
        let (mut kept, mut behind) = (Kept::default(), None);
        judged(&overlay, &mut kept, &mut behind, Some(&page), &showing(&["hel  "], (32, 184), 16));
        let mut covered = showing(&["he"], (32, 184), 16);
        covered.extend((2..5).map(|n| TextCell { x: 32 + 8 * n, glyph: CARD_BASE, ..letter() }));
        let rgba = judged(&overlay, &mut kept, &mut behind, Some(&page), &covered);
        assert_eq!(pixel(&rgba, 2 * 32 + 2, 2 * 184 + 2), PAPER, "`h` on its ground");
        assert_eq!(pixel(&rgba, 2 * 48 + 2, 2 * 184 + 2), [7, 7, 7], "the other window");
        assert!(!glyphs_in(&first_line(&rgba)), "no Chinese");
    }

    /// A 1x frame of paper whose cell at `at` holds a few pixels of the `▼` the engine writes after a page that waits
    /// for A (`01:D1DC`).
    fn frame_with_mark(at: (usize, usize)) -> Vec<Color> {
        let mut frame = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        for (dx, dy) in [(2, 4), (3, 4), (4, 4), (3, 5)] {
            frame[(at.1 + dy) * 256 + at.0 + dx] = Color::rgb(0xFF, 0x52, 0x00);
        }
        frame
    }

    /// How many pixels of the picture `rgba` (512 wide) are `colour`, and the box around them (x0, y0, x1, y1).
    fn colour_box(rgba: &[u8], colour: [u8; 3]) -> (usize, Option<(usize, usize, usize, usize)>) {
        let mut count = 0;
        let mut found: Option<(usize, usize, usize, usize)> = None;
        for y in 0..448 {
            for x in 0..512 {
                if pixel(rgba, x, y) == colour {
                    count += 1;
                    found = Some(found.map_or((x, y, x + 1, y + 1), |(x0, y0, x1, y1)| {
                        (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1))
                    }));
                }
            }
        }
        (count, found)
    }

    /// The story page of [`overlay`] waiting for A: "AB" / "C" over the English's two lines (9 and 4 cells, the `▼`
    /// after the fourth cell of the second line, at x 64, y 200).
    fn waiting_page(state: u8) -> (OnScreen, Vec<TextCell>) {
        let page = OnScreen { state, ..screen(1.0) };
        let cells = vec![letter(), TextCell { y: 200, ..letter() }];
        (page, cells)
    }

    #[test]
    fn the_continue_mark_of_a_page_waiting_for_a_goes_after_the_last_character_of_the_translation()
    {
        // The original's `▼` (no text tile; the HD picture of its cell is `cell_colour(64, 200)`) waits after the
        // English's last letter. Its cell's picture goes whole after the translation's last character, `C` on the
        // second line, and the English's place takes the paper with the rest of the window. A page that waits for A
        // is state 2 (more pages) or 3 (the last)
        let overlay = overlay();
        let mark = cell_colour(64, 200);
        for state in [2, 3] {
            let mut kept = Kept::default();
            let (page, cells) = waiting_page(state);
            let lit = frame_with_mark((64, 200));
            let drawn =
                drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &lit, &[(64, 200)]);
            let (count, place) = colour_box(&drawn, mark);
            assert_eq!(count, 16 * 16, "state {state}: the mark's cell, whole, at scale 2");
            let (x0, y0, x1, y1) = place.unwrap();
            assert_eq!((x1 - x0, y1 - y0), (16, 16), "state {state}: in one piece");
            assert_eq!(
                cell_pixel(&drawn, 64, 200),
                PAPER,
                "state {state}: not where the English had it"
            );
            // `C`'s ink: on the second line's em (from a pixel above its row, 12 pixels high), right of the window's
            // left edge
            let ink: Vec<(usize, usize)> = (2 * 199..2 * 211)
                .flat_map(|y| (2 * 32..2 * 232).map(move |x| (x, y)))
                .filter(|&(x, y)| ![PAPER, mark].contains(&pixel(&drawn, x, y)))
                .collect();
            let right = ink.iter().map(|&(x, _)| x).max().expect("`C` is drawn");
            assert!(
                (right + 1..=right + 8).contains(&x0),
                "state {state}: right after `C` (its ink ends at {right}, the mark starts at {x0})"
            );
            assert!(
                (2 * 199..2 * 211).contains(&y0) && (2 * 199..=2 * 211).contains(&y1),
                "state {state}: on `C`'s line ({y0} to {y1})"
            );
        }
    }

    #[test]
    fn the_continue_mark_blinks_with_the_originals() {
        // The mark is drawn only on the frames the original shows its tile (every 8 frames the engine writes it in its
        // other colours, `$2A` / `$29`; a frame with no `▼` tile there, a blank one, shows none)
        let overlay = overlay();
        let mark = cell_colour(64, 200);
        let mut kept = Kept::default();
        let (page, cells) = waiting_page(2);
        let lit = frame_with_mark((64, 200));
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &lit, &[(64, 200)]);
        assert_eq!(colour_box(&drawn, mark).0, 256, "on");
        let mut dark = cells.clone();
        dark.push(TextCell { x: 64, y: 200, glyph: BLANK, ..letter() });
        let paper = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&page), &dark, &paper, &[]);
        assert_eq!(colour_box(&drawn, mark).0, 0, "off: nothing drawn");
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &lit, &[(64, 200)]);
        assert_eq!(colour_box(&drawn, mark).0, 256, "on again");
    }

    #[test]
    fn no_continue_mark_is_drawn_for_a_page_that_does_not_wait_for_a() {
        let overlay = overlay();
        let mark = cell_colour(64, 200);
        let lit = frame_with_mark((64, 200));
        // A page still being written (state 1, all its cells written), a page written at once (the Item screen's
        // help, state 0), a timed wait (state 6): the original writes no `▼`
        let at = [(64, 200)];
        for state in [0, 1, 6] {
            let mut kept = Kept::default();
            let (page, cells) = waiting_page(state);
            let drawn = drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &lit, &at);
            assert_eq!(colour_box(&drawn, mark).0, 0, "state {state}");
        }
        // A page kept while the engine names nothing (the engine names the next page, which the screen does not show
        // yet): the original's `▼` stays after it while its window still shows it, as when the engine last named the
        // page waiting for A; a page last named while it was written has none
        let mut kept = Kept::default();
        let (page, cells) = waiting_page(2);
        drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &lit, &at);
        let drawn = drawn_over_marked(&overlay, &mut kept, None, &cells, &lit, &at);
        assert_eq!(colour_box(&drawn, mark).0, 256, "not named, last named waiting for A");
        let mut kept = Kept::default();
        let (written, _) = waiting_page(1);
        drawn_over_marked(&overlay, &mut kept, Some(&written), &cells, &lit, &at);
        let drawn = drawn_over_marked(&overlay, &mut kept, None, &cells, &lit, &at);
        assert_eq!(colour_box(&drawn, mark).0, 0, "not named, last named while written");
        // A cell with no text tile that shows only the window's paper is no `▼`
        let mut kept = Kept::default();
        let paper = vec![Color::rgb(0xFF, 0xEF, 0xCE); 256 * 224];
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&page), &cells, &paper, &[]);
        assert_eq!(colour_box(&drawn, mark).0, 0, "only paper there");
        // Nor is a letter there (its ink is not the paper)
        let mut kept = Kept::default();
        let mut lettered = cells.clone();
        lettered.push(TextCell { x: 64, y: 200, ..letter() });
        let drawn = drawn_over_marked(&overlay, &mut kept, Some(&page), &lettered, &lit, &[]);
        assert_eq!(colour_box(&drawn, mark).0, 0, "a letter there");
    }

    #[test]
    fn a_cell_that_is_not_the_continue_marks_tile_is_not_drawn_after_the_translation() {
        // The cell after the English's last character shows something other than the window's paper, but the tile
        // map has no `▼` there: an English line that ran past the window leaves the screen beside it there (Porunga's
        // `Dragon Balls.`, nail-porunga 9590) or a portrait's corner (Vegeta's `for immortality!"`, 10214), before the
        // engine writes the `▼`. Nothing is drawn after the translation
        let overlay = overlay();
        let (page, cells) = waiting_page(2);
        let mut kept = Kept::default();
        let drawn = drawn_over_marked(
            &overlay,
            &mut kept,
            Some(&page),
            &cells,
            &frame_with_mark((64, 200)),
            &[],
        );
        assert_eq!(colour_box(&drawn, cell_colour(64, 200)).0, 0, "no `▼` tile: nothing copied");
    }

    #[test]
    fn a_continue_mark_past_the_tile_maps_columns_goes_after_the_translation_too() {
        // A line of 30 cells from column 4: the engine writes its 29th and 30th letters and the `▼` at the start of
        // the tile-map row under it (x 0, 8 and 16 of y 192, [`spill_of`]). The `▼`'s cell takes the picture beside it
        // there, and its picture goes after the translation's last character
        let overlay = overlay();
        let mut kept = Kept::default();
        let waiting = OnScreen { state: 2, lines: vec![30], ..screen(1.0) };
        let at = |x: i32, y: i32| TextCell { x, y, ..letter() };
        let mut cells: Vec<TextCell> = (0..28).map(|n| at(32 + 8 * n, 184)).collect();
        cells.extend([at(0, 192), at(8, 192)]);
        let lit = frame_with_mark((16, 192));
        let drawn =
            drawn_over_marked(&overlay, &mut kept, Some(&waiting), &cells, &lit, &[(16, 192)]);
        assert_eq!(
            cell_pixel(&drawn, 16, 192),
            cell_colour(16, 184),
            "the `▼`'s cell: the picture beside it"
        );
        assert_eq!(
            colour_box(&drawn, cell_colour(16, 192)).0,
            256,
            "the `▼` after the translation"
        );
        // On the frames after, the `▼`'s cell takes the picture beside it before the page is drawn
        // ([`paint_spilled`]): the mark after the translation is still the original's `▼` (nail-porunga 9591 to 9595,
        // 10217 to 10219: the screen's ground, a portrait's corner, were drawn there)
        let drawn =
            drawn_over_marked(&overlay, &mut kept, Some(&waiting), &cells, &lit, &[(16, 192)]);
        assert_eq!(
            colour_box(&drawn, cell_colour(16, 192)).0,
            256,
            "the `▼` after the translation, the frame after"
        );
    }

    /// The 68 pages whose English shows no letter at all, only a portrait or blanks or a `…` the
    /// HD font has no glyph for. They have no English layout to follow: the overlay lays their translation out by
    /// itself in the window's blank ground ([`blank_ground`]), so a line too many runs out of the window. The list
    /// is every page of the English patch with none of its cells to compare, from a scan of the whole game, kept
    /// here so that the check does not lean on a file outside the repository.
    const EMPTY_PAGES: [&str; 68] = [
        "battle:143",
        "battle:147",
        "battle:148",
        "battle:279/p3",
        "battle:281",
        "str2:0x06C",
        "str2:0x06D",
        "str2:0x06E",
        "str2:0x072",
        "str2:0x09C",
        "str2:0x09D",
        "str2:0x0A9",
        "str2:0x0AA",
        "str2:0x0AB",
        "str2:0x0AC",
        "str2:0x0AD",
        "str2:0x0D4",
        "str2:0x0D6",
        "str2:0x0D8",
        "str2:0x0DA",
        "str2:0x0DC",
        "str2:0x0DD",
        "str2:0x0DE",
        "str2:0x0DF",
        "str2:0x0E0",
        "str2:0x0E2",
        "str2:0x0E6",
        "str2:0x0E8",
        "str2:0x0F1",
        "str2:0x0F2",
        "str2:0x0FC",
        "str2:0x10C",
        "str2:0x113",
        "str2:0x118",
        "str2:0x119",
        "str2:0x11E",
        "str2:0x124",
        "str2:0x125",
        "str2:0x12E",
        "str2:0x12F",
        "str2:0x130",
        "str2:0x131",
        "str2:0x132",
        "str2:0x133",
        "str2:0x134",
        "str2:0x13D",
        "str2:0x147",
        "str2:0x14A",
        "str2:0x14B",
        "str2:0x14C",
        "str2:0x163",
        "str2:0x164",
        "str2:0x167",
        "str2:0x168",
        "str2:0x16B",
        "str3:0x51",
        "str3:0x5C",
        "str3:0x5D",
        "str3:0x5E",
        "str3:0x82",
        "str3:0x87",
        "str3:0x88",
        "str3:0x89",
        "str3:0x8A",
        "str3:0x8C",
        "str3:0x8F",
        "str3:0x95",
        "str3:0x96",
    ];

    /// The message table (`message_table`) a page's key names: `battle:` is table 1, `str2:` table 2, `str3:` table 3.
    fn empty_page_table(key: &str) -> u8 {
        match key.split(':').next() {
            Some("battle") => 1,
            Some("str2") => 2,
            Some("str3") => 3,
            other => panic!("{key}: no window known for the prefix {other:?}"),
        }
    }

    /// How many lines of [`PITCH`] a window's paper holds: the tallest block of lines (from the first line's glyphs
    /// to the last line's, an em tall: the block [`centred`] centres) that is not taller than the paper
    /// ([`window_paper`]). The same count as the lines of a page that is not centred, which start 12 pixels into the
    /// paper (`str3:0x5C`'s three lines of the story window end 1 pixel under it by [`RISE`]).
    fn window_rows(table: u8) -> usize {
        let (_, height) = window_paper(table).expect("a window with a paper");
        ((height - EM as i32) / PITCH + 1) as usize
    }

    /// The lines the overlay draws for `text` in the window of `table`: [`wrap`] at the window's width
    /// ([`window_cells`]) with the pack's font, from the page's first line that shows to its last.
    fn lines_in_window(face: &ChineseFace, table: u8, text: &str) -> usize {
        let wrapped = wrap(text, (8 * window_cells(table, 0)) as f32, |ch| face.advance(ch));
        let inked = |line: &Vec<char>| line.iter().any(|ch| !ch.is_whitespace());
        let first = wrapped.iter().position(inked).expect("a page with a letter");
        let last = wrapped.iter().rposition(inked).expect("a page with a letter");
        last - first + 1
    }

    #[test]
    fn the_windows_hold_five_lines_of_dialogue_and_three_of_story_and_battle() {
        // measured paper (`window_paper`): battle 1x y 180 to 235, dialogue 148 to 235, story 172 to 227
        assert_eq!((window_rows(1), window_rows(2), window_rows(3)), (3, 5, 3));
    }

    #[test]
    fn the_translation_of_a_page_with_no_english_fits_the_lines_of_its_window() {
        let face = wenkai_tc();
        let tsv = include_str!("../../../translations/zh-tw/zh-tw.tsv");
        let table = Translation::parse(tsv).expect("the pack reads");
        assert_eq!(
            EMPTY_PAGES.len(),
            EMPTY_PAGES.iter().collect::<std::collections::HashSet<_>>().len()
        );
        // A name's slot takes the longest name of the pack: the page may be shown with any of the units
        let longest_name = (0..=0xFF_u32)
            .filter_map(|unit| table.text(&format!("unit:0x{unit:02X}")))
            .map(|name| render(name, |_| String::new()).replace('\n', ""))
            .max_by_key(|name| name.chars().count())
            .expect("the pack names its units");
        let mut too_long = Vec::new();
        for key in EMPTY_PAGES {
            let text = table.text(key).unwrap_or_else(|| panic!("{key} has no translation"));
            let text = render(text, |slot| match slot {
                Slot::Name(_) => longest_name.clone(),
                Slot::Number(_) => "100".to_owned(),
                Slot::Phrase(n) => table
                    .text(&format!("phrase:{n}"))
                    .map(|phrase| render(phrase, |_| String::new()).replace('\n', ""))
                    .unwrap_or_default(),
            });
            let window = empty_page_table(key);
            let (lines, room) = (lines_in_window(&face, window, &text), window_rows(window));
            if lines > room {
                too_long.push(format!(
                    "{key}: {lines} lines, its window (table {window}) holds {room}"
                ));
            }
        }
        assert!(too_long.is_empty(), "the window runs out:\n{}", too_long.join("\n"));
    }

    /// The rows of the Item help's window (table 0): its paper, measured on the map's and the battle's Item help
    /// (`mm_browse2` 3890, `bi_help30` with object 1F 4850: 2x y 345 to 452), runs from 12 pixels above the English's
    /// first line to 43 under it, 55 pixels as the story window's: three lines of [`PITCH`].
    const HELP_ROWS: usize = 3;

    /// Pages whose translation runs past the rows of its window though it fits the English's: the English page has
    /// that many lines itself, and the overlay draws on the English's rows (each checked in `en_text.tsv`).
    /// (`battle:278`: two pages of Frieza's talk made one, `merge of 2 pages into 1: 4 rows`, its English four lines.)
    const LONGER_ENGLISH: [(&str, usize); 1] = [("battle:278", 4)];

    #[test]
    fn the_translation_of_every_page_fits_the_lines_of_its_window() {
        // Every page of the Item help (0), the battle (1), the dialogue (2) and the story (3): the rows the overlay
        // draws (`wrap` at the window's width; a centred page from its first line with a letter, a page laid on the
        // English's rows (a choice, `{auto}`) from its first line) are no more than its window's
        let face = wenkai_tc();
        let tsv = include_str!("../../../translations/zh-tw/zh-tw.tsv");
        let table = Translation::parse(tsv).expect("the pack reads");
        let longest_name = (0..=0xFF_u32)
            .filter_map(|unit| table.text(&format!("unit:0x{unit:02X}")))
            .map(|name| render(name, |_| String::new()).replace('\n', ""))
            .max_by_key(|name| name.chars().count())
            .expect("the pack names its units");
        let longer: std::collections::HashMap<&str, usize> = LONGER_ENGLISH.into_iter().collect();
        let mut too_long = Vec::new();
        let mut pages = 0;
        for row in tsv.lines() {
            let Some((key, zh)) = row.split('\t').next().zip(row.split('\t').nth(1)) else {
                continue;
            };
            let window = match key.split(':').next() {
                Some("help") => 0,
                Some("battle") => 1,
                Some("str2") => 2,
                Some("str3") => 3,
                _ => continue,
            };
            let text = render(zh, |slot| match slot {
                Slot::Name(_) => longest_name.clone(),
                Slot::Number(_) => "100".to_owned(),
                Slot::Phrase(n) => table
                    .text(&format!("phrase:{n}"))
                    .map(|phrase| render(phrase, |_| String::new()).replace('\n', ""))
                    .unwrap_or_default(),
            });
            let wrapped = wrap(&text, (8 * window_cells(window, 0)) as f32, |ch| face.advance(ch));
            let inked = |line: &Vec<char>| line.iter().any(|ch| !ch.is_whitespace());
            let Some(last) = wrapped.iter().rposition(inked) else { continue };
            let first = if centres(window, zh) {
                wrapped.iter().position(inked).expect("a line with a letter")
            } else {
                0
            };
            pages += 1;
            let lines = last - first + 1;
            let room = if window == 0 { HELP_ROWS } else { window_rows(window) };
            let room = room.max(longer.get(key).copied().unwrap_or(0));
            if lines > room {
                too_long.push(format!(
                    "{key}: {lines} lines, its window (table {window}) holds {room}"
                ));
            }
        }
        assert!(pages > 1100, "{pages} pages checked");
        assert!(too_long.is_empty(), "the window runs out:\n{}", too_long.join("\n"));
    }
}
