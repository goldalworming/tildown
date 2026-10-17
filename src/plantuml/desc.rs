//! Diagram berbasis graf PlantUML (`cucadiagram`): class, object, use case,
//! component, deployment, state. Parser entitas + relasi bersama; tata letak
//! lewat [`super::graph`] (pengganti Graphviz), bentuk & ukuran mengikuti skin.
// ponytail: tanpa sprite/ikon, `together`, port, `hide @unlinked`, `remove`;
// generik `<T>` ditampilkan sebagai bagian nama.

use super::graph::{self, Graph};
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    // class
    Class,
    Abstract,
    Interface,
    Enum,
    Annotation,
    Entity,
    Exception,
    Metaclass,
    Stereotype,
    Protocol,
    Struct,
    Record,
    Dataclass,
    Object,
    Map,
    // description
    Usecase,
    Actor,
    Component,
    Iface, // lingkaran kecil `()`/`interface` di diagram komponen
    Node,
    Database,
    Cloud,
    Folder,
    Frame,
    Artifact,
    Card,
    File,
    Queue,
    Stack,
    Storage,
    Rectangle,
    Agent,
    Boundary,
    Control,
    EntityD,
    Person,
    Label,
    Hexagon,
    Collections,
    Package,
    // state
    State,
    Start,
    End,
    Choice,
    Fork,
    History,
    DeepHistory,
    Note,
}

impl Kind {
    fn is_classlike(self) -> bool {
        matches!(
            self,
            Kind::Class
                | Kind::Abstract
                | Kind::Interface
                | Kind::Enum
                | Kind::Annotation
                | Kind::Entity
                | Kind::Exception
                | Kind::Metaclass
                | Kind::Stereotype
                | Kind::Protocol
                | Kind::Struct
                | Kind::Record
                | Kind::Dataclass
        )
    }

    /// Huruf & warna lingkaran spot (skin `spot*`).
    fn spot(self) -> Option<(char, u32)> {
        Some(match self {
            Kind::Class => ('C', 0xADD1B2),
            Kind::Abstract => ('A', 0xA9DCDF),
            Kind::Interface => ('I', 0xB4A7E5),
            Kind::Enum => ('E', 0xEB937F),
            Kind::Annotation => ('@', 0xE3664A),
            Kind::Entity => ('E', 0xADD1B2),
            Kind::Exception => ('X', 0xD94321),
            Kind::Metaclass => ('M', 0xCCCCCC),
            Kind::Stereotype => ('S', 0xFF77FF),
            Kind::Protocol => ('P', 0xB4A7E5),
            Kind::Struct => ('S', 0xADD1B2),
            Kind::Record => ('R', 0xADD1B2),
            Kind::Dataclass => ('D', 0x7E57C2),
            _ => return None,
        })
    }

    fn container_ok(self) -> bool {
        matches!(
            self,
            Kind::Package
                | Kind::Node
                | Kind::Folder
                | Kind::Frame
                | Kind::Cloud
                | Kind::Database
                | Kind::Rectangle
                | Kind::Component
                | Kind::Card
                | Kind::Artifact
                | Kind::Storage
                | Kind::Queue
                | Kind::Stack
                | Kind::File
                | Kind::Hexagon
                | Kind::Agent
                | Kind::State
        )
    }
}

