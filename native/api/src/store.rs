//! The world's gaps, and the messages the server and the client send each other about them.

/// Where a gap is: its dimension and block.
pub type Key = (String, [i32; 3]);

/// A gap: where it is, which way travellers come out of it, its name, who placed it, and who
/// else may use it.
///
/// Who may go through a gap (to it, from another gap):
/// - its owner, always;
/// - locked: only the players its owner shared it with;
/// - not locked: everyone when its owner made it global with a Yukari umbrella, else only the
///   players who have found it, by right-clicking it once.
#[derive(Clone, Debug, PartialEq)]
pub struct Gap {
    pub dimension: String,
    pub pos: [i32; 3],
    pub facing: String,
    pub name: String,
    /// The player who placed it; 0 for one nobody owns (from before gaps had owners), open to all.
    pub owner: u128,
    pub locked: bool,
    /// Usable by everyone without finding it first (still not while locked).
    pub global: bool,
    /// The players who have found it.
    pub known: Vec<u128>,
    /// While locked, the players its owner lets use it, with their names as they were.
    pub shared: Vec<(u128, String)>,
}

impl Gap {
    pub fn key(&self) -> Key {
        (self.dimension.clone(), self.pos)
    }

    /// Whether `player` may go through it (see [`Gap`]).
    pub fn usable_by(&self, player: u128) -> bool {
        if self.owner == 0 {
            return !self.locked;
        }
        if self.owner == player {
            return true;
        }
        if self.locked {
            return self.is_shared_with(player);
        }
        self.global || self.known.contains(&player)
    }

    pub fn is_shared_with(&self, player: u128) -> bool {
        self.shared.iter().any(|(p, _)| *p == player)
    }

    /// Only its owner may break a locked gap.
    pub fn breakable_by(&self, player: u128) -> bool {
        !self.locked || self.owner == 0 || self.owner == player
    }

    /// `player` found it: whether that is news.
    pub fn discover(&mut self, player: u128) -> bool {
        if player == self.owner || self.known.contains(&player) {
            return false;
        }
        self.known.push(player);
        true
    }

    /// Shares it with `player` (or stops sharing): whether anything changed.
    pub fn share(&mut self, player: u128, name: &str, on: bool) -> bool {
        let had = self.is_shared_with(player);
        if on && !had && player != self.owner {
            self.shared.push((player, clean_player(name)));
            true
        } else if !on && had {
            self.shared.retain(|(p, _)| *p != player);
            true
        } else {
            false
        }
    }
}

/// Every gap of the world, kept in its folder (`yukari_gap/gaps.tsv`).
#[derive(Debug, Default, PartialEq)]
pub struct Store {
    pub gaps: Vec<Gap>,
}

/// The longest a gap's name may be, in characters.
pub const NAME_MAX: usize = 32;

/// A name fit for a line of the store and the messages: no tabs or line breaks, and not too long.
pub fn clean(name: &str) -> String {
    let name: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    name.trim().chars().take(NAME_MAX).collect::<String>().trim().to_owned()
}

/// A player's name fit for the store's lists: letters, digits and `_` (all a name can have).
fn clean_player(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(16).collect()
}

impl Store {
    pub fn find(&self, key: &Key) -> Option<&Gap> {
        self.gaps.iter().find(|g| &g.key() == key)
    }

    pub fn find_mut(&mut self, key: &Key) -> Option<&mut Gap> {
        self.gaps.iter_mut().find(|g| &g.key() == key)
    }

    /// `wanted`, or, when another gap (not `except`) has that name already, `wanted (2)`,
    /// `wanted (3)`... the first nobody has.
    pub fn unique_name(&self, wanted: &str, except: Option<&Key>) -> String {
        let taken = |name: &str| {
            self.gaps.iter().any(|g| Some(&g.key()) != except && g.name.eq_ignore_ascii_case(name))
        };
        if !taken(wanted) {
            return wanted.to_owned();
        }
        (2..).map(|n| format!("{wanted} ({n})")).find(|n| !taken(n)).unwrap_or_else(|| wanted.to_owned())
    }

    /// The gaps `player` may go to from `origin`, in name order.
    pub fn destinations(&self, player: u128, origin: Option<&Key>) -> Vec<&Gap> {
        let mut list: Vec<&Gap> =
            self.gaps.iter().filter(|g| g.usable_by(player) && Some(&g.key()) != origin).collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        list
    }

