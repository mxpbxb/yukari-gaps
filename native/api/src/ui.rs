//! The client's two screens, drawn over the world as the mod's own screen: naming a gap, and the
//! gaps to travel to, as tiles in rows that wrap, scrolled when there are more than fit.

use std::sync::Mutex;

use horizon_client::mod_api::{Host, HudBatch};

use crate::store::{self, Entry, Key, NAME_MAX};

pub const MOD_ID: &str = "yukari_gap_native";
const GAP_ITEM: &str = "yukari_gap:yukari_gap";

/// What the mod's screen shows.
#[derive(Debug, Default)]
pub enum Ui {
    #[default]
    Closed,
    Naming {
        key: Key,
        /// Its name now: kept when nothing is typed.
        current: String,
        text: String,
        locked: bool,
    },
    List {
        entries: Vec<Entry>,
        /// The first row shown.
        first_row: usize,
        /// Whether the scroll bar's thumb is held.
        dragging: bool,
    },
}

pub static UI: Mutex<Ui> = Mutex::new(Ui::Closed);

/// What the input hooks (which can't send) leave for the client tick to send, and whether it
/// closes the screen then.
#[derive(Debug, Default)]
pub struct Outbox {
    pub messages: Vec<(&'static str, Vec<u8>)>,
    pub close: bool,
}

pub static OUTBOX: Mutex<Outbox> = Mutex::new(Outbox { messages: Vec::new(), close: false });

fn send(channel: &'static str, data: Vec<u8>) {
    if let Ok(mut out) = OUTBOX.lock() {
        out.messages.push((channel, data));
        out.close = true;
    }
}

// ---- Layout ----

type Rect = [f32; 4];

fn inside(r: Rect, p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

const TILE_W: f32 = 120.0;
const TILE_H: f32 = 34.0;
const GAP: f32 = 6.0;
const PAD: f32 = 10.0;
const HEADER: f32 = 22.0;
const FOOTER: f32 = 16.0;
const BAR_W: f32 = 8.0;

/// Where the travel screen's parts are for a window `w` by `h` GUI pixels and `count` gaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListLayout {
    pub panel: Rect,
    /// Where the tiles go.
    pub view: Rect,
    pub cols: usize,
    pub rows: usize,
    /// How many rows fit in the view at once.
    pub visible_rows: usize,
    /// The scroll bar's track, when there are more rows than fit.
    pub track: Option<Rect>,
}

impl ListLayout {
    pub fn new(w: f32, h: f32, count: usize) -> Self {
        // Most of the window: a chest's panel is 176 wide, this is several times that.
        let margin_x = (w * 0.06).max(8.0);
        let margin_y = (h * 0.06).max(8.0);
        let panel = [margin_x, margin_y, (w - 2.0 * margin_x).max(TILE_W + 2.0 * PAD + BAR_W), (h - 2.0 * margin_y).max(120.0)];
        let view = [
            panel[0] + PAD,
            panel[1] + HEADER,
            panel[2] - 2.0 * PAD - BAR_W - GAP,
            panel[3] - HEADER - FOOTER - PAD,
        ];
        let cols = (((view[2] + GAP) / (TILE_W + GAP)).floor() as usize).max(1);
        let rows = count.div_ceil(cols);
        let visible_rows = (((view[3] + GAP) / (TILE_H + GAP)).floor() as usize).max(1);
        let track = (rows > visible_rows).then(|| [view[0] + view[2] + GAP, view[1], BAR_W, view[3]]);
        Self { panel, view, cols, rows, visible_rows, track }
    }

    pub fn max_first_row(&self) -> usize {
        self.rows.saturating_sub(self.visible_rows)
    }

    /// The tiles' width, stretched so the rows fill the view.
    fn tile_w(&self) -> f32 {
        (self.view[2] - GAP * (self.cols as f32 - 1.0)) / self.cols as f32
    }

    /// Where tile `index` is drawn, when its row is in view.
    pub fn tile(&self, index: usize, first_row: usize) -> Option<Rect> {
        let (row, col) = (index / self.cols, index % self.cols);
        if row < first_row || row >= first_row + self.visible_rows {
            return None;
        }
        let tw = self.tile_w();
        let x = self.view[0] + col as f32 * (tw + GAP);
        let y = self.view[1] + (row - first_row) as f32 * (TILE_H + GAP);
        Some([x, y, tw, TILE_H])
    }

