//! Yukari Gaps Beta for Horizon: gaps to travel between, inspired by Waystones. MIT licensed.
//!
//! A gap is one block; its block entity draws the gap, pipef0x's Yukari Gap Demo look with its two
//! swinging bows, turned on its upright axis to face whoever looks at it. Place one and name it, and lock it if only you should use it.
//! Right-click any gap to see every other gap you may use, and click one to go there for free.
//! Others have to find a gap (right-click it once) before they can travel to it, unless its owner
//! made it global with a Yukari umbrella, which makes it a Giga Gap. A locked gap is only its owner's, and of the players
//! they share it with. Sneak and right-click your own to rename it, lock it or share it.

mod store;
mod ui;
pub mod shader;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use horizon_client::mod_api::{
    BlockEntityEvent, BlockEntityRenderBoundingBoxEvent, BlockEntitySubmitEvent, ClientTickPostEvent,
    Compound, DestroyBlockEvent, Flow, Hooks, Host, InteractEntityEvent, NativeServerPlayer, NetFromClientEvent,
    NetFromServerEvent, PlaceBlockEvent, RightClickBlockEvent, ScreenCharTypedPreEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, Tag, Text,
};

use store::{Entry, Gap, Key, Naming, Player, Store, clean};
use ui::{MOD_ID, UI, Ui};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}

struct Mod;

/// The gap: its block, item and block entity type all have this id.
const GAP: &str = "yukari_gap:yukari_gap";
/// The Yukari umbrella: its owner right-clicks a gap with it to make it global, or not, which
/// uses the umbrella up. It can't be crafted: Yukari drops it, 70 times in 100, and Yukari Great
/// Doll every time (Touhou Mobs drops it, when one of its bosses is beaten for good).
const UMBRELLA: &str = "yukari_gap:yukari_umbrella";
/// The picture of the gap, from the content mod's `textures/entity/gap.png`: pipef0x's Yukari Gap
/// Demo shader (MIT), baked into an animated sprite. The gap breathes and its fog and eyes drift.
const GAP_SPRITE: &str = "yukari_gap:entity/gap";
/// The bow tied at each tip of the gap, pipef0x's, from `textures/entity/bow.png`.
const BOW_SPRITE: &str = "yukari_gap:entity/bow";
/// The Giga Gap: what a gap becomes when it is made global, and turns back from. A gap in every
/// way, drawn bigger with its own picture (`textures/entity/giga_gap.png`, baked by
/// `bake_gap_alt.py` from a back wall and an eye). It has no item: it drops a Yukari Gap.
const GIGA: &str = "yukari_gap:giga_gap";
const GIGA_SPRITE: &str = "yukari_gap:entity/giga_gap";

/// Either gap block.
fn is_gap(block: &str) -> bool {
    block == GAP || block == GIGA
}

/// The picture a gap block is drawn with.
fn gap_sprite(block: &str) -> &'static str {
    if block == GIGA { GIGA_SPRITE } else { GAP_SPRITE }
}
/// How big the picture is, in blocks.
const GAP_SIZE: f32 = 2.25;
/// Where the gap's tips are in the picture, from its top left and from its bottom right corner,
/// as a share of its side: the demo's Bezier ends, (0.125, 0.125) and (0.875, 0.875).
const GAP_TIP: f32 = 0.125;
/// How big a bow is, as a share of its gap: the demo's 0.45 of its 2 block gap.
const BOW_SHARE: f32 = 0.225;
/// How much bigger the Giga Gap is drawn than the gap, bows and all.
const GIGA_SCALE: f32 = 1.25;

/// How big a gap block's picture is, in blocks.
fn gap_size(block: &str) -> f32 {
    if block == GIGA { GAP_SIZE * GIGA_SCALE } else { GAP_SIZE }
}

/// How far above the block a picture `size` blocks big has its bottom edge: low enough that the
/// gap's lower tip sits on the top of the block, where the second block starts.
fn gap_lift(size: f32) -> f32 {
    1.0 - size * GAP_TIP
}