    pub fn encode(&self) -> String {
        let mut out = String::new();
        for g in &self.gaps {
            let [x, y, z] = g.pos;
            let known = g.known.iter().map(|p| format!("{p:032x}")).collect::<Vec<_>>().join(",");
            let shared = g.shared.iter().map(|(p, n)| format!("{p:032x}={n}")).collect::<Vec<_>>().join(",");
            out += &format!(
                "H\t{}\t{x}\t{y}\t{z}\t{}\t{:032x}\t{}\t{}\t{known}\t{shared}\t{}\n",
                g.dimension,
                g.facing,
                g.owner,
                u8::from(g.locked),
                u8::from(g.global),
                clean(&g.name)
            );
        }
        out
    }

    /// Reads a store. Gaps saved before finding (`G` lines) keep their owner and lock, and nobody
    /// has found them yet; before owners (`W` lines, and `A` lines for who had opened which) they
    /// are nobody's, open to everyone.
    pub fn decode(text: &str) -> Self {
        let mut store = Store::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let Some(pos) = f.get(2..5).and_then(parse_pos) else { continue };
            let base = |owner: u128, locked: bool, name: String| Gap {
                dimension: f[1].to_owned(),
                pos,
                facing: f[5].to_owned(),
                owner,
                locked,
                global: false,
                known: Vec::new(),
                shared: Vec::new(),
                name,
            };
            let owner = |s: &str| u128::from_str_radix(s, 16).unwrap_or(0);
            let gap = match f[0] {
                "H" if f.len() >= 12 => Gap {
                    global: f[8] == "1",
                    known: f[9].split(',').filter(|s| !s.is_empty()).map(owner).filter(|p| *p != 0).collect(),
                    shared: f[10]
                        .split(',')
                        .filter_map(|s| s.split_once('='))
                        .map(|(p, n)| (owner(p), n.to_owned()))
                        .filter(|(p, _)| *p != 0)
                        .collect(),
                    ..base(owner(f[6]), f[7] == "1", f[11..].join(" "))
                },
                "G" if f.len() >= 9 => base(owner(f[6]), f[7] == "1", f[8..].join(" ")),
                "W" if f.len() >= 8 => base(0, false, f[7..].join(" ")),
                _ => continue,
            };
            store.gaps.retain(|g| g.key() != gap.key());
            store.gaps.push(gap);
        }
        store
    }
}

fn parse_pos(f: &[&str]) -> Option<[i32; 3]> {
    Some([f.first()?.parse().ok()?, f.get(1)?.parse().ok()?, f.get(2)?.parse().ok()?])
}

fn key_fields(key: &Key) -> String {
    let (dimension, [x, y, z]) = key;
    format!("{dimension}\t{x}\t{y}\t{z}")
}

fn parse_key(f: &[&str]) -> Option<Key> {
    Some((f.first()?.to_string(), parse_pos(f.get(1..4)?)?))
}

// ---- Messages ----

/// Server to client: name the gap you placed (or rename it, sneaking), with whom it is shared.
pub const NAME: &str = "yukari_gap_native:name";
/// Server to client: where you can go from the gap you used.
pub const LIST: &str = "yukari_gap_native:list";
/// Client to server: the gap's name and whether it is locked.
pub const NAMED: &str = "yukari_gap_native:named";
/// Client to server: go to this gap.
pub const GO: &str = "yukari_gap_native:go";
/// Client to server: share the locked gap with a player, or stop.
pub const SHARE: &str = "yukari_gap_native:share";

/// A player in the sharing list: online now, or offline but shared with.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub uuid: u128,
    pub name: String,
    pub shared: bool,
    pub online: bool,
}

/// What the naming screen starts with: the gap, its name, lock and global, and the players it
/// can be shared with.
#[derive(Clone, Debug, PartialEq)]
pub struct Naming {
    pub key: Key,
    pub name: String,
    pub locked: bool,
    pub global: bool,
    pub players: Vec<Player>,
}

/// The most players one naming message carries.
pub const PLAYERS_MAX: usize = 200;

pub fn encode_naming(n: &Naming) -> Vec<u8> {
    let mut out = format!("{}\t{}\t{}\t{}", key_fields(&n.key), u8::from(n.locked), u8::from(n.global), clean(&n.name));
    for p in n.players.iter().take(PLAYERS_MAX) {
        out += &format!("\nP\t{:032x}\t{}\t{}\t{}", p.uuid, u8::from(p.shared), u8::from(p.online), clean_player(&p.name));
    }
    out.into_bytes()
}

