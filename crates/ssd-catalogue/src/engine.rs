//! The string the message engine is writing, read from work RAM and the ROM (`docs/re/data/text-engine.md`): which
//! string, page, line and cell, where the window's text starts, how far the page is written, and what the name and
//! number slots hold. A translation drawn over the original's window ([`crate::translation`]) is keyed and paced
//! by it. Nothing is written; nothing of the ROM's text is kept beyond the call.

use crate::names::{TEXT_BANK, table_name};
use crate::rom::RomView;
use crate::text::decode_tile;
use crate::translation::{page_key, render, string_key};
use ssd_game::data as d;
use std::collections::{HashMap, HashSet};

/// A string longer than this has lost its end.
const MAX_STRING: usize = 2048;
/// `message_handler` while the handler of `E0 2D` runs (`01:DA96`): the gravity training's choice, after the page
/// `str3:0x46` ("Choose the level of / gravity training / {num:1}x Gravity", message `$125`). Up and Down change the
/// level in [`GRAVITY_LEVEL`], and the handler writes it, (level + 1) x 10, as three tiles straight into the window's
/// third line (`$7E3000 + $06EA`, `01:DADF`-`01:DB25`), over the page the engine wrote. The number slot `$1610`
/// follows it from 10 to 90 but keeps the last one for 100 (`01:DB08` writes the tiles and `$1611` only), so while
/// the handler runs the first number slot is read from the level.
const GRAVITY_CHOICE: u8 = 0x80 + 0x2D;
/// `$0734`: the level of the gravity training's choice, 0 (10x) to 9 (100x) (`01:DAC6`-`01:DADC`).
const GRAVITY_LEVEL: u32 = 0x7E_0734;
/// The cells a name or a phrase is taken to have where the ROM does not say (its slot is empty).
const SLOT_CELLS: usize = 6;
/// The tile-map offset the handler of `E0 2D` writes the gravity level's three tiles at (`$7E3000 + $06EA`).
const GRAVITY_TILES: u16 = 0x06EA;
const GRAVITY_CELLS: usize = 3;

/// What the engine shows: the coordinates of the page being written and how far it is.
#[derive(Debug, Clone, PartialEq)]
pub struct OnScreen {
    /// `message_table` and `message_string`.
    pub table: u8,
    pub string: u16,
    /// The page of the string (0 the first) and its key in a translation (`str3:0x3E/p2`).
    pub page: usize,
    pub key: String,
    /// The low nibble of `message_state`: 1 writing, 2 and 3 waiting for A, 5 writing a name, 6 a timed wait; 0 for
    /// a message written in one frame (`message_state` bit 5 or 6); 3 also for the page before the one a stopped string
    /// was left at ([`OnScreen::stopped`]).
    pub state: u8,
    /// No message is up: a story transition stopped the string after this page and the bytes wait at the next page's
    /// first byte (`stopped_on_screen`). The page is on the screen only while its English shows there.
    pub stopped: bool,
    /// `message_line` and `counter_071f`: where the next cell goes.
    pub line: u8,
    pub cell: u8,
    /// `message_text_base`: the tile-map offset (bytes, 32 cells of 2 bytes a row) of the text's first cell.
    pub base: u16,
    /// The page shows a portrait: its first line starts one cell to the left.
    pub portrait: bool,
    /// The cells of each line of the page in the original (spaces and slots counted).
    pub lines: Vec<usize>,
    /// What the engine writes in each cell of each line of the page, as the HD font reads it: a letter, digit or
    /// mark, `' '` for a space (the engine moves on without writing the cell), and `None` for a cell not to compare:
    /// a tile the HD font has no glyph for (`…`, kana, the fixed tiles of `C8`-`CC` and `FC`, the second set's) and
    /// the cells the game writes again after the engine (the gravity training's choice, [`GRAVITY_CHOICE`]). A
    /// name or number slot is the tiles the engine copies from it, an empty name slot none (unlike `lines`, which
    /// counts a name as it is spelled and a number trimmed). Empty for the ending's pages and the intro.
    pub english: Vec<Vec<Option<char>>>,
    /// The share of the page's cells written, 0 to 1 (1 when the page waits or was written at once).
    pub progress: f32,
    /// The names in the slots of `{name:1}` to `{name:3}`, when a slot holds one.
    pub names: [Option<NameSlot>; 3],
    /// The numbers in the slots of `{num:1}` to `{num:3}` (empty when a slot holds none).
    pub numbers: [String; 3],
}

/// A name in a slot: its key in a translation (`unit:0x02`, `object:7`, `attack:12`) and the original's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSlot {
    pub key: String,
    pub english: String,
}

impl OnScreen {
    /// The tile-map cell (column, row) of the text's first cell.
    #[must_use]
    pub fn base_cell(&self) -> (u16, u16) {
        ((self.base % 64) / 2, self.base / 64)
    }
}

/// The byte of work RAM at the bus address `address` (bank `$7E`).
fn byte(wram: &[u8], address: u32) -> Option<u8> {
    wram.get((address - 0x7E_0000) as usize).copied()
}

fn word(wram: &[u8], address: u32) -> Option<u16> {
    Some(u16::from(byte(wram, address)?) | u16::from(byte(wram, address + 1)?) << 8)
}

/// What the engine is writing, from the work RAM `wram` at the end of a frame and the ROM image `rom`; `None` when
/// no message is up, the engine is between two strings (state 0, 4: its bytes still name the last string), or the
/// string cannot be read. `hint` is the page shown before, for a string longer than the 256 bytes its offset
/// counts. Without a message, in the ending, the page of the ending's texts on the screen ([`ending_on_screen`]);
/// in the intro, its text ([`intro_on_screen`]); else, with the bytes stopped at the first byte of a page, the page
/// before it (`stopped_on_screen`, [`OnScreen::stopped`]).
#[must_use]
pub fn on_screen(rom: &[u8], wram: &[u8], hint: Option<usize>) -> Option<OnScreen> {
    message_on_screen(rom, wram, hint, false)
        .or_else(|| ending_on_screen(rom, wram))
        .or_else(|| intro_on_screen(rom, wram))
        .or_else(|| stopped_on_screen(rom, wram, hint))
}

/// The page a stopped string left on the screen ([`OnScreen::stopped`], state 3): no message is up (`message_state`
/// $00) and the bytes still name a string, its offset the first byte of a page after the first, one with a cell
/// ([`stopped_page`]); `None` otherwise. Whether the page still shows is the screen's to say: the translation names it
/// only where its English shows.
fn stopped_on_screen(rom: &[u8], wram: &[u8], hint: Option<usize>) -> Option<OnScreen> {
    message_on_screen(rom, wram, hint, true)
}

/// The message engine's page of [`on_screen`]; with `stopped`, of [`stopped_on_screen`] only.
fn message_on_screen(
    rom: &[u8],
    wram: &[u8],
    hint: Option<usize>,
    stopped: bool,
) -> Option<OnScreen> {
    let rom = RomView::new(rom).ok()?;
    let status = byte(wram, d::MESSAGE_STATE)?;
    let instant = status & 0x60 != 0;
    let state = status & 0x0F;
    if stopped {
        if status != 0 {
            return None;
        }
    } else if status & 0x80 == 0 || !(instant || matches!(state, 1 | 2 | 3 | 5 | 6)) {
        return None;
    }
    let table = byte(wram, d::MESSAGE_TABLE)?;
    let string = word(wram, d::MESSAGE_STRING)?;
    let slots: [Option<(NameSlot, Vec<Option<char>>)>; 3] = std::array::from_fn(|n| {
        let slot = word(wram, d::MESSAGE_NAME_SLOTS + 2 * n as u32)?;
        name_slot(rom, slot)
    });
    let names = slots.clone().map(|slot| slot.map(|(name, _)| name));
    let number_tiles = |n: usize| {
        let at = (d::MESSAGE_NUMBER_SLOTS - 0x7E_0000) as usize + 16 * n;
        wram.get(at..at + 16).unwrap_or_default()
    };
    let mut numbers: [String; 3] = std::array::from_fn(|n| number(number_tiles(n)));
    let mut number_english: [Vec<Option<char>>; 3] = std::array::from_fn(|n| {
        number_tiles(n)
            .iter()
            .take_while(|&&tile| tile != 0xFF)
            .map(|&tile| on_screen_char(tile))
            .collect()
    });
    let gravity = byte(wram, d::MESSAGE_HANDLER) == Some(GRAVITY_CHOICE);
    if gravity && let Some(level) = byte(wram, GRAVITY_LEVEL) {
        numbers[0] = ((u32::from(level.min(9)) + 1) * 10).to_string();
    }
    if gravity {
        number_english[0].fill(None);
    }
    let start = string_address(rom, table, string)?;
    let slot_cells = |code: u8| -> usize {
        match code {
            0xF9..=0xFB => names[usize::from(0xFB - code)]
                .as_ref()
                .map_or(SLOT_CELLS, |name| name.english.chars().count()),
            0xF6..=0xF8 => numbers[usize::from(0xF8 - code)].chars().count(),
            _ => SLOT_CELLS,
        }
    };
    let slot_english = |code: u8| -> Vec<Option<char>> {
        match code {
            0xF9..=0xFB => slots[usize::from(0xFB - code)]
                .as_ref()
                .map(|(_, english)| english.clone())
                .unwrap_or_default(),
            0xF6..=0xF8 => number_english[usize::from(0xF8 - code)].clone(),
            // (a phrase, `E0`-`E7`: no string of the English ROM has one)
            _ => vec![None; SLOT_CELLS],
        }
    };
    let (pages, bytes) =
        pages(rom, start, if table == 1 { 2 } else { 1 }, &slot_cells, &slot_english)?;
    let offset = usize::from(byte(wram, d::COUNTER_071D)?);
    // In state 2 the offset is already past the page's `FE FF`
    let at = if state == 2 { offset.wrapping_sub(2) & 0xFF } else { offset };
    let page = if stopped { stopped_page(&pages, bytes, at)? } else { page_of(&pages, at, hint)? };
    let lines = pages[page].lines.clone();
    // (the page before the one the bytes stopped at is whole: its last cell written)
    let (line, cell) = if stopped {
        let last = lines.len().saturating_sub(1);
        (u8::try_from(last).ok()?, u8::try_from(lines[last]).ok()?)
    } else {
        (byte(wram, d::MESSAGE_LINE)?, byte(wram, d::COUNTER_071F)?)
    };
    let base = word(wram, d::MESSAGE_TEXT_BASE)?;
    let mut english = pages[page].english.clone();
    if gravity {
        written_again(&mut english, base, GRAVITY_TILES, GRAVITY_CELLS);
    }
    let progress = if instant || !matches!(state, 1 | 5) {
        1.0
    } else {
        let total: usize = lines.iter().sum();
        // While a name is written (state 5) the cell does not move: the slot's code is still the next byte, and
        // its name counts as written
        let naming = match rom.u8(start + offset as u32) {
            Ok(code @ (0xF6..=0xFB | 0xE0..=0xE7)) if state == 5 => slot_cells(code),
            _ => 0,
        };
        let written: usize =
            lines.iter().take(usize::from(line)).sum::<usize>() + usize::from(cell) + naming;
        if total == 0 { 1.0 } else { (written as f32 / total as f32).min(1.0) }
    };
    Some(OnScreen {
        table,
        string,
        page,
        key: page_key(&string_key(table, u32::from(string)), page),
        state: if stopped {
            3
        } else if instant {
            0
        } else {
            state
        },
        stopped,
        line,
        cell,
        base,
        portrait: pages[page].portrait,
        lines,
        english,
        progress,
        names,
        numbers,
    })
}