/// How far a bow swings each way, in degrees (the demo's 2π/16), and how long a swing there and
/// back takes, in ticks (the demo's 4 seconds).
const BOW_SWING: f32 = 22.5;
const BOW_PERIOD: f64 = 80.0;
/// Full bright, sky and block light 15: the demo's gap and bows ignore the light.
const FULL_BRIGHT: u32 = 240 | 240 << 8;

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

/// Alice Dolls Restringed's Yukari Doll: her owner's portable gap. A click opens the travel screen
/// where she is, and she follows them through; she is no gap to travel to.
const DOLL_KIND: &str = "minecraft:chicken";
const YUKARI_DOLL: &str = "Yukari Doll";
/// The tag Alice Dolls gives her, naming her owner.
fn owner_tag(owner: u128) -> String {
    format!("alice_dolls.owner.{owner:032x}")
}
/// The Yukari Doll each player last opened the travel screen at, instead of a gap.
static PORTABLE: LazyLock<Mutex<HashMap<u128, u32>>> = LazyLock::new(Default::default);

/// Whether Yukari Doll `id` is within reach of `player`.
fn doll_near(host: &dyn Host, player: &NativeServerPlayer, id: u32) -> bool {
    let Some(e) = host.entity(id) else { return false };
    let here = host.entity_level(e).is_some_and(|l| l.dimension() == player.dimension);
    here && host.entity_position(e).is_some_and(|at| {
        (0..3).map(|i| (player.position[i] - at[i]).powi(2)).sum::<f64>() <= REACH * REACH
    })
}

/// Sends `player` the travel screen from their Yukari Doll `id`: every gap they may go to.
fn show_portable(host: &mut dyn Host, player: &NativeServerPlayer, id: u32) {
    let entries = with_store(host, |s| {
        let list = s
            .destinations(player.uuid, None)
            .into_iter()
            .map(|g| Entry { key: g.key(), name: g.name.clone(), locked: g.locked, global: g.global })
            .collect::<Vec<_>>();
        (list, false)
    })
    .unwrap_or_default();
    if let Ok(mut opened) = OPENED_AT.lock() {
        opened.remove(&player.uuid);
    }
    if let Ok(mut p) = PORTABLE.lock() {
        p.insert(player.uuid, id);
    }
    host.play_local_sound("minecraft:block.amethyst_block.resonate", player.position, 1.0, 1.2, 0);
    let _ = host.net_send_to_player(player.uuid, store::LIST, &store::encode_list(&entries));
}

fn player_named(host: &dyn Host, uuid: u128) -> Option<NativeServerPlayer> {
    host.server_players().into_iter().find(|p| p.uuid == uuid)
}

fn near(player: &NativeServerPlayer, key: &Key) -> bool {
    let (dimension, pos) = key;
    let d = |i: usize| player.position[i] - (f64::from(pos[i]) + 0.5);
    player.dimension == *dimension && d(0) * d(0) + d(1) * d(1) + d(2) * d(2) <= REACH * REACH
}

/// Who the owner of `gap` can share it with: everyone online but them, then those it is shared
/// with who are offline, by name.
fn share_list(host: &dyn Host, gap: &Gap, owner: u128) -> Vec<Player> {
    let mut players: Vec<Player> = host
        .server_players()
        .into_iter()
        .filter(|p| p.uuid != owner)
        .map(|p| Player { uuid: p.uuid, shared: gap.is_shared_with(p.uuid), online: true, name: p.name })
        .collect();
    players.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    for (uuid, name) in &gap.shared {
        if !players.iter().any(|p| p.uuid == *uuid) {
            players.push(Player { uuid: *uuid, name: name.clone(), shared: true, online: false });
        }
    }
    players
}

