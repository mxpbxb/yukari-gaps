//! Yukari Gaps Beta for Horizon: gaps to travel between, inspired by Waystones. MIT licensed.
//!
//! A gap is one block; its block entity draws the eye gap, leaning and turned on its upright axis
//! to face whoever looks at it. Place one and name it, and lock it if only you should use it.
//! Right-click any gap to see every other gap you may use, and click one to go there for some
//! experience. Sneak and right-click your own to rename it or change its lock.

mod store;
mod ui;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use horizon_client::mod_api::{
    BlockEntityEvent, BlockEntityRenderBoundingBoxEvent, BlockEntitySubmitEvent, ClientTickPostEvent,
    Compound, DestroyBlockEvent, Flow, Hooks, Host, NativeServerPlayer, NetFromClientEvent,
    NetFromServerEvent, PlaceBlockEvent, RightClickBlockEvent, ScreenCharTypedPreEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, Text,
};

use store::{Entry, Gap, Key, Naming, Store, clean};
use ui::{MOD_ID, UI, Ui};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}

struct Mod;

/// The gap: its block, item and block entity type all have this id.
const GAP: &str = "yukari_gap:yukari_gap";
/// The picture of the gap, from the content mod's `textures/entity/gap.png`.
const GAP_SPRITE: &str = "yukari_gap:entity/gap";
/// How big the picture is, in blocks.
const GAP_SIZE: f32 = 2.0;
/// How far above the block the picture's bottom edge is, before it is tilted.
const GAP_LIFT: f32 = 1.0;
/// How far the picture leans in its own plane, in degrees.
const GAP_TILT: f32 = 45.0;

// ---- The world's gaps ----

/// The open world's store, and the file it came from: another world loads its own.
static STORE: LazyLock<Mutex<Option<(PathBuf, Store)>>> = LazyLock::new(Default::default);

fn store_path(host: &(impl Host + ?Sized)) -> Option<PathBuf> {
    Some(host.world_dir()?.join("yukari_gap").join("gaps.tsv"))
}

/// Runs `f` on the open world's store, loading it first if this world's isn't loaded; saves it
/// when `f` says it changed it.
fn with_store<R>(host: &(impl Host + ?Sized), f: impl FnOnce(&mut Store) -> (R, bool)) -> Option<R> {
    let path = store_path(host)?;
    let mut guard = STORE.lock().ok()?;
    if guard.as_ref().is_none_or(|(p, _)| *p != path) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        *guard = Some((path.clone(), Store::decode(&text)));
    }
    let (_, store) = guard.as_mut()?;
    let (result, changed) = f(store);
    if changed {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, store.encode());
    }
    Some(result)
}

// ---- Names ----

const FIRST: [&str; 24] = [
    "Ael", "Bran", "Cor", "Dun", "Eld", "Fen", "Gal", "Har", "Ith", "Kel", "Lor", "Mor", "Nor", "Orm",
    "Quel", "Ros", "Sil", "Thal", "Ul", "Vor", "Wen", "Yr", "Ash", "Brin",
];
const LAST: [&str; 16] = [
    "ford", "wick", "mere", "hold", "vale", "stead", "moor", "crest", "fell", "haven", "rest", "wyn",
    "dale", "gate", "hollow", "watch",
];

/// A name for a gap at `pos` until its owner names it: where to start among the names comes from
/// `pos`, and the first one no gap has from there is taken.
fn new_name(pos: [i32; 3], store: &Store) -> String {
    let hash = (i64::from(pos[0]).wrapping_mul(73_856_093)
        ^ i64::from(pos[1]).wrapping_mul(19_349_663)
        ^ i64::from(pos[2]).wrapping_mul(83_492_791))
    .unsigned_abs() as usize;
    let all = FIRST.len() * LAST.len();
    let name_at = |n: usize| format!("{}{}", FIRST[n % FIRST.len()], LAST[(n / FIRST.len()) % LAST.len()]);
    let free = |name: &str| !store.gaps.iter().any(|g| g.name.eq_ignore_ascii_case(name));
    (0..all)
        .map(|i| name_at(hash.wrapping_add(i) % all))
        .find(|n| free(n))
        .unwrap_or_else(|| store.unique_name(&name_at(hash % all), None))
}

// ---- The two halves ----