pub fn decode_naming(data: &[u8]) -> Option<Naming> {
    let text = std::str::from_utf8(data).ok()?;
    let mut lines = text.lines();
    let f: Vec<&str> = lines.next()?.split('\t').collect();
    let players = lines
        .filter_map(|line| {
            let p: Vec<&str> = line.split('\t').collect();
            (p.first() == Some(&"P") && p.len() >= 5).then(|| Player {
                uuid: u128::from_str_radix(p[1], 16).unwrap_or(0),
                shared: p[2] == "1",
                online: p[3] == "1",
                name: p[4].to_owned(),
            })
        })
        .collect();
    Some(Naming {
        key: parse_key(&f)?,
        locked: *f.get(4)? == "1",
        global: *f.get(5)? == "1",
        name: f.get(6..)?.join(" "),
        players,
    })
}

/// The owner shares the gap at `key` with a player, or stops.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub key: Key,
    pub player: u128,
    pub name: String,
    pub on: bool,
}

pub fn encode_share(s: &Share) -> Vec<u8> {
    format!("{}\t{:032x}\t{}\t{}", key_fields(&s.key), s.player, u8::from(s.on), clean_player(&s.name)).into_bytes()
}

pub fn decode_share(data: &[u8]) -> Option<Share> {
    let text = std::str::from_utf8(data).ok()?;
    let f: Vec<&str> = text.split('\t').collect();
    Some(Share {
        key: parse_key(&f)?,
        player: u128::from_str_radix(f.get(4)?, 16).ok()?,
        on: *f.get(5)? == "1",
        name: f.get(6).map_or_else(String::new, |s| (*s).to_owned()),
    })
}

/// One gap in the travel screen: where, its name, and how it is open (locked: only its owner and
/// those it is shared with see it; global: everyone does).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: Key,
    pub name: String,
    pub locked: bool,
    pub global: bool,
}

/// The most gaps one list carries, well within a message's 30 000 bytes.
pub const LIST_MAX: usize = 400;

pub fn encode_list(entries: &[Entry]) -> Vec<u8> {
    let mut out = String::new();
    for e in entries.iter().take(LIST_MAX) {
        out += &format!("{}\t{}\t{}\t{}\n", key_fields(&e.key), u8::from(e.locked), u8::from(e.global), clean(&e.name));
    }
    out.into_bytes()
}

pub fn decode_list(data: &[u8]) -> Vec<Entry> {
    let Ok(text) = std::str::from_utf8(data) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            Some(Entry {
                key: parse_key(&f)?,
                locked: *f.get(4)? == "1",
                global: *f.get(5)? == "1",
                name: f.get(6..)?.join(" "),
            })
        })
        .collect()
}

pub fn encode_go(key: &Key) -> Vec<u8> {
    key_fields(key).into_bytes()
}

