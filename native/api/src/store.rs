//! The world's gaps, and the messages the server and the client send each other about them.

/// Where a gap is: its dimension and block.
pub type Key = (String, [i32; 3]);

/// A gap: where it is, which way travellers come out of it, its name, who placed it, and whether
/// only they may use it.
#[derive(Clone, Debug, PartialEq)]
pub struct Gap {
    pub dimension: String,
    pub pos: [i32; 3],
    pub facing: String,
    pub name: String,
    /// The player who placed it; 0 for one nobody owns (from before gaps had owners).
    pub owner: u128,
    pub locked: bool,
}

impl Gap {
    pub fn key(&self) -> Key {
        (self.dimension.clone(), self.pos)
    }

    /// Whether `player` may go through it: anyone, unless it is locked to its owner.
    pub fn usable_by(&self, player: u128) -> bool {
        !self.locked || self.owner == player
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
            out += &format!(
                "G\t{}\t{x}\t{y}\t{z}\t{}\t{:032x}\t{}\t{}\n",
                g.dimension,
                g.facing,
                g.owner,
                u8::from(g.locked),
                clean(&g.name)
            );
        }
        out
    }

    /// Reads a store; one saved before gaps had owners (`W` lines, and `A` lines for who had
    /// opened which) gives gaps nobody owns, open to everyone.
    pub fn decode(text: &str) -> Self {
        let mut store = Store::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let Some(pos) = f.get(2..5).and_then(parse_pos) else { continue };
            let gap = match f[0] {
                "G" if f.len() >= 9 => Gap {
                    dimension: f[1].to_owned(),
                    pos,
                    facing: f[5].to_owned(),
                    owner: u128::from_str_radix(f[6], 16).unwrap_or(0),
                    locked: f[7] == "1",
                    name: f[8..].join(" "),
                },
                "W" if f.len() >= 8 => Gap {
                    dimension: f[1].to_owned(),
                    pos,
                    facing: f[5].to_owned(),
                    owner: 0,
                    locked: false,
                    name: f[7..].join(" "),
                },
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

/// Server to client: name the gap you placed (or rename it, sneaking).
pub const NAME: &str = "yukari_gap_native:name";
/// Server to client: where you can go from the gap you used.
pub const LIST: &str = "yukari_gap_native:list";
/// Client to server: the gap's name and whether it is locked.
pub const NAMED: &str = "yukari_gap_native:named";
/// Client to server: go to this gap.
pub const GO: &str = "yukari_gap_native:go";

/// What the naming screen starts with: the gap, its name and its lock.
#[derive(Clone, Debug, PartialEq)]
pub struct Naming {
    pub key: Key,
    pub name: String,
    pub locked: bool,
}

pub fn encode_naming(n: &Naming) -> Vec<u8> {
    format!("{}\t{}\t{}", key_fields(&n.key), u8::from(n.locked), clean(&n.name)).into_bytes()
}

pub fn decode_naming(data: &[u8]) -> Option<Naming> {
    let text = std::str::from_utf8(data).ok()?;
    let f: Vec<&str> = text.split('\t').collect();
    Some(Naming { key: parse_key(&f)?, locked: *f.get(4)? == "1", name: f.get(5..)?.join(" ") })
}

/// One gap in the travel screen: where, its name, and whether it is locked
/// (only its owner, the player, sees it then).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: Key,
    pub name: String,
    pub locked: bool,
}

/// The most gaps one list carries, well within a message's 30 000 bytes.
pub const LIST_MAX: usize = 400;

pub fn encode_list(entries: &[Entry]) -> Vec<u8> {
    let mut out = String::new();
    for e in entries.iter().take(LIST_MAX) {
        out += &format!("{}\t{}\t{}\n", key_fields(&e.key), u8::from(e.locked), clean(&e.name));
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
                name: f.get(5..)?.join(" "),
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
        }
    }

    #[test]
    fn the_store_survives_saving() {
        let store = Store { gaps: vec![gap("Home", 0, 7 << 70 | 3, true), gap("Two words", -5, 0, false)] };
        assert_eq!(Store::decode(&store.encode()), store);
    }

    #[test]
    fn gaps_saved_before_owners_are_open_to_everyone() {
        let old = "W\tminecraft:overworld\t1\t64\t2\teast\tyukari_gap:yukari_gap\tAshford\n\
                   A\t00000000000000000000000000000001\tminecraft:overworld\t1\t64\t2\n";
        let store = Store::decode(old);
        assert_eq!(store.gaps, vec![Gap { pos: [1, 64, 2], ..gap("Ashford", 0, 0, false) }]);
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
    fn a_locked_gap_is_only_its_owners() {
        let store = Store { gaps: vec![gap("Mine", 0, 1, true), gap("Ours", 1, 1, false), gap("Theirs", 2, 2, false)] };
        let names = |p| store.destinations(p, None).iter().map(|g| g.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(1), ["Mine", "Ours", "Theirs"]);
        assert_eq!(names(2), ["Ours", "Theirs"]);
        let here = store.gaps[1].key();
        assert_eq!(store.destinations(2, Some(&here)).len(), 1);
    }

    #[test]
    fn names_are_cleaned_and_kept_short() {
        assert_eq!(clean("  a\tb\nc "), "a b c");
        assert_eq!(clean(&"x".repeat(50)).len(), NAME_MAX);
    }

    #[test]
    fn the_messages_come_back_as_they_went() {
        let key: Key = ("minecraft:the_nether".into(), [-3, 70, 12]);
        let n = Naming { key: key.clone(), name: "My gap".into(), locked: true };
        assert_eq!(decode_naming(&encode_naming(&n)), Some(n));
        let list = vec![Entry { key: key.clone(), name: "A b".into(), locked: false }];
        assert_eq!(decode_list(&encode_list(&list)), list);
        assert_eq!(decode_go(&encode_go(&key)), Some(key));
    }
}