fn above(pos: [i32; 3]) -> [i32; 3] {
    [pos[0], pos[1] + 1, pos[2]]
}

fn below(pos: [i32; 3]) -> [i32; 3] {
    [pos[0], pos[1] - 1, pos[2]]
}

fn is_air(block: &str) -> bool {
    matches!(block, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
}

/// Where the gap whose half is at `pos` is kept: its lower half.
fn lower_half(host: &dyn Host, pos: [i32; 3]) -> [i32; 3] {
    if host.block_property(pos, "half").as_deref() == Some("upper") { below(pos) } else { pos }
}

/// Breaking one half breaks the other: a guard, so that break doesn't come back here.
static BREAKING: Mutex<bool> = Mutex::new(false);

// ---- Facing ----

/// The way a gap faces when a player looking along `yaw` places it: towards them.
fn facing_towards(yaw: f32) -> &'static str {
    let yaw = yaw.rem_euclid(360.0);
    // Minecraft's yaw: 0 looks south, 90 west, 180 north, 270 east.
    match ((yaw + 45.0) / 90.0) as i32 % 4 {
        0 => "north",
        1 => "east",
        2 => "south",
        _ => "west",
    }
}

fn step(facing: &str) -> [i32; 3] {
    match facing {
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        _ => [0, 0, -1],
    }
}

/// The yaw that looks along `facing`.
fn yaw_of(facing: &str) -> f32 {
    match facing {
        "south" => 0.0,
        "west" => 90.0,
        "north" => 180.0,
        _ => 270.0,
    }
}

/// The turn about the upright axis that points a quad's front (+z) along (`dx`, `dz`).
fn facing_yaw(dx: f64, dz: f64) -> f32 {
    dx.atan2(dz).to_degrees() as f32
}

// ---- Experience ----

/// What a trip to `to` costs in levels: one for each full 500 blocks, at most three, and three
/// to another dimension. Free in creative.
fn cost(player: &NativeServerPlayer, to: &Gap) -> i32 {
    if player.game_mode == 1 {
        return 0;
    }
    if player.dimension != to.dimension {
        return 3;
    }
    let d = |i: usize| player.position[i] - (f64::from(to.pos[i]) + 0.5);
    let distance = (d(0) * d(0) + d(2) * d(2)).sqrt();
    ((distance / 500.0) as i32).min(3)
}

/// `Player.getXpNeededForNextLevel`.
fn xp_for_level(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level * 2
    }
}

/// The experience points a player at `level` and `progress` has in all.
fn total_xp(level: i32, progress: f32) -> i32 {
    (0..level).map(xp_for_level).sum::<i32>() + (progress * xp_for_level(level) as f32) as i32
}

// ---- The server's side ----

/// A right-click reaches the game once per hand: one counts only after a pause.
static LAST_CLICK: LazyLock<Mutex<HashMap<u128, Instant>>> = LazyLock::new(Default::default);
const CLICK_GAP: Duration = Duration::from_millis(250);

fn fresh_click(player: u128) -> bool {
    let Ok(mut clicks) = LAST_CLICK.lock() else { return true };
    let now = Instant::now();
    let fresh = clicks.get(&player).is_none_or(|at| now.duration_since(*at) >= CLICK_GAP);
    clicks.insert(player, now);
    fresh
}

/// The gap each player last opened the travel screen at: a trip has to leave from it.
static OPENED_AT: LazyLock<Mutex<HashMap<u128, Key>>> = LazyLock::new(Default::default);
/// How near the gap a player has to stay for the trip to go.
const REACH: f64 = 8.0;

fn player_named(host: &dyn Host, uuid: u128) -> Option<NativeServerPlayer> {
    host.server_players().into_iter().find(|p| p.uuid == uuid)
}

fn near(player: &NativeServerPlayer, key: &Key) -> bool {
    let (dimension, pos) = key;
    let d = |i: usize| player.position[i] - (f64::from(pos[i]) + 0.5);
    player.dimension == *dimension && d(0) * d(0) + d(1) * d(1) + d(2) * d(2) <= REACH * REACH
}

/// Asks the owner of the gap at `key` to name it: the client opens the naming screen.
fn ask_name(host: &mut dyn Host, player: u128, gap: &Gap) {
    let naming = Naming { key: gap.key(), name: gap.name.clone(), locked: gap.locked };
    let _ = host.net_send_to_player(player, store::NAME, &store::encode_naming(&naming));
}