fn kind_of(word: &str) -> Option<Kind> {
    Some(match word {
        "class" => Kind::Class,
        "abstract" => Kind::Abstract,
        "interface" => Kind::Interface,
        "enum" => Kind::Enum,
        "annotation" => Kind::Annotation,
        "entity" => Kind::Entity,
        "exception" => Kind::Exception,
        "metaclass" => Kind::Metaclass,
        "stereotype" => Kind::Stereotype,
        "protocol" => Kind::Protocol,
        "struct" => Kind::Struct,
        "record" => Kind::Record,
        "dataclass" => Kind::Dataclass,
        "object" => Kind::Object,
        "map" => Kind::Map,
        "usecase" => Kind::Usecase,
        "actor" => Kind::Actor,
        "component" => Kind::Component,
        "node" => Kind::Node,
        "database" => Kind::Database,
        "cloud" => Kind::Cloud,
        "folder" => Kind::Folder,
        "frame" => Kind::Frame,
        "artifact" => Kind::Artifact,
        "card" => Kind::Card,
        "file" => Kind::File,
        "queue" => Kind::Queue,
        "stack" => Kind::Stack,
        "storage" => Kind::Storage,
        "rectangle" | "archimate" => Kind::Rectangle,
        "agent" => Kind::Agent,
        "boundary" => Kind::Boundary,
        "control" => Kind::Control,
        "person" => Kind::Person,
        "label" => Kind::Label,
        "hexagon" => Kind::Hexagon,
        "collections" => Kind::Collections,
        "package" | "namespace" => Kind::Package,
        "state" => Kind::State,
        "circle" | "()" => Kind::Iface,
        "diamond" => Kind::Choice,
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Decor {
    None,
    Extends,
    Composition,
    Aggregation,
    NotNavigable,
    Crowfoot,
    CircleCrowfoot,
    CircleLine,
    DoubleLine,
    LineCrowfoot,
    Arrow,
    ArrowTriangle,
    Circle,
    CircleFill,
    CircleConnect,
    Parenthesis,
    Square,
    Plus,
    HalfUp,
    HalfDown,
}

const DECORS1: &[(&str, Decor)] = &[
    ("<||", Decor::Extends),
    ("<|:", Decor::Extends),
    ("<|", Decor::Extends),
    ("^", Decor::Extends),
    ("}o", Decor::CircleCrowfoot),
    ("|o", Decor::CircleLine),
    ("||", Decor::DoubleLine),
    ("}|", Decor::LineCrowfoot),
    ("<<", Decor::ArrowTriangle),
    ("<_", Decor::Arrow),
    ("0)", Decor::CircleConnect),
    ("*", Decor::Composition),
    ("o", Decor::Aggregation),
    ("x", Decor::NotNavigable),
    ("}", Decor::Crowfoot),
    ("<", Decor::Arrow),
    ("0", Decor::Circle),
    ("@", Decor::CircleFill),
    (")", Decor::Parenthesis),
    ("#", Decor::Square),
    ("+", Decor::Plus),
];

const DECORS2: &[(&str, Decor)] = &[
    ("||>", Decor::Extends),
    (":|>", Decor::Extends),
    ("|>", Decor::Extends),
    ("^", Decor::Extends),
    ("o{", Decor::CircleCrowfoot),
    ("o|", Decor::CircleLine),
    ("||", Decor::DoubleLine),
    ("|{", Decor::LineCrowfoot),
    (">>", Decor::ArrowTriangle),
    ("_>", Decor::Arrow),
    ("(0", Decor::CircleConnect),
    ("\\\\", Decor::HalfUp),
    ("//", Decor::HalfDown),
    ("*", Decor::Composition),
    ("o", Decor::Aggregation),
    ("x", Decor::NotNavigable),
    ("{", Decor::Crowfoot),
    (">", Decor::Arrow),
    ("0", Decor::Circle),
    ("@", Decor::CircleFill),
    ("(", Decor::Parenthesis),
    ("#", Decor::Square),
    ("+", Decor::Plus),
];

impl Decor {
    /// Ruang yang dipakai dekorasi di ujung sisi (garis dipendekkan sebesar ini).
    fn len(self) -> f32 {
        match self {
            Decor::Extends => 18.0,
            Decor::Composition | Decor::Aggregation => 12.0,
            Decor::Arrow => 0.0,
            Decor::ArrowTriangle => 8.0,
            Decor::Circle | Decor::CircleFill | Decor::CircleConnect => 8.0,
            Decor::Square => 8.0,
            Decor::Plus => 16.0,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Member {
    t: usize,
    /// '-', '#', '~', '+' atau ' '.
    vis: char,
}

#[derive(Clone, Debug)]
struct Ent {
    id: String,
    name: usize,
    kind: Kind,
    stereo: Option<usize>,
    color: Option<Col>,
    line_color: Option<Col>,
    /// Kompartemen anggota (class: [field, method] atau sesuai pemisah).
    comps: Vec<Vec<Member>>,
    /// Pemisah eksplisit dipakai (jangan pisah field/method otomatis).
    explicit: bool,
    /// Baris map `key => value`.
    rows: Vec<(usize, usize)>,
    group: Option<usize>,
    /// Untuk note: arah relatif target.
    declared: bool,
    /// Urutan deklarasi lewat pernyataan (`state X`, `X : ...`). PlantUML membuat
    /// entitas ini sebelum yang hanya muncul di panah, jadi dicetak lebih dulu ke dot.
    decl: Option<usize>,
}

#[derive(Clone, Debug)]
struct Group {
    title: usize,
    stereo: Option<usize>,
    kind: Kind,
    parent: Option<usize>,
    color: Option<Col>,
    /// Entitas pemilik (komposit state/komponen): judul diambil darinya.
    ent: Option<usize>,
    /// Region konkuren (state `--`): indeks entitas mulai tiap region.
    regions: Vec<usize>,
}

#[derive(Clone, Debug)]
struct Link {
    a: usize,
    b: usize,
    d1: Decor,
    d2: Decor,
    dash: bool,
    dot: bool,
    bold: bool,
    hidden: bool,
    color: Option<Col>,
    len: u32,
    /// 'u' 'd' 'l' 'r' atau ' '.
    dir: char,
    label: Option<usize>,
    l1: Option<usize>,
    l2: Option<usize>,
    /// Arah label `>`/`<` (segitiga kecil).
    mark: i8,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Flavor {
    Class,
    Desc,
    State,
}

pub struct Desc {
    texts: Texts,
    ents: Vec<Ent>,
    groups: Vec<Group>,
    links: Vec<Link>,
    lr: bool,
    hide_empty: bool,
    hide_circle: bool,
    hide_fields: bool,
    hide_methods: bool,
    hide_stereo: bool,
    flavor: Flavor,
}

impl Diagram for Desc {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        place(self, size)
    }
}

// ---------------------------------------------------------------------------
// Parse

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Ambil `<<stereo>>` dan `#warna` dari ekor deklarasi; sisanya dikembalikan.
fn take_extras(s: &str) -> (String, Option<String>, Option<Col>, Option<Col>) {
    let mut rest = s.to_string();
    let mut stereo = None;
    if let (Some(a), Some(b)) = (rest.find("<<"), rest.rfind(">>")) {
        if a < b {
            stereo = Some(rest[a + 2..b].trim().to_string());
            rest = format!("{}{}", &rest[..a], &rest[b + 2..]);
        }
    }
    let mut color = None;
    let mut line = None;
    let mut out = Vec::new();
    for tok in rest.split_whitespace() {
        if tok.starts_with('#') && !tok.starts_with("##") {
            // #warna, #warna;line:red, ##[dashed]red
            for (i, part) in tok.trim_start_matches('#').split(';').enumerate() {
                let p = part.trim();
                if let Some(c) = p.strip_prefix("line:").or(p.strip_prefix("line.")) {
                    line = Col::parse(c);
                } else if i == 0 {
                    color = Col::parse(p).or(color);
                } else if let Some(c) = p.strip_prefix("back:") {
                    color = Col::parse(c);
                }
            }
        } else if tok.starts_with("##") {
            line = Col::parse(
                tok.trim_start_matches('#')
                    .trim_start_matches(|c| c != ']')
                    .trim_start_matches(']'),
            );
        } else {
            out.push(tok);
        }
    }
    (out.join(" "), stereo, color, line)
}

/// Nama + alias: `"Nama panjang" as X`, `X as "Nama"`, `X`.
fn name_alias(s: &str) -> (String, String) {
    let s = s.trim();
    if let Some((l, r)) = split_as(s) {
        let (l, r) = (l.trim(), r.trim());
        // `X as "Nama"`: id kiri; selain itu (`"Nama" as X`, `Nama as X`) id kanan.
        if r.starts_with('"') && !l.starts_with('"') {
            return (
                strip_shorthand(l).to_string(),
                strip_shorthand(r).to_string(),
            );
        }
        return (
            strip_shorthand(r).to_string(),
            strip_shorthand(l).to_string(),
        );
    }
    let n = strip_shorthand(s).to_string();
    (n.clone(), n)
}

fn split_as(s: &str) -> Option<(&str, &str)> {
    // " as " di luar kutip.
    let b = s.as_bytes();
    let mut q = false;
    for i in 0..b.len() {
        if b[i] == b'"' {
            q = !q;
        }
        if !q && s[i..].len() > 4 && s[i..].starts_with(" as ") {
            return Some((&s[..i], &s[i + 4..]));
        }
    }
    None
}

fn strip_shorthand(s: &str) -> &str {
    let s = s.trim();
    let t = if (s.starts_with('[') && s.ends_with(']'))
        || (s.starts_with('(') && s.ends_with(')'))
        || (s.starts_with(':') && s.ends_with(':') && s.len() > 1)
    {
        &s[1..s.len() - 1]
    } else {
        s
    };
    strip_quotes(t)
}

/// Panah relasi: (awal, panjang) di `s`, mis. `<|--`, `*-->`, `.up.>`, `-[#red,dashed]->`.
fn find_arrow(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let mut q = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            q = !q;
        }
        if !q && (c == b'-' || c == b'.' || c == b'=') {
            // Mundur untuk dekor kiri.
            let mut st = i;
            while st > 0
                && b"<|*o}x^#+0@)(_:{".contains(&b[st - 1])
                && !(b[st - 1] == b'o' && st >= 2 && b[st - 2].is_ascii_alphanumeric())
                && !(b[st - 1] == b'x' && st >= 2 && b[st - 2].is_ascii_alphanumeric())
            {
                st -= 1;
            }
            // Maju: badan, gaya [..], arah, badan, dekor kanan.
            let mut j = i;
            let mut body = 0;
            while j < b.len() {
                match b[j] {
                    b'-' | b'.' | b'=' | b'~' => {
                        body += 1;
                        j += 1;
                    }
                    b'[' => match s[j..].find(']') {
                        Some(k) => j += k + 1,
                        None => break,
                    },
                    _ => {
                        let rest = &s[j..].to_lowercase();
                        let dirw = [
                            "left", "right", "up", "down", "le", "ri", "do", "l", "r", "u", "d",
                        ]
                        .iter()
                        .find(|w| {
                            rest.starts_with(**w)
                                && rest[w.len()..].starts_with(['-', '.', '=', '['])
                        });
                        match dirw {
                            Some(w) if body > 0 => j += w.len(),
                            _ => break,
                        }
                    }
                }
            }
            if body == 0 {
                i += 1;
                continue;
            }
            while j < b.len() && b">|*o{x^#+0@()_\\/:".contains(&b[j]) {
                // Dekor kanan berhenti sebelum nama (huruf setelah o/x).
                if (b[j] == b'o' || b[j] == b'x')
                    && b.get(j + 1).is_some_and(|c| c.is_ascii_alphanumeric())
                {
                    break;
                }
                j += 1;
            }
            // Harus dibatasi spasi/nama di kedua sisi, bukan bagian kata.
            if body >= 1 && (st == 0 || !b[st - 1].is_ascii_alphanumeric() || b[st] != b[i] || true)
            {
                return Some((st, j - st));
            }
        }
        i += 1;
    }
    None
}

pub fn parse_lines(lines: &[String]) -> Option<Desc> {
    let mut d = Desc {
        texts: Texts::default(),
        ents: Vec::new(),
        groups: Vec::new(),
        links: Vec::new(),
        lr: false,
        hide_empty: false,
        hide_circle: false,
        hide_fields: false,
        hide_methods: false,
        hide_stereo: false,
        flavor: Flavor::Desc,
    };
    let ls: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    // Tebak jenis: class bila ada kata kunci class-family; state bila [*]/state.
    let any = |f: &dyn Fn(&str) -> bool| ls.iter().any(|l| f(&l.to_lowercase()));
    let class_kw = [
        "class ",
        "abstract ",
        "enum ",
        "annotation ",
        "exception ",
        "metaclass ",
        "struct ",
        "record ",
        "protocol ",
        "dataclass ",
        "object ",
        "map ",
        "stereotype ",
    ];
    let desc_kw = [
        "archimate ",
        "component ",
        "node ",
        "database ",
        "cloud ",
        "folder ",
        "frame ",
        "artifact ",
        "card ",
        "file ",
        "queue ",
        "stack ",
        "storage ",
        "rectangle ",
        "agent ",
        "boundary ",
        "control ",
        "person ",
        "usecase ",
        "actor ",
        "hexagon ",
        "collections ",
        "label ",
    ];
    let is_class = any(&|l| {
        class_kw.iter().any(|k| l.starts_with(k))
            || l.contains("<|--")
            || l.contains("--|>")
            || l.contains("<|..")
            || l.contains("..|>")
    });
    let is_desc = any(&|l| {
        desc_kw.iter().any(|k| l.starts_with(k))
            || l.starts_with('[') && !l.starts_with("[*]")
            || l.starts_with('(')
            || l.starts_with(':') && l[1..].contains(':')
    });
    let is_state = any(&|l| l.contains("[*]") || l.starts_with("state "));
    d.flavor = if is_class && !(is_desc && !any(&|l| l.starts_with("class "))) {
        Flavor::Class
    } else if is_state && !is_desc {
        Flavor::State
    } else if is_desc || !any(&|l| l.starts_with("interface ") || l.starts_with("entity ")) {
        Flavor::Desc
    } else {
        Flavor::Class
    };
    let mut stack: Vec<usize> = Vec::new(); // grup terbuka
    let mut i = 0;
    let mut any_content = false;
    while i < ls.len() {
        let l = ls[i];
        i += 1;
        let low = l.to_lowercase();
        let w0 = low.split_whitespace().next().unwrap_or("");
        match w0 {
            "left" if low.contains("direction") => {
                d.lr = true;
                continue;
            }
            "top" if low.contains("direction") => continue,
            "hide" | "show" => {
                let on = w0 == "hide";
                let r = &low[4..];
                if r.contains("empty")
                    && (r.contains("member")
                        || r.contains("field")
                        || r.contains("method")
                        || r.contains("description"))
                {
                    d.hide_empty = on;
                } else if r.contains("circle") || r.contains("spot") {
                    d.hide_circle = on;
                } else if r.contains("members") {
                    d.hide_fields = on;
                    d.hide_methods = on;
                } else if r.contains("field") || r.contains("attribute") {
                    d.hide_fields = on;
                } else if r.contains("method") {
                    d.hide_methods = on;
                } else if r.contains("stereotype") {
                    d.hide_stereo = on;
                }
                continue;
            }
            "}" => {
                stack.pop();
                continue;
            }
            "together" => {
                // Diabaikan: isi tetap di grup induk; buka "grup palsu".
                if l.ends_with('{') {
                    stack.push(usize::MAX);
                }
                continue;
            }
            "set" | "remove" | "restore" | "allow_mixing" | "allowmixing" | "skinparam"
            | "scale" | "!pragma" | "mainframe" | "newpage" => continue,
            _ => {}
        }
        if l == "--" || l == "||" {
            // Region konkuren state.
            if let Some(&g) = stack.last().filter(|&&g| g != usize::MAX) {
                d.groups[g].regions.push(d.ents.len());
            }
            continue;
        }
        let cur_group = stack.iter().rev().copied().find(|&g| g != usize::MAX);
        // Note.
        if w0 == "note" || w0 == "floating" {
            d.note(l, &ls, &mut i, cur_group)?;
            any_content = true;
            continue;
        }
        // Relasi (paling umum).
        if let Some((st, n)) = find_arrow(l) {
            let left = l[..st].trim();
            let kw_left = kind_of(
                left.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str(),
            );
            if !left.is_empty() && (kw_left.is_none() || left.split_whitespace().count() == 1) {
                if d.link(l, st, n, cur_group).is_some() {
                    any_content = true;
                    continue;
                }
            }
        }
        // Deklarasi: kata kunci, atau `abstract class`.
        let (kind, rest) = {
            let mut words = l.splitn(2, char::is_whitespace);
            let first = words.next().unwrap_or("").to_lowercase();
            let r = words.next().unwrap_or("").trim();
            if first == "abstract" && r.to_lowercase().starts_with("class ") {
                (Some(Kind::Abstract), r[6..].trim())
            } else if first == "static" && r.to_lowercase().starts_with("class ") {
                (Some(Kind::Class), r[6..].trim())
            } else {
                (kind_of(&first), r)
            }
        };
        if let Some(mut kind) = kind {
            if kind == Kind::Entity && d.flavor != Flavor::Class {
                kind = Kind::EntityD;
            }
            if kind == Kind::Interface && d.flavor == Flavor::Desc {
                kind = Kind::Iface;
            }
            let opens = rest.ends_with('{');
            let body = rest.trim_end_matches('{').trim();
            // `class X extends Y implements Z`
            let (body, ext) = split_extends(body);
            let (core, stereo, color, line) = take_extras(body);
            let (id, name) = name_alias(&core);
            // Generik `<T>` tetap jadi bagian nama.
            let e = d.entity(&id, &name, kind, cur_group, true);
            d.mark_decl(e);
            let stereo = if l.to_lowercase().starts_with("archimate") {
                None
            } else {
                stereo
            };
            if let Some(s) = stereo {
                // Stereotip khusus state: <<choice>>, <<fork>>, <<join>>, <<end>>...
                match (kind, s.to_lowercase().as_str()) {
                    (Kind::State, "choice") => d.ents[e].kind = Kind::Choice,
                    (Kind::State, "fork" | "join") => d.ents[e].kind = Kind::Fork,
                    (Kind::State, "end") => d.ents[e].kind = Kind::End,
                    (Kind::State, "start") => d.ents[e].kind = Kind::Start,
                    (Kind::State, "history") => d.ents[e].kind = Kind::History,
                    (Kind::State, "history*") => d.ents[e].kind = Kind::DeepHistory,
                    _ => {
                        let t = d
                            .texts
                            .add(Txt::new(&format!("«{}»", s), 12.0).italic().center());
                        d.ents[e].stereo = Some(t);
                    }
                }
            }
            if color.is_some() {
                d.ents[e].color = color;
            }
            if line.is_some() {
                d.ents[e].line_color = line;
            }
            for (sup, imp) in ext {
                let s = d.entity(
                    &sup,
                    &sup,
                    if imp { Kind::Interface } else { kind },
                    cur_group,
                    false,
                );
                d.links.push(Link {
                    a: s,
                    b: e,
                    d1: Decor::Extends,
                    d2: Decor::None,
                    dash: imp,
                    dot: false,
                    bold: false,
                    hidden: false,
                    color: None,
                    len: 2,
                    dir: ' ',
                    label: None,
                    l1: None,
                    l2: None,
                    mark: 0,
                });
            }
            any_content = true;
            if opens {
                if kind.is_classlike() || matches!(kind, Kind::Object) {
                    d.body(e, &ls, &mut i);
                } else if kind == Kind::Map {
                    d.map_body(e, &ls, &mut i);
                } else if kind.container_ok() || kind == Kind::State {
                    let g = d.group_for(e, kind, cur_group, color);
                    stack.push(g);
                } else {
                    d.body(e, &ls, &mut i);
                }
            }
            continue;
        }
        // Shorthand: `[Komponen]`, `(Usecase)`, `:Actor:`, `() Iface`.
        if l.starts_with('[') || l.starts_with('(') || (l.starts_with(':') && l[1..].contains(':'))
        {
            let (core, stereo, color, _) = take_extras(l);
            let kind = if core.starts_with('[') {
                Kind::Component
            } else if core.starts_with("()") {
                Kind::Iface
            } else if core.starts_with('(') {
                Kind::Usecase
            } else {
                Kind::Actor
            };
            let core = core.trim_start_matches("()").trim();
            let (id, name) = name_alias(core);
            let e = d.entity(&id, &name, kind, cur_group, true);
            if let Some(s) = stereo {
                let t = d
                    .texts
                    .add(Txt::new(&format!("«{}»", s), 12.0).italic().center());
                d.ents[e].stereo = Some(t);
            }
            d.ents[e].color = color.or(d.ents[e].color);
            any_content = true;
            continue;
        }
        // `X : anggota` / `State : deskripsi`
        if let Some((a, b)) = l.split_once(':') {
            let a = a.trim();
            if !a.is_empty() && !a.contains(' ') || a.starts_with('"') {
                let id = strip_shorthand(a).to_string();
                let kind = match d.flavor {
                    Flavor::State => Kind::State,
                    Flavor::Class => Kind::Class,
                    Flavor::Desc => Kind::Rectangle,
                };
                let e = d.entity(&id, &id, kind, cur_group, false);
                d.mark_decl(e);
                d.add_member(e, b.trim());
                any_content = true;
                continue;
            }
        }
        // `state "Nama" as X` tertangani di atas; baris lain: gagal.
        return None;
    }
    if d.flavor != Flavor::State {
        apply_single_strategy(&mut d);
    }
    (any_content && !d.ents.is_empty()).then_some(d)
}

/// CucaDiagram.applySingleStrategy PlantUML: per grup, daun tanpa tautan (≥ 3)
/// disusun jadi kisi ⌈√n⌉ kolom dengan tautan tak terlihat (sebaris minlen 0,
/// antarbaris minlen 1); grup berisi kisi seperti itu (≥ 3) dikisi lagi.
fn apply_single_strategy(d: &mut Desc) {
    fn branch(n: usize) -> usize {
        let r = (n as f64).sqrt() as usize;
        if r * r == n {
            r
        } else {
            r + 1
        }
    }
    // SquareMaker.putInSquare: (atas/kiri, bawah/kanan, minlen).
    fn square(n: usize) -> Vec<(usize, usize, bool)> {
        let b = branch(n);
        let mut head = 0;
        let mut v = Vec::new();
        for i in 1..n {
            if i - head == b {
                v.push((head, i, true));
                head = i;
            } else {
                v.push((i - 1, i, false));
            }
        }
        v
    }
    let hidden = |a: usize, b: usize, len: u32| Link {
        a,
        b,
        d1: Decor::None,
        d2: Decor::None,
        dash: false,
        dot: false,
        bold: false,
        hidden: true,
        color: None,
        len,
        dir: ' ',
        label: None,
        l1: None,
        l2: None,
        mark: 0,
    };
    let linked: Vec<bool> = (0..d.ents.len())
        .map(|e| d.links.iter().any(|l| l.a == e || l.b == e))
        .collect();
    let mut magmas: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
    for g in std::iter::once(None).chain((0..d.groups.len()).map(Some)) {
        let st: Vec<usize> = (0..d.ents.len())
            .filter(|&e| {
                d.ents[e].group == g && !linked[e] && !d.groups.iter().any(|gr| gr.ent == Some(e))
            })
            .collect();
        if st.len() < 3 {
            continue;
        }
        for (i, j, td) in square(st.len()) {
            d.links.push(hidden(st[i], st[j], if td { 1 } else { 0 }));
        }
        magmas.push((g, st));
    }
    // MagmaList: kontainer magma = induk grupnya (magma root tak punya kontainer).
    for g in std::iter::once(None).chain((0..d.groups.len()).map(Some)) {
        let ms: Vec<&Vec<usize>> = magmas
            .iter()
            .filter(|m| m.0.is_some_and(|gg| d.groups[gg].parent == g))
            .map(|m| &m.1)
            .collect();
        if ms.len() < 3 {
            continue;
        }
        for (i, j, td) in square(ms.len()) {
            let (a, b) = if td {
                // bawah-kiri magma atas → atas-kiri magma bawah
                let n = ms[i].len();
                (ms[i][((n - 1) / branch(n)) * branch(n)], ms[j][0])
            } else {
                // atas-kanan magma kiri → atas-kiri magma kanan
                (ms[i][branch(ms[i].len()) - 1], ms[j][0])
            };
            let l = hidden(a, b, if td { 1 } else { 0 });
            d.links.push(l);
        }
    }
}

/// `X extends A, B implements C` → (X, [(A, false), (B, false), (C, true)]).
fn split_extends(s: &str) -> (&str, Vec<(String, bool)>) {
    let low = s.to_lowercase();
    let (mut head, mut out) = (s, Vec::new());
    for (kw, imp) in [(" implements ", true), (" extends ", false)] {
        if let Some(k) = low.find(kw) {
            let tail = &s[k + kw.len()..];
            let tail_end = tail
                .to_lowercase()
                .find(" implements ")
                .unwrap_or(tail.len());
            for n in tail[..tail_end].split(',') {
                let n = n.trim().trim_end_matches('{').trim();
                if !n.is_empty() {
                    out.push((n.to_string(), imp));
                }
            }
            if k < head.len() {
                head = &s[..k];
            }
        }
    }
    (head.trim(), out)
}

impl Desc {
    fn entity(
        &mut self,
        id: &str,
        name: &str,
        kind: Kind,
        group: Option<usize>,
        declared: bool,
    ) -> usize {
        let id = id.trim();
        if let Some(k) = self.ents.iter().position(|e| e.id == id) {
            if declared {
                let e = &mut self.ents[k];
                if !e.declared {
                    e.kind = kind;
                    e.group = group;
                    e.declared = true;
                    let t = name.to_string();
                    let center = !kind.is_classlike();
                    let mut tx = Txt::new(&unescape(&t), 14.0);
                    if center {
                        tx = tx.center();
                    }
                    self.texts.0[e.name] = tx;
                }
            }
            return k;
        }
        let special = match id {
            "[*]" => Some(Kind::Start),
            _ => None,
        };
        let kind = special.unwrap_or(kind);
        let mut tx = Txt::new(&unescape(name), 14.0);
        if !kind.is_classlike() {
            tx = tx.center();
        }
        if matches!(kind, Kind::Abstract | Kind::Interface) {
            tx = tx.italic();
        }
        let t = self.texts.add(tx);
        self.ents.push(Ent {
            id: id.to_string(),
            name: t,
            kind,
            stereo: None,
            color: None,
            line_color: None,
            comps: Vec::new(),
            explicit: false,
            rows: Vec::new(),
            group,
            declared,
            decl: None,
        });
        self.ents.len() - 1
    }

    fn mark_decl(&mut self, k: usize) {
        if self.ents[k].decl.is_none() {
            self.ents[k].decl = Some(self.ents.iter().filter(|e| e.decl.is_some()).count());
        }
    }

    fn add_member(&mut self, e: usize, m: &str) {
        let kind = self.ents[e].kind;
        let mut s = m.trim().to_string();
        let mut vis = ' ';
        let mut txt_italic = false;
        let mut txt_under = false;
        for (tag, it, un) in [
            ("{abstract}", true, false),
            ("{static}", false, true),
            ("{classifier}", false, true),
            ("{field}", false, false),
            ("{method}", false, false),
        ] {
            if s.contains(tag) {
                s = s.replace(tag, "").trim().to_string();
                txt_italic |= it;
                txt_under |= un;
            }
        }
        if kind.is_classlike() {
            if let Some(c) = s.chars().next() {
                if "-#~+".contains(c)
                    && s.len() > 1
                    && !s.starts_with("--")
                    && !s.starts_with("++")
                    && !s.starts_with("##")
                {
                    vis = c;
                    s = s[1..].trim().to_string();
                }
            }
        }
        let mut tx = Txt::new(&unescape(&s), 14.0);
        if txt_italic {
            tx = tx.italic();
        }
        if txt_under {
            tx.s = format!("__{}__", tx.s);
        }
        let t = self.texts.add(tx);
        let is_method = s.contains('(');
        let ent = &mut self.ents[e];
        if ent.comps.is_empty() {
            ent.comps = if kind.is_classlike() {
                vec![Vec::new(), Vec::new()]
            } else {
                vec![Vec::new()]
            };
        }
        let idx = if kind.is_classlike() && !ent.explicit {
            is_method as usize
        } else {
            ent.comps.len() - 1
        };
        ent.comps[idx].push(Member { t, vis });
    }

    /// Isi `{ ... }` class/object: anggota & pemisah.
    fn body(&mut self, e: usize, ls: &[&str], i: &mut usize) {
        while *i < ls.len() {
            let l = ls[*i];
            *i += 1;
            if l == "}" {
                return;
            }
            let sep = ["--", "..", "==", "__"].iter().find(|p| {
                l.starts_with(**p)
                    && l.chars()
                        .all(|c| "-.=_ ".contains(c) || c.is_alphanumeric() || c == ' ')
                    && (l.len() == 2 || l.ends_with(**p))
            });
            if sep.is_some() {
                let ent = &mut self.ents[e];
                if !ent.explicit {
                    // Pemisah eksplisit: gabungkan field+method otomatis jadi satu urutan.
                    let mut all: Vec<Member> = ent.comps.drain(..).flatten().collect();
                    ent.comps = vec![std::mem::take(&mut all)];
                    ent.explicit = true;
                }
                ent.comps.push(Vec::new());
                continue;
            }
            self.add_member(e, l);
        }
    }

    fn map_body(&mut self, e: usize, ls: &[&str], i: &mut usize) {
        while *i < ls.len() {
            let l = ls[*i];
            *i += 1;
            if l == "}" {
                return;
            }
            let (k, v) = l.split_once("=>").unwrap_or((l, ""));
            let kt = self.texts.add(Txt::new(&unescape(k.trim()), 14.0));
            let vt = self.texts.add(Txt::new(&unescape(v.trim()), 14.0));
            self.ents[e].rows.push((kt, vt));
        }
    }

    fn group_for(
        &mut self,
        e: usize,
        kind: Kind,
        parent: Option<usize>,
        color: Option<Col>,
    ) -> usize {
        let title = self.ents[e].name;
        let stereo = self.ents[e].stereo;
        self.groups.push(Group {
            title,
            stereo,
            kind,
            parent,
            color,
            ent: Some(e),
            regions: Vec::new(),
        });
        let g = self.groups.len() - 1;
        // Entitas pemilik dipakai sebagai grup, bukan node.
        self.ents[e].group = parent;
        self.texts.0[title].bold = !matches!(kind, Kind::State);
        g
    }

    fn note(&mut self, l: &str, ls: &[&str], i: &mut usize, group: Option<usize>) -> Option<()> {
        let low = l.to_lowercase();
        let (head, text) = match l.split_once(':') {
            Some((h, t)) if !h.contains('"') => (h.to_string(), t.trim().to_string()),
            _ => {
                if let (Some(a), Some(b)) = (l.find('"'), l.rfind('"')) {
                    if a < b {
                        // note "teks" as N1
                        let t = l[a + 1..b].to_string();
                        let alias = l[b + 1..].trim().trim_start_matches("as").trim();
                        let id = if alias.is_empty() {
                            format!("__note{}", self.ents.len())
                        } else {
                            alias.to_string()
                        };
                        let n = self.entity(&id, "", Kind::Note, group, true);
                        let tt = self.texts.add(Txt::new(&unescape(&t), 13.0));
                        self.ents[n].name = tt;
                        return Some(());
                    }
                }
                let mut v = Vec::new();
                while *i < ls.len() {
                    let x = ls[*i];
                    *i += 1;
                    if x.to_lowercase().replace(' ', "") == "endnote" {
                        break;
                    }
                    v.push(x);
                }
                (l.to_string(), v.join("\n"))
            }
        };
        let words: Vec<&str> = head.split_whitespace().collect();
        let pos = words.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        let target = if words.get(2).is_some_and(|w| w.eq_ignore_ascii_case("of")) {
            words.get(3..).map(|w| w.join(" "))
        } else {
            None
        };
        let (target, _, color, _) = match target {
            Some(t) => {
                let (a, b, c, d) = take_extras(&t);
                (Some(a), b, c, d)
            }
            None => (None, None, None, None),
        };
        let id = format!("__note{}", self.ents.len());
        let n = self.entity(&id, "", Kind::Note, group, true);
        let tt = self.texts.add(Txt::new(&unescape(&text), 13.0));
        self.ents[n].name = tt;
        self.ents[n].color = color;
        if low.starts_with("note as") || low.starts_with("floating") {
            if let Some(alias) = words.get(2) {
                self.ents[n].id = alias.to_string();
            }
            return Some(());
        }
        if let Some(t) = target {
            let t = strip_shorthand(&t).to_string();
            let k = self.ents.iter().position(|e| e.id == t).unwrap_or_else(|| {
                let kind = if self.flavor == Flavor::State {
                    Kind::State
                } else {
                    Kind::Class
                };
                self.entity(&t, &t, kind, group, false)
            });
            let (a, b, len, dir) = match pos.as_str() {
                "left" => (n, k, 0, 'r'),
                "right" => (k, n, 0, 'r'),
                "top" => (n, k, 1, 'd'),
                _ => (k, n, 1, 'd'),
            };
            self.links.push(Link {
                a,
                b,
                d1: Decor::None,
                d2: Decor::None,
                dash: true,
                dot: false,
                bold: false,
                hidden: false,
                color: None,
                len,
                dir,
                label: None,
                l1: None,
                l2: None,
                mark: 0,
            });
        }
        Some(())
    }

    fn link(&mut self, l: &str, st: usize, n: usize, group: Option<usize>) -> Option<()> {
        let arrow = &l[st..st + n];
        let left = l[..st].trim();
        let right_all = l[st + n..].trim();
        let colon = right_all
            .char_indices()
            .scan(false, |q, (k, c)| {
                if c == '"' {
                    *q = !*q;
                }
                Some((k, c, *q))
            })
            .find(|&(_, c, q)| c == ':' && !q)
            .map(|x| x.0);
        let (right, label) = match colon {
            Some(k) => (right_all[..k].trim(), Some(right_all[k + 1..].trim())),
            None => (right_all, None),
        };
        // Label ujung: `A "1" *-- "many" B`.
        let (left, l1) = split_end_label(left, true);
        let (right, l2) = split_end_label(right, false);
        let (right, _stereo, color2, _) = take_extras(&right);
        let left = left.trim();
        if left.is_empty() || right.trim().is_empty() {
            return None;
        }
        // Dekor kiri/kanan, badan, gaya.
        let body_start = arrow.find(['-', '.', '=', '~'])?;
        let body_end = arrow.rfind(['-', '.', '=', '~'])? + 1;
        let (h1, mid, h2) = (
            &arrow[..body_start],
            &arrow[body_start..body_end],
            &arrow[body_end..],
        );
        let d1 = DECORS1.iter().find(|(k, _)| *k == h1).map_or(
            if h1.is_empty() {
                Some(Decor::None)
            } else {
                None
            },
            |x| Some(x.1),
        )?;
        let d2 = DECORS2.iter().find(|(k, _)| *k == h2).map_or(
            if h2.is_empty() {
                Some(Decor::None)
            } else {
                None
            },
            |x| Some(x.1),
        )?;
        let mut lk = Link {
            a: 0,
            b: 0,
            d1,
            d2,
            dash: mid.contains('.'),
            dot: false,
            bold: mid.contains('='),
            hidden: false,
            color: color2,
            len: 1,
            dir: ' ',
            label: None,
            l1: None,
            l2: None,
            mark: 0,
        };
        // Gaya [..] dan arah.
        let mut plain = String::new();
        let mut k = 0;
        let mb = mid.as_bytes();
        while k < mb.len() {
            if mb[k] == b'[' {
                let e = mid[k..].find(']').map_or(mid.len(), |x| k + x);
                for p in mid[k + 1..e].split(',') {
                    let p = p.trim().to_lowercase();
                    match p.as_str() {
                        "dashed" => lk.dash = true,
                        "dotted" => lk.dot = true,
                        "bold" => lk.bold = true,
                        "hidden" => lk.hidden = true,
                        "plain" => {}
                        _ if p.starts_with("thickness") => lk.bold = true,
                        _ => lk.color = Col::parse(&p).or(lk.color),
                    }
                }
                k = e + 1;
            } else {
                plain.push(mb[k] as char);
                k += 1;
            }
        }
        let low = plain.to_lowercase();
        let letters: String = low.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        lk.dir = match letters.as_str() {
            "" => ' ',
            s if s.starts_with('u') => 'u',
            s if s.starts_with('d') => 'd',
            s if s.starts_with('l') => 'l',
            s if s.starts_with('r') => 'r',
            _ => ' ',
        };
        let dashes = low
            .chars()
            .filter(|c| matches!(c, '-' | '.' | '=' | '~'))
            .count() as u32;
        lk.len = if lk.dir != ' ' {
            1
        } else {
            dashes.saturating_sub(1)
        };
        let kind_default = match self.flavor {
            Flavor::Class => Kind::Class,
            Flavor::State => Kind::State,
            Flavor::Desc => Kind::Rectangle,
        };
        let ent_of = |d: &mut Desc, raw: &str| -> usize {
            let raw = raw.trim();
            let kind = if raw.starts_with('[') && raw != "[*]" {
                Kind::Component
            } else if raw.starts_with("()") {
                Kind::Iface
            } else if raw.starts_with('(') {
                Kind::Usecase
            } else if raw.starts_with(':') {
                Kind::Actor
            } else if raw == "[*]" {
                Kind::Start
            } else {
                kind_default
            };
            let raw = raw.trim_start_matches("()").trim();
            let (id, name) = name_alias(raw);
            let declared = kind != kind_default;
            d.entity(&id, &name, kind, group, declared)
        };
        let a = ent_of(self, left);
        let mut b = ent_of(self, &right);
        // [*] di kanan = akhir (state); [*] di dalam komposit = mulai/akhir lokal.
        if right.trim() == "[*]" {
            let id = format!("__end{}", group.map_or(0, |g| g + 1));
            b = self.entity(&id, "", Kind::End, group, true);
        }
        if left.trim() == "[*]" && group.is_some() {
            let id = format!("__start{}", group.unwrap() + 1);
            let s = self.entity(&id, "", Kind::Start, group, true);
            lk.a = s;
        } else {
            lk.a = a;
        }
        lk.b = b;
        if let Some(t) = l1 {
            lk.l1 = Some(self.texts.add(Txt::new(&unescape(&t), 13.0)));
        }
        if let Some(t) = l2 {
            lk.l2 = Some(self.texts.add(Txt::new(&unescape(&t), 13.0)));
        }
        if let Some(t) = label {
            let mut t = t.trim().to_string();
            if t.ends_with('>') && !t.ends_with("->") && !t.ends_with(">>") {
                lk.mark = 1;
                t = t.trim_end_matches('>').trim().to_string();
            } else if t.starts_with('<') && !t.starts_with("<<") {
                lk.mark = -1;
                t = t.trim_start_matches('<').trim().to_string();
            }
            let t = t.replace("<<", "«").replace(">>", "»");
            if !t.is_empty() {
                lk.label = Some(self.texts.add(Txt::new(&unescape(&t), 13.0)));
            }
        }
        self.links.push(lk);
        Some(())
    }
}

/// `A "1"` → (A, "1") (kiri) / `"many" B` → (B, "many") (kanan).
fn split_end_label(s: &str, left: bool) -> (String, Option<String>) {
    let s = s.trim();
    if left {
        if s.ends_with('"') {
            if let Some(k) = s[..s.len() - 1].rfind('"') {
                let head = s[..k].trim();
                if !head.is_empty() {
                    return (head.to_string(), Some(s[k + 1..s.len() - 1].to_string()));
                }
            }
        }
    } else if s.starts_with('"') {
        if let Some(k) = s[1..].find('"') {
            let tail = s[k + 2..].trim();
            if !tail.is_empty() {
                return (tail.to_string(), Some(s[1..k + 1].to_string()));
            }
        }
    }
    (s.to_string(), None)
}

// ---------------------------------------------------------------------------
// Ukuran & gambar entitas

const ROW: f32 = 1.3; // tambahan tinggi baris anggota (17,6 untuk teks 16,3)

/// Ukuran (w, h) entitas.
fn ent_size(d: &Desc, e: &Ent, size: &[(f32, f32)]) -> (f32, f32) {
    let (tw, th) = size[e.name];
    let (sw, sh) = e
        .stereo
        .filter(|_| !d.hide_stereo)
        .map_or((0.0, 0.0), |s| size[s]);
    match e.kind {
        k if k.is_classlike() || k == Kind::Object => {
            let circle = k != Kind::Object && !d.hide_circle;
            let head_w = if circle {
                30.0 + tw.max(sw) + 3.0
            } else {
                tw.max(sw) + 14.0
            };
            let head_h = if circle {
                (th + sh + 12.4).max(32.0)
            } else {
                th + sh + 5.3
            };
            let comps = visible_comps(d, e);
            let mut w = head_w;
            let mut h = head_h;
            for c in &comps {
                h += c.iter().map(|m| size[m.t].1 + ROW).sum::<f32>() + 8.0;
                for m in c.iter() {
                    let pad = if m.vis != ' ' { 26.0 } else { 12.0 };
                    w = w.max(size[m.t].0 + pad);
                }
            }
            (w, h)
        }
        Kind::Map => {
            let kw = e.rows.iter().map(|r| size[r.0].0).fold(0.0, f32::max);
            let vw = e.rows.iter().map(|r| size[r.1].0).fold(0.0, f32::max);
            let w = (kw + vw + 22.0).max(tw + 14.0);
            let h = th
                + 5.3
                + e.rows
                    .iter()
                    .map(|r| size[r.0].1.max(size[r.1].1) + 5.3)
                    .sum::<f32>();
            (w, h)
        }
        Kind::Usecase => (tw + 35.3, th + 14.6 + sh),
        Kind::Actor | Kind::Person => (tw.max(26.0), 58.0 + 2.0 + th + sh),
        Kind::Boundary | Kind::Control | Kind::EntityD => (tw.max(46.0), 34.0 + th + sh),
        Kind::Iface => (tw.max(16.0), 16.0 + 15.0 + th),
        Kind::Component => (tw.max(sw) + 40.0, th + sh + 31.3),
        Kind::Database => (tw.max(sw).max(30.0) + 20.0, th + sh + 40.0),
        Kind::Queue => (tw.max(sw) + 35.0, th + sh + 20.0),
        Kind::Cloud => (tw.max(sw) + 40.0, th + sh + 30.0),
        Kind::Node => (tw.max(sw) + 30.0, th + sh + 30.0),
        Kind::Folder => (tw.max(sw).max(30.0) + 20.0, th + sh + 21.0 + 24.0),
        Kind::Artifact | Kind::File => (tw.max(sw) + 30.0, th + sh + 24.3),
        Kind::Frame => (tw.max(sw) + 40.0, th + sh + 31.3),
        Kind::Card | Kind::Rectangle | Kind::Label | Kind::Agent | Kind::Collections => {
            let lines = visible_comps(d, e);
            let extra: f32 = lines
                .iter()
                .map(|c| c.iter().map(|m| size[m.t].1).sum::<f32>())
                .sum();
            let wmax = lines
                .iter()
                .flat_map(|c| c.iter())
                .map(|m| size[m.t].0)
                .fold(tw.max(sw), f32::max);
            (
                wmax + 20.0,
                th + sh + 7.3 + extra + if e.kind == Kind::Label { -7.3 } else { 0.0 },
            )
        }
        Kind::Storage => (tw.max(sw) + 20.0, th + sh + 21.3),
        Kind::Stack => (tw.max(sw) + 50.0, th + sh + 21.3),
        Kind::Hexagon => (tw.max(sw) + 40.0, th + sh + 21.3),
        Kind::Package => (tw.max(sw) + 20.0, th + sh + 30.0),
        Kind::State => {
            let lines: f32 = e.comps.iter().flatten().map(|m| size[m.t].1).sum();
            let lw = e
                .comps
                .iter()
                .flatten()
                .map(|m| size[m.t].0)
                .fold(0.0, f32::max);
            (
                (tw.max(lw).max(sw) + 20.0).max(50.0),
                (th + sh + 11.3 + lines.max(16.0) + 6.0).max(50.0),
            )
        }
        Kind::Start => (20.0, 20.0),
        Kind::End => (22.0, 22.0),
        Kind::Choice => (24.0, 24.0),
        Kind::Fork => (80.0, 8.0),
        Kind::History | Kind::DeepHistory => (22.0, 22.0),
        Kind::Note => (tw + 21.0, th + 11.3),
        _ => (tw + 20.0, th + 20.0),
    }
}

fn visible_comps<'a>(d: &Desc, e: &'a Ent) -> Vec<&'a Vec<Member>> {
    let k = e.kind;
    let mut comps: Vec<&Vec<Member>> = Vec::new();
    let empty: &'static Vec<Member> = Box::leak(Box::new(Vec::new()));
    if k.is_classlike() {
        let (f, m) = if e.explicit {
            (None, None)
        } else {
            (e.comps.first(), e.comps.get(1))
        };
        if e.explicit {
            for c in &e.comps {
                if !(d.hide_empty && c.is_empty()) {
                    comps.push(c);
                }
            }
        } else {
            let f = f.unwrap_or(empty);
            let m = m.unwrap_or(empty);
            if !d.hide_fields && !(d.hide_empty && f.is_empty()) {
                comps.push(f);
            }
            if !d.hide_methods && !(d.hide_empty && m.is_empty()) {
                comps.push(m);
            }
        }
    } else if k == Kind::Object {
        let f = e.comps.first().unwrap_or(empty);
        if !(d.hide_empty && f.is_empty()) {
            comps.push(f);
        }
    } else {
        for c in &e.comps {
            comps.push(c);
        }
    }
    comps
}

fn ink(e: &Ent) -> Col {
    e.line_color.unwrap_or(Col::Ink)
}

/// Gambar entitas di kotak [x, y, w, h].
fn draw_ent(p: &mut Vec<Prim>, d: &Desc, e: &Ent, r: [f32; 4], size: &[(f32, f32)]) {
    let [x, y, w, h] = r;
    let fill = e.color.unwrap_or(Col::Box);
    let lc = ink(e);
    let (tw, th) = size[e.name];
    let stereo = e.stereo.filter(|_| !d.hide_stereo);
    let sh = stereo.map_or(0.0, |s| size[s].1);
    let cx = x + w / 2.0;
    // Nama (+ stereotip) di tengah kotak pada tinggi `ty`.
    let label = |p: &mut Vec<Prim>, ty: f32| {
        if let Some(s) = stereo {
            p.push(Prim::Text(s, cx - size[s].0 / 2.0, ty));
        }
        p.push(Prim::Text(e.name, cx - tw / 2.0, ty + sh));
    };
    match e.kind {
        k if k.is_classlike() || k == Kind::Object => {
            p.push(rect(r, 2.5, fill, lc, 0.5));
            let circle = k != Kind::Object && !d.hide_circle;
            let head_h = if circle {
                (th + sh + 12.4).max(32.0)
            } else {
                th + sh + 5.3
            };
            if circle {
                let tmax = tw.max(stereo.map_or(0.0, |s| size[s].0));
                let gw = 30.0 + tmax;
                let gx = cx - gw / 2.0;
                let (scx, scy) = (gx + 15.0, y + head_h / 2.0);
                let (ch, col) = k.spot().unwrap();
                p.push(ellipse(
                    [scx - 11.0, scy - 11.0, 22.0, 22.0],
                    Col::Rgb(col),
                    Col::Ink,
                    1.0,
                ));
                spot_letter(p, ch, scx, scy);
                let tx = gx + 30.0 + (tmax - tw) / 2.0;
                let ty = y + (head_h - th - sh) / 2.0;
                if let Some(s) = stereo {
                    p.push(Prim::Text(s, gx + 30.0 + (tmax - size[s].0) / 2.0, ty));
                }
                p.push(Prim::Text(e.name, tx, ty + sh));
            } else {
                label(p, y + 2.65);
            }
            let mut yy = y + head_h;
            for c in visible_comps(d, e) {
                p.push(line(vec![(x, yy), (x + w, yy)], lc, 0.5));
                let mut ry = yy + 4.0;
                for m in c.iter() {
                    let (_, mh) = size[m.t];
                    let tx = if m.vis != ' ' { x + 20.0 } else { x + 6.0 };
                    if m.vis != ' ' {
                        vis_icon(p, m.vis, x + 11.0, ry + mh / 2.0 + 0.6, size[m.t].0 > 0.0);
                    }
                    p.push(Prim::Text(m.t, tx, ry + ROW / 2.0));
                    ry += mh + ROW;
                }
                yy = ry + 4.0;
            }
        }
        Kind::Map => {
            p.push(rect(r, 2.5, fill, lc, 0.5));
            let hh = th + 5.3;
            p.push(Prim::Text(e.name, cx - tw / 2.0, y + 2.65));
            let kw = e.rows.iter().map(|r| size[r.0].0).fold(0.0, f32::max) + 11.0;
            let mut yy = y + hh;
            for &(k, v) in &e.rows {
                let rh = size[k].1.max(size[v].1) + 5.3;
                p.push(line(vec![(x, yy), (x + w, yy)], lc, 1.0));
                p.push(line(vec![(x + kw, yy), (x + kw, yy + rh)], lc, 1.0));
                p.push(Prim::Text(k, x + (kw - size[k].0) / 2.0, yy + 2.65));
                p.push(Prim::Text(v, x + kw + 5.0, yy + 2.65));
                yy += rh;
            }
        }
        Kind::Usecase => {
            p.push(ellipse(r, fill, lc, 0.5));
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Actor | Kind::Person => {
            let fy = y;
            p.push(ellipse([cx - 8.0, fy, 16.0, 16.0], fill, lc, 0.5));
            p.push(line(vec![(cx, fy + 16.0), (cx, fy + 43.0)], lc, 0.5));
            p.push(line(
                vec![(cx - 13.0, fy + 24.0), (cx + 13.0, fy + 24.0)],
                lc,
                0.5,
            ));
            p.push(line(
                vec![
                    (cx - 13.0, fy + 58.0),
                    (cx, fy + 43.0),
                    (cx + 13.0, fy + 58.0),
                ],
                lc,
                0.5,
            ));
            label(p, fy + 60.0);
        }
        Kind::Boundary => {
            p.push(line(
                vec![(cx - 22.0, y + 4.0), (cx - 22.0, y + 28.0)],
                lc,
                0.5,
            ));
            p.push(line(
                vec![(cx - 22.0, y + 16.0), (cx - 12.0, y + 16.0)],
                lc,
                0.5,
            ));
            p.push(ellipse([cx - 12.0, y + 4.0, 24.0, 24.0], fill, lc, 0.5));
            label(p, y + 34.0);
        }
        Kind::Control => {
            p.push(ellipse([cx - 12.0, y + 4.0, 24.0, 24.0], fill, lc, 0.5));
            p.push(open_head((cx - 2.0, y + 4.0), (-1.0, 0.0), lc, 0.5));
            label(p, y + 34.0);
        }
        Kind::EntityD => {
            p.push(ellipse([cx - 12.0, y + 2.0, 24.0, 24.0], fill, lc, 0.5));
            p.push(line(
                vec![(cx - 12.0, y + 28.0), (cx + 12.0, y + 28.0)],
                lc,
                0.5,
            ));
            label(p, y + 34.0);
        }
        Kind::Iface => {
            p.push(ellipse([cx - 8.0, y, 16.0, 16.0], fill, lc, 0.5));
            label(p, y + 16.0 + 15.0 - 13.0 + 2.0);
        }
        Kind::Component => {
            p.push(rect(r, 2.5, fill, lc, 0.5));
            let (ix, iy) = (x + w - 20.0, y + 5.0);
            p.push(rect([ix, iy, 15.0, 10.0], 0.0, fill, lc, 0.5));
            p.push(rect([ix - 2.0, iy + 2.0, 4.0, 2.0], 0.0, fill, lc, 0.5));
            p.push(rect([ix - 2.0, iy + 6.0, 4.0, 2.0], 0.0, fill, lc, 0.5));
            label(p, y + 21.86 - 4.0);
        }
        Kind::Database => {
            cylinder(p, r, fill, lc);
            label(p, y + 22.0 + (h - 22.0 - th - sh) / 2.0);
        }
        Kind::Queue => {
            // Silinder mendatar.
            let mut pts = vec![(x + 5.0, y)];
            pts.push((x + w - 5.0, y));
            vector::bezier(
                &mut pts,
                (x + w - 5.0, y),
                (x + w + 1.0, y),
                (x + w + 1.0, y + h),
                (x + w - 5.0, y + h),
            );
            pts.push((x + 5.0, y + h));
            vector::bezier(
                &mut pts,
                (x + 5.0, y + h),
                (x - 1.0, y + h),
                (x - 1.0, y),
                (x + 5.0, y),
            );
            p.push(poly(pts, fill, lc, 0.5));
            let mut arc = vec![(x + w - 5.0, y)];
            vector::bezier(
                &mut arc,
                (x + w - 5.0, y),
                (x + w - 11.0, y),
                (x + w - 11.0, y + h),
                (x + w - 5.0, y + h),
            );
            p.push(line(arc, lc, 0.5));
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Cloud => {
            cloud(p, r, fill, lc);
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Node => {
            node3d(p, r, fill, lc, 0.5);
            label(p, y + 10.0 + (h - 10.0 - th - sh) / 2.0);
        }
        Kind::Folder | Kind::Package => {
            folder(p, r, tw.min(w - 20.0), fill, lc, 0.5);
            label(p, y + 21.0 + (h - 21.0 - th - sh) / 2.0);
        }
        Kind::Artifact => {
            p.push(rect(r, 2.5, fill, lc, 0.5));
            let (ix, iy) = (x + w - 17.0, y + 5.0);
            p.push(poly(
                vec![
                    (ix, iy),
                    (ix, iy + 14.0),
                    (ix + 12.0, iy + 14.0),
                    (ix + 12.0, iy + 6.0),
                    (ix + 6.0, iy),
                ],
                fill,
                lc,
                0.5,
            ));
            p.push(line(
                vec![(ix + 6.0, iy), (ix + 6.0, iy + 6.0), (ix + 12.0, iy + 6.0)],
                lc,
                0.5,
            ));
            p.push(Prim::Text(e.name, x + 10.0, y + (h - th) / 2.0));
        }
        Kind::File => {
            p.push(poly(
                vec![
                    (x, y),
                    (x + w - 10.0, y),
                    (x + w, y + 10.0),
                    (x + w, y + h),
                    (x, y + h),
                ],
                fill,
                lc,
                0.5,
            ));
            p.push(line(
                vec![
                    (x + w - 10.0, y),
                    (x + w - 10.0, y + 10.0),
                    (x + w, y + 10.0),
                ],
                lc,
                0.5,
            ));
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Frame => {
            p.push(rect(r, 2.5, fill, lc, 0.5));
            let tab = tw.min(w) * 0.0 + 28.6;
            p.push(line(
                vec![
                    (x + tab, y),
                    (x + tab, y + 5.0),
                    (x + tab - 7.0, y + 12.0),
                    (x, y + 12.0),
                ],
                lc,
                0.5,
            ));
            label(p, y + 12.0 + (h - 12.0 - th - sh) / 2.0);
        }
        Kind::Storage => {
            p.push(rect(r, (h / 2.0).min(35.0), fill, lc, 0.5));
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Stack => {
            p.push(rect([x + 15.0, y, w - 30.0, h], 2.5, fill, Col::None, 0.0));
            p.push(line(
                vec![
                    (x, y),
                    (x + 15.0, y),
                    (x + 15.0, y + h),
                    (x + w - 15.0, y + h),
                    (x + w - 15.0, y),
                    (x + w, y),
                ],
                lc,
                0.5,
            ));
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Hexagon => {
            let k = 15.0;
            p.push(poly(
                vec![
                    (x + k, y),
                    (x + w - k, y),
                    (x + w, y + h / 2.0),
                    (x + w - k, y + h),
                    (x + k, y + h),
                    (x, y + h / 2.0),
                ],
                fill,
                lc,
                0.5,
            ));
            label(p, y + (h - th - sh) / 2.0);
        }
        Kind::Label => {
            label(p, y);
        }
        Kind::Card | Kind::Rectangle | Kind::Agent | Kind::Collections => {
            if e.kind == Kind::Collections {
                p.push(rect([x + 4.0, y - 4.0, w, h], 2.5, fill, lc, 0.5));
            }
            p.push(rect(r, 2.5, fill, lc, 0.5));
            let comps = visible_comps(d, e);
            let lines: f32 = comps
                .iter()
                .map(|c| c.iter().map(|m| size[m.t].1).sum::<f32>())
                .sum();
            let mut ty = y + (h - th - sh - lines) / 2.0;
            label(p, ty);
            ty += th + sh;
            for m in comps.iter().flat_map(|c| c.iter()) {
                p.push(Prim::Text(m.t, cx - size[m.t].0 / 2.0, ty));
                ty += size[m.t].1;
            }
        }
        Kind::State => {
            p.push(rect(r, 12.5, fill, lc, 0.5));
            let hh = th + sh + 11.3;
            p.push(line(vec![(x, y + hh), (x + w, y + hh)], lc, 0.5));
            label(p, y + 5.65);
            let mut ty = y + hh + 3.0;
            for m in e.comps.iter().flatten() {
                p.push(Prim::Text(m.t, x + 10.0, ty));
                ty += size[m.t].1;
            }
        }
        Kind::Start => p.push(ellipse(r, Col::Rgb(0x222222), Col::Rgb(0x222222), 1.0)),
        Kind::End => {
            p.push(ellipse(r, Col::None, Col::Rgb(0x222222), 1.0));
            p.push(ellipse(
                [x + 5.0, y + 5.0, 12.0, 12.0],
                Col::Rgb(0x222222),
                Col::Rgb(0x222222),
                1.0,
            ));
        }
        Kind::Choice => p.push(poly(
            vec![(cx, y), (x + w, y + h / 2.0), (cx, y + h), (x, y + h / 2.0)],
            fill,
            lc,
            0.5,
        )),
        Kind::Fork => p.push(rect(r, 0.0, Col::Rgb(0x555555), Col::Rgb(0x555555), 1.0)),
        Kind::History | Kind::DeepHistory => {
            p.push(ellipse(r, fill, lc, 0.5));
            // Huruf H (dan *) dengan garis.
            let (hx, hy) = (cx - 4.0, y + 6.0);
            p.push(line(vec![(hx, hy), (hx, hy + 10.0)], lc, 1.5));
            p.push(line(vec![(hx + 8.0, hy), (hx + 8.0, hy + 10.0)], lc, 1.5));
            p.push(line(vec![(hx, hy + 5.0), (hx + 8.0, hy + 5.0)], lc, 1.5));
        }
        Kind::Note => {
            let fill = e.color.unwrap_or(Col::Note);
            p.extend(
                super::activity::opale(r, None)
                    .into_iter()
                    .map(|q| match q {
                        Prim::Path {
                            pts,
                            closed,
                            stroke,
                            w,
                            dash,
                            ..
                        } => Prim::Path {
                            pts,
                            closed,
                            fill,
                            stroke,
                            w,
                            dash,
                        },
                        t => t,
                    }),
            );
            p.push(Prim::Text(e.name, x + 6.0, y + 5.65));
        }
        _ => {
            p.push(rect(r, 2.5, fill, lc, 0.5));
            label(p, y + (h - th - sh) / 2.0);
        }
    }
}

/// Huruf di lingkaran spot (C, I, A, E, ...), digambar sebagai garis tebal.
fn spot_letter(p: &mut Vec<Prim>, ch: char, cx: f32, cy: f32) {
    let c = Col::Text;
    let w = 1.6;
    let pts: Vec<Vec<Pt>> = match ch {
        'C' => {
            let mut arc = Vec::new();
            for i in 0..=12 {
                let a = (40.0 + 280.0 * i as f32 / 12.0f32).to_radians();
                arc.push((cx + 4.2 * a.cos(), cy - 5.0 * a.sin()));
            }
            vec![arc]
        }
        'I' => vec![
            vec![(cx, cy - 5.0), (cx, cy + 5.0)],
            vec![(cx - 3.0, cy - 5.0), (cx + 3.0, cy - 5.0)],
            vec![(cx - 3.0, cy + 5.0), (cx + 3.0, cy + 5.0)],
        ],
        'A' => vec![
            vec![(cx - 4.5, cy + 5.0), (cx, cy - 5.0), (cx + 4.5, cy + 5.0)],
            vec![(cx - 2.5, cy + 1.5), (cx + 2.5, cy + 1.5)],
        ],
        'E' => vec![
            vec![
                (cx + 4.0, cy - 5.0),
                (cx - 3.5, cy - 5.0),
                (cx - 3.5, cy + 5.0),
                (cx + 4.0, cy + 5.0),
            ],
            vec![(cx - 3.5, cy), (cx + 2.5, cy)],
        ],
        'X' => vec![
            vec![(cx - 4.0, cy - 5.0), (cx + 4.0, cy + 5.0)],
            vec![(cx + 4.0, cy - 5.0), (cx - 4.0, cy + 5.0)],
        ],
        'M' => vec![vec![
            (cx - 4.5, cy + 5.0),
            (cx - 4.5, cy - 5.0),
            (cx, cy + 1.0),
            (cx + 4.5, cy - 5.0),
            (cx + 4.5, cy + 5.0),
        ]],
        'S' => {
            let mut s = Vec::new();
            vector::bezier(
                &mut s,
                (cx + 3.5, cy - 4.0),
                (cx, cy - 7.0),
                (cx - 6.0, cy - 3.0),
                (cx, cy),
            );
            vector::bezier(
                &mut s,
                (cx, cy),
                (cx + 6.0, cy + 3.0),
                (cx, cy + 7.0),
                (cx - 3.5, cy + 4.0),
            );
            vec![s]
        }
        'P' | 'R' | 'D' => {
            let mut v = vec![vec![
                (cx - 3.5, cy + 5.0),
                (cx - 3.5, cy - 5.0),
                (cx + 1.0, cy - 5.0),
            ]];
            let mut b = vec![(cx + 1.0, cy - 5.0)];
            vector::bezier(
                &mut b,
                (cx + 1.0, cy - 5.0),
                (cx + 5.0, cy - 5.0),
                (cx + 5.0, cy),
                (cx + 1.0, cy),
            );
            b.push((cx - 3.5, cy));
            v.push(b);
            if ch == 'R' {
                v.push(vec![(cx, cy), (cx + 4.0, cy + 5.0)]);
            }
            v
        }
        '@' => {
            let mut arc = Vec::new();
            for i in 0..=16 {
                let a = (360.0 * i as f32 / 16.0f32).to_radians();
                arc.push((cx + 4.5 * a.cos(), cy + 4.5 * a.sin()));
            }
            vec![arc]
        }
        _ => Vec::new(),
    };
    for l in pts {
        p.push(line(l, c, w));
    }
}

/// Ikon visibilitas (skin `visibilityIcon`).
fn vis_icon(p: &mut Vec<Prim>, v: char, cx: f32, cy: f32, _field: bool) {
    match v {
        '-' => p.push(rect(
            [cx - 3.0, cy - 3.0, 6.0, 6.0],
            0.0,
            Col::None,
            Col::Rgb(0xC82930),
            1.0,
        )),
        '#' => p.push(poly(
            vec![
                (cx, cy - 4.0),
                (cx + 4.0, cy),
                (cx, cy + 4.0),
                (cx - 4.0, cy),
            ],
            Col::None,
            Col::Rgb(0xB38D22),
            1.0,
        )),
        '~' => p.push(poly(
            vec![(cx, cy - 4.0), (cx + 4.0, cy + 3.0), (cx - 4.0, cy + 3.0)],
            Col::None,
            Col::Rgb(0x1963A0),
            1.0,
        )),
        _ => p.push(ellipse(
            [cx - 3.0, cy - 3.0, 6.0, 6.0],
            Col::Rgb(0x84BE84),
            Col::Rgb(0x038048),
            1.0,
        )),
    }
}

fn cylinder(p: &mut Vec<Prim>, r: [f32; 4], fill: Col, lc: Col) {
    let [x, y, w, h] = r;
    let cx = x + w / 2.0;
    let mut pts = vec![(x, y + 10.0)];
    vector::bezier(&mut pts, (x, y + 10.0), (x, y), (cx, y), (cx, y));
    vector::bezier(
        &mut pts,
        (cx, y),
        (x + w, y),
        (x + w, y + 10.0),
        (x + w, y + 10.0),
    );
    pts.push((x + w, y + h - 10.0));
    vector::bezier(
        &mut pts,
        (x + w, y + h - 10.0),
        (x + w, y + h),
        (cx, y + h),
        (cx, y + h),
    );
    vector::bezier(
        &mut pts,
        (cx, y + h),
        (x, y + h),
        (x, y + h - 10.0),
        (x, y + h - 10.0),
    );
    p.push(poly(pts, fill, lc, 0.5));
    let mut arc = vec![(x, y + 10.0)];
    vector::bezier(
        &mut arc,
        (x, y + 10.0),
        (x, y + 20.0),
        (cx, y + 20.0),
        (cx, y + 20.0),
    );
    vector::bezier(
        &mut arc,
        (cx, y + 20.0),
        (x + w, y + 20.0),
        (x + w, y + 10.0),
        (x + w, y + 10.0),
    );
    p.push(line(arc, lc, 0.5));
}

fn cloud(p: &mut Vec<Prim>, r: [f32; 4], fill: Col, lc: Col) {
    let [x, y, w, h] = r;
    // Lengkungan di sekeliling elips.
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    let n = ((w + h) / 18.0).round().max(8.0) as usize;
    let mut pts = Vec::new();
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let p0 = (cx + rx * a0.cos(), cy + ry * a0.sin());
        let p1 = (cx + rx * a1.cos(), cy + ry * a1.sin());
        let am = (a0 + a1) / 2.0;
        let bulge = 7.0;
        let c = (cx + (rx + bulge) * am.cos(), cy + (ry + bulge) * am.sin());
        if i == 0 {
            pts.push(p0);
        }
        vector::bezier(&mut pts, p0, c, c, p1);
    }
    p.push(poly(pts, fill, lc, 0.5));
}

fn node3d(p: &mut Vec<Prim>, r: [f32; 4], fill: Col, lc: Col, w0: f32) {
    let [x, y, w, h] = r;
    let d = 10.0;
    p.push(poly(
        vec![
            (x, y + d),
            (x + d, y),
            (x + w, y),
            (x + w, y + h - d),
            (x + w - d, y + h),
            (x, y + h),
        ],
        fill,
        lc,
        w0,
    ));
    p.push(line(
        vec![(x + w - d, y + h), (x + w - d, y + d), (x + w, y)],
        lc,
        w0,
    ));
    p.push(line(vec![(x, y + d), (x + w - d, y + d)], lc, w0));
}

fn folder(p: &mut Vec<Prim>, r: [f32; 4], title_w: f32, fill: Col, lc: Col, lw: f32) {
    let [x, y, w, h] = r;
    let tab = (title_w + 4.0).max(20.0).min(w - 10.0);
    let th = 21.0;
    p.push(poly(
        vec![
            (x, y),
            (x + tab, y),
            (x + tab + 7.0, y + th),
            (x + w, y + th),
            (x + w, y + h),
            (x, y + h),
        ],
        fill,
        lc,
        lw,
    ));
    p.push(line(vec![(x, y + th), (x + tab + 7.0, y + th)], lc, lw));
}

/// Dekorasi ujung sisi di `tip`, `dir` = arah masuk ke node (dari garis ke tip).
fn decor(p: &mut Vec<Prim>, dc: Decor, tip: Pt, dir: Pt, c: Col) {
    let l = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
    let (ux, uy) = (dir.0 / l, dir.1 / l);
    let (nx, ny) = (-uy, ux);
    let at = |back: f32, side: f32| (tip.0 - ux * back + nx * side, tip.1 - uy * back + ny * side);
    match dc {
        Decor::None => {}
        Decor::Extends => p.push(poly(
            vec![tip, at(18.0, 6.0), at(18.0, -6.0)],
            Col::Paper,
            c,
            1.0,
        )),
        Decor::Composition => p.push(poly(
            vec![tip, at(6.0, 4.0), at(12.0, 0.0), at(6.0, -4.0)],
            c,
            c,
            1.0,
        )),
        Decor::Aggregation => p.push(poly(
            vec![tip, at(6.0, 4.0), at(12.0, 0.0), at(6.0, -4.0)],
            Col::Paper,
            c,
            1.0,
        )),
        Decor::Arrow => p.push(poly(
            vec![tip, at(9.0, 4.0), at(5.0, 0.0), at(9.0, -4.0)],
            c,
            c,
            1.0,
        )),
        Decor::ArrowTriangle => p.push(poly(vec![tip, at(8.0, 3.0), at(8.0, -3.0)], c, c, 1.0)),
        Decor::NotNavigable => {
            p.push(line(vec![at(10.0, -4.0), at(18.0, 4.0)], c, 1.0));
            p.push(line(vec![at(10.0, 4.0), at(18.0, -4.0)], c, 1.0));
        }
        Decor::Crowfoot => {
            p.push(line(vec![at(10.0, 0.0), at(0.0, -6.0)], c, 1.0));
            p.push(line(vec![at(10.0, 0.0), at(0.0, 6.0)], c, 1.0));
            p.push(line(vec![at(10.0, 0.0), tip], c, 1.0));
        }
        Decor::CircleCrowfoot => {
            p.push(line(vec![at(10.0, 0.0), at(0.0, -6.0)], c, 1.0));
            p.push(line(vec![at(10.0, 0.0), at(0.0, 6.0)], c, 1.0));
            let cc = at(15.0, 0.0);
            p.push(ellipse(
                [cc.0 - 4.0, cc.1 - 4.0, 8.0, 8.0],
                Col::Paper,
                c,
                1.0,
            ));
        }
        Decor::LineCrowfoot => {
            p.push(line(vec![at(10.0, 0.0), at(0.0, -6.0)], c, 1.0));
            p.push(line(vec![at(10.0, 0.0), at(0.0, 6.0)], c, 1.0));
            p.push(line(vec![at(14.0, -6.0), at(14.0, 6.0)], c, 1.0));
        }
        Decor::CircleLine => {
            p.push(line(vec![at(6.0, -6.0), at(6.0, 6.0)], c, 1.0));
            let cc = at(14.0, 0.0);
            p.push(ellipse(
                [cc.0 - 4.0, cc.1 - 4.0, 8.0, 8.0],
                Col::Paper,
                c,
                1.0,
            ));
        }
        Decor::DoubleLine => {
            p.push(line(vec![at(6.0, -6.0), at(6.0, 6.0)], c, 1.0));
            p.push(line(vec![at(10.0, -6.0), at(10.0, 6.0)], c, 1.0));
        }
        Decor::Circle | Decor::CircleConnect => {
            let cc = at(4.0, 0.0);
            p.push(ellipse(
                [cc.0 - 4.0, cc.1 - 4.0, 8.0, 8.0],
                Col::Paper,
                c,
                1.0,
            ));
        }
        Decor::CircleFill => {
            let cc = at(4.0, 0.0);
            p.push(ellipse([cc.0 - 4.0, cc.1 - 4.0, 8.0, 8.0], c, c, 1.0));
        }
        Decor::Parenthesis => {
            let mut arc = Vec::new();
            for i in 0..=10 {
                let a = (-70.0 + 140.0 * i as f32 / 10.0f32).to_radians();
                let (s, co) = (a.sin(), a.cos());
                let q = at(10.0 - 8.0 * co, 8.0 * s);
                arc.push(q);
            }
            p.push(line(arc, c, 1.0));
        }
        Decor::Square => p.push(poly(
            vec![
                tip,
                at(0.0, 4.0),
                at(8.0, 4.0),
                at(8.0, -4.0),
                at(0.0, -4.0),
            ],
            Col::Paper,
            c,
            1.0,
        )),
        Decor::Plus => {
            let cc = at(8.0, 0.0);
            p.push(ellipse(
                [cc.0 - 8.0, cc.1 - 8.0, 16.0, 16.0],
                Col::Paper,
                c,
                1.0,
            ));
            p.push(line(vec![at(0.0, 0.0), at(16.0, 0.0)], c, 1.0));
            p.push(line(vec![at(8.0, -8.0), at(8.0, 8.0)], c, 1.0));
        }
        Decor::HalfUp => p.push(line(vec![tip, at(10.0, 5.0)], c, 1.0)),
        Decor::HalfDown => p.push(line(vec![tip, at(10.0, -5.0)], c, 1.0)),
    }
}

// ---------------------------------------------------------------------------
// Tata letak

fn place(d: &Desc, size: &[(f32, f32)]) -> Placed {
    place_scope(d, size, None, 7.0).0
}

/// Bantalan atas cluster/komposit (judul).
/// Tinggi & lebar deskripsi komposit state (`Jalan : teks`).
fn group_desc(d: &Desc, g: &Group, size: &[(f32, f32)]) -> (f32, f32) {
    let ms: Vec<usize> = g.ent.map_or(Vec::new(), |e| {
        d.ents[e].comps.iter().flatten().map(|m| m.t).collect()
    });
    if ms.is_empty() {
        return (0.0, 0.0);
    }
    (
        ms.iter().map(|&t| size[t].1).sum::<f32>() + 4.0,
        ms.iter().map(|&t| size[t].0).fold(0.0, f32::max),
    )
}

fn group_top(d: &Desc, g: &Group, size: &[(f32, f32)]) -> f32 {
    let th = size[g.title].1;
    let sh = g.stereo.map_or(0.0, |s| size[s].1);
    match g.kind {
        Kind::Package | Kind::Folder => th + sh + 14.0,
        Kind::Node => th + sh + 20.0,
        Kind::State => th + sh + 11.3 + 12.0 + group_desc(d, g, size).0,
        _ => th + sh + 12.0,
    }
}

/// Tata isi `scope` (None = seluruh diagram). Seperti PlantUML
/// (CucaDiagramSimplifierState): di diagram state, komposit "autarkic" — tak ada
/// sisi yang menembus batasnya — ditata terpisah lalu menjadi satu node.
/// Hasil kedua: posisi y garis pemisah region milik `scope` (koordinat hasil).
fn place_scope(
    d: &Desc,
    size: &[(f32, f32)],
    scope: Option<usize>,
    margin: f32,
) -> (Placed, Vec<f32>) {
    // Grup x berada di dalam (atau sama dengan) top?
    let inside = |mut x: Option<usize>, top: usize| {
        while let Some(c) = x {
            if c == top {
                return true;
            }
            x = d.groups[c].parent;
        }
        false
    };
    let in_scope = |x: Option<usize>| scope.is_none_or(|s| inside(x, s));
    let autarkic: Vec<bool> = (0..d.groups.len())
        .map(|g| {
            d.flavor == Flavor::State
                && !d.lr
                && d.groups[g].kind != Kind::Package
                && Some(g) != scope
                && in_scope(Some(g))
                && d.links
                    .iter()
                    .all(|l| inside(d.ents[l.a].group, g) == inside(d.ents[l.b].group, g))
        })
        .collect();
    // Komposit runtuh teratas yang memuat grup x (di bawah scope).
    let collapsed_in = |mut x: Option<usize>| {
        let mut r = None;
        while let Some(c) = x {
            if Some(c) == scope {
                break;
            }
            if autarkic[c] {
                r = Some(c);
            }
            x = d.groups[c].parent;
        }
        r
    };
    let collapsed = |g: usize| collapsed_in(Some(g)) == Some(g);
    let visible_cluster =
        |g: usize| Some(g) != scope && in_scope(Some(g)) && collapsed_in(Some(g)).is_none();
    let unscope = |c: usize| if Some(c) == scope { None } else { Some(c) };
    // Entitas yang menjadi grup tidak jadi node.
    let as_group: Vec<Option<usize>> = (0..d.ents.len())
        .map(|e| d.groups.iter().position(|g| g.ent == Some(e)))
        .collect();
    let mut node_of = vec![usize::MAX; d.ents.len()];
    let mut nodes = Vec::new();
    let mut ent_of_node = Vec::new();
    // Urutan node = urutan cetak PlantUML (menentukan pemutusan siklus & urutan
    // awal di dot): komposit runtuh dulu, lalu daun; di diagram state, entitas yang
    // dideklarasikan (`decl`) — termasuk komposit — mendahului sisanya.
    let mut inner: Vec<(usize, Placed, Vec<f32>)> = Vec::new();
    let mut group_node = vec![usize::MAX; d.groups.len()];
    let mut items: Vec<Result<usize, usize>> = (0..d.groups.len())
        .filter(|&g| collapsed(g))
        .map(Ok)
        .chain(
            (0..d.ents.len())
                .filter(|&k| {
                    let e = &d.ents[k];
                    as_group[k].is_none() && in_scope(e.group) && collapsed_in(e.group).is_none()
                })
                .map(Err),
        )
        .collect();
    if d.flavor == Flavor::State {
        let ent = |it: &Result<usize, usize>| match *it {
            Ok(g) => d.groups[g].ent,
            Err(k) => Some(k),
        };
        items.sort_by_key(|it| {
            let dc = ent(it).and_then(|k| d.ents[k].decl);
            (dc.is_none(), dc.unwrap_or(0), ent(it).unwrap_or(0))
        });
    }
    for it in items {
        match it {
            Ok(g) => {
                let gr = &d.groups[g];
                let (pl, seps) = place_scope(d, size, Some(g), 0.0);
                let top = group_top(d, gr, size);
                // InnerStateAutonom: max(judul, deskripsi, gambar isi) + 2·MARGIN +
                // 2·MARGIN_LINE (+ MARGIN bila ada deskripsi); gambar isi (svek) =
                // konten + 15, konten digeser 6.
                let (dh, dw) = group_desc(d, gr, size);
                let mf = if dh > 0.0 { 5.0 } else { 0.0 };
                let w = (pl.w + 15.0).max(size[gr.title].0).max(dw) + 20.0 + mf;
                let h = pl.h + top + 13.0;
                let cluster = gr
                    .parent
                    .and_then(|p| unscope(gr.ent.map_or(p, |e| region_of(d, p, e))));
                group_node[g] = nodes.len();
                if let Some(e) = gr.ent {
                    node_of[e] = nodes.len();
                }
                ent_of_node.push(usize::MAX);
                nodes.push(graph::Node {
                    w,
                    h,
                    shape: graph::Shape::Rect,
                    cluster,
                });
                inner.push((g, pl, seps));
            }
            Err(k) => {
                let e = &d.ents[k];
                let (w, h) = ent_size(d, e, size);
                let shape = match e.kind {
                    Kind::Usecase
                    | Kind::Start
                    | Kind::End
                    | Kind::History
                    | Kind::DeepHistory
                    | Kind::Iface => graph::Shape::Ellipse,
                    _ => graph::Shape::Rect,
                };
                node_of[k] = nodes.len();
                ent_of_node.push(k);
                nodes.push(graph::Node {
                    w,
                    h,
                    shape,
                    cluster: e.group.and_then(|g| unscope(region_of(d, g, k))),
                });
            }
        }
    }
    // Grup kosong tetap butuh isi: beri node kosong kecil.
    for (g, _) in d.groups.iter().enumerate() {
        if !visible_cluster(g) {
            continue;
        }
        let has = nodes
            .iter()
            .any(|n| n.cluster.is_some_and(|c| in_group(d, c, g)))
            || d.groups.iter().any(|h| h.parent == Some(g));
        if !has {
            nodes.push(graph::Node {
                w: 40.0,
                h: 10.0,
                shape: graph::Shape::Rect,
                cluster: Some(g),
            });
            ent_of_node.push(usize::MAX);
        }
    }
    let mut clusters: Vec<graph::Cluster> = d
        .groups
        .iter()
        .map(|g| {
            let tw = size[g.title].0;
            let top = group_top(d, g, size);
            graph::Cluster {
                parent: g.parent.and_then(unscope),
                pad: [top, 12.0, 12.0, 12.0],
                min_w: tw + 30.0,
            }
        })
        .collect();
    // Region konkuren: sub-cluster tanpa bingkai, ditumpuk lewat sisi tersembunyi.
    for (g, gr) in d.groups.iter().enumerate() {
        for k in 0..gr.regions.len() + 1 {
            let _ = k;
            clusters.push(graph::Cluster {
                parent: unscope(g),
                pad: [6.0, 6.0, 6.0, 6.0],
                min_w: 0.0,
            });
        }
    }
    let mut edges = Vec::new();
    let mut edge_link = Vec::new();
    for (k, l) in d.links.iter().enumerate() {
        // Sisi ke/dari entitas-grup: pakai node pertama di dalam grup.
        let endpoint = |e: usize| -> Option<usize> {
            if node_of[e] != usize::MAX {
                return Some(node_of[e]);
            }
            let g = as_group[e].filter(|&g| visible_cluster(g))?;
            nodes
                .iter()
                .position(|n| n.cluster.is_some_and(|c| in_group(d, c, g)))
                .or_else(|| {
                    let sub: Vec<usize> = (0..d.groups.len())
                        .filter(|&h| d.groups[h].parent == Some(g))
                        .collect();
                    nodes
                        .iter()
                        .position(|n| n.cluster.is_some_and(|c| sub.contains(&c)))
                })
        };
        let (Some(mut a), Some(mut b)) = (endpoint(l.a), endpoint(l.b)) else {
            continue;
        };
        let mut minlen = l.len;
        let mut ordered = false;
        match l.dir {
            'u' => std::mem::swap(&mut a, &mut b),
            'l' => {
                std::mem::swap(&mut a, &mut b);
                minlen = 0;
                ordered = true;
            }
            'r' => {
                minlen = 0;
                ordered = true;
            }
            'd' => minlen = minlen.max(1),
            _ => {}
        }
        if d.lr && l.dir == ' ' {
            // left to right: `->` tetap sejajar (sekarang tegak), `-->` jadi mendatar.
        }
        let mut label = l.label.map(|t| size[t]);
        if let Some(lb) = &mut label {
            if l.mark != 0 {
                lb.0 += 12.0;
            }
        }
        edges.push(graph::Edge {
            a,
            b,
            minlen,
            label,
            ordered,
        });
        let swapped = l.dir == 'u' || l.dir == 'l';
        let vis = |e: usize| as_group[e].filter(|&g| !collapsed(g));
        let (ga, gb) = (vis(l.a), vis(l.b));
        edge_link.push((k, swapped, if swapped { (gb, ga) } else { (ga, gb) }));
    }
    let mut hidden_from = edges.len();
    for (g, gr) in d.groups.iter().enumerate() {
        if gr.regions.is_empty() {
            continue;
        }
        let base = region_base(d, g);
        for k in 0..gr.regions.len() {
            let ra = nodes.iter().position(|nd| nd.cluster == Some(base + k));
            let rb = nodes.iter().position(|nd| nd.cluster == Some(base + k + 1));
            if let (Some(a), Some(b)) = (ra, rb) {
                edges.push(graph::Edge {
                    a,
                    b,
                    minlen: 1,
                    label: None,
                    ordered: false,
                });
            }
        }
    }
    hidden_from = hidden_from.min(edges.len());
    let _ = hidden_from;
    let g = Graph {
        nodes: nodes.clone(),
        edges,
        clusters,
        lr: d.lr,
        // Isi komposit: PlantUML tak mengirim nodesep/ranksep, jadi bawaan dot
        // (0.25in / 0.5in).
        nodesep: if scope.is_some() { 18.0 } else { 35.0 },
        ranksep: if scope.is_some() { 36.0 } else { 60.0 },
    };
    let o = graph::layout(&g);
    let mut back = Vec::new();
    let mut p = Vec::new();
    // Grup (cluster) di belakang.
    for (g, pl, seps) in inner {
        let gr = &d.groups[g];
        let (cx, cy) = o.pos[group_node[g]];
        let n = &nodes[group_node[g]];
        let r = [cx - n.w / 2.0, cy - n.h / 2.0, n.w, n.h];
        draw_group(&mut back, d, gr, r, size);
        // Gambar isi rata kiri di x = MARGIN, konten +6 di dalamnya.
        let (ox, oy) = (r[0] + 11.0, r[1] + group_top(d, gr, size));
        for y in seps {
            back.push(dashed(
                vec![(r[0] + 5.0, oy + y), (r[0] + r[2] - 5.0, oy + y)],
                Col::Ink,
                1.5,
                (8.0, 10.0),
            ));
        }
        for mut q in pl.prims {
            q.shift(ox, oy);
            p.push(q);
        }
    }
    let mut seps = Vec::new();
    for (gi, gr) in d.groups.iter().enumerate() {
        if Some(gi) == scope && !gr.regions.is_empty() {
            let base = region_base(d, gi);
            for k in 0..gr.regions.len() {
                let (ra, rb) = (o.clusters[base + k], o.clusters[base + k + 1]);
                seps.push((ra[1] + ra[3] + rb[1]) / 2.0);
            }
        }
        if !visible_cluster(gi) {
            continue;
        }
        let r = o.clusters[gi];
        draw_group(&mut back, d, gr, r, size);
        if !gr.regions.is_empty() {
            let base = region_base(d, gi);
            for k in 0..gr.regions.len() {
                let (ra, rb) = (o.clusters[base + k], o.clusters[base + k + 1]);
                let yy = if d.lr {
                    (ra[0] + ra[2] + rb[0]) / 2.0
                } else {
                    (ra[1] + ra[3] + rb[1]) / 2.0
                };
                if d.lr {
                    back.push(dashed(
                        vec![
                            (yy, r[1] + size[gr.title].1 + 16.0),
                            (yy, r[1] + r[3] - 5.0),
                        ],
                        Col::Ink,
                        1.5,
                        (8.0, 10.0),
                    ));
                } else {
                    back.push(dashed(
                        vec![(r[0] + 5.0, yy), (r[0] + r[2] - 5.0, yy)],
                        Col::Ink,
                        1.5,
                        (8.0, 10.0),
                    ));
                }
            }
        }
    }
    for (ni, &ek) in ent_of_node.iter().enumerate() {
        if ek == usize::MAX {
            continue;
        }
        let (cx, cy) = o.pos[ni];
        let n = &nodes[ni];
        draw_ent(
            &mut p,
            d,
            &d.ents[ek],
            [cx - n.w / 2.0, cy - n.h / 2.0, n.w, n.h],
            size,
        );
    }
    // Sisi.
    for (ei, &(lk, flipped, (ga, gb))) in edge_link.iter().enumerate() {
        let l = &d.links[lk];
        if l.hidden {
            continue;
        }
        let mut pts = o.edges[ei].clone();
        if pts.len() < 2 {
            continue;
        }
        // Ujung di state/komponen komposit: potong di tepi kotak cluster (lhead).
        if let Some(g) = ga {
            cut_at_box(&mut pts, o.clusters[g], true);
        }
        if let Some(g) = gb {
            cut_at_box(&mut pts, o.clusters[g], false);
        }
        if flipped {
            pts.reverse();
        }
        let c = l.color.unwrap_or(Col::Ink);
        // Pendekkan untuk dekorasi.
        let n = pts.len();
        let dir_end = (pts[n - 1].0 - pts[n - 2].0, pts[n - 1].1 - pts[n - 2].1);
        let dir_start = (pts[0].0 - pts[1].0, pts[0].1 - pts[1].1);
        let (tip_a, tip_b) = (pts[0], pts[n - 1]);
        shorten(&mut pts, l.d1.len(), true);
        shorten(&mut pts, l.d2.len(), false);
        let w = if l.bold { 2.0 } else { 1.0 };
        p.push(if l.dash {
            dashed(pts.clone(), c, w, (7.0, 7.0))
        } else if l.dot {
            dashed(pts.clone(), c, w, (1.0, 3.0))
        } else {
            line(pts.clone(), c, w)
        });
        decor(&mut p, l.d1, tip_a, dir_start, c);
        decor(&mut p, l.d2, tip_b, dir_end, c);
        if let (Some(t), Some(lp)) = (l.label, o.labels[ei]) {
            let (tw, th) = size[t];
            let mw = if l.mark != 0 { 12.0 } else { 0.0 };
            let x0 = lp.0 - (tw + mw) / 2.0;
            let tx = if l.mark < 0 { x0 + mw } else { x0 };
            p.push(Prim::Text(t, tx, lp.1 - th / 2.0));
            if l.mark != 0 {
                let mx = if l.mark > 0 { x0 + tw + 4.0 } else { x0 + 1.0 };
                let my = lp.1;
                let s = l.mark as f32;
                p.push(poly(
                    vec![
                        (mx + 4.0 + s * 4.0, my),
                        (mx + 4.0 - s * 4.0, my - 4.5),
                        (mx + 4.0 - s * 4.0, my + 4.5),
                    ],
                    Col::Text,
                    Col::Text,
                    1.0,
                ));
            }
        }
        // Label ujung dekat ujungnya, di sisi kiri garis.
        for (lt, tip, dir) in [(l.l1, tip_a, dir_start), (l.l2, tip_b, dir_end)] {
            if let Some(t) = lt {
                let (tw, th) = size[t];
                let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
                let (ux, uy) = (dir.0 / len, dir.1 / len);
                let back_pt = (tip.0 - ux * (th + 8.0), tip.1 - uy * (th + 8.0));
                let side = if uy.abs() > ux.abs() {
                    (-(tw / 2.0 + 8.0), 0.0)
                } else {
                    (0.0, -(th / 2.0 + 6.0))
                };
                p.push(Prim::Text(
                    t,
                    back_pt.0 + side.0 - tw / 2.0,
                    back_pt.1 + side.1 - th / 2.0,
                ));
            }
        }
    }
    let mut all = back;
    all.extend(p);
    let y0 = all
        .iter()
        .map(|q| q.bounds(size)[1])
        .fold(f32::MAX, f32::min);
    let seps = seps.into_iter().map(|y| y - y0 + margin).collect();
    (
        Placed {
            prims: all,
            w: o.w,
            h: o.h,
        }
        .fit(size, margin),
        seps,
    )
}

/// Cluster `c` adalah grup `g` atau salah satu region-nya.
fn in_group(d: &Desc, c: usize, g: usize) -> bool {
    if c == g {
        return true;
    }
    let gr = &d.groups[g];
    !gr.regions.is_empty()
        && (region_base(d, g)..region_base(d, g) + gr.regions.len() + 1).contains(&c)
}

/// Indeks cluster region pertama milik grup `g` (cluster region ditaruh setelah grup).
fn region_base(d: &Desc, g: usize) -> usize {
    d.groups.len()
        + d.groups[..g]
            .iter()
            .map(|x| x.regions.len() + 1)
            .sum::<usize>()
}

/// Cluster tempat entitas `e` (anggota grup `g`) ditata: region-nya bila ada.
fn region_of(d: &Desc, g: usize, e: usize) -> usize {
    let gr = &d.groups[g];
    if gr.regions.is_empty() {
        return g;
    }
    let k = gr.regions.iter().filter(|&&start| e >= start).count();
    region_base(d, g) + k
}

/// Buang bagian jalur di dalam kotak `r` dari ujung awal (`start`) atau akhir.
fn cut_at_box(pts: &mut Vec<Pt>, r: [f32; 4], start: bool) {
    let inside = |p: Pt| p.0 > r[0] && p.0 < r[0] + r[2] && p.1 > r[1] && p.1 < r[1] + r[3];
    if !start {
        pts.reverse();
    }
    let mut k = 0;
    while k + 1 < pts.len() && inside(pts[k + 1]) {
        k += 1;
    }
    if k + 1 < pts.len() && inside(pts[k]) {
        // Titik potong antara pts[k] (dalam) dan pts[k+1] (luar), bagi dua.
        let (mut a, mut b) = (pts[k], pts[k + 1]);
        for _ in 0..20 {
            let m = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            if inside(m) {
                a = m;
            } else {
                b = m;
            }
        }
        pts.drain(..k);
        pts[0] = b;
    }
    if !start {
        pts.reverse();
    }
}

fn shorten(pts: &mut Vec<Pt>, by: f32, start: bool) {
    if by <= 0.0 || pts.len() < 2 {
        return;
    }
    let mut left = by;
    loop {
        if pts.len() < 2 {
            return;
        }
        let (a, b) = if start {
            (pts[0], pts[1])
        } else {
            (pts[pts.len() - 1], pts[pts.len() - 2])
        };
        let l = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if l > left || pts.len() == 2 {
            let t = (left / l.max(1e-6)).min(0.95);
            let np = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if start {
                pts[0] = np;
            } else {
                let n = pts.len();
                pts[n - 1] = np;
            }
            return;
        }
        left -= l;
        if start {
            pts.remove(0);
        } else {
            pts.pop();
        }
    }
}

fn draw_group(p: &mut Vec<Prim>, d: &Desc, g: &Group, r: [f32; 4], size: &[(f32, f32)]) {
    let [x, y, w, h] = r;
    let (tw, th) = size[g.title];
    let fill = g.color.unwrap_or(Col::None);
    let lc = g
        .ent
        .and_then(|e| d.ents[e].line_color)
        .unwrap_or(Col::Text);
    let stereo = g.stereo;
    let sh = stereo.map_or(0.0, |s| size[s].1);
    match g.kind {
        Kind::Package | Kind::Folder => {
            let tab = tw + 8.0;
            let tab_h = th + sh + 6.0;
            p.push(poly(
                vec![
                    (x, y),
                    (x + tab, y),
                    (x + tab + 7.0, y + tab_h),
                    (x + w, y + tab_h),
                    (x + w, y + h),
                    (x, y + h),
                ],
                fill,
                lc,
                1.5,
            ));
            p.push(line(
                vec![(x, y + tab_h), (x + tab + 7.0, y + tab_h)],
                lc,
                1.5,
            ));
            if let Some(s) = stereo {
                p.push(Prim::Text(s, x + 4.0, y + 2.0));
            }
            p.push(Prim::Text(g.title, x + 4.0, y + 2.0 + sh));
        }
        Kind::Node => {
            node3d(p, r, fill, Col::Ink, 1.0);
            p.push(Prim::Text(g.title, x + (w - 10.0 - tw) / 2.0, y + 14.0));
        }
        Kind::Frame => {
            p.push(rect(r, 0.0, fill, lc, 1.5));
            let tab = tw + 20.0;
            p.push(line(
                vec![
                    (x + tab, y),
                    (x + tab, y + th - 2.0),
                    (x + tab - 8.0, y + th + 6.0),
                    (x, y + th + 6.0),
                ],
                lc,
                1.5,
            ));
            p.push(Prim::Text(g.title, x + 4.0, y + 3.0));
        }
        Kind::Cloud => {
            cloud(p, r, fill, Col::Ink);
            p.push(Prim::Text(g.title, x + (w - tw) / 2.0, y + 8.0));
        }
        Kind::Database => {
            cylinder(p, r, fill, Col::Ink);
            p.push(Prim::Text(g.title, x + (w - tw) / 2.0, y + 22.0));
        }
        Kind::State => {
            let hh = th + sh + 11.3;
            let f = g.color.unwrap_or(Col::Box);
            p.push(rect(r, 12.5, Col::Paper, Col::None, 0.0));
            // Kepala berisi warna, badan putih (komposit).
            p.push(Prim::Path {
                pts: head_path(x, y, w, hh + group_desc(d, g, size).0),
                closed: true,
                fill: f,
                stroke: Col::None,
                w: 0.0,
                dash: None,
            });
            p.push(rect(r, 12.5, Col::None, Col::Ink, 0.5));
            p.push(line(vec![(x, y + hh), (x + w, y + hh)], Col::Ink, 0.5));
            p.push(Prim::Text(g.title, x + (w - tw) / 2.0, y + 5.65));
            if let Some(e) = g.ent {
                let mut ty = y + hh + 2.0;
                for m in d.ents[e].comps.iter().flatten() {
                    p.push(Prim::Text(m.t, x + 5.0, ty));
                    ty += size[m.t].1;
                }
                let dh = group_desc(d, g, size).0;
                if dh > 0.0 {
                    p.push(line(
                        vec![(x, y + hh + dh), (x + w, y + hh + dh)],
                        Col::Ink,
                        0.5,
                    ));
                }
            }
        }
        _ => {
            p.push(rect(r, 2.5, fill, lc, 1.0));
            if let Some(s) = stereo {
                p.push(Prim::Text(s, x + (w - size[s].0) / 2.0, y + 2.0));
            }
            p.push(Prim::Text(g.title, x + (w - tw) / 2.0, y + 2.0 + sh));
        }
    }
}

/// Kepala state komposit: atas membulat (r 12,5), bawah siku.
fn head_path(x: f32, y: f32, w: f32, hh: f32) -> Vec<Pt> {
    let r = 12.5f32.min(hh);
    let mut v = vec![(x, y + hh), (x, y + r)];
    for i in 0..=6 {
        let a = (180.0 + 90.0 * i as f32 / 6.0f32).to_radians();
        v.push((x + r + r * a.cos(), y + r + r * a.sin()));
    }
    for i in 0..=6 {
        let a = (270.0 + 90.0 * i as f32 / 6.0f32).to_radians();
        v.push((x + w - r + r * a.cos(), y + r + r * a.sin()));
    }
    v.push((x + w, y + hh));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Desc> {
        parse_lines(&s.lines().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn class_relations() {
        let d = parse("abstract class Hewan {\n - nama : String\n + makan()\n}\ninterface I\nHewan <|-- Kucing\nI <|.. Kucing\nA \"1\" *-- \"many\" B : punya >\nK -up-> L\npackage P {\n class Q\n}\nnote right of Kucing : hai").unwrap();
        assert_eq!(d.flavor, Flavor::Class);
        let h = d.ents.iter().find(|e| e.id == "Hewan").unwrap();
        assert_eq!(h.kind, Kind::Abstract);
        assert_eq!(h.comps[0].len(), 1);
        assert_eq!(h.comps[1].len(), 1);
        assert_eq!(h.comps[0][0].vis, '-');
        assert_eq!(d.links[0].d1, Decor::Extends);
        assert!(d.links[1].dash);
        assert_eq!(d.links[2].d1, Decor::Composition);
        assert!(d.links[2].l1.is_some() && d.links[2].l2.is_some() && d.links[2].mark == 1);
        assert_eq!(d.links[3].dir, 'u');
        assert_eq!(d.groups.len(), 1);
        let pl = d.place(&vec![(40.0, 16.0); d.texts.0.len()]);
        assert!(pl.w > 0.0 && pl.h > 0.0);
    }

    #[test]
    fn usecase_component_state() {
        let u = parse("left to right direction\nactor P\nrectangle T {\n usecase \"Beli\" as U1\n (Bayar)\n}\nP --> U1\nU1 .> (Bayar) : <<include>>").unwrap();
        assert_eq!(u.flavor, Flavor::Desc);
        assert!(u.lr);
        assert!(u
            .ents
            .iter()
            .any(|e| e.kind == Kind::Usecase && e.id == "Bayar"));
        let c =
            parse("[Web] --> HTTP\nnode S {\n [API]\n database DB\n}\n[API] --> DB : SQL").unwrap();
        assert!(c
            .ents
            .iter()
            .any(|e| e.kind == Kind::Component && e.id == "API"));
        let s = parse(
            "[*] --> Diam\nDiam --> Jalan : mulai\nstate Jalan {\n [*] --> Pelan\n}\nJalan --> [*]",
        )
        .unwrap();
        assert_eq!(s.flavor, Flavor::State);
        assert!(s.ents.iter().any(|e| e.kind == Kind::End));
        let pl = s.place(&vec![(40.0, 16.0); s.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