    /// The scroll bar's thumb.
    pub fn thumb(&self, first_row: usize) -> Option<Rect> {
        let track = self.track?;
        let h = (track[3] * self.visible_rows as f32 / self.rows as f32).max(12.0);
        let y = track[1] + (track[3] - h) * first_row as f32 / self.max_first_row().max(1) as f32;
        Some([track[0], y, track[2], h])
    }

    /// The first row for the pointer at `y` on the track.
    pub fn row_at(&self, y: f32) -> usize {
        let Some(track) = self.track else { return 0 };
        let t = ((y - track[1]) / track[3]).clamp(0.0, 1.0);
        ((t * self.max_first_row() as f32).round() as usize).min(self.max_first_row())
    }
}

/// The naming screen's parts.
#[derive(Clone, Copy, Debug)]
struct NamingLayout {
    panel: Rect,
    field: Rect,
    lock: Rect,
    done: Rect,
}

impl NamingLayout {
    fn new(w: f32, h: f32) -> Self {
        let (pw, ph) = (280.0, 120.0);
        let panel = [(w - pw) / 2.0, (h - ph) / 2.0, pw, ph];
        let (x, y) = (panel[0], panel[1]);
        Self {
            panel,
            field: [x + 12.0, y + 32.0, pw - 24.0, 20.0],
            lock: [x + 12.0, y + 64.0, (pw - 32.0) / 2.0, 20.0],
            done: [x + 20.0 + (pw - 32.0) / 2.0, y + 64.0, (pw - 32.0) / 2.0, 20.0],
        }
    }
}

// ---- Drawing ----

const PANEL: u32 = 0xF0_1A_10_24;
const EDGE: u32 = 0xFF_8A_4F_C4;
const TILE: u32 = 0xFF_2C_1E_3A;
const TILE_HOVER: u32 = 0xFF_4A_33_63;
const WHITE: u32 = 0xFF_FF_FF;
const GREY: u32 = 0xA0_A0_A0;
const GOLD: u32 = 0xFF_C8_4A;
const PURPLE: u32 = 0xD8_A8_FF;

fn frame(host: &dyn Host, batch: &mut HudBatch, r: Rect, fill: u32, edge: u32) {
    host.fill(batch, [r[0] - 1.0, r[1] - 1.0, r[2] + 2.0, r[3] + 2.0], edge);
    host.fill(batch, r, fill);
}

fn button(host: &dyn Host, batch: &mut HudBatch, r: Rect, label: &str, mouse: [f32; 2], colour: u32) {
    let fill = if inside(r, mouse) { 0xFF_5A_3F_78 } else { 0xFF_36_26_48 };
    frame(host, batch, r, fill, EDGE);
    let w = host.text_width(label);
    host.text(batch, label, [r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - 8.0) / 2.0], colour, true);
}