/// Sends `player` the travel screen for the gap at `origin`.
fn show_list(host: &mut dyn Host, player: &NativeServerPlayer, origin: &Key) {
    let entries = with_store(host, |s| {
        let list = s
            .destinations(player.uuid, Some(origin))
            .into_iter()
            .map(|g| Entry { key: g.key(), name: g.name.clone(), cost: cost(player, g), locked: g.locked })
            .collect::<Vec<_>>();
        (list, false)
    })
    .unwrap_or_default();
    if let Ok(mut opened) = OPENED_AT.lock() {
        opened.insert(player.uuid, origin.clone());
    }
    let _ = host.net_send_to_player(player.uuid, store::LIST, &store::encode_list(&entries));
}

/// Takes `uuid` to the gap at `to`, if they may go there from where they are.
fn travel(host: &mut dyn Host, uuid: u128, to: &Key) {
    let Some(player) = player_named(host, uuid) else { return };
    let origin = OPENED_AT.lock().ok().and_then(|o| o.get(&uuid).cloned());
    if !origin.is_some_and(|o| near(&player, &o)) {
        return;
    }
    let Some(gap) = with_store(host, |s| (s.find(to).cloned(), false)).flatten() else {
        host.action_bar(uuid, Text::literal("That gap is gone"));
        return;
    };
    if !gap.usable_by(uuid) {
        host.action_bar(uuid, Text::literal(format!("{} is locked", gap.name)));
        return;
    }
    let levels = cost(&player, &gap);
    if player.xp_level < levels {
        host.action_bar(uuid, Text::literal(format!("You need {levels} levels to go to {}", gap.name)));
        return;
    }
    // In front of the gap, looking away from it.
    let [dx, _, dz] = step(&gap.facing);
    let at = [f64::from(gap.pos[0] + dx) + 0.5, f64::from(gap.pos[1]), f64::from(gap.pos[2] + dz) + 0.5];
    let from = player.position;
    if !host.teleport_player(uuid, &gap.dimension, at, yaw_of(&gap.facing), 0.0) {
        host.action_bar(uuid, Text::literal(format!("Could not reach {}", gap.name)));
        return;
    }
    if let Ok(mut opened) = OPENED_AT.lock() {
        opened.remove(&uuid);
    }
    if levels > 0 {
        let level = player.xp_level - levels;
        let progress = player.xp_progress;
        host.set_player_status(uuid, None, None, Some((level, progress, total_xp(level, progress))));
    }
    host.play_local_sound("minecraft:entity.enderman.teleport", from, 1.0, 1.0, 0);
    host.play_local_sound("minecraft:entity.enderman.teleport", at, 1.0, 1.0, 0);
    host.action_bar(uuid, Text::literal(gap.name.clone()));
}

/// The owner named the gap: the name (a number added when another gap has it already) and lock.
fn rename(host: &mut dyn Host, uuid: u128, naming: Naming) {
    let Some(player) = player_named(host, uuid) else { return };
    if !near(&player, &naming.key) {
        return;
    }
    let wanted = clean(&naming.name);
    let result = with_store(host, |s| {
        let Some(old) = s.find(&naming.key).map(|g| g.name.clone()) else { return (Err("That gap is gone"), false) };
        if s.find(&naming.key).is_some_and(|g| g.owner != 0 && g.owner != uuid) {
            return (Err("Only its owner can name this gap"), false);
        }
        let name = if wanted.is_empty() { old } else { s.unique_name(&wanted, Some(&naming.key)) };
        let gap = s.find_mut(&naming.key).expect("found above");
        gap.name = name.clone();
        gap.locked = naming.locked;
        gap.owner = uuid;
        (Ok(name), true)
    });
    let text = match result {
        Some(Ok(name)) if naming.locked => format!("Gap named {name}, locked to you"),
        Some(Ok(name)) => format!("Gap named {name}"),
        Some(Err(why)) => why.to_owned(),
        None => return,
    };
    host.action_bar(uuid, Text::literal(text));
}

// ---- The client's side ----

/// Whether the screen open is this mod's own.
fn ours(host: &dyn Host, id: &str) -> bool {
    id == "mod" && host.mod_screen() == Some(MOD_ID)
}