/// `game_mode` while the ending runs: `00:96E7` calls `00:B013`, which runs a frame loop of its own (`00:B0CD`) through
/// the captions after Frieza, the credits and, with Vegeta in the party, his scene and the hidden fight (still `$11`).
const ENDING_MODE: u8 = 0x11;
/// `game_mode` of the second ending, after the hidden fight is won: `00:B340` writes texts 2 (at once), 3 (`$73` =
/// `$3F0`) and 4 (`$770`) as `00:B013` writes the first ones.
const SECOND_ENDING_MODE: u8 = 0x12;
/// Not a `message_table` value: the [`OnScreen::table`] of a page of the ending's texts.
pub const ENDING_TABLE: u8 = 0x11;
/// The ending's texts, bank 0 (`docs/re/data/text-engine.md`): the immediates of the calls to `00:B2B1` at `00:B071`,
/// `00:B25C`, `00:B3DD`, `00:B45F` and `00:B4BE`; their keys are `ending:0` to `ending:4` (`xtask text-dump`). `FF`
/// ends a line, `FF FF` a page, `FF 00` the text. Not the message engine's.
pub const ENDING_TEXTS: [u32; 5] = [0x00_BD87, 0x00_BEC7, 0x00_BFB4, 0x00_BFF0, 0x00_C052];
/// `$7B`: the address (bank 0) of the next byte `00:BA73` reads, a cell every 8 frames; `00:B2B1` points it at a
/// text. Past a page's `FF FF` it is the next page's first byte (`00:BA9C`); at the text's `FF 00` it stays on the
/// `FF` (`00:BAA4`).
const ENDING_POINTER: u32 = 0x7E_007B;
/// `$7F`: non-zero while the program waits with a page written (`$10` after its `FF FF`, `00:BA9F`; `$12` at the
/// text's end, `00:BAA7`), counting down; 0 while a page is written.
const ENDING_WAIT: u32 = 0x7E_007F;
/// `$63`: the tile-map offset of a page's first cell (`00:B2B3`, `00:BA98`; `$0548`, column 4 of row 21).
const ENDING_BASE: u32 = 0x7E_0063;
/// `$65`: a line's first cell, from the start of the row before it (`00:BA8A`: `$7D & $FFC0` plus `$65`; `$0088`).
const ENDING_LINE: u32 = 0x7E_0065;
/// The BG1 tile-map buffer `00:BAEA` writes a cell into at `$7D` (the tile, then attribute 0).
const ENDING_MAP: u32 = 0x7E_3000;

/// The page of the ending's texts on the screen, from the work RAM `wram` at the end of a frame and the ROM image
/// `rom`: in game mode `$11` or `$12`, the text and page `$7B` is in (written up to it), or the page before while the program
/// waits after it (`$7F`); its key is `ending:N` (`/p2`, `/p3`.. for the next pages), its table [`ENDING_TABLE`].
/// `None` outside the ending, when no cell of the page is written yet, or when the tile map no longer holds every
/// written cell as `00:BAEA` wrote it (the screen cleared between two pages or after the text, the hidden fight
/// after Vegeta's scene, in mode `$11` with `$7B` left in his text).
#[must_use]
pub fn ending_on_screen(rom: &[u8], wram: &[u8]) -> Option<OnScreen> {
    if !matches!(byte(wram, d::GAME_MODE)? & 0x7F, ENDING_MODE | SECOND_ENDING_MODE) {
        return None;
    }
    let rom = RomView::new(rom).ok()?;
    let pointer = u32::from(word(wram, ENDING_POINTER)?);
    let (text, pages) = ENDING_TEXTS.iter().enumerate().find_map(|(text, &start)| {
        let pages = ending_pages(rom, start)?;
        (start..=pages.last()?.1).contains(&pointer).then_some((text, pages))
    })?;
    let mut page = pages.iter().position(|&(start, end)| (start..=end).contains(&pointer))?;
    let mut upto = pointer;
    if byte(wram, ENDING_WAIT)? != 0 && page > 0 && pointer == pages[page].0 {
        page -= 1;
        upto = pages[page].1;
    }
    let (start, end) = pages[page];
    let line_start = word(wram, ENDING_LINE)?;
    let mut offset = word(wram, ENDING_BASE)?;
    let (mut lines, mut written) = (vec![0_usize], Vec::new());
    let (mut line, mut cell) = (0_usize, 0_usize);
    for address in start..end {
        let code = rom.u8(address).ok()?;
        if code == 0xFF {
            offset = (offset & !0x3F).wrapping_add(line_start);
            lines.push(0);
            if address < upto {
                (line, cell) = (line + 1, 0);
            }
            continue;
        }
        *lines.last_mut()? += 1;
        if address < upto {
            written.push((offset, code));
            cell += 1;
        }
        offset = offset.wrapping_add(2);
    }
    let on_map = |&(offset, tile): &(u16, u8)| {
        word(wram, ENDING_MAP + u32::from(offset)) == Some(u16::from(tile))
    };
    if written.is_empty() || !written.iter().all(on_map) {
        return None;
    }
    let total: usize = lines.iter().sum();
    let whole = upto == end;
    Some(OnScreen {
        table: ENDING_TABLE,
        string: u16::try_from(text).ok()?,
        page,
        key: page_key(&format!("ending:{text}"), page),
        state: if whole { 2 } else { 1 },
        stopped: false,
        line: u8::try_from(line).ok()?,
        cell: u8::try_from(cell).ok()?,
        base: word(wram, ENDING_BASE)?,
        portrait: false,
        lines,
        english: Vec::new(),
        progress: if whole || total == 0 {
            1.0
        } else {
            (written.len() as f32 / total as f32).min(1.0)
        },
        names: [None, None, None],
        numbers: Default::default(),
    })
}

/// The pages of the ending's text at `start`: the address of each one's first byte and of the `FF` that ends it.
fn ending_pages(rom: RomView<'_>, start: u32) -> Option<Vec<(u32, u32)>> {
    let (mut pages, mut page, mut address) = (Vec::new(), start, start);
    while address - start < MAX_STRING as u32 {
        if rom.u8(address).ok()? != 0xFF {
            address += 1;
            continue;
        }
        match rom.u8(address + 1).ok()? {
            0x00 => {
                pages.push((page, address));
                return Some(pages);
            }
            0xFF => {
                pages.push((page, address));
                address += 2;
                page = address;
            }
            _ => address += 1,
        }
    }
    None
}

/// `game_mode` through the title routine `00:9D97` (the logo, the attract loop with its intro, the title screen),
/// before the main loop runs a mode (`docs/re/data/title-chapters.md`).
const TITLE_MODE: u8 = 0x87;
/// Not a `message_table` value: the [`OnScreen::table`] of the intro's text.
pub const INTRO_TABLE: u8 = 0x87;
/// The key of the intro's text; its lines are `intro:0` to `intro:31` in a translation ([`intro_key`]).
pub const INTRO_KEY: &str = "intro";
/// The lines of the intro's text: the even rows 0 to 62 of its tile map, line `n` on row `2n`.
pub const INTRO_LINES: usize = 32;
/// `decode_bit_tree_to_9000` (`docs/re/routines/decode_bit_tree_to_9000.md`), called once, by the intro's loader
/// (`00:A3C6` -> `00:FD30`): it walks the node table at `00:FD49` from the root `$1A`, a bit of the stream at `00:F150`
/// a step (least significant first), and writes each leaf's low byte and a zero byte, 2,048 words, to `$7E:9000`:
/// the intro's tile map, 32 cells a row, 64 rows. `00:A3CE` copies rows 0 to 31 to BG1's map (`$6000`), `00:A3DE` rows
/// 32 to 63 to BG2's (`$6800`), then `00:A417` clears `$7E:9000-97FF`; rows 32 to 63 stay at `$7E:9800`.
const INTRO_TREE: u32 = 0x00_FD49;
const INTRO_STREAM: u32 = 0x00_F150;
const INTRO_ROOT: u16 = 0x001A;
/// The cells of the intro's tile map, and those of its first half (rows 0 to 31, BG1's).
const INTRO_MAP_CELLS: usize = 64 * 32;
const INTRO_HALF: usize = INTRO_MAP_CELLS / 2;
/// Where rows 32 to 63 of the intro's tile map stay while it runs (BG2's text).
const INTRO_KEPT: u32 = 0x7E_9800;
/// `$7C`: 0 in the attract loop (`00:A458`), `$0140` on the title screen (`00:A5D7`).
const INTRO_ATTRACT: u32 = 0x7E_007C;
/// `$73`: the intro loop's passes left, `$06A8` (`00:A468`) down to 1 (`00:A50E`: the loop ends at 0).
const INTRO_LEFT: u32 = 0x7E_0073;
const INTRO_FRAMES: u16 = 0x06A8;
/// `$39` and `$35`: the vertical scroll of BG1 and BG2, written to `BG1VOFS` and `BG2VOFS` by the NMI (`00:831B`,
/// `00:8307`). The intro starts both at `$0100` (`00:A45F`); `00:AD9E` adds 1 to `$39`, up to `$0300`, on every other
/// pass of the loop (a pass takes about two frames) and, from `$0200`, to `$35` with it, which goes on after `$39`
/// stops: the text scrolls up the screen a row of pixels about every four frames.
const INTRO_BG1_SCROLL: u32 = 0x7E_0039;
const INTRO_BG2_SCROLL: u32 = 0x7E_0035;
/// The rows of the 1x frame (239 high) a line of the intro starting there shows letters on: a line at -7 has the last
/// of its 8 rows of letters on the frame's row 0. One row more at the top: the scroll read at the end of a frame can
/// be a row ahead of the one the frame was drawn with (it moves a row about every four frames), so the last line's
/// last row of letters can still be there when the scroll says it is gone.
const INTRO_SCREEN: std::ops::Range<i32> = -8..239;