/// Asks the owner of the gap at `key` to name it: the client opens the naming screen.
fn ask_name(host: &mut dyn Host, player: u128, gap: &Gap) {
    let players = share_list(host, gap, player);
    let naming = Naming { key: gap.key(), name: gap.name.clone(), locked: gap.locked, global: gap.global, players };
    let _ = host.net_send_to_player(player, store::NAME, &store::encode_naming(&naming));
}

/// The owner shares their gap with a player, or stops: only while it is locked does it matter,
/// but the list is kept either way.
fn share(host: &mut dyn Host, uuid: u128, s: store::Share) {
    let Some(player) = player_named(host, uuid) else { return };
    if !near(&player, &s.key) {
        return;
    }
    // A name from the server's own list, when they are online.
    let name = player_named(host, s.player).map_or(s.name.clone(), |p| p.name);
    let result = with_store(host, |st| match st.find_mut(&s.key) {
        None => (Err("That gap is gone".to_owned()), false),
        Some(g) if g.owner != uuid => (Err("Only its owner can share this gap".to_owned()), false),
        Some(g) => {
            let changed = g.share(s.player, &name, s.on);
            (Ok(g.name.clone()), changed)
        }
    });
    match result {
        Some(Ok(gap)) if s.on => {
            host.action_bar(uuid, Text::literal(format!("{name} can use {gap}")));
            host.action_bar(s.player, Text::literal(format!("{} shared the gap {gap} with you", player.name)));
        }
        Some(Ok(gap)) => {
            host.action_bar(uuid, Text::literal(format!("{name} can no longer use {gap}")));
        }
        Some(Err(why)) => {
            host.action_bar(uuid, Text::literal(why));
        }
        None => {}
    }
}

/// Makes the gap at `lower` a Giga Gap, or a Yukari Gap again, both halves, if it isn't already.
fn set_kind(host: &mut dyn Host, dimension: &str, lower: [i32; 3], giga: bool) {
    let block = if giga { GIGA } else { GAP };
    if host.block(lower) == block || !is_gap(&host.block(lower)) {
        return;
    }
    let Some(state) = host.default_state(block) else { return };
    let up = above(lower);
    if is_gap(&host.block(up)) {
        host.set_block(dimension, up, state, 3);
        host.set_block_property(up, "half", "upper");
    }
    host.set_block(dimension, lower, state, 3);
}

/// Its owner right-clicked the gap with a Yukari umbrella: it turns global, a Giga Gap, or back.
fn toggle_global(host: &mut dyn Host, uuid: u128, key: &Key) {
    let result = with_store(host, |s| match s.find_mut(key) {
        Some(g) if g.owner == uuid || g.owner == 0 => {
            g.owner = uuid;
            g.global = !g.global;
            (Some((g.name.clone(), g.global, g.locked)), true)
        }
        _ => (None, false),
    })
    .flatten();
    if let Some((_, global, _)) = &result {
        set_kind(host, &key.0, key.1, *global);
    }
    let text = match result {
        Some((name, true, true)) => format!("{name} is a Giga Gap, but locked: unlock it to open it to everyone"),
        Some((name, true, false)) => format!("{name} is a Giga Gap: everyone can use it"),
        Some((name, false, _)) => format!("{name} is no longer a Giga Gap: players have to find it"),
        None => "Only its owner can make this gap a Giga Gap".to_owned(),
    };
    host.action_bar(uuid, Text::literal(text));
}

/// Sends `player` the travel screen for the gap at `origin`.
fn show_list(host: &mut dyn Host, player: &NativeServerPlayer, origin: &Key) {
    let entries = with_store(host, |s| {
        let list = s
            .destinations(player.uuid, Some(origin))
            .into_iter()
            .map(|g| Entry { key: g.key(), name: g.name.clone(), locked: g.locked, global: g.global })
            .collect::<Vec<_>>();
        (list, false)
    })
    .unwrap_or_default();
    if let Ok(mut opened) = OPENED_AT.lock() {
        opened.insert(player.uuid, origin.clone());
    }
    if let Ok(mut p) = PORTABLE.lock() {
        p.remove(&player.uuid);
    }
    let _ = host.net_send_to_player(player.uuid, store::LIST, &store::encode_list(&entries));
}