fn blink() -> bool {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    ms / 500 % 2 == 0
}

impl Hooks for Mod {
    /// A gap was placed: it faces the player (that side is where travellers come out), gets a
    /// name for now, and its owner is asked to name it.
    fn place_block(&self, host: &mut dyn Host, event: &mut PlaceBlockEvent<'_>) {
        if *event.stack.item != *GAP {
            return;
        }
        let Some(player) = host.acting_player() else { return };
        let up = above(event.pos);
        if !is_air(&host.block(up)) {
            // No room for its upper half: it comes back as its item.
            host.destroy_block(event.pos, None, false);
            host.action_bar(player.uuid, Text::literal("There is no room for a gap here: it needs two blocks"));
            return;
        }
        host.set_block(&player.dimension, up, event.state, 3);
        host.set_block_property(up, "half", "upper");
        let facing = facing_towards(player.yaw).to_owned();
        let given = event.stack.custom_name().map(|n| clean(&n)).filter(|n| !n.is_empty());
        let key = (player.dimension.clone(), event.pos);
        let gap = with_store(host, |s| {
            s.gaps.retain(|g| g.key() != key);
            let name = match &given {
                Some(name) => s.unique_name(name, None),
                None => new_name(event.pos, s),
            };
            let gap = Gap { dimension: key.0.clone(), pos: key.1, facing, name, owner: player.uuid, locked: false };
            s.gaps.push(gap.clone());
            (gap, true)
        });
        if let Some(gap) = gap {
            ask_name(host, player.uuid, &gap);
        }
    }

    /// A broken gap is gone for everyone. A locked one only its owner can break.
    fn destroy_block(&self, host: &mut dyn Host, event: &mut DestroyBlockEvent) -> Flow {
        if host.block(event.pos) != GAP {
            return Flow::Continue;
        }
        let Ok(breaking) = BREAKING.lock() else { return Flow::Continue };
        if *breaking {
            return Flow::Continue;
        }
        drop(breaking);
        let Some(player) = host.acting_player() else { return Flow::Continue };
        let upper = host.block_state_property(event.state, "half") == Some("upper");
        let lower = if upper { below(event.pos) } else { event.pos };
        let key = (player.dimension.clone(), lower);
        let gone = with_store(host, |s| match s.find(&key) {
            Some(g) if !g.usable_by(player.uuid) => (Err(g.name.clone()), false),
            Some(g) => {
                let name = g.name.clone();
                s.gaps.retain(|g| g.key() != key);
                (Ok(Some(name)), true)
            }
            None => (Ok(None), false),
        });
        match gone {
            Some(Err(name)) => {
                host.action_bar(player.uuid, Text::literal(format!("{name} is locked: only its owner can break it")));
                Flow::Cancel
            }
            Some(Ok(gone)) => {
                // The other half goes too; only the lower one drops the gap, unless in creative.
                let other = if upper { lower } else { above(event.pos) };
                if host.block(other) == GAP {
                    let creative = player.game_mode == 1;
                    let tool = host.held_stack(player.uuid);
                    if let Ok(mut b) = BREAKING.lock() {
                        *b = true;
                    }
                    host.destroy_block(other, tool.as_ref(), creative);
                    if let Ok(mut b) = BREAKING.lock() {
                        *b = false;
                    }
                }
                if let Some(name) = gone {
                    host.action_bar(player.uuid, Text::literal(format!("Gap closed: {name}")));
                }
                Flow::Continue
            }
            None => Flow::Continue,
        }
    }