/// `text` cut to `width` pixels, with "..." when it doesn't fit.
fn fit(host: &dyn Host, text: &str, width: f32) -> String {
    if host.text_width(text) <= width {
        return text.to_owned();
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() && host.text_width(&format!("{cut}...")) > width {
        cut.pop();
    }
    format!("{cut}...")
}

fn dimension_label(dimension: &str) -> &str {
    match dimension {
        "minecraft:overworld" => "Overworld",
        "minecraft:the_nether" => "Nether",
        "minecraft:the_end" => "The End",
        other => other.rsplit(':').next().unwrap_or(other),
    }
}

pub fn draw(host: &dyn Host, batch: &mut HudBatch, window: Rect, mouse: [f32; 2], blink: bool) {
    let Ok(ui) = UI.lock() else { return };
    let (w, h) = (window[2], window[3]);
    match &*ui {
        Ui::Closed => {}
        Ui::Naming { text, locked, current, .. } => {
            host.fill(batch, window, 0x90_00_00_00);
            let l = NamingLayout::new(w, h);
            frame(host, batch, l.panel, PANEL, EDGE);
            host.text(batch, "Name this gap", [l.panel[0] + 12.0, l.panel[1] + 12.0], PURPLE, true);
            frame(host, batch, l.field, 0xFF_00_00_00, 0xFF_A0_A0_A0);
            let cursor = if blink { "_" } else { "" };
            let at = [l.field[0] + 5.0, l.field[1] + 6.0];
            if text.is_empty() {
                // What it is called if nothing is typed.
                host.text(batch, &fit(host, current, l.field[2] - 14.0), at, 0x60_60_60, false);
                host.text(batch, cursor, at, WHITE, false);
            } else {
                let shown = fit(host, text, l.field[2] - 14.0);
                host.text(batch, &format!("{shown}{cursor}"), at, WHITE, false);
            }
            let (label, colour) = if *locked { ("Locked", GOLD) } else { ("Lock", WHITE) };
            button(host, batch, l.lock, label, mouse, colour);
            button(host, batch, l.done, "Done", mouse, WHITE);
            let hint = if *locked { "Only you can use it" } else { "Everyone can use it" };
            host.text(batch, hint, [l.panel[0] + 12.0, l.panel[1] + 96.0], GREY, false);
        }
        Ui::List { entries, first_row, .. } => {
            host.fill(batch, window, 0x90_00_00_00);
            let l = ListLayout::new(w, h, entries.len());
            frame(host, batch, l.panel, PANEL, EDGE);
            host.text(batch, "Yukari Gap", [l.panel[0] + PAD, l.panel[1] + 8.0], PURPLE, true);
            let count = format!("{} gaps", entries.len());
            let cw = host.text_width(&count);
            host.text(batch, &count, [l.panel[0] + l.panel[2] - PAD - cw, l.panel[1] + 8.0], GREY, false);
            let mut hovered = None;
            for (i, e) in entries.iter().enumerate() {
                let Some(r) = l.tile(i, *first_row) else { continue };
                let over = inside(r, mouse);
                if over {
                    hovered = Some(e);
                }
                frame(host, batch, r, if over { TILE_HOVER } else { TILE }, if e.locked { 0xFF_C8_96_2A } else { 0xFF_5A_3F_78 });
                host.item(batch, GAP_ITEM, [r[0] + 4.0, r[1] + (r[3] - 16.0) / 2.0], 16.0);
                let text_x = r[0] + 24.0;
                let name = fit(host, &e.name, r[2] - 28.0);
                host.text(batch, &name, [text_x, r[1] + 6.0], WHITE, true);
                let price = match e.cost {
                    0 => "Free".to_owned(),
                    1 => "1 level".to_owned(),
                    n => format!("{n} levels"),
                };
                let line = if e.locked { format!("{price} - Locked") } else { price };
                host.text(batch, &fit(host, &line, r[2] - 28.0), [text_x, r[1] + 19.0], if e.locked { GOLD } else { GREY }, false);
            }
            if let (Some(track), Some(thumb)) = (l.track, l.thumb(*first_row)) {
                host.fill(batch, track, 0xFF_10_0A_16);
                host.fill(batch, thumb, if inside(thumb, mouse) { 0xFF_C8_A0_FF } else { 0xFF_8A_4F_C4 });
            }
            let footer = match hovered {
                Some(e) => {
                    let (dim, [x, y, z]) = &e.key;
                    format!("{}  -  {x}, {y}, {z}  ({})", e.name, dimension_label(dim))
                }
                None if entries.is_empty() => "No other gaps yet".to_owned(),
                None => "Click a gap to go there".to_owned(),
            };
            let fy = l.panel[1] + l.panel[3] - FOOTER + 2.0;
            host.text(batch, &fit(host, &footer, l.panel[2] - 2.0 * PAD), [l.panel[0] + PAD, fy], GREY, false);
        }
    }
}

// ---- Input ----

/// A typed character, for the name.
pub fn typed(c: char) {
    if let Ok(mut ui) = UI.lock()
        && let Ui::Naming { text, .. } = &mut *ui
        && !c.is_control()
        && text.chars().count() < NAME_MAX
    {
        text.push(c);
    }
}

/// A key: whether it is ours (Escape goes on and closes the screen, as on any).
pub fn key(name: &str) -> bool {
    let Ok(mut ui) = UI.lock() else { return false };
    if name == "key.keyboard.escape" {
        return false;
    }
    match &mut *ui {
        Ui::Naming { key, text, locked, .. } => match name {
            "key.keyboard.backspace" => {
                text.pop();
            }
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
                let naming = store::Naming { key: key.clone(), name: text.clone(), locked: *locked };
                send(store::NAMED, store::encode_naming(&naming));
            }
            _ => {}
        },
        Ui::List { entries, first_row, .. } => {
            let max = ListLayout::new(1.0e4, 1.0e4, entries.len()).max_first_row();
            match name {
                "key.keyboard.up" | "key.keyboard.page.up" => *first_row = first_row.saturating_sub(1),
                "key.keyboard.down" | "key.keyboard.page.down" => *first_row = (*first_row + 1).min(max),
                _ => {}
            }
        }
        Ui::Closed => return false,
    }
    true
}