/// The key of line `line` of the intro's text in a translation (`intro:0` to `intro:31`).
#[must_use]
pub fn intro_key(line: usize) -> String {
    format!("{INTRO_KEY}:{line}")
}

/// The intro's text, the "LONG, LONG AGO" that scrolls up over Goku in the attract loop, from the work RAM `wram` at
/// the end of a frame and the ROM image `rom`: in game mode `$87` with `$7C` 0 and the intro's frames running (`$73`),
/// while `$7E:9800` holds the second half of its tile map as the ROM decodes it and one line or more of it is on the
/// screen. Its key is [`INTRO_KEY`], its table [`INTRO_TABLE`], its `lines` the cells of each of its
/// [`INTRO_LINES`] lines; [`intro_line_top`] says where each one is. `None` otherwise (the title screen, the logo and
/// the clouds of the attract loop, the frames before the first line comes up and after the last one has gone).
#[must_use]
pub fn intro_on_screen(rom: &[u8], wram: &[u8]) -> Option<OnScreen> {
    if byte(wram, d::GAME_MODE)? != TITLE_MODE
        || word(wram, INTRO_ATTRACT)? != 0
        || !(1..=INTRO_FRAMES).contains(&word(wram, INTRO_LEFT)?)
    {
        return None;
    }
    let map = intro_map(RomView::new(rom).ok()?)?;
    let kept = (0..INTRO_HALF).all(|cell| {
        word(wram, INTRO_KEPT + 2 * cell as u32) == Some(u16::from(map[INTRO_HALF + cell]))
    });
    if !kept {
        return None;
    }
    let screen = OnScreen {
        table: INTRO_TABLE,
        string: word(wram, INTRO_BG2_SCROLL)?,
        page: 0,
        key: INTRO_KEY.to_owned(),
        state: 2,
        stopped: false,
        line: 0,
        cell: 0,
        base: word(wram, INTRO_BG1_SCROLL)?,
        portrait: false,
        lines: map
            .chunks(64)
            .map(|row| row[..32].iter().rposition(|&tile| tile != 0).map_or(0, |last| last + 1))
            .collect(),
        english: Vec::new(),
        progress: 1.0,
        names: [None, None, None],
        numbers: Default::default(),
    };
    (0..INTRO_LINES)
        .any(|line| intro_line_top(&screen, line).is_some_and(|top| INTRO_SCREEN.contains(&top)))
        .then_some(screen)
}

/// The row of the 1x frame line `line` of the intro's text starts on (the top of its letters), from its scroll in
/// `screen` (an [`intro_on_screen`]: `base` is BG1's vertical scroll, `string` BG2's); `None` for another screen or
/// past the last line. Lines 0 to 15 are BG1's, 16 to 31 BG2's; a map 64 rows high (512 lines) shows its line `L`
/// on the frame's row `L - scroll - 1` (the frame's first row is the scanline after the scroll's), wrapping.
#[must_use]
pub fn intro_line_top(screen: &OnScreen, line: usize) -> Option<i32> {
    if screen.table != INTRO_TABLE || line >= INTRO_LINES {
        return None;
    }
    let (scroll, row) =
        if line < INTRO_LINES / 2 { (screen.base, line) } else { (screen.string, line - 16) };
    let top = (16 * row as i32 - i32::from(scroll & 0x1FF) - 1).rem_euclid(512);
    Some(if top >= 256 { top - 512 } else { top })
}

/// The intro's tile map as `decode_bit_tree_to_9000` writes it: [`INTRO_MAP_CELLS`] tiles, 32 a row, rows 0 to 63.
/// `None` when the ROM's tables do not decode (a leaf not reached within 32 bits, or past the ROM).
fn intro_map(rom: RomView<'_>) -> Option<Vec<u8>> {
    let (mut map, mut stream, mut mask) = (Vec::with_capacity(INTRO_MAP_CELLS), 0_u32, 0x01_u8);
    while map.len() < INTRO_MAP_CELLS {
        let mut node = INTRO_ROOT;
        let mut steps = 0;
        while node & 0x8000 == 0 {
            steps += 1;
            if steps > 32 {
                return None;
            }
            let bit = rom.u8(INTRO_STREAM + stream).ok()? & mask != 0;
            node = rom
                .u16(INTRO_TREE + u32::from((node << 2).wrapping_add(if bit { 2 } else { 0 })))
                .ok()?;
            (mask, stream) = if mask == 0x80 { (0x01, stream + 1) } else { (mask << 1, stream) };
        }
        map.push(node as u8);
    }
    Some(map)
}

/// A table of bank `$02` names: its pointers, their size in bytes, and the translation key of an index.
type NameTable = (u32, u32, fn(u32) -> String);

/// The names the game writes straight into its tile maps (the battle's lists of fighters, attacks and objects, the
/// characters' names), as the screen spells them, and their translations in `table`: the strings of the four name
/// tables of bank `$02` whose keys (`string:N`, `unit:0xNN`, `attack:N`, `object:N`) the translation has. A name
/// shorter than two letters, or one the tables spell two ways, is left out (a single letter on screen is not a
/// name). An entry that points at the string of an entry before it (in the order string, unit, attack, object) is
/// that entry's name on screen and is skipped: the English patch points some entries at another's string (Nail's
/// `unit:0x22` at Chiaotzu's `string:6`), which the translation names as they are in the story, and the screen
/// spells them alike. Nothing of the ROM is kept but the map.
#[must_use]
pub fn screen_names(
    rom: &[u8],
    table: &crate::translation::Translation,
) -> HashMap<String, String> {
    let Ok(rom) = RomView::new(rom) else { return HashMap::new() };
    let tables: [NameTable; 4] = [
        (d::STRING_POINTERS, d::STRING_POINTERS_SIZE, |i| format!("string:{i}")),
        (d::UNIT_NAME_POINTERS, d::UNIT_NAME_POINTERS_SIZE, |i| format!("unit:0x{i:02X}")),
        (d::ATTACK_NAME_POINTERS, d::ATTACK_NAME_POINTERS_SIZE, |i| format!("attack:{i}")),
        (d::OBJECT_NAME_POINTERS, d::OBJECT_NAME_POINTERS_SIZE, |i| format!("object:{i}")),
    ];
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    let mut strings: HashSet<u32> = HashSet::new();
    for (pointers, size, key) in tables {
        for index in 0..size / 2 {
            let Ok(string) = rom.pointer(pointers + 2 * index, TEXT_BANK) else { continue };
            if !strings.insert(string) {
                continue;
            }
            let Ok(name) = table_name(rom, pointers, index) else { continue };
            let english = name.trimmed();
            let Some(text) = table.text(&key(index)) else { continue };
            let translated = render(text, |_| String::new()).replace('\n', "");
            if !name.is_text() {
                // One word and a tile the HD font has no letter for after it (object 30's `Saiba<71>`): on the screen
                // the word, which is the whole name's (not the start of unit 4's `Saibai`)
                if let Some(word) = ended_word(english) {
                    if !translated.trim().is_empty() {
                        add(&mut names, word.to_owned(), translated);
                    }
                    continue;
                }
                // Two words set apart by one tile the HD font has no letter for (`KI-USE`, tile $43 between): on
                // the screen two words; each is a name of its own where the translation sets a `-` between its two
                // words (`KI-使用`)
                for (english, translated) in marked_words(english).into_iter().flatten().zip(
                    translated
                        .split_once('-')
                        .map(|(a, b)| [a.to_owned(), b.to_owned()])
                        .into_iter()
                        .flatten(),
                ) {
                    if english.chars().count() >= 2
                        && english != translated
                        && !translated.trim().is_empty()
                    {
                        add(&mut names, english.to_owned(), translated);
                    }
                }
                continue;
            }
            if english.chars().count() < 2 || translated.trim().is_empty() {
                continue;
            }
            add(&mut names, english.to_owned(), translated);
        }
    }
    // The menu blocks of bank `$02` (`block:0xNN`): a row of a block is a word of its own (`Atk.`, `Pick`)
    for index in 0..MENU_BLOCKS {
        let Some(text) = table.text(&format!("block:0x{index:02X}")) else { continue };
        let translated = render(text, |_| String::new());
        let Some(rows) = block_rows(rom, index) else { continue };
        for (english, translated) in rows.iter().zip(translated.split('\n')) {
            // Tiles the HD font has no letter for at the ends of a row are a frame's marks (the dashes of
            // `--Speed--`): the word between them is the row's, as the screen's letters spell it
            let english = english.trim_matches(|ch: char| ch == ' ' || ch == '\u{fffd}');
            let translated = translated.trim();
            if english.chars().count() < 2 || translated.is_empty() || english.contains('\u{fffd}')
            {
                continue;
            }
            names
                .entry(english.to_owned())
                .and_modify(|seen| {
                    if seen.as_deref() != Some(translated) {
                        *seen = None;
                    }
                })
                .or_insert(Some(translated.to_owned()));
        }
    }
    // (a name the translation keeps as it is, `Lv` or `HP`, is left to the HD letters)
    names
        .into_iter()
        .filter_map(|(english, translated)| Some((english, translated?)))
        .filter(|(english, translated)| english != translated)
        .collect()
}