    /// Right-click a gap to choose where to go. Its owner, sneaking, renames it or changes its lock.
    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        if host.block(event.pos) != GAP {
            return Flow::Continue;
        }
        let Some(player) = host.acting_player() else { return Flow::Continue };
        if !fresh_click(player.uuid) {
            return Flow::Cancel;
        }
        let key = (player.dimension.clone(), lower_half(host, event.pos));
        let facing = facing_towards(player.yaw).to_owned();
        let gap = with_store(host, |s| {
            // A gap set with a command is taken in as it is found, as its finder's.
            if s.find(&key).is_none() {
                let name = new_name(key.1, s);
                let gap = Gap { dimension: key.0.clone(), pos: key.1, facing, name, owner: player.uuid, locked: false };
                s.gaps.push(gap);
                return (s.find(&key).cloned(), true);
            }
            (s.find(&key).cloned(), false)
        })
        .flatten();
        let Some(gap) = gap else { return Flow::Cancel };
        let owner = gap.owner == player.uuid || gap.owner == 0;
        if player.sneaking && owner {
            ask_name(host, player.uuid, &gap);
        } else if !gap.usable_by(player.uuid) {
            host.action_bar(player.uuid, Text::literal(format!("{} is locked by its owner", gap.name)));
        } else {
            let at = [f64::from(key.1[0]) + 0.5, f64::from(key.1[1]) + 1.0, f64::from(key.1[2]) + 0.5];
            host.play_local_sound("minecraft:block.amethyst_block.resonate", at, 1.0, 1.0, 0);
            show_list(host, &player, &key);
        }
        Flow::Cancel
    }

    fn net_from_client(&self, host: &mut dyn Host, event: &mut NetFromClientEvent<'_>) {
        match event.channel {
            store::NAMED => {
                if let Some(naming) = store::decode_naming(event.data) {
                    rename(host, event.player, naming);
                }
            }
            store::GO => {
                if let Some(to) = store::decode_go(event.data) {
                    travel(host, event.player, &to);
                }
            }
            _ => {}
        }
    }

    /// The server asks for a screen: the naming screen or the travel screen.
    fn net_from_server(&self, host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        let next = match event.channel {
            store::NAME => match store::decode_naming(event.data) {
                Some(n) => Ui::Naming { key: n.key, current: n.name, text: String::new(), locked: n.locked },
                None => return,
            },
            store::LIST => Ui::List { entries: store::decode_list(event.data), first_row: 0, dragging: false },
            _ => return,
        };
        if let Ok(mut ui) = UI.lock() {
            *ui = next;
        }
        if host.mod_screen() != Some(MOD_ID) && !host.mod_screen_open(MOD_ID) {
            if let Ok(mut ui) = UI.lock() {
                *ui = Ui::Closed;
            }
        }
    }

    /// Sends what the screens chose, closes them after, and forgets a screen Escape closed.
    fn client_tick_post(&self, host: &mut dyn Host, _: &mut ClientTickPostEvent) {
        let out = ui::OUTBOX.lock().map(|mut o| std::mem::take(&mut *o)).unwrap_or_default();
        for (channel, data) in out.messages {
            let _ = host.net_send_to_server(channel, &data);
        }
        if out.close {
            host.mod_screen_close(MOD_ID);
        }
        if host.mod_screen() != Some(MOD_ID)
            && let Ok(mut ui) = UI.lock()
        {
            *ui = Ui::Closed;
        }
    }

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        if ours(host, event.screen.id) {
            ui::draw(host, event.batch, event.screen.rect, event.screen.mouse, blink());
        }
    }

    fn screen_char_typed_pre(&self, host: &dyn Host, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
        if !ours(host, event.screen.id) {
            return Flow::Continue;
        }
        ui::typed(event.c);
        Flow::Cancel
    }

    /// Every key but Escape is the screen's: the inventory key doesn't close it while typing.
    fn screen_key_pressed_pre(&self, host: &dyn Host, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        if !ours(host, event.screen.id) || event.key == "key.keyboard.escape" {
            return Flow::Continue;
        }
        ui::key(event.key);
        Flow::Cancel
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        if !ours(host, event.screen.id) {
            return Flow::Continue;
        }
        ui::pressed(event.screen.rect, event.screen.mouse, event.button);
        Flow::Cancel
    }

    fn screen_mouse_button_released_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
        if !ours(host, event.screen.id) {
            return Flow::Continue;
        }
        ui::released();
        Flow::Cancel
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn Host, event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        if !ours(host, event.screen.id) {
            return Flow::Continue;
        }
        ui::dragged(event.screen.rect, event.screen.mouse);
        Flow::Cancel
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        if !ours(host, event.screen.id) {
            return Flow::Continue;
        }
        ui::scrolled(event.screen.rect, event.scroll[1]);
        Flow::Cancel
    }

    /// Every gap has a block entity, which draws it.
    fn block_entity(&self, _: &mut dyn Host, event: &mut BlockEntityEvent<'_>) -> Option<(String, Compound)> {
        (event.block == GAP).then(|| (GAP.to_owned(), Compound::default()))
    }

    /// The gap's picture, a block above the block, leaning `GAP_TILT` degrees, and turned on its
    /// upright axis to face the camera wherever it is.
    fn block_entity_submit(&self, host: &dyn Host, event: &mut BlockEntitySubmitEvent<'_>) {
        if event.block != GAP || host.block_state_property(event.state, "half") == Some("upper") {
            return;
        }
        let Some(camera) = host.camera() else { return };
        let dx = camera.position[0] - (f64::from(event.pos[0]) + 0.5);
        let dz = camera.position[2] - (f64::from(event.pos[2]) + 0.5);
        let draw = &mut *event.draw;
        draw.push();
        let middle = GAP_LIFT + GAP_SIZE / 2.0;
        draw.rotate_about([0.0, 1.0, 0.0], facing_yaw(dx, dz), [0.5, 0.0, 0.5]);
        // The lean, about the line from the picture's middle to the camera.
        draw.rotate_about([0.0, 0.0, 1.0], GAP_TILT, [0.5, middle, 0.5]);
        let (left, right) = (0.5 - GAP_SIZE / 2.0, 0.5 + GAP_SIZE / 2.0);
        let (bottom, top) = (GAP_LIFT, GAP_LIFT + GAP_SIZE);
        // The front, towards the camera; and the back, a hair behind it and the right way round
        // from there, should the turn ever leave the camera behind it.
        let z = 0.5;
        let front = [[left, top, z], [left, bottom, z], [right, bottom, z], [right, top, z]];
        host.draw_quad(draw, GAP_SPRITE, front, [0.0, 0.0, 16.0, 16.0], 0xFF_FF_FF);
        let z = 0.5 - 0.002;
        let back = [[right, top, z], [right, bottom, z], [left, bottom, z], [left, top, z]];
        host.draw_quad(draw, GAP_SPRITE, back, [16.0, 0.0, 0.0, 16.0], 0xFF_FF_FF);
        draw.pop();
    }

    /// The tilted picture reaches past the block's usual margin: it is drawn while any of it is in view.
    fn block_entity_render_bounding_box(&self, _: &dyn Host, event: &mut BlockEntityRenderBoundingBoxEvent<'_>) {
        if event.block != GAP {
            return;
        }
        // Its corners, turned any way about the middle, stay within half the diagonal of it.
        let reach = GAP_SIZE * std::f32::consts::FRAC_1_SQRT_2;
        let middle = GAP_LIFT + GAP_SIZE / 2.0;
        *event.bounds = [0.5 - reach, middle - reach, 0.5 - reach, 0.5 + reach, middle + reach, 0.5 + reach];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(name: &str, x: i32) -> Gap {
        Gap {
            dimension: "minecraft:overworld".into(),
            pos: [x, 64, 0],
            facing: "east".into(),
            name: name.into(),
            owner: 1,
            locked: false,
        }
    }

    #[test]
    fn made_up_names_are_never_taken_twice() {
        let mut store = Store::default();
        for i in 0..500 {
            let name = new_name([0, 64, 0], &store);
            assert!(!store.gaps.iter().any(|g| g.name.eq_ignore_ascii_case(&name)), "{name}");
            store.gaps.push(gap(&name, i));
        }
    }

    #[test]
    fn a_gap_faces_whoever_placed_it() {
        assert_eq!(facing_towards(0.0), "north"); // looking south
        assert_eq!(facing_towards(90.0), "east"); // looking west
        assert_eq!(facing_towards(180.0), "south");
        assert_eq!(facing_towards(-90.0), "west"); // looking east
    }

    #[test]
    fn the_gap_turns_to_face_the_camera() {
        assert_eq!(facing_yaw(0.0, 1.0), 0.0);
        assert_eq!(facing_yaw(1.0, 0.0), 90.0);
        assert_eq!(facing_yaw(0.0, -1.0), 180.0);
        assert_eq!(facing_yaw(-1.0, 0.0), -90.0);
    }

    #[test]
    fn experience_adds_up_as_vanilla_counts_it() {
        assert_eq!(total_xp(0, 0.0), 0);
        assert_eq!(total_xp(1, 0.0), 7);
        assert_eq!(total_xp(30, 0.0), 1395);
    }
}