pub fn pressed(window: Rect, mouse: [f32; 2], button_: i32) {
    if button_ != 0 {
        return;
    }
    let Ok(mut ui) = UI.lock() else { return };
    match &mut *ui {
        Ui::Naming { key, text, locked, .. } => {
            let l = NamingLayout::new(window[2], window[3]);
            if inside(l.lock, mouse) {
                *locked = !*locked;
            } else if inside(l.done, mouse) {
                let naming = store::Naming { key: key.clone(), name: text.clone(), locked: *locked };
                send(store::NAMED, store::encode_naming(&naming));
            }
        }
        Ui::List { entries, first_row, dragging } => {
            let l = ListLayout::new(window[2], window[3], entries.len());
            if let Some(track) = l.track
                && inside([track[0] - 2.0, track[1], track[2] + 4.0, track[3]], mouse)
            {
                *dragging = true;
                *first_row = l.row_at(mouse[1]);
                return;
            }
            let chosen = entries.iter().enumerate().find_map(|(i, e)| {
                l.tile(i, *first_row).filter(|r| inside(*r, mouse)).map(|_| e.key.clone())
            });
            if let Some(key) = chosen {
                send(store::GO, store::encode_go(&key));
            }
        }
        Ui::Closed => {}
    }
}

pub fn released() {
    if let Ok(mut ui) = UI.lock()
        && let Ui::List { dragging, .. } = &mut *ui
    {
        *dragging = false;
    }
}

pub fn dragged(window: Rect, mouse: [f32; 2]) {
    if let Ok(mut ui) = UI.lock()
        && let Ui::List { entries, first_row, dragging: true } = &mut *ui
    {
        *first_row = ListLayout::new(window[2], window[3], entries.len()).row_at(mouse[1]);
    }
}

pub fn scrolled(window: Rect, by: f64) {
    if let Ok(mut ui) = UI.lock()
        && let Ui::List { entries, first_row, .. } = &mut *ui
    {
        let max = ListLayout::new(window[2], window[3], entries.len()).max_first_row();
        if by > 0.0 {
            *first_row = first_row.saturating_sub(1);
        } else if by < 0.0 {
            *first_row = (*first_row + 1).min(max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiles_wrap_into_rows_and_scroll_when_they_dont_fit() {
        // A 1920x1080 window at GUI scale 3.
        let few = ListLayout::new(640.0, 360.0, 3);
        assert!(few.cols >= 3);
        assert_eq!(few.rows, 1);
        assert!(few.track.is_none());
        let many = ListLayout::new(640.0, 360.0, 200);
        assert!(many.rows > many.visible_rows);
        assert!(many.track.is_some());
        // The first tile of the second row starts back at the left.
        let first = many.tile(0, 0).unwrap();
        let second_row = many.tile(many.cols, 0).unwrap();
        assert_eq!(first[0], second_row[0]);
        assert!(second_row[1] > first[1]);
        // Scrolled to the end, the last tile is in view, the first isn't.
        let end = many.max_first_row();
        assert!(many.tile(199, end).is_some());
        assert!(many.tile(0, end).is_none());
        // The track's top and bottom are the first and last rows.
        let track = many.track.unwrap();
        assert_eq!(many.row_at(track[1]), 0);
        assert_eq!(many.row_at(track[1] + track[3]), end);
    }

    #[test]
    fn every_tile_in_view_is_inside_the_panel() {
        let l = ListLayout::new(427.0, 240.0, 60);
        for i in 0..60 {
            if let Some(r) = l.tile(i, 0) {
                assert!(r[0] >= l.view[0] && r[0] + r[2] <= l.view[0] + l.view[2] + 0.01);
                assert!(r[1] + r[3] <= l.view[1] + l.view[3] + 0.01);
            }
        }
    }
}