pub fn decode_go(data: &[u8]) -> Option<Key> {
    let text = std::str::from_utf8(data).ok()?;
    parse_key(&text.split('\t').collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(name: &str, x: i32, owner: u128, locked: bool) -> Gap {
        Gap {
            dimension: "minecraft:overworld".into(),
            pos: [x, 64, 0],
            facing: "east".into(),
            name: name.into(),
            owner,
            locked,
            global: false,
            known: Vec::new(),
            shared: Vec::new(),
        }
    }

    #[test]
    fn the_store_survives_saving() {
        let mut home = gap("Home", 0, 7 << 70 | 3, true);
        home.share(9, "Alice_1", true);
        home.known = vec![5, 6];
        let mut market = gap("Two words", -5, 0, false);
        market.global = true;
        let store = Store { gaps: vec![home, market] };
        assert_eq!(Store::decode(&store.encode()), store);
    }

    #[test]
    fn gaps_saved_before_owners_are_open_to_everyone() {
        let old = "W\tminecraft:overworld\t1\t64\t2\teast\tyukari_gap:yukari_gap\tAshford\n\
                   A\t00000000000000000000000000000001\tminecraft:overworld\t1\t64\t2\n";
        let store = Store::decode(old);
        assert_eq!(store.gaps, vec![Gap { pos: [1, 64, 2], ..gap("Ashford", 0, 0, false) }]);
        assert!(store.gaps[0].usable_by(42));
    }

    #[test]
    fn gaps_saved_before_finding_keep_owner_and_lock_and_are_found_by_nobody() {
        let old = "G\tminecraft:overworld\t1\t64\t2\teast\t00000000000000000000000000000007\t1\tMy home\n";
        let store = Store::decode(old);
        assert_eq!(store.gaps, vec![Gap { pos: [1, 64, 2], ..gap("My home", 0, 7, true) }]);
    }

    #[test]
    fn a_name_someone_has_gets_the_next_number() {
        let mut store = Store { gaps: vec![gap("Home", 0, 1, false)] };
        assert_eq!(store.unique_name("Market", None), "Market");
        assert_eq!(store.unique_name("home", None), "home (2)");
        store.gaps.push(gap("Home (2)", 1, 1, false));
        assert_eq!(store.unique_name("Home", None), "Home (3)");
        // Renaming a gap to its own name keeps it.
        assert_eq!(store.unique_name("Home", Some(&store.gaps[0].key())), "Home");
    }

    #[test]
    fn others_must_find_a_gap_before_they_can_use_it() {
        let mut g = gap("Market", 0, 1, false);
        assert!(g.usable_by(1));
        assert!(!g.usable_by(2));
        assert!(g.discover(2));
        assert!(!g.discover(2));
        assert!(!g.discover(1), "the owner never needs to");
        assert!(g.usable_by(2));
        assert!(!g.usable_by(3));
    }

    #[test]
    fn a_global_gap_is_everyones_until_it_is_locked() {
        let mut g = gap("Square", 0, 1, false);
        g.global = true;
        assert!(g.usable_by(3));
        g.locked = true;
        assert!(!g.usable_by(3));
        assert!(g.usable_by(1));
    }

    #[test]
    fn a_locked_gap_is_its_owners_and_those_it_is_shared_with() {
        let mut g = gap("Mine", 0, 1, true);
        g.discover(2);
        assert!(!g.usable_by(2), "finding a locked gap is not enough");
        assert!(g.share(2, "Bob", true));
        assert!(!g.share(2, "Bob", true));
        assert!(!g.share(1, "Owner", true), "not with its owner");
        assert!(g.usable_by(2));
        assert!(g.share(2, "Bob", false));
        assert!(!g.usable_by(2), "until revoked");
        assert!(!g.breakable_by(2) && g.breakable_by(1));
        g.locked = false;
        assert!(g.breakable_by(2), "anyone may break an unlocked gap");
        assert!(g.usable_by(2), "unlocked again, finding it counts");
    }

    #[test]
    fn destinations_are_the_gaps_a_player_may_use() {
        let mut theirs = gap("Theirs", 2, 2, false);
        theirs.discover(1);
        let mut square = gap("Square", 3, 2, false);
        square.global = true;
        let store = Store { gaps: vec![gap("Mine", 0, 1, true), gap("Unfound", 1, 2, false), theirs, square] };
        let names = |p| store.destinations(p, None).iter().map(|g| g.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(1), ["Mine", "Square", "Theirs"]);
        assert_eq!(names(2), ["Square", "Theirs", "Unfound"]);
        assert_eq!(names(3), ["Square"]);
    }

    #[test]
    fn names_are_cleaned_and_kept_short() {
        assert_eq!(clean("  a\tb\nc "), "a b c");
        assert_eq!(clean(&"x".repeat(50)).len(), NAME_MAX);
        assert_eq!(clean_player("Bad=Name,\t1"), "BadName1");
    }

    #[test]
    fn the_messages_come_back_as_they_went() {
        let key: Key = ("minecraft:the_nether".into(), [-3, 70, 12]);
        let players = vec![
            Player { uuid: 5 << 64 | 9, name: "Alice".into(), shared: true, online: false },
            Player { uuid: 2, name: "Bob_2".into(), shared: false, online: true },
        ];
        let n = Naming { key: key.clone(), name: "My gap".into(), locked: true, global: false, players };
        assert_eq!(decode_naming(&encode_naming(&n)), Some(n));
        let alone = Naming { key: key.clone(), name: "Two  words".into(), locked: false, global: true, players: vec![] };
        assert_eq!(decode_naming(&encode_naming(&alone)), Some(alone));
        let list = vec![Entry { key: key.clone(), name: "A b".into(), locked: false, global: true }];
        assert_eq!(decode_list(&encode_list(&list)), list);
        assert_eq!(decode_go(&encode_go(&key)), Some(key.clone()));
        let s = Share { key, player: 77, name: "Carol".into(), on: true };
        assert_eq!(decode_share(&encode_share(&s)), Some(s));
    }
}