/// Takes `uuid` to the gap at `to`, if they may go there from where they are.
fn travel(host: &mut dyn Host, uuid: u128, to: &Key) {
    let Some(player) = player_named(host, uuid) else { return };
    let origin = OPENED_AT.lock().ok().and_then(|o| o.get(&uuid).cloned());
    let from_gap = origin.is_some_and(|o| near(&player, &o));
    // Or from their Yukari Doll, beside them still.
    let doll = if from_gap { None } else { PORTABLE.lock().ok().and_then(|p| p.get(&uuid).copied()) };
    let doll = doll.filter(|&id| doll_near(host, &player, id));
    if !from_gap && doll.is_none() {
        return;
    }
    let Some(gap) = with_store(host, |s| (s.find(to).cloned(), false)).flatten() else {
        host.action_bar(uuid, Text::literal("That gap is gone"));
        return;
    };
    if !gap.usable_by(uuid) {
        let why = if gap.locked { "is locked" } else { "hasn't been found by you yet" };
        host.action_bar(uuid, Text::literal(format!("{} {why}", gap.name)));
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
    // Yukari comes through after them, a step further out.
    if let Some(id) = doll {
        PORTABLE.lock().ok().map(|mut p| p.remove(&uuid));
        let beside = [at[0] + f64::from(dx), at[1], at[2] + f64::from(dz)];
        host.teleport_mob(&gap.dimension, id, beside);
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
        if &*event.stack.item != GAP {
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
            let gap = Gap {
                dimension: key.0.clone(),
                pos: key.1,
                facing,
                name,
                owner: player.uuid,
                locked: false,
                global: false,
                known: Vec::new(),
                shared: Vec::new(),
            };
            s.gaps.push(gap.clone());
            (gap, true)
        });
        if let Some(gap) = gap {
            ask_name(host, player.uuid, &gap);
        }
    }

    /// A broken gap is gone for everyone. A locked one only its owner can break.
    fn destroy_block(&self, host: &mut dyn Host, event: &mut DestroyBlockEvent) -> Flow {
        let kind = host.block(event.pos);
        if !is_gap(&kind) {
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
            Some(g) if !g.breakable_by(player.uuid) => (Err(g.name.clone()), false),
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
            gone => {
                // The gap drops itself, whatever it was mined with: the block asks for a pickaxe,
                // and its loot is only rolled for the right tool, so the item is made here and the
                // halves broken with what they'd drop collected, not popped (the assets mod stays
                // as servers know it). In creative nothing drops, as with any block.
                let gone = gone.and_then(Result::ok).flatten();
                let creative = player.game_mode == 1;
                // The player breaks the upper half, which drops nothing of its own; the lower one,
                // which the loot table drops the gap for, is broken here. Should the lower half
                // itself be the one mined, both are broken here.
                let here: Vec<[i32; 3]> = if upper { vec![lower] } else { vec![above(event.pos), event.pos] };
                if let Ok(mut b) = BREAKING.lock() {
                    *b = true;
                }
                for pos in here {
                    if is_gap(&host.block(pos)) {
                        let _ = host.destroy_block(pos, None, true);
                    }
                }
                if let Ok(mut b) = BREAKING.lock() {
                    *b = false;
                }
                if !creative {
                    let at = [f64::from(lower[0]) + 0.5, f64::from(lower[1]) + 0.5, f64::from(lower[2]) + 0.5];
                    // A Giga Gap drops the Yukari Gap it was made from.
                    let stack: Compound = [("id".into(), Tag::String(GAP.into())), ("count".into(), Tag::Int(1))].into();
                    let nbt: Compound = [("Item".into(), Tag::Compound(stack)), ("PickupDelay".into(), Tag::Short(10))].into();
                    host.summon(&player.dimension, "minecraft:item", at, nbt);
                }
                if let Some(name) = gone {
                    host.action_bar(player.uuid, Text::literal(format!("Gap closed: {name}")));
                }
                if upper { Flow::Continue } else { Flow::Cancel }
            }
        }
    }

    /// Right-click a gap to find it and choose where to go. Its owner, sneaking, renames it,
    /// locks it or shares it; with a Yukari umbrella, makes it global or not.
    /// Its owner clicked their Yukari Doll with an empty hand: the travel screen, from her.
    fn interact_entity(&self, host: &mut dyn Host, event: &mut InteractEntityEvent<'_>) -> Flow {
        if event.kind != DOLL_KIND || event.stack.is_some() {
            return Flow::Continue;
        }
        let Ok(id) = u32::try_from(event.entity) else { return Flow::Continue };
        let Some(e) = host.entity(id) else { return Flow::Continue };
        if host.entity_name(e).as_deref() != Some(YUKARI_DOLL) {
            return Flow::Continue;
        }
        let Some(player) = host.acting_player() else { return Flow::Continue };
        if player.sneaking || !host.entity_tags(e).is_some_and(|t| t.contains(&owner_tag(player.uuid))) {
            return Flow::Continue;
        }
        if fresh_click(player.uuid) {
            show_portable(host, &player, id);
        }
        Flow::Cancel
    }

    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        if !is_gap(&host.block(event.pos)) {
            return Flow::Continue;
        }
        let Some(player) = host.acting_player() else { return Flow::Continue };
        if !fresh_click(player.uuid) {
            return Flow::Cancel;
        }
        let key = (player.dimension.clone(), lower_half(host, event.pos));
        let facing = facing_towards(player.yaw).to_owned();
        let umbrella = host.held().is_some_and(|(item, _, _)| item == UMBRELLA);
        let found = with_store(host, |s| {
            // A gap set with a command is taken in as it is found, as its finder's.
            if s.find(&key).is_none() {
                let name = new_name(key.1, s);
                let gap = Gap {
                    dimension: key.0.clone(),
                    pos: key.1,
                    facing,
                    name,
                    owner: player.uuid,
                    locked: false,
                    global: false,
                    known: Vec::new(),
                    shared: Vec::new(),
                };
                s.gaps.push(gap);
                return (s.find(&key).cloned().map(|g| (g, false)), true);
            }
            // Touching it is finding it.
            let new = s.find_mut(&key).is_some_and(|g| g.discover(player.uuid));
            (s.find(&key).cloned().map(|g| (g, new)), new)
        })
        .flatten();
        let Some((gap, newly_found)) = found else { return Flow::Cancel };
        // A gap made global before Giga Gaps were is made one now.
        set_kind(host, &key.0, key.1, gap.global);
        let owner = gap.owner == player.uuid || gap.owner == 0;
        if umbrella && owner {
            toggle_global(host, player.uuid, &key);
            // An umbrella is used up by it, in Creative too: one use each.
            host.take_items(UMBRELLA, 1);
        } else if player.sneaking && owner {
            ask_name(host, player.uuid, &gap);
        } else if gap.locked && !gap.usable_by(player.uuid) {
            host.action_bar(player.uuid, Text::literal(format!("{} is locked by its owner", gap.name)));
        } else {
            if newly_found {
                host.action_bar(player.uuid, Text::literal(format!("Found a gap: {}", gap.name)));
            }
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
            store::SHARE => {
                if let Some(s) = store::decode_share(event.data) {
                    share(host, event.player, s);
                }
            }
            _ => {}
        }
    }

    /// The server asks for a screen: the naming screen or the travel screen.
    fn net_from_server(&self, host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        let next = match event.channel {
            store::NAME => match store::decode_naming(event.data) {
                Some(n) => Ui::Naming {
                    key: n.key,
                    current: n.name,
                    text: String::new(),
                    locked: n.locked,
                    global: n.global,
                    players: n.players,
                    sharing: false,
                    first_row: 0,
                },
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
        is_gap(event.block).then(|| (GAP.to_owned(), Compound::default()))
    }

    /// The gap's picture, above the block, turned on its upright axis to face the camera
    /// wherever it is, with a bow swinging at each tip, as pipef0x's Yukari Gap Demo draws it.
    fn block_entity_submit(&self, host: &dyn Host, event: &mut BlockEntitySubmitEvent<'_>) {
        if !is_gap(event.block) || host.block_state_property(event.state, "half") == Some("upper") {
            return;
        }
        let Some(camera) = host.camera() else { return };
        let dx = camera.position[0] - (f64::from(event.pos[0]) + 0.5);
        let dz = camera.position[2] - (f64::from(event.pos[2]) + 0.5);
        let sprite = gap_sprite(event.block);
        let size = gap_size(event.block);
        let lift = gap_lift(size);
        let draw = &mut *event.draw;
        let swing = bow_swing(draw.game_time as f64 + f64::from(draw.partial));
        let light = draw.light;
        draw.light = FULL_BRIGHT;
        draw.push();
        draw.rotate_about([0.0, 1.0, 0.0], facing_yaw(dx, dz), [0.5, 0.0, 0.5]);
        let (left, right) = (0.5 - size / 2.0, 0.5 + size / 2.0);
        let (bottom, top) = (lift, lift + size);
        // The front, towards the camera; and the back, a hair behind it and the right way round
        // from there, should the turn ever leave the camera behind it.
        let z = 0.5;
        let front = [[left, top, z], [left, bottom, z], [right, bottom, z], [right, top, z]];
        host.draw_quad(draw, sprite, front, [0.0, 0.0, 16.0, 16.0], 0xFF_FF_FF);
        let z = 0.5 - 0.002;
        let back = [[right, top, z], [right, bottom, z], [left, bottom, z], [left, top, z]];
        host.draw_quad(draw, sprite, back, [16.0, 0.0, 0.0, 16.0], 0xFF_FF_FF);
        // The bows, a little in front of the gap: the top one swings one way as the bottom one
        // swings the other.
        let inset = size * GAP_TIP;
        let bow = size * BOW_SHARE;
        draw_bow(host, draw, [left + inset, top - inset], bow, swing);
        draw_bow(host, draw, [right - inset, bottom + inset], bow, -swing);
        draw.pop();
        draw.light = light;
    }

    /// The tilted picture reaches past the block's usual margin: it is drawn while any of it is in view.
    fn block_entity_render_bounding_box(&self, _: &dyn Host, event: &mut BlockEntityRenderBoundingBoxEvent<'_>) {
        if !is_gap(event.block) {
            return;
        }
        // Its corners, turned any way about the middle, stay within half the diagonal of it; and
        // the bows, centred on the tips, reach half a bow past those.
        let size = gap_size(event.block);
        let reach = size * std::f32::consts::FRAC_1_SQRT_2 + size * BOW_SHARE * 0.5;
        let middle = gap_lift(size) + size / 2.0;
        *event.bounds = [0.5 - reach, middle - reach, 0.5 - reach, 0.5 + reach, middle + reach, 0.5 + reach];
    }
}

/// How far the top bow is turned at `ticks` (game time plus partial tick), in degrees: there and
/// back across `BOW_SWING` each way every `BOW_PERIOD` ticks, eased in and out cubically.
fn bow_swing(ticks: f64) -> f32 {
    let cycle = (ticks / BOW_PERIOD).rem_euclid(1.0);
    let ramp = if cycle < 0.5 { cycle * 2.0 } else { 2.0 - cycle * 2.0 };
    let eased = if ramp < 0.5 { 4.0 * ramp * ramp * ramp } else { 1.0 - (-2.0 * ramp + 2.0).powi(3) / 2.0 };
    (-1.0 + 2.0 * eased) as f32 * BOW_SWING
}

/// A bow `size` blocks across centred at `at` (x, y) on the gap's plane, a hair in front of it and
/// turned `degrees` about its middle; its back too, a hair behind the gap's back.
fn draw_bow(host: &dyn Host, draw: &mut horizon_client::mod_api::BlockEntityDraw, at: [f32; 2], size: f32, degrees: f32) {
    let h = size / 2.0;
    let [x, y] = at;
    draw.push();
    draw.rotate_about([0.0, 0.0, 1.0], degrees, [x, y, 0.5]);
    let z = 0.5 + 0.005;
    let front = [[x - h, y + h, z], [x - h, y - h, z], [x + h, y - h, z], [x + h, y + h, z]];
    host.draw_quad(draw, BOW_SPRITE, front, [0.0, 0.0, 16.0, 16.0], 0xFF_FF_FF);
    let z = 0.5 - 0.007;
    let back = [[x + h, y + h, z], [x + h, y - h, z], [x - h, y - h, z], [x - h, y + h, z]];
    host.draw_quad(draw, BOW_SPRITE, back, [16.0, 0.0, 0.0, 16.0], 0xFF_FF_FF);
    draw.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bows_swing_there_and_back() {
        assert_eq!(bow_swing(0.0), -BOW_SWING);
        assert!((bow_swing(BOW_PERIOD / 2.0) - BOW_SWING).abs() < 1e-4);
        assert!(bow_swing(BOW_PERIOD / 4.0).abs() < 1e-4);
        assert_eq!(bow_swing(BOW_PERIOD), bow_swing(0.0));
        assert_eq!(bow_swing(-BOW_PERIOD / 2.0), bow_swing(BOW_PERIOD / 2.0));
    }

    #[test]
    fn the_giga_gap_is_a_gap_with_its_own_picture() {
        assert!(is_gap(GAP) && is_gap(GIGA));
        assert!(!is_gap("minecraft:glass"));
        assert_eq!(gap_sprite(GAP), GAP_SPRITE);
        assert_eq!(gap_sprite(GIGA), GIGA_SPRITE);
    }

    #[test]
    fn the_lower_tip_sits_on_the_block() {
        assert!((gap_lift(GAP_SIZE) + GAP_SIZE * GAP_TIP - 1.0).abs() < 1e-6);
        // The bigger Giga Gap's lower tip sits on the block too.
        let alt = gap_size(GIGA);
        assert!((gap_lift(alt) + alt * GAP_TIP - 1.0).abs() < 1e-6);
    }

    #[test]
    fn yukaris_tag_is_the_one_alice_dolls_gives_her() {
        assert_eq!(owner_tag(0xab), "alice_dolls.owner.000000000000000000000000000000ab");
    }

    #[test]
    fn the_giga_gap_is_a_quarter_bigger() {
        assert_eq!(gap_size(GAP), GAP_SIZE);
        assert!((gap_size(GIGA) - GAP_SIZE * 1.25).abs() < 1e-6);
    }

    fn gap(name: &str, x: i32) -> Gap {
        Gap {
            dimension: "minecraft:overworld".into(),
            pos: [x, 64, 0],
            facing: "east".into(),
            name: name.into(),
            owner: 1,
            locked: false,
            global: false,
            known: Vec::new(),
            shared: Vec::new(),
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
    fn yukari_comes_through_only_by_touhou_mobs_roll() {
        // This mod summons no Yukari itself: Touhou Mobs rolls once in 2,000 on gap travel.
        let src = include_str!("lib.rs");
        let summons = src.matches(&format!("host.summon(&gap.dimension, {}", "YUKARI")).count();
        assert_eq!(summons, 0);
        assert!(!src.contains(&format!("const {}: &str = \"thmobs:yukari\"", "YUKARI")));
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
}