/// Adds the name `english` -> `translated` to `names`: a name the tables spell two ways is left out (`None`).
fn add(names: &mut HashMap<String, Option<String>>, english: String, translated: String) {
    names
        .entry(english)
        .and_modify(|seen| {
            if seen.as_deref() != Some(translated.as_str()) {
                *seen = None;
            }
        })
        .or_insert(Some(translated));
}

/// The two words of a name spelled `word<XX>word` (one tile without a letter between two words of letters only,
/// `KI<43>USE`), or `None`.
fn marked_words(name: &str) -> Option<[&str; 2]> {
    let (left, rest) = name.split_once('<')?;
    let (code, right) = rest.split_once('>')?;
    let word = |w: &str| !w.is_empty() && w.chars().all(|ch| ch.is_ascii_alphabetic());
    (code.len() == 2 && code.chars().all(|ch| ch.is_ascii_hexdigit()) && word(left) && word(right))
        .then_some([left, right])
}

/// The word of a name spelled `word<XX>` (a word of two letters or more, then one tile without a letter and nothing
/// after it: object 30's `Saiba<71>`), or `None`.
fn ended_word(name: &str) -> Option<&str> {
    let (word, rest) = name.split_once('<')?;
    let code = rest.strip_suffix('>')?;
    (word.chars().count() >= 2
        && word.chars().all(|ch| ch.is_ascii_alphabetic())
        && code.len() == 2
        && code.chars().all(|ch| ch.is_ascii_hexdigit()))
    .then_some(word)
}

/// The blocks of `bank2_pointer_table` that can be text (`docs/re/data/menus.md`, `xtask text-dump`).
const MENU_BLOCKS: u32 = 80;

/// The rows of menu block `index` as the screen spells them (a tile the HD font has no letter for is U+FFFD); `None`
/// for a block that is not text (no place in the tile map, or less than half letters).
fn block_rows(rom: RomView<'_>, index: u32) -> Option<Vec<String>> {
    let address = rom.pointer(d::BANK2_POINTER_TABLE + 2 * index, 0x02).ok()?;
    let head = rom.slice(address, 4).ok()?;
    let (width, height) = (usize::from(head[0]) + 1, usize::from(head[1]) + 1);
    let offset = u16::from_le_bytes([head[2], head[3]]);
    let tiles = rom.slice(address + 4, width * height).ok()?;
    let letters = tiles.iter().filter(|&&tile| decode_tile(tile).is_some()).count();
    if offset == 0 || letters * 2 < tiles.len() {
        return None;
    }
    let spell =
        |row: &[u8]| row.iter().map(|&tile| decode_tile(tile).unwrap_or('\u{fffd}')).collect();
    Some(tiles.chunks(width).map(spell).collect())
}

/// The bus address of string `index` of the `message_table` value `table`: `string_table_pointers` gives the
/// table (a word, then the bank), whose words point into its bank.
fn string_address(rom: RomView<'_>, table: u8, index: u16) -> Option<u32> {
    if table > 7 {
        return None;
    }
    let entry = d::STRING_TABLE_POINTERS + 3 * u32::from(table);
    let pointers = u32::from(rom.u16(entry).ok()?) | u32::from(rom.u8(entry + 2).ok()?) << 16;
    rom.pointer(pointers + 2 * u32::from(index), (pointers >> 16) as u8).ok()
}

/// The name a slot word holds (the first byte the index, the second the type: bit 7 a unit, 6 an object, 5 an
/// attack), and what the engine writes in its cells ([`OnScreen::english`]); `None` for an empty slot or another type.
fn name_slot(rom: RomView<'_>, slot: u16) -> Option<(NameSlot, Vec<Option<char>>)> {
    let (index, kind) = (slot & 0xFF, slot >> 8);
    let (key, table) = if kind & 0x80 != 0 {
        (format!("unit:0x{index:02X}"), d::UNIT_NAME_POINTERS)
    } else if kind & 0x40 != 0 {
        (format!("object:{index}"), d::OBJECT_NAME_POINTERS)
    } else if kind & 0x20 != 0 {
        (format!("attack:{index}"), d::ATTACK_NAME_POINTERS)
    } else {
        return None;
    };
    let name = table_name(rom, table, u32::from(index)).ok()?;
    // (as `trimmed`: the padding spaces at its end are no cells of the page)
    let mut tiles = name.tiles.as_slice();
    while let [rest @ .., SPACE] = tiles {
        tiles = rest;
    }
    let cells = tiles.iter().map(|&tile| on_screen_char(tile)).collect();
    Some((NameSlot { key, english: name.trimmed().to_owned() }, cells))
}

/// The space tile: the engine moves on a cell without writing it.
const SPACE: u8 = 0x01;

/// What the HD font reads a cell written with the tile `code` as ([`OnScreen::english`]): its letter, digit or mark,
/// `' '` for a space; `None` for a tile it has no glyph for (`…`, kana, kanji, the frames).
fn on_screen_char(code: u8) -> Option<char> {
    decode_tile(code).filter(|&ch| ch != '…')
}

/// Marks the `count` cells the game writes again from the tile-map offset `offset` (bytes) as not compared, in the
/// `english` of a page whose text starts at `base` (a line is `$80` bytes, two tile rows; every line of a page starts
/// at `base`, a portrait's too: scans of the screen show its opening quote and the lines under it in one column).
fn written_again(english: &mut [Vec<Option<char>>], base: u16, offset: u16, count: usize) {
    let Some(from) = offset.checked_sub(base) else { return };
    let (line, within) = (usize::from(from / 0x80), usize::from(from % 0x80));
    if within >= 0x40 {
        return;
    }
    let first = within / 2;
    if let Some(cells) = english.get_mut(line) {
        for cell in cells.iter_mut().skip(first).take(count) {
            *cell = None;
        }
    }
}

/// The text of a number slot: its tiles up to `FF`, spaces trimmed.
fn number(tiles: &[u8]) -> String {
    let text: String = tiles
        .iter()
        .take_while(|&&tile| tile != 0xFF)
        .filter_map(|&tile| decode_tile(tile))
        .collect();
    text.trim().to_owned()
}

/// One page of a string: the byte offset (from the string's start) of its first byte, its lines' cells, what the
/// engine writes in them ([`OnScreen::english`]), and whether it shows a portrait.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Page {
    start: usize,
    lines: Vec<usize>,
    english: Vec<Vec<Option<char>>>,
    portrait: bool,
}

impl Page {
    fn at(start: usize) -> Self {
        Self { start, lines: vec![0], english: vec![Vec::new()], portrait: false }
    }
}

/// The pages of the string at `start`, as `01:CFB4` reads it (`portrait_operands` is 2 in table 1), and its length in
/// bytes (its `FF FF` or `FD` counted); `slot_cells` gives the cells of a name, number or phrase code, `slot_english`
/// what the engine writes in them. `None` when the string leaves the ROM or has no end.
fn pages(
    rom: RomView<'_>,
    start: u32,
    portrait_operands: usize,
    slot_cells: &dyn Fn(u8) -> usize,
    slot_english: &dyn Fn(u8) -> Vec<Option<char>>,
) -> Option<(Vec<Page>, usize)> {
    let mut pages = vec![Page::at(0)];
    let mut at = 0;
    while at < MAX_STRING {
        let code = rom.u8(start + at as u32).ok()?;
        at += 1;
        let page = pages.last_mut()?;
        let line = page.lines.last_mut()?;
        let english = page.english.last_mut()?;
        match code {
            0xFF if rom.u8(start + at as u32).ok()? == 0xFF => return Some((pages, at + 1)),
            0xFF => {
                page.lines.push(0);
                page.english.push(Vec::new());
            }
            // `FE FF`: the next page starts after the `FF`
            0xFE => {
                at += 1;
                pages.push(Page::at(at));
            }
            0xFD => return Some((pages, at)),
            0xF4 => {
                page.portrait = true;
                at += portrait_operands;
            }
            0xF6..=0xFB | 0xE0..=0xE7 => {
                *line += slot_cells(code);
                english.extend(slot_english(code));
            }
            _ => {
                *line += 1;
                english.push(on_screen_char(code));
            }
        }
    }
    None
}

/// The ROM-derived story pages near the message currently named by WRAM.
/// A story cutscene can leave an earlier page visibly on the PPU after the
/// registers have advanced to a different string. Unlike an OCR or box-colour
/// heuristic, these candidates contain the actual English glyphs from the ROM,
/// which the renderer must *also* locate on the current PPU before using one.
///
/// The pages are provisional: the caller should select only a uniquely visible
/// page. Variable-length name or number slots cannot be reconstructed safely
/// from an older message; those candidates are deliberately omitted.
#[must_use]
pub fn story_pages_near(
    rom: &[u8], current: &OnScreen, radius: u16,
) -> Vec<OnScreen> {
    if current.table != 3 || radius > 4 {
        return Vec::new();
    }
    let Ok(view) = RomView::new(rom) else { return Vec::new() };
    let mut found = Vec::new();
    for index in current.string.saturating_sub(radius)..=current.string.saturating_add(radius) {
        let Some(start) = string_address(view, 3, index) else { continue };
        let Some((pages, _)) = pages(
            view, start, 1,
            &|_| SLOT_CELLS,
            &|_| Vec::new(),
        ) else { continue };
        for (number, page) in pages.into_iter().enumerate() {
            // A template page copied from the current frame is used only for
            // its PPU window base. Dynamic slot pages cannot be compared.
            if page.english.iter().flatten().any(Option::is_none)
                || !page.english.iter().flatten()
                    .any(|ch| ch.is_some_and(|c| c.is_ascii_alphabetic()))
            {
                continue;
            }
            let mut candidate = current.clone();
            candidate.string = index;
            candidate.page = number;
            candidate.key = page_key(&string_key(3, u32::from(index)), number);
            candidate.lines = page.lines;
            candidate.english = page.english;
            candidate.portrait = page.portrait;
            candidate.state = 1;
            candidate.stopped = false;
            candidate.line = 1;
            candidate.cell = 0;
            candidate.progress = 1.0;
            found.push(candidate);
        }
    }
    found
}

/// The page whose bytes hold the offset `at` (the engine counts it in a byte: a string longer than 256 bytes has
/// several candidates, and the one at or after `hint` is taken).
fn page_of(pages: &[Page], at: usize, hint: Option<usize>) -> Option<usize> {
    let ends = pages.iter().skip(1).map(|page| page.start).chain([usize::MAX]);
    let holds: Vec<usize> = pages
        .iter()
        .zip(ends)
        .enumerate()
        .filter(|(_, (page, end))| (page.start..*end).take(256).any(|offset| offset & 0xFF == at))
        .map(|(index, _)| index)
        .take(8)
        .collect();
    holds.iter().copied().find(|&index| Some(index) >= hint).or_else(|| holds.first().copied())
}

/// The page on the screen while no message is up (`message_state` $00) and the offset `at` is the first byte of a
/// page after the first, one with a cell, of a string of `bytes` bytes: the page before it, whole. A bank-4 story
/// transition stops the message so after a page and starts it again at the next one (`str3:0x4C`, "34 days later…",
/// then the ship's flight for about 450 frames before `/p2`); meanwhile the window keeps the page. A message that
/// closed leaves the offset past its string's end, and a message up in state 0 is between two strings or clearing its
/// window: neither is such a page. Only in a string of 256 bytes at most: the offset is a byte, and in a longer string
/// one offset can be the first byte of two pages. Whether the page still shows is the screen's to say (the translation
/// names it only where its English shows).
fn stopped_page(pages: &[Page], bytes: usize, at: usize) -> Option<usize> {
    if bytes > 256 {
        return None;
    }
    let next = pages.iter().position(|page| page.start == at).filter(|&next| next > 0)?;
    (pages[next].lines.iter().sum::<usize>() > 0).then(|| next - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::rom_with;

    /// A work RAM with a message up in `status`, of table 3 string 0, at `offset`, line and cell.
    fn wram(status: u8, offset: u8, line: u8, cell: u8) -> Vec<u8> {
        let mut wram = vec![0_u8; 0x20000];
        let at = |address: u32| (address - 0x7E_0000) as usize;
        wram[at(d::MESSAGE_STATE)] = status;
        wram[at(d::MESSAGE_TABLE)] = 3;
        wram[at(d::COUNTER_071D)] = offset;
        wram[at(d::MESSAGE_LINE)] = line;
        wram[at(d::COUNTER_071F)] = cell;
        wram[at(d::MESSAGE_TEXT_BASE)..][..2].copy_from_slice(&0x05C8_u16.to_le_bytes());
        wram
    }

    /// A ROM whose table 3 is at 07:9000 and whose string 0 is "ab" / "c" FE FF "dddd" FF FF at 07:9100.
    fn rom() -> Vec<u8> {
        rom_with(&[
            (d::STRING_TABLE_POINTERS + 9, &[0x00, 0x90, 0x07]),
            (0x07_9000, &[0x00, 0x91]),
            (0x07_9100, &[0x05, 0x07, 0xFF, 0x09, 0xFE, 0xFF, 0x0A, 0x0A, 0x0A, 0x0A, 0xFF, 0xFF]),
        ])
    }

    #[test]
    fn story_nearby_candidates_preserve_rom_text_and_page_numbers() {
        let rom = rom();
        let current = on_screen(&rom, &wram(0x81, 3, 1, 0), None).unwrap();
        let pages = story_pages_near(&rom, &current, 0);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].key, "str3:0x00");
        assert_eq!(pages[1].key, "str3:0x00/p2");
        assert_eq!(pages[0].english[0], [Some('a'), Some('b')]);
        assert_eq!(pages[1].english[0], [Some('d'); 4]);
        assert_eq!(pages[1].base, current.base);
    }

    #[test]
    fn the_page_line_and_progress_follow_the_engines_bytes() {
        let rom = rom();
        // Writing the first page: "ab" done, the cursor at the start of line 1
        let shown = on_screen(&rom, &wram(0x81, 3, 1, 0), None).unwrap();
        assert_eq!((shown.key.as_str(), shown.page, shown.state), ("str3:0x00", 0, 1));
        assert_eq!(shown.lines, [2, 1]);
        assert!((shown.progress - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(shown.base_cell(), (4, 23));
        // The page waits for A: the offset is past its FE FF, and the page is whole
        let waiting = on_screen(&rom, &wram(0x82, 6, 0, 0), None).unwrap();
        assert_eq!((waiting.page, waiting.progress), (0, 1.0));
        // The second page, half written
        let second = on_screen(&rom, &wram(0x81, 8, 0, 2), None).unwrap();
        assert_eq!((second.key.as_str(), second.lines.as_slice()), ("str3:0x00/p2", &[4][..]));
        assert!((second.progress - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_name_being_written_counts_as_written() {
        // String 1 of table 3: "a" FB FF FF, at 07:9200; the slot holds no name, so it counts six cells
        let mut rom = rom();
        let more = rom_with(&[(0x07_9002, &[0x00, 0x92]), (0x07_9200, &[0x05, 0xFB, 0xFF, 0xFF])]);
        let view = RomView::new(&more).unwrap();
        for address in [0x07_9002_u32, 0x07_9200] {
            let at = view.offset(address).unwrap();
            rom[at..at + 4].copy_from_slice(&more[at..at + 4]);
        }
        let mut ram = wram(0x85, 1, 0, 1);
        ram[(d::MESSAGE_STRING - 0x7E_0000) as usize] = 1;
        let shown = on_screen(&rom, &ram, None).unwrap();
        assert_eq!((shown.key.as_str(), shown.lines.as_slice()), ("str3:0x01", &[7][..]));
        assert!((shown.progress - 1.0).abs() < 1e-6, "{}", shown.progress);
    }

    /// `rom()` with the bytes of `more` written over it.
    fn rom_and(more: &[(u32, &[u8])]) -> Vec<u8> {
        let mut rom = rom();
        let extra = rom_with(more);
        let view = RomView::new(&extra).unwrap();
        for &(address, bytes) in more {
            let at = view.offset(address).unwrap();
            rom[at..at + bytes.len()].copy_from_slice(&extra[at..at + bytes.len()]);
        }
        rom
    }

    #[test]
    fn each_page_has_the_english_of_its_cells() {
        let rom = rom();
        let first = on_screen(&rom, &wram(0x81, 3, 1, 0), None).unwrap();
        assert_eq!(first.english, [vec![Some('a'), Some('b')], vec![Some('c')]]);
        let second = on_screen(&rom, &wram(0x81, 8, 0, 2), None).unwrap();
        assert_eq!(second.english, [vec![Some('d'); 4]]);
    }

    #[test]
    fn a_space_is_a_cell_and_tiles_the_hd_font_has_no_glyph_for_are_not_compared() {
        // `a b`, then `…` ($40, no glyph in the HD font), a kana ($60), the tile $132 (FC), a fixed tile (C8), a tile
        // of the second set (A5), `!` and `,` (the English patch's comma tile, $7B)
        let rom = rom_and(&[(
            0x07_9100,
            &[0x05, 0x01, 0x07, 0x40, 0x60, 0xFC, 0xC8, 0xA5, 0x3F, 0x7B, 0xFF, 0xFF],
        )]);
        let shown = on_screen(&rom, &wram(0x83, 12, 0, 10), None).unwrap();
        assert_eq!(
            shown.english,
            [vec![
                Some('a'),
                Some(' '),
                Some('b'),
                None,
                None,
                None,
                None,
                None,
                Some('!'),
                Some(',')
            ]]
        );
        assert_eq!(shown.lines, [10], "the lines' cells as before");
    }

    #[test]
    fn the_slots_are_the_tiles_the_engine_writes_from_them() {
        // `a{name:1} {num:1}`: unit 2 in the first name slot, spelled `Go` and a tile without a letter ($71, the
        // `Saiba<71>` of object 30); the first number slot `$1610` holds a space and `12`, as the engine copies it
        let rom = rom_and(&[
            (0x07_9100, &[0x05, 0xFB, 0x01, 0xF8, 0xFF, 0xFF]),
            (d::UNIT_NAME_POINTERS + 4, &[0x00, 0x90]),
            (0x02_9000, &[3, 0xED, 0x1B, 0x71]),
        ]);
        let mut ram = wram(0x83, 6, 0, 8);
        let at = |address: u32| (address - 0x7E_0000) as usize;
        ram[at(d::MESSAGE_NAME_SLOTS)..][..2].copy_from_slice(&[0x02, 0x80]);
        ram[at(d::MESSAGE_NUMBER_SLOTS)..][..4].copy_from_slice(&[0x01, 0x81, 0x82, 0xFF]);
        let shown = on_screen(&rom, &ram, None).unwrap();
        assert_eq!(shown.names[0].as_ref().map(|name| name.english.as_str()), Some("Go<71>"));
        assert_eq!(
            shown.english,
            [vec![
                Some('a'),
                Some('G'),
                Some('o'),
                None,
                Some(' '),
                Some(' '),
                Some('1'),
                Some('2')
            ]]
        );
        // A name padded with spaces at its end (unit 3, `Go  `): as `english`, the spaces are no cells
        let padded = rom_and(&[
            (0x07_9100, &[0x05, 0xFB, 0x01, 0xF8, 0xFF, 0xFF]),
            (d::UNIT_NAME_POINTERS + 6, &[0x10, 0x90]),
            (0x02_9010, &[4, 0xED, 0x1B, 0x01, 0x01]),
        ]);
        ram[at(d::MESSAGE_NAME_SLOTS)..][..2].copy_from_slice(&[0x03, 0x80]);
        let shown = on_screen(&padded, &ram, None).unwrap();
        assert_eq!(shown.english[0][..3], [Some('a'), Some('G'), Some('o')]);
        assert_eq!(shown.english[0].len(), 7);
        // An empty name slot: the engine writes nothing there
        ram[at(d::MESSAGE_NAME_SLOTS)..][..2].copy_from_slice(&[0, 0]);
        let empty = on_screen(&rom, &ram, None).unwrap();
        assert_eq!(empty.english, [vec![Some('a'), Some(' '), Some(' '), Some('1'), Some('2')]]);
    }

    #[test]
    fn a_portrait_takes_no_cell() {
        // Table 1 (two operands after F4) at 07:9400, its string 0 `F4 05 06` "ab" / "c"
        let rom = rom_and(&[
            (d::STRING_TABLE_POINTERS + 3, &[0x00, 0x94, 0x07]),
            (0x07_9400, &[0x00, 0x95]),
            (0x07_9500, &[0xF4, 0x05, 0x06, 0x05, 0x07, 0xFF, 0x09, 0xFF, 0xFF]),
        ]);
        let mut ram = wram(0x83, 9, 1, 1);
        ram[(d::MESSAGE_TABLE - 0x7E_0000) as usize] = 1;
        let shown = on_screen(&rom, &ram, None).unwrap();
        assert!(shown.portrait);
        assert_eq!(shown.english, [vec![Some('a'), Some('b')], vec![Some('c')]]);
    }

    #[test]
    fn a_string_that_closes_by_itself_ends_at_its_fd() {
        // `ab{auto}`, then bytes that are no part of it
        let rom = rom_and(&[(0x07_9100, &[0x05, 0x07, 0xFD, 0x09, 0x09, 0xFF, 0xFF])]);
        let shown = on_screen(&rom, &wram(0x86, 3, 0, 2), None).unwrap();
        assert_eq!(shown.english, [vec![Some('a'), Some('b')]]);
    }

    #[test]
    fn the_cells_the_gravity_choice_writes_again_are_not_compared() {
        // `a` / `b` / `{num:1}` and 18 `x`: line 2 starts at $05C8 + $100 = $06C8, so the handler's three tiles at
        // $06EA are its cells 17 to 19; the slot holds `10`
        let mut text = vec![0x05, 0xFF, 0x07, 0xFF, 0xF8];
        text.extend([0x58; 18]);
        text.extend([0xFF, 0xFF]);
        let rom = rom_and(&[(0x07_9100, &text)]);
        let mut ram = wram(0x83, 27, 2, 20);
        let at = |address: u32| (address - 0x7E_0000) as usize;
        ram[at(d::MESSAGE_NUMBER_SLOTS)..][..3].copy_from_slice(&[0x81, 0x80, 0xFF]);
        let mut line: Vec<Option<char>> = vec![Some('1'), Some('0')];
        line.extend([Some('x'); 18]);
        let plain = on_screen(&rom, &ram, None).unwrap();
        assert_eq!(plain.english[2], line, "no handler");
        ram[at(d::MESSAGE_HANDLER)] = GRAVITY_CHOICE;
        let choice = on_screen(&rom, &ram, None).unwrap();
        for cell in [0, 1, 17, 18, 19] {
            line[cell] = None;
        }
        assert_eq!(choice.english[2], line, "the slot and the handler's three tiles");
        assert_eq!(choice.english[..2], plain.english[..2]);
    }

    #[test]
    fn no_message_or_a_state_between_strings_shows_nothing() {
        let rom = rom();
        assert_eq!(on_screen(&rom, &wram(0x00, 3, 0, 0), None), None);
        assert_eq!(
            on_screen(&rom, &wram(0x80, 3, 0, 0), None),
            None,
            "state 0: the last string's bytes"
        );
        assert!(on_screen(&rom, &wram(0xC0, 0, 0, 0), None).is_some(), "written in one frame");
    }

    #[test]
    fn a_string_stopped_at_the_first_byte_of_a_page_shows_the_page_before() {
        // A story transition stops the message after a page (`message_state` $00, `str3:0x4C` "34 days later…" at
        // 07:BE26: about 450 frames with its page on the screen) and starts it again at the next page: the bytes
        // still name the string, the offset is the next page's first byte. The page on the screen is the one before,
        // whole and waiting.
        let rom = rom();
        let paused = on_screen(&rom, &wram(0x00, 6, 0, 0), None).unwrap();
        assert_eq!((paused.key.as_str(), paused.page, paused.state), ("str3:0x00", 0, 3));
        assert!(paused.stopped, "named as a stopped string's page");
        assert!(
            !on_screen(&rom, &wram(0x82, 6, 0, 0), None).unwrap().stopped,
            "a page waiting for A"
        );
        assert_eq!(paused.english, [vec![Some('a'), Some('b')], vec![Some('c')]]);
        assert_eq!((paused.progress, paused.line, paused.cell), (1.0, 1, 1));
        // Not when the offset is inside a page, past the string's end (a message that closed), or the first page's
        // first byte (no page before it)
        assert_eq!(on_screen(&rom, &wram(0x00, 8, 0, 0), None), None, "inside the second page");
        assert_eq!(on_screen(&rom, &wram(0x00, 12, 0, 0), None), None, "past the end");
        assert_eq!(
            on_screen(&rom, &wram(0x00, 0, 0, 0), None),
            None,
            "the first page's first byte"
        );
        // Not with a message up in state 0: between two pages the engine clears the window (`01:CC0A`)
        assert_eq!(on_screen(&rom, &wram(0x80, 6, 0, 0), None), None, "state 0 of a message up");
        // Nor at the start of a page with no cell, the `FE FF` just before the string's `FF FF`
        let tail = rom_and(&[(0x07_9100, &[0x05, 0x07, 0xFE, 0xFF, 0xFF, 0xFF])]);
        assert_eq!(on_screen(&tail, &wram(0x00, 4, 0, 0), None), None, "an empty last page");
    }

    #[test]
    fn a_string_longer_than_its_offset_counts_is_not_taken_as_stopped() {
        // The offset is a byte: in a string of more than 256 bytes the offset of a page's first byte can be another
        // page's too. "ab" FE FF, 254 `d`, FE FF, "e": the third page starts at byte 260, offset 4 like the second
        let mut text = vec![0x05, 0x07, 0xFE, 0xFF];
        text.extend([0x0A; 254]);
        text.extend([0xFE, 0xFF, 0x0B, 0xFF, 0xFF]);
        let rom = rom_and(&[(0x07_9100, &text)]);
        assert_eq!(on_screen(&rom, &wram(0x00, 4, 0, 0), None), None);
        // (a page waiting for A in it is read as before)
        assert!(on_screen(&rom, &wram(0x82, 6, 0, 0), None).is_some());
    }

    #[test]
    fn the_endings_page_comes_before_a_stopped_strings() {
        // In the ending no message is up and the message bytes keep what they held: a string stopped at a page's first
        // byte does not take the place of the ending's page
        let rom = rom_and(&[
            (0x00_BD87, &[0xFF, 0x00]),
            (0x00_BEC7, &[0x05, 0x06, 0xFF, 0x07, 0xFF, 0xFF, 0x08, 0x08, 0xFF, 0x00]),
            (0x00_BFB4, &[0xFF, 0x00]),
            (0x00_BFF0, &[0xFF, 0x00]),
        ]);
        let mut ram = ending_wram(0xBECA, 0, &FIRST_PAGE[..2]);
        let at = |address: u32| (address - 0x7E_0000) as usize;
        ram[at(d::MESSAGE_TABLE)] = 3;
        ram[at(d::COUNTER_071D)] = 6;
        assert_eq!(on_screen(&rom, &ram, None).map(|shown| shown.key), Some("ending:1".to_owned()));
        // (without the ending's page, the stopped string's)
        ram[at(d::GAME_MODE)] = 0x07;
        assert_eq!(
            on_screen(&rom, &ram, None).map(|shown| shown.key),
            Some("str3:0x00".to_owned())
        );
    }

    #[test]
    fn a_long_string_takes_the_page_at_or_after_the_hint() {
        let pages = [
            Page { start: 0, lines: vec![1], english: Vec::new(), portrait: false },
            Page { start: 200, lines: vec![1], english: Vec::new(), portrait: false },
            Page { start: 300, lines: vec![1], english: Vec::new(), portrait: false },
        ];
        assert_eq!(page_of(&pages, 10, None), Some(0));
        assert_eq!(page_of(&pages, 10, Some(2)), Some(2), "offset 266 is page 2's");
        assert_eq!(page_of(&pages, 210, None), Some(1));
    }

    #[test]
    fn a_block_row_framed_by_tiles_without_a_letter_is_the_word_between_them() {
        // Block 4 at 02:9000, 7 x 2 tiles: 0x02 has no letter (the dashes of `--Speed--`), 0x05 0x07 0x09 `abc`
        let block = [6, 1, 0x40, 0x00, 2, 2, 5, 7, 9, 2, 2, 2, 5, 2, 7, 1, 1, 1];
        let rom = rom_with(&[(d::BANK2_POINTER_TABLE + 8, &[0x00, 0x90]), (0x02_9000, &block)]);
        let table =
            crate::translation::Translation::parse("key\tzh\nblock:0x04\t訊息速度\\n快慢\n")
                .unwrap();
        let names = screen_names(&rom, &table);
        assert_eq!(names.get("abc").map(String::as_str), Some("訊息速度"), "{names:?}");
        assert!(
            !names.values().any(|zh| zh == "快慢"),
            "a tile without a letter inside the row: not a word"
        );
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn an_entry_pointing_at_the_string_of_an_entry_before_it_is_not_a_name_of_its_own() {
        // Units 0x0D and 0x22 point at one string `abc` (Chiaotzu's, the English patch's spelling of Nail), the
        // translation names them as in the story; units 0x23 and 0x24 are two strings spelled `dd`
        let unit = |id: u32| d::UNIT_NAME_POINTERS + 2 * id;
        let rom = rom_with(&[
            (unit(0x0D), &[0x00, 0x90]),
            (unit(0x22), &[0x00, 0x90]),
            (unit(0x23), &[0x10, 0x90]),
            (unit(0x24), &[0x20, 0x90]),
            (0x02_9000, &[3, 5, 7, 9]),
            (0x02_9010, &[2, 0x0A, 0x0A]),
            (0x02_9020, &[2, 0x0A, 0x0A]),
        ]);
        let table = crate::translation::Translation::parse(
            "key\tzh\nunit:0x0D\t餃子\nunit:0x22\t尼爾\nunit:0x23\t甲\nunit:0x24\t乙\n",
        )
        .unwrap();
        let names = screen_names(&rom, &table);
        assert_eq!(
            names.get("abc").map(String::as_str),
            Some("餃子"),
            "the first entry's: {names:?}"
        );
        assert!(!names.values().any(|zh| zh == "尼爾"), "the other entry is no name on screen");
        assert_eq!(
            names.get("dd"),
            None,
            "two strings spelled alike, translated two ways: left out"
        );
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn a_name_of_one_word_and_a_tile_without_a_letter_after_it_is_that_word() {
        // Object 30 as the English patch spells it: `Saiba<71>` (`ab<02>` here: 0x05 0x07 `ab`, 0x02 no letter); the
        // screen shows the word and the tile, and the word is the whole translation's (not the start of another name,
        // unit 4's `Saibai`, `abc` here)
        let rom = rom_with(&[
            (d::OBJECT_NAME_POINTERS + 2 * 30, &[0x00, 0x90]),
            (d::UNIT_NAME_POINTERS + 2 * 4, &[0x10, 0x90]),
            (0x02_9000, &[3, 5, 7, 2]),
            (0x02_9010, &[3, 5, 7, 9]),
        ]);
        let table = crate::translation::Translation::parse(
            "key\tzh\nobject:30\t栽培人\nunit:0x04\t球根人\n",
        )
        .unwrap();
        let names = screen_names(&rom, &table);
        assert_eq!(names.get("ab").map(String::as_str), Some("栽培人"), "{names:?}");
        assert_eq!(names.get("abc").map(String::as_str), Some("球根人"), "{names:?}");
        assert_eq!(ended_word("Saiba<71>"), Some("Saiba"));
        assert_eq!(ended_word("KI<43>USE"), None, "a word after the tile: two words");
        assert_eq!(ended_word("Ty<6C><BA><43>"), None, "more than one tile");
        assert_eq!(ended_word("<71>"), None, "no word");
        assert_eq!(ended_word("S<71>"), None, "a single letter is no name on screen");
    }

    #[test]
    fn a_name_of_two_words_a_tile_without_a_letter_sets_apart_is_those_words() {
        assert_eq!(marked_words("KI<43>USE"), Some(["KI", "USE"]));
        assert_eq!(marked_words("Ty<6C><BA><43>"), None, "more than one tile, or nothing after it");
        assert_eq!(marked_words("Saiba<71>"), None, "a tile at the end");
        assert_eq!(marked_words("<43>"), None);
        assert_eq!(marked_words("p<37>fR<AA><31>"), None, "two marks");
        assert_eq!(marked_words("KI"), None, "no mark");
    }

    #[test]
    fn the_gravity_choice_shows_the_level_it_writes_in_its_first_number() {
        // The handler of E0 2D (the gravity training's choice) runs over the page: the slot $1610 holds "90", as
        // the handler leaves it for 100x
        let rom = rom();
        let mut ram = wram(0x91, 6, 0, 0);
        let at = |address: u32| (address - 0x7E_0000) as usize;
        ram[at(d::MESSAGE_NUMBER_SLOTS)..][..4].copy_from_slice(&[0x89, 0x80, 0x24, 0xFF]);
        assert_eq!(
            on_screen(&rom, &ram, None).unwrap().numbers[0],
            "90",
            "no handler: the slot's digits"
        );
        ram[at(d::MESSAGE_HANDLER)] = GRAVITY_CHOICE;
        for (level, shown) in [(0, "10"), (7, "80"), (9, "100")] {
            ram[at(GRAVITY_LEVEL)] = level;
            assert_eq!(on_screen(&rom, &ram, None).unwrap().numbers[0], shown, "level {level}");
        }
        // Another handler leaves the slot alone
        ram[at(d::MESSAGE_HANDLER)] = 0x80 + 0x2E;
        assert_eq!(on_screen(&rom, &ram, None).unwrap().numbers[0], "90");
    }

    #[test]
    fn numbers_are_their_digits() {
        assert_eq!(number(&[0x01, 0x81, 0x82, 0x80, 0xFF, 0x85]), "120");
        assert_eq!(number(&[0xFF]), "");
    }

    /// A work RAM in the ending (game mode `$11`): `$7B` at `pointer`, `$7F` at `wait`, a page's first cell at
    /// `$0548` (column 4 of row 21) and a line `$88` from its row's start, the tile map holding the `cells`
    /// (offset, tile) as `00:BAEA` writes them.
    fn ending_wram(pointer: u16, wait: u8, cells: &[(u16, u8)]) -> Vec<u8> {
        let mut wram = vec![0_u8; 0x20000];
        let at = |address: u32| (address - 0x7E_0000) as usize;
        wram[at(d::GAME_MODE)] = 0x11;
        wram[at(ENDING_POINTER)..][..2].copy_from_slice(&pointer.to_le_bytes());
        wram[at(ENDING_WAIT)] = wait;
        wram[at(ENDING_BASE)..][..2].copy_from_slice(&0x0548_u16.to_le_bytes());
        wram[at(ENDING_LINE)..][..2].copy_from_slice(&0x0088_u16.to_le_bytes());
        for &(offset, tile) in cells {
            wram[at(ENDING_MAP) + usize::from(offset)..][..2]
                .copy_from_slice(&u16::from(tile).to_le_bytes());
        }
        wram
    }

    /// The ending's text 1, "ab" / "c" FF FF "dd" FF 00 at 00:BEC7 (its pages 00:BEC7-BECB and 00:BECD-BECF), after
    /// a text 0 of no cell at 00:BD87; texts 2 and 3 of no cell either.
    fn ending_rom() -> Vec<u8> {
        rom_with(&[
            (0x00_BD87, &[0xFF, 0x00]),
            (0x00_BEC7, &[0x05, 0x06, 0xFF, 0x07, 0xFF, 0xFF, 0x08, 0x08, 0xFF, 0x00]),
            (0x00_BFB4, &[0xFF, 0x00]),
            (0x00_BFF0, &[0xFF, 0x00]),
        ])
    }

    /// The cells of the first page: "ab" on row 21 from column 4, "c" on row 23.
    const FIRST_PAGE: [(u16, u8); 3] = [(0x0548, 0x05), (0x054A, 0x06), (0x05C8, 0x07)];

    #[test]
    fn the_ending_page_and_progress_follow_its_pointer() {
        let rom = ending_rom();
        // "ab" and the line's FF read: the next cell is the first of line 1
        let writing = ending_on_screen(&rom, &ending_wram(0xBECA, 0, &FIRST_PAGE[..2])).unwrap();
        assert_eq!((writing.key.as_str(), writing.page, writing.state), ("ending:1", 0, 1));
        assert_eq!((writing.table, writing.string), (ENDING_TABLE, 1));
        assert_eq!((writing.lines.as_slice(), writing.line, writing.cell), (&[2, 1][..], 1, 0));
        assert!((writing.progress - 2.0 / 3.0).abs() < 1e-6, "{}", writing.progress);
        assert_eq!(writing.base_cell(), (4, 21));
        // Past the page's FF FF the program waits with the page written: still the first page, whole
        let waiting = ending_on_screen(&rom, &ending_wram(0xBECD, 0x10, &FIRST_PAGE)).unwrap();
        assert_eq!((waiting.key.as_str(), waiting.state, waiting.progress), ("ending:1", 2, 1.0));
        assert_eq!((waiting.line, waiting.cell), (1, 1));
        // The second page, one cell of two written over the cleared screen
        let second = ending_on_screen(&rom, &ending_wram(0xBECE, 0, &[(0x0548, 0x08)])).unwrap();
        assert_eq!(
            (second.key.as_str(), second.page, second.lines.as_slice()),
            ("ending:1/p2", 1, &[2][..])
        );
        assert!((second.progress - 0.5).abs() < 1e-6, "{}", second.progress);
        // At the text's FF 00 the pointer stays on the FF: the last page, whole
        let end =
            ending_on_screen(&rom, &ending_wram(0xBECF, 0x12, &[(0x0548, 0x08), (0x054A, 0x08)]))
                .unwrap();
        assert_eq!((end.key.as_str(), end.state, end.progress), ("ending:1/p2", 2, 1.0));
    }

    #[test]
    fn the_ending_shows_a_page_only_while_the_tile_map_holds_its_written_cells() {
        let rom = ending_rom();
        assert!(ending_on_screen(&rom, &ending_wram(0xBECA, 0, &FIRST_PAGE[..2])).is_some());
        // The screen cleared (between two pages, after the text), or another picture in its place (the hidden
        // fight after Vegeta's scene, in mode $11 with $7B left in his text)
        assert_eq!(ending_on_screen(&rom, &ending_wram(0xBECA, 0, &[])), None, "cleared");
        assert_eq!(
            ending_on_screen(&rom, &ending_wram(0xBECA, 0, &[(0x0548, 0x05), (0x054A, 0x01)])),
            None
        );
        // The next page before its first cell: nothing of it on the screen yet
        assert_eq!(
            ending_on_screen(&rom, &ending_wram(0xBECD, 0, &FIRST_PAGE)),
            None,
            "no cell written"
        );
        // Not the ending, or a pointer in none of its texts
        let mut other_mode = ending_wram(0xBECA, 0, &FIRST_PAGE[..2]);
        for (mode, what) in
            [(0x02, "a battle"), (0x04, "the hidden fight's map"), (0x0D, "game over")]
        {
            other_mode[(d::GAME_MODE - 0x7E_0000) as usize] = mode;
            assert_eq!(ending_on_screen(&rom, &other_mode), None, "{what}");
        }
        assert_eq!(ending_on_screen(&rom, &ending_wram(0x1234, 0, &FIRST_PAGE[..2])), None);
    }

    #[test]
    fn the_second_ending_after_the_hidden_fight_is_read_in_mode_12() {
        // Text 4 of the English ROM, 00:C052: a line of blank tiles (`01`), its page all the same on the screen
        let mut rom = ending_rom();
        let more = rom_with(&[(0x00_C052, &[0x01, 0x01, 0x01, 0xFF, 0x00])]);
        let at = RomView::new(&more).unwrap().offset(0x00_C052).unwrap();
        rom[at..at + 5].copy_from_slice(&more[at..at + 5]);
        let mut ram = ending_wram(0xC054, 0, &[(0x0548, 0x01), (0x054A, 0x01)]);
        ram[(d::GAME_MODE - 0x7E_0000) as usize] = 0x12;
        let blank = ending_on_screen(&rom, &ram).unwrap();
        assert_eq!(
            (blank.key.as_str(), blank.lines.as_slice(), blank.state),
            ("ending:4", &[3][..], 1)
        );
        assert!((blank.progress - 2.0 / 3.0).abs() < 1e-6, "{}", blank.progress);
    }

    #[test]
    fn the_screen_without_a_message_is_the_endings_page() {
        let rom = ending_rom();
        let ram = ending_wram(0xBECA, 0, &FIRST_PAGE[..2]);
        assert_eq!(on_screen(&rom, &ram, None).map(|shown| shown.key), Some("ending:1".to_owned()));
    }

    /// A ROM whose intro tile map is blank but for "LLLL" at the start of row 0 (line 0) and "LL" at the start of row
    /// 32 (line 16): a root node of two leaves, the blank tile (bit 0) and the title set's `L` (`$EF`, bit 1).
    fn intro_rom() -> Vec<u8> {
        let mut stream = vec![0_u8; INTRO_MAP_CELLS / 8];
        for cell in [0, 1, 2, 3, INTRO_HALF, INTRO_HALF + 1] {
            stream[cell / 8] |= 1 << (cell % 8);
        }
        let root = INTRO_TREE + 4 * u32::from(INTRO_ROOT);
        rom_with(&[(root, &[0x00, 0x80, 0xEF, 0x80]), (INTRO_STREAM, &stream)])
    }

    /// A work RAM in the intro: game mode `$87`, `$7C` 0, `$73` at `left`, BG1's scroll `bg1` and BG2's `bg2`, and
    /// rows 32 to 63 of `intro_rom`'s map at `$7E:9800`.
    fn intro_wram(left: u16, bg1: u16, bg2: u16) -> Vec<u8> {
        let mut wram = vec![0_u8; 0x20000];
        let at = |address: u32| (address - 0x7E_0000) as usize;
        wram[at(d::GAME_MODE)] = TITLE_MODE;
        wram[at(INTRO_LEFT)..][..2].copy_from_slice(&left.to_le_bytes());
        wram[at(INTRO_BG1_SCROLL)..][..2].copy_from_slice(&bg1.to_le_bytes());
        wram[at(INTRO_BG2_SCROLL)..][..2].copy_from_slice(&bg2.to_le_bytes());
        wram[at(INTRO_KEPT)] = 0xEF;
        wram[at(INTRO_KEPT) + 2] = 0xEF;
        wram
    }

    #[test]
    fn the_intro_is_its_lines_and_where_its_scroll_puts_them() {
        let rom = intro_rom();
        // `$39` = `$112` (frame 2100 of a power-on): BG1's line 0 on the frame's row 512 - `$112` - 1 = 237
        let screen = intro_on_screen(&rom, &intro_wram(0x05F6, 0x0112, 0x0100)).unwrap();
        assert_eq!(
            (screen.key.as_str(), screen.table, screen.state, screen.progress),
            ("intro", INTRO_TABLE, 2, 1.0)
        );
        assert_eq!(screen.lines.len(), INTRO_LINES);
        assert_eq!((screen.lines[0], screen.lines[1], screen.lines[16]), (4, 0, 2));
        assert_eq!(intro_line_top(&screen, 0), Some(237));
        assert_eq!(intro_line_top(&screen, 1), Some(253));
        // (line 15: 240 - `$112` - 1 = -35, above the frame)
        assert_eq!(intro_line_top(&screen, 15), Some(-35));
        // Lines 16 to 31 are BG2's, by its own scroll: `$35` = `$11B` puts line 16 on row 228 (frame 3100)
        let both = intro_on_screen(&rom, &intro_wram(0x03E5, 0x021A, 0x011B)).unwrap();
        assert_eq!((intro_line_top(&both, 0), intro_line_top(&both, 16)), (Some(-27), Some(228)));
        assert_eq!(intro_line_top(&both, 31), Some(228 + 240 - 512));
        assert_eq!(intro_line_top(&both, INTRO_LINES), None);
        assert_eq!(intro_key(31), "intro:31");
    }

    #[test]
    fn the_intro_is_read_only_while_it_runs_and_a_line_of_it_is_on_the_screen() {
        let rom = intro_rom();
        assert!(intro_on_screen(&rom, &intro_wram(0x05F6, 0x0112, 0x0100)).is_some());
        // Before the first line comes up (both scrolls at `$0100`: line 0 on row 255) and after the last has gone
        assert_eq!(intro_on_screen(&rom, &intro_wram(0x06A8, 0x0100, 0x0100)), None, "not up yet");
        assert_eq!(intro_on_screen(&rom, &intro_wram(0x0006, 0x0300, 0x030B)), None, "gone");
        // Not the intro: another game mode, the title screen (`$7C` = `$0140`), the intro's loop over (`$73` 0)
        for (address, value, what) in [
            (d::GAME_MODE, 0x07_u8, "the story's mode 7"),
            (d::GAME_MODE, 0x11, "the ending"),
            (INTRO_ATTRACT, 0x40, "the title screen"),
            (INTRO_ATTRACT + 1, 0x01, "the title screen"),
            (INTRO_LEFT, 0x00, "the loop is over"),
            (INTRO_KEPT, 0x00, "another tile map at $7E:9800"),
            (INTRO_KEPT + 2 * INTRO_HALF as u32 - 2, 0x5A, "another tile map at $7E:9800"),
        ] {
            let mut ram = intro_wram(0x05F6, 0x0112, 0x0100);
            ram[(address - 0x7E_0000) as usize] = value;
            if address == INTRO_LEFT {
                ram[(address - 0x7E_0000) as usize + 1] = 0;
            }
            assert_eq!(intro_on_screen(&rom, &ram), None, "{what}");
        }
        // A ROM whose tables do not decode the intro's map (the Japanese ROM's intro is another stream)
        assert_eq!(intro_on_screen(&rom_with(&[]), &intro_wram(0x05F6, 0x0112, 0x0100)), None);
    }

    #[test]
    fn the_intro_stays_up_while_the_last_line_still_shows_a_row_of_letters() {
        let rom = intro_rom();
        // The last line (31, BG2's line 15) on the frame's row 239 - scroll; BG1's lines are all above the frame
        // (`$0300`). The scroll read at the end of a frame can be a row ahead of the one the frame was drawn with,
        // so the letters' last row (the frame's row 0, the line at -7) is on the screen one frame after the scroll
        // puts the line at -8.
        let last = |bg2: u16| intro_on_screen(&rom, &intro_wram(0x0100, 0x0300, bg2));
        for (bg2, top) in [(0x02F6_u16, -7), (0x02F7, -8)] {
            let screen = last(bg2).unwrap_or_else(|| panic!("the line at {top}: letters on row 0"));
            assert_eq!(intro_line_top(&screen, 31), Some(top));
        }
        assert_eq!(last(0x02F8), None, "the line at -9: its letters are above the frame");
    }

    #[test]
    fn the_screen_without_a_message_or_the_ending_is_the_intro() {
        let rom = intro_rom();
        let ram = intro_wram(0x05F6, 0x0112, 0x0100);
        assert_eq!(on_screen(&rom, &ram, None).map(|shown| shown.key), Some(INTRO_KEY.to_owned()));
    }

    #[test]
    fn the_language_pack_has_a_row_for_each_line_of_the_intro() {
        let table = crate::translation::Translation::parse(include_str!(
            "../../../translations/zh-tw/zh-tw.tsv"
        ))
        .unwrap();
        for line in 0..INTRO_LINES {
            assert!(
                table.text(&intro_key(line)).is_some_and(|zh| !zh.trim().is_empty()),
                "{}",
                intro_key(line)
            );
        }
        assert_eq!(table.text(&intro_key(INTRO_LINES)), None);
    }

    #[test]
    fn the_language_pack_has_a_row_for_each_english_page_of_the_ending() {
        // ending:0, the captions after Frieza, five pages in the English ROM; ending:1, Vegeta's scene, six; after the
        // hidden fight ending:2 one and ending:3 two; ending:4 is a line of blank tiles, nothing to draw over
        let table = crate::translation::Translation::parse(include_str!(
            "../../../translations/zh-tw/zh-tw.tsv"
        ))
        .unwrap();
        for (text, pages) in [(0, 5), (1, 6), (2, 1), (3, 2), (4, 0)] {
            let key = format!("ending:{text}");
            for page in 0..pages {
                let row = table.text(&page_key(&key, page));
                assert!(row.is_some_and(|zh| !zh.trim().is_empty()), "{}", page_key(&key, page));
            }
            assert_eq!(table.text(&page_key(&key, pages)), None, "{key} has {pages} pages");
        }
    }
}
