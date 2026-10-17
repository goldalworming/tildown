//! Sequence diagram PlantUML (juga `sequenceDiagram` mermaid).
//!
//! Konstanta mengikuti skin `rose` / `plantuml.skin` dan SVG keluaran PlantUML:
//! pesan 13 px, peserta 14 px, lifeline putus-putus 5,5, batang aktivasi 10,
//! langkah pesan = tinggi teks + 15, bingkai grup 16 di luar lifeline.
// ponytail: jarak kolom dengan satu lintasan batasan (bukan pemecah `Real`
// PlantUML), teoz/mainframe/newpage diabaikan.

use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Participant,
    Actor,
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Head {
    Filled,
    Open,
    Half,
    Cross,
    Circle,
    None,
}

pub struct Part {
    pub id: String,
    pub label: usize,
    pub stereo: Option<usize>,
    pub kind: Kind,
    pub color: Option<Col>,
    /// Indeks event `create` (kepala digambar di sana, bukan di atas).
    pub created: Option<usize>,
}

/// Ujung pesan: peserta atau tepi kiri/kanan (`[->`, `->]`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum End {
    P(usize),
    Left,
    Right,
}

#[derive(Debug)]
pub enum Ev {
    Msg {
        a: End,
        b: End,
        t: usize,
        num: Option<usize>,
        dotted: bool,
        head: Head,
        tail: Head,
        color: Option<Col>,
        act: bool,
        deact: bool,
        destroy: bool,
    },
    /// side: -1 kiri, 0 di atas (a..=b), 1 kanan; `on_msg` = menempel ke pesan sebelumnya.
    Note {
        a: usize,
        b: usize,
        side: i8,
        t: usize,
        shape: u8,
        color: Option<Col>,
        on_msg: bool,
        across: bool,
    },
    Act(usize, bool, Option<Col>),
    Destroy(usize),
    Group {
        kind: usize,
        label: usize,
        color: Option<Col>,
    },
    Else(usize),
    End,
    Ref {
        a: usize,
        b: usize,
        t: usize,
    },
    Sep(usize),
    Delay(usize),
    Space(f32),
    Create(usize),
}

pub struct Seq {
    pub texts: Texts,
    pub parts: Vec<Part>,
    pub evs: Vec<Ev>,
    pub foot: bool,
    /// `box`: (judul, warna, peserta pertama, peserta terakhir).
    pub boxes: Vec<(usize, Option<Col>, usize, usize)>,
}

impl Diagram for Seq {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        place(self, size)
    }
}

// ---------------------------------------------------------------------------
// Parse

pub fn parse(src: &str) -> Option<Seq> {
    let lines: Vec<String> = src.lines().map(String::from).collect();
    parse_lines(&lines)
}

pub fn parse_lines(lines: &[String]) -> Option<Seq> {
    let ls: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"))
        .collect();
    let mm = ls.first() == Some(&"sequenceDiagram");
    let mut s = Seq {
        texts: Texts::default(),
        parts: Vec::new(),
        evs: Vec::new(),
        foot: !mm,
        boxes: Vec::new(),
    };
    let mut num: Option<(u32, u32)> = None; // (berikut, langkah)
    let mut calls: Vec<(usize, usize)> = Vec::new();
    let mut open_box: Option<(usize, Option<Col>, usize)> = None;
    let mut depth = 0i32;
    let mut it = ls.into_iter().skip(mm as usize).peekable();
    let mut msgs = 0;
    while let Some(l) = it.next() {
        let low = l.to_lowercase();
        let w0 = low
            .split(|c: char| c.is_whitespace() || c == ':')
            .next()
            .unwrap_or("");
        let rest = l.get(w0.len()..).unwrap_or("").trim();
        match w0 {
            "hide" => s.foot &= !low.contains("footbox"),
            "show" | "autoactivate" | "mainframe" | "newpage" | "links" | "link" | "skinparam"
            | "!pragma" | "teoz" => {}
            "box" => {
                let (title, color) = title_color(rest);
                let t = s.texts.add(Txt::new(&unescape(&title), 13.0).bold());
                open_box = Some((t, color, s.parts.len()));
            }
            "end" if low.starts_with("end box") || (mm && open_box.is_some() && depth == 0) => {
                if let Some((t, c, first)) = open_box.take() {
                    if s.parts.len() > first {
                        s.boxes.push((t, c, first, s.parts.len() - 1));
                    }
                }
            }
            "autonumber" => {
                if low.contains("stop") {
                    num = None;
                } else {
                    let mut nums = rest
                        .split_whitespace()
                        .filter_map(|n| n.parse::<u32>().ok());
                    let start = nums.next().unwrap_or(1);
                    num = Some((start, nums.next().unwrap_or(1)));
                }
            }
            "participant" | "actor" | "boundary" | "control" | "entity" | "database"
            | "collections" | "queue" => {
                let kind = match w0 {
                    "actor" => Kind::Actor,
                    "boundary" => Kind::Boundary,
                    "control" => Kind::Control,
                    "entity" => Kind::Entity,
                    "database" => Kind::Database,
                    "collections" => Kind::Collections,
                    "queue" => Kind::Queue,
                    _ => Kind::Participant,
                };
                s.decl(kind, rest, mm);
            }
            "create" => {
                let r = rest.trim();
                let (kind, r) = match r.split_once(' ') {
                    Some((k, r2))
                        if [
                            "participant",
                            "actor",
                            "boundary",
                            "control",
                            "entity",
                            "database",
                            "collections",
                            "queue",
                        ]
                        .contains(&k.to_lowercase().as_str()) =>
                    {
                        (k.to_lowercase(), r2)
                    }
                    _ => ("participant".into(), r),
                };
                let kind = match kind.as_str() {
                    "actor" => Kind::Actor,
                    "database" => Kind::Database,
                    _ => Kind::Participant,
                };
                let i = s.decl(kind, r, mm);
                s.parts[i].created = Some(s.evs.len());
                s.evs.push(Ev::Create(i));
            }
            "activate" | "deactivate" | "destroy" => {
                let mut tok = rest.split_whitespace();
                let name = tok.next().unwrap_or("").trim_matches('"');
                let i = s.part(name);
                let color = tok.next().and_then(Col::parse);
                match w0 {
                    "activate" => {
                        if let Some(Ev::Msg {
                            a: End::P(a),
                            b: End::P(b),
                            ..
                        }) = s.evs.iter().rev().find(|e| matches!(e, Ev::Msg { .. }))
                        {
                            if *b == i {
                                calls.push((*a, i));
                            }
                        }
                        s.evs.push(Ev::Act(i, true, color));
                    }
                    "deactivate" => {
                        uncall(&mut calls, i);
                        s.evs.push(Ev::Act(i, false, None));
                    }
                    _ => s.evs.push(Ev::Destroy(i)),
                }
            }
            "note" | "rnote" | "hnote" => {
                let shape = match w0 {
                    "rnote" => 1,
                    "hnote" => 2,
                    _ => 0,
                };
                s.note(l, shape, &mut it)?;
            }
            "ref" => {
                // ref over A, B : teks   |  ref over A \n ... \n end ref
                let (spec, text) = match l.split_once(':') {
                    Some((a, b)) => (a, b.to_string()),
                    None => {
                        let mut v = Vec::new();
                        for x in it.by_ref() {
                            if x.to_lowercase().replace(' ', "") == "endref" {
                                break;
                            }
                            v.push(x);
                        }
                        (l, v.join("\n"))
                    }
                };
                let names: Vec<usize> = spec
                    .split_whitespace()
                    .skip(2)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split(',')
                    .map(|n| n.trim().trim_matches('"'))
                    .filter(|n| !n.is_empty())
                    .map(|n| s.part(n))
                    .collect();
                let (a, b) = (*names.iter().min()?, *names.iter().max()?);
                let t = s.texts.add(Txt::new(&unescape(&text), 12.0).center());
                s.evs.push(Ev::Ref { a, b, t });
            }
            "alt" | "opt" | "loop" | "par" | "par2" | "break" | "critical" | "group" | "rect"
            | "region" => {
                depth += 1;
                let (r, color) = title_color(rest);
                let (k, lb) = match w0 {
                    "group" => (r.clone(), String::new()),
                    "rect" | "region" => (String::new(), String::new()),
                    _ => (
                        w0.to_string(),
                        if r.is_empty() {
                            String::new()
                        } else {
                            format!("[{r}]")
                        },
                    ),
                };
                let kind = s.texts.add(Txt::new(&unescape(&k), 13.0).bold());
                let label = s.texts.add(Txt::new(&unescape(&lb), 11.0).bold());
                let color = if w0 == "rect" {
                    color.or(Col::parse(rest))
                } else {
                    color
                };
                s.evs.push(Ev::Group { kind, label, color });
            }
            "else" | "and" | "option" => {
                let t = if rest.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", rest)
                };
                let t = s.texts.add(Txt::new(&unescape(&t), 11.0).bold());
                s.evs.push(Ev::Else(t));
            }
            "end" => {
                depth -= 1;
                s.evs.push(Ev::End);
            }
            "return" => {
                let (c, d) = calls.pop()?;
                let t = s.texts.add(Txt::new(&unescape(rest), 13.0));
                let n = next_num(&mut num, &mut s.texts);
                s.evs.push(Ev::Msg {
                    a: End::P(d),
                    b: End::P(c),
                    t,
                    num: n,
                    dotted: true,
                    head: Head::Filled,
                    tail: Head::None,
                    color: None,
                    act: false,
                    deact: true,
                    destroy: false,
                });
            }
            _ if l.starts_with("==") => {
                let t = s
                    .texts
                    .add(Txt::new(&unescape(l.trim_matches('=')), 13.0).bold());
                s.evs.push(Ev::Sep(t));
            }
            _ if l.starts_with("...") => {
                let t = s.texts.add(Txt::new(&unescape(l.trim_matches('.')), 11.0));
                s.evs.push(Ev::Delay(t));
            }
            _ if l.starts_with("||") => {
                let n: f32 = l.trim_matches('|').trim().parse().unwrap_or(20.0);
                s.evs.push(Ev::Space(n));
            }
            _ if mm && (w0 == "title" || w0 == "accdescr" || w0 == "acctitle") => {}
            _ => {
                s.msg(l, mm, &mut num)?;
                msgs += 1;
                if let Some(&Ev::Msg {
                    a: End::P(a),
                    b: End::P(b),
                    act,
                    deact,
                    ..
                }) = s.evs.last()
                {
                    if deact {
                        uncall(&mut calls, a);
                    }
                    if act {
                        calls.push((a, b));
                    }
                }
            }
        }
    }
    (msgs > 0
        || !s.parts.is_empty()
            && s.evs
                .iter()
                .any(|e| matches!(e, Ev::Note { .. } | Ev::Ref { .. })))
    .then_some(s)
}

fn next_num(num: &mut Option<(u32, u32)>, texts: &mut Texts) -> Option<usize> {
    let (n, step) = (*num)?;
    *num = Some((n + step, step));
    Some(texts.add(Txt::new(&n.to_string(), 13.0).bold()))
}

/// `"Judul" #warna` → (judul, warna).
fn title_color(s: &str) -> (String, Option<Col>) {
    let mut color = None;
    let mut parts = Vec::new();
    for tok in split_keep_quotes(s) {
        if tok.starts_with('#') && color.is_none() {
            color = Col::parse(&tok);
            if color.is_some() {
                continue;
            }
        }
        if tok.starts_with("<<") {
            continue;
        }
        parts.push(tok);
    }
    (parts.join(" ").trim_matches('"').to_string(), color)
}

/// Pisah per spasi, tapi "kutipan berspasi" tetap satu token.
fn split_keep_quotes(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    for c in s.chars() {
        match c {
            '"' => {
                q = !q;
                cur.push(c);
            }
            c if c.is_whitespace() && !q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn uncall(calls: &mut Vec<(usize, usize)>, k: usize) {
    if let Some(j) = calls.iter().rposition(|c| c.1 == k) {
        calls.remove(j);
    }
}

/// Nama peserta di awal `s` (kutip atau kata); sisanya ikut dikembalikan.
fn name(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if let Some(r) = s.strip_prefix('"') {
        let k = r.find('"')?;
        return Some((&r[..k], &r[k + 1..]));
    }
    let k = s
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '@'))
        .unwrap_or(s.len());
    (k > 0).then(|| (&s[..k], &s[k..]))
}

/// Panjang panah di awal `s`: `->`, `-->>`, `<-`, `->x`, `-[#red]>`, `-)` (mermaid), ...
fn arrow_len(s: &str, mm: bool) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if !mm
        && matches!(b.first(), Some(b'o' | b'x'))
        && b.get(1).is_some_and(|c| matches!(c, b'-' | b'<'))
    {
        i = 1;
    }
    while i < b.len() {
        match b[i] {
            b'-' | b'<' | b'>' | b'/' | b'\\' | b')' => i += 1,
            b'[' if i > 0 => match s[i..].find(']') {
                Some(k) => i += k + 1,
                None => break,
            },
            b'x' | b'o'
                if i > 0
                    && if mm {
                        b[i] == b'x' && b[i - 1] == b'-'
                    } else {
                        !b.get(i + 1).is_some_and(u8::is_ascii_alphanumeric)
                    } =>
            {
                i += 1
            }
            _ => break,
        }
    }
    if s[..i].contains('-') {
        i
    } else {
        0
    }
}

impl Seq {
    fn part(&mut self, id: &str) -> usize {
        if let Some(i) = self.parts.iter().position(|p| p.id == id) {
            return i;
        }
        let label = self.texts.add(Txt::new(&unescape(id), 14.0));
        self.parts.push(Part {
            id: id.to_string(),
            label,
            stereo: None,
            kind: Kind::Participant,
            color: None,
            created: None,
        });
        self.parts.len() - 1
    }

    /// `participant "Nama panjang" as A #red <<stereo>>`. Tanpa kutip, PlantUML
    /// memakai sisi kanan `as` sebagai id, mermaid sisi kiri.
    fn decl(&mut self, kind: Kind, rest: &str, mm: bool) -> usize {
        let mut toks = split_keep_quotes(rest);
        let mut color = None;
        let mut stereo = None;
        toks.retain(|t| {
            if t.starts_with('#') {
                color = Col::parse(t).or(color);
                return color.is_none() && false;
            }
            true
        });
        let joined = toks.join(" ");
        let mut rest = joined.as_str();
        if let (Some(a), Some(b)) = (rest.find("<<"), rest.find(">>")) {
            if a < b {
                stereo = Some(rest[a + 2..b].trim().to_string());
            }
        }
        let owned;
        if let Some(a) = rest.find("<<") {
            owned = rest[..a].trim().to_string();
            rest = &owned;
        }
        let rest = rest.split(" order ").next().unwrap_or(rest);
        let (id, label) = match rest.split_once(" as ") {
            Some((l, r)) if l.trim().starts_with('"') || !mm && !r.trim().starts_with('"') => {
                (r, l)
            }
            Some((l, r)) => (l, r),
            None => (rest, rest),
        };
        let i = self.part(id.trim().trim_matches('"'));
        let label = self.texts.add(Txt::new(&unescape(label), 14.0));
        let p = &mut self.parts[i];
        p.label = label;
        p.kind = kind;
        p.color = color.or(p.color);
        if let Some(st) = stereo {
            let t = self
                .texts
                .add(Txt::new(&format!("«{}»", st), 12.0).italic());
            self.parts[i].stereo = Some(t);
        }
        i
    }

    fn note<'a>(
        &mut self,
        l: &str,
        shape: u8,
        it: &mut impl Iterator<Item = &'a str>,
    ) -> Option<()> {
        let (spec, text) = match l.split_once(':') {
            Some((sp, t)) => (sp, t.to_string()),
            None => {
                let mut v = Vec::new();
                for x in it.by_ref() {
                    if matches!(
                        x.to_lowercase().replace(' ', "").as_str(),
                        "endnote" | "endrnote" | "endhnote"
                    ) {
                        break;
                    }
                    v.push(x);
                }
                (l, v.join("\n"))
            }
        };
        let mut color = None;
        let w: Vec<&str> = spec
            .split_whitespace()
            .filter(|w| {
                if w.starts_with('#') {
                    color = Col::parse(w);
                    false
                } else {
                    true
                }
            })
            .collect();
        let pos = w.get(1).map(|x| x.to_lowercase()).unwrap_or_default();
        let (side, across) = match pos.as_str() {
            "left" => (-1, false),
            "right" => (1, false),
            "over" => (0, false),
            "across" => (0, true),
            _ => return None,
        };
        let skip = if w.get(2).is_some_and(|x| x.eq_ignore_ascii_case("of")) {
            3
        } else {
            2
        };
        let names = w[skip.min(w.len())..].join(" ");
        let mut ids: Vec<usize> = names
            .split(',')
            .map(|n| n.trim().trim_matches('"'))
            .filter(|n| !n.is_empty())
            .map(|n| self.part(n))
            .collect();
        let mut on_msg = false;
        if ids.is_empty() && !across {
            let Some(Ev::Msg { a, b, .. }) =
                self.evs.iter().rev().find(|e| matches!(e, Ev::Msg { .. }))
            else {
                return None;
            };
            let p = |e: &End| if let End::P(i) = e { Some(*i) } else { None };
            let (pa, pb) = (p(a).or(p(b))?, p(b).or(p(a))?);
            ids = vec![if side < 0 { pa.min(pb) } else { pa.max(pb) }];
            on_msg = true;
        }
        if across {
            ids = vec![0, self.parts.len().saturating_sub(1)];
        }
        let (a, b) = (*ids.iter().min()?, *ids.iter().max()?);
        let t = self.texts.add(Txt::new(&unescape(&text), 13.0));
        self.evs.push(Ev::Note {
            a,
            b,
            side,
            t,
            shape,
            color,
            on_msg,
            across,
        });
        Some(())
    }

    /// `A -> B ++ : teks`, `[-> A`, `A ->] : x`, `A o->x B`.
    fn msg(&mut self, l: &str, mm: bool, num: &mut Option<(u32, u32)>) -> Option<()> {
        let (head_s, text) = l.split_once(':').unwrap_or((l, ""));
        let h = head_s.trim_start();
        let (a, rest): (End, &str) = if let Some(r) = h.strip_prefix('[').or(h.strip_prefix('?')) {
            (End::Left, r)
        } else {
            let (n, r) = name(h)?;
            (End::P(self.part(n)), r)
        };
        let rest = rest.trim_start();
        let mut n = arrow_len(rest, mm);
        if n == 0 {
            return None;
        }
        if mm
            && n > 2
            && rest.as_bytes()[n - 1] == b'-'
            && matches!(rest.as_bytes()[n - 2], b'>' | b')' | b'x')
        {
            n -= 1;
        }
        let (arr, mut rest) = rest.split_at(n);
        let (mut act, mut deact, mut destroy) = (false, false, false);
        if mm {
            if let Some(r) = rest.trim_start().strip_prefix('+') {
                (act, rest) = (true, r);
            } else if let Some(r) = rest.trim_start().strip_prefix('-') {
                (deact, rest) = (true, r);
            }
        }
        let r = rest.trim_start();
        let (b, tail): (End, &str) = if let Some(t) = r.strip_prefix(']').or(r.strip_prefix('?')) {
            (End::Right, t)
        } else if r.is_empty() && arr.ends_with(']') {
            (End::Right, "")
        } else {
            let (nm, t) = name(r)?;
            (End::P(self.part(nm)), t)
        };
        let mut color = None;
        for tok in tail.split_whitespace() {
            match tok {
                "++" => act = true,
                "--" => deact = true,
                "**" => {}
                "!!" => destroy = true,
                "--++" | "++--" => {
                    act = true;
                    deact = true;
                }
                _ if tok.starts_with('#') => color = Col::parse(tok),
                _ => return None,
            }
        }
        let core: String = arr
            .split('[')
            .map(|p| match p.split_once(']') {
                Some((inner, after)) => {
                    if let Some(c) = inner.split(',').find_map(|x| Col::parse(x.trim())) {
                        color = Some(c);
                    }
                    after
                }
                None => p,
            })
            .collect();
        let left = core.contains('<');
        let right = core.contains('>')
            || core.contains(')')
            || core.contains('\\')
            || core.contains('/')
            || !left;
        let hd = |c: &str| -> Head {
            if c.contains('x') {
                Head::Cross
            } else if c.contains('o') {
                Head::Circle
            } else if mm {
                if c.contains(">>") {
                    Head::Filled
                } else if c.contains(')') {
                    Head::Open
                } else if c.contains('>') {
                    Head::None
                } else {
                    Head::None
                }
            } else if c.contains(">>") || c.contains("<<") {
                Head::Open
            } else if c.contains('\\') || c.contains('/') {
                Head::Half
            } else {
                Head::Filled
            }
        };
        // Ujung kepala: bagian setelah garis terakhir.
        let (pre, post) = match (core.find('-'), core.rfind('-')) {
            (Some(f), Some(e)) => (&core[..f], &core[e + 1..]),
            _ => ("", core.as_str()),
        };
        let (mut a, mut b) = (a, b);
        let (mut head, mut tail) = (
            hd(post),
            if pre.contains('o') {
                Head::Circle
            } else if pre.contains('x') {
                Head::Cross
            } else {
                Head::None
            },
        );
        if !right || left && !core.contains('>') {
            // Panah ke kiri: `A <- B` → B ke A.
            (a, b) = (b, a);
            head = hd(pre);
            tail = if post.contains('o') {
                Head::Circle
            } else if post.contains('x') {
                Head::Cross
            } else {
                Head::None
            };
        } else if left {
            tail = hd(pre);
        }
        let t = self.texts.add(Txt::new(&unescape(text.trim()), 13.0));
        let n = next_num(num, &mut self.texts);
        self.evs.push(Ev::Msg {
            a,
            b,
            t,
            num: n,
            dotted: core.contains("--"),
            head,
            tail,
            color,
            act,
            deact,
            destroy,
        });
        Some(())
    }
}

// ---------------------------------------------------------------------------
// Tata letak

const BAR: f32 = 10.0;

struct G {
    y0: f32,
    x0: f32,
    x1: f32,
    kind: usize,
    label: usize,
    color: Option<Col>,
    elses: Vec<(f32, usize)>,
}

fn grow(gs: &mut [G], x0: f32, x1: f32) {
    for g in gs {
        g.x0 = g.x0.min(x0);
        g.x1 = g.x1.max(x1);
    }
}

/// Lebar/tinggi kepala peserta (tanpa label untuk actor dkk., label di bawah).
/// Tinggi teks seperti PlantUML: ukuran font × 1,1654 per baris.
fn plh(texts: &[Txt], size: &[(f32, f32)], i: usize) -> f32 {
    if size[i].1 <= 0.0 {
        return 0.0;
    }
    texts[i].size * 1.1654 * texts[i].s.split('\n').count() as f32
}

fn head_dim(p: &Part, size: &[(f32, f32)], texts: &[Txt]) -> (f32, f32) {
    let (lw, lh) = (size[p.label].0, plh(texts, size, p.label));
    let sh = p.stereo.map_or(0.0, |s| plh(texts, size, s));
    match p.kind {
        Kind::Participant | Kind::Collections | Kind::Queue => (
            (lw.max(p.stereo.map_or(0.0, |s| size[s].0)) + 14.0),
            lh + 15.3
                + sh
                + if p.kind == Kind::Collections {
                    4.0
                } else {
                    0.0
                },
        ),
        Kind::Actor => (lw.max(26.0), 58.0 + lh + 1.0 + sh),
        Kind::Database => (lw.max(36.0), 46.0 + lh + 1.0 + sh),
        Kind::Boundary | Kind::Control | Kind::Entity => (lw.max(44.0), 28.0 + lh + 4.0 + sh),
    }
}

fn place(s: &Seq, size: &[(f32, f32)]) -> Placed {
    let n = s.parts.len();
    let sz = |i: usize| size[i];
    // Tinggi baris seperti PlantUML (ukuran font × 1,1654 per baris) untuk jarak
    // vertikal; metrik font kita sedikit lebih tinggi dan selisihnya menumpuk.
    let lh = |i: usize| plh(&s.texts.0, size, i);
    let dims: Vec<(f32, f32)> = s
        .parts
        .iter()
        .map(|p| head_dim(p, size, &s.texts.0))
        .collect();
    let hh = dims.iter().fold(0.0f32, |a, d| a.max(d.1));
    let msgw = |t: usize, num: Option<usize>| sz(t).0 + num.map_or(0.0, |k| sz(k).0 + 4.0);

    // Kolom: jarak minimum antarpeserta (lihat modul doc).
    let mut cons: Vec<(usize, usize, f32)> = (1..n)
        .map(|j| (j - 1, j, (dims[j - 1].0 + dims[j].0) / 2.0 + 10.0))
        .collect();
    let mut left_need = dims.first().map_or(0.0, |d| d.0 / 2.0);
    let mut right_need = dims.last().map_or(0.0, |d| d.0 / 2.0);
    for e in &s.evs {
        match *e {
            Ev::Msg {
                a: End::P(a),
                b: End::P(b),
                t,
                num,
                ..
            } if a != b => {
                let w = msgw(t, num);
                let d = if b > a { w + 34.0 } else { w + 50.0 };
                cons.push((a.min(b), a.max(b), d));
            }
            Ev::Msg {
                a: End::P(a),
                b: End::P(_),
                t,
                num,
                ..
            } => {
                let need = msgw(t, num) + 7.0 + BAR + 16.0;
                if a + 1 < n {
                    cons.push((a, a + 1, need + dims[a + 1].0 / 2.0));
                } else {
                    right_need = right_need.max(need);
                }
            }
            Ev::Msg {
                a: End::Left,
                b: End::P(b),
                t,
                num,
                ..
            }
            | Ev::Msg {
                a: End::P(b),
                b: End::Left,
                t,
                num,
                ..
            } if b == 0 => {
                left_need = left_need.max(msgw(t, num) + 30.0);
            }
            Ev::Msg {
                a: End::Right,
                b: End::P(a),
                t,
                num,
                ..
            }
            | Ev::Msg {
                a: End::P(a),
                b: End::Right,
                t,
                num,
                ..
            } if a + 1 == n => {
                right_need = right_need.max(msgw(t, num) + 30.0);
            }
            Ev::Note {
                a,
                b,
                side,
                t,
                across: false,
                ..
            } => {
                let w = sz(t).0 + 20.0;
                match side {
                    -1 if a > 0 => cons.push((a - 1, a, w + 15.0)),
                    -1 => left_need = left_need.max(w + 10.0),
                    1 if a + 1 < n => cons.push((a, a + 1, w + 15.0)),
                    1 => right_need = right_need.max(w + 10.0),
                    0 if a == b => {
                        if a > 0 {
                            cons.push((a - 1, a, w / 2.0 + 10.0));
                        } else {
                            left_need = left_need.max(w / 2.0);
                        }
                        if a + 1 < n {
                            cons.push((a, a + 1, w / 2.0 + 10.0));
                        } else {
                            right_need = right_need.max(w / 2.0);
                        }
                    }
                    0 => cons.push((a, b, w - 20.0)),
                    _ => {}
                }
            }
            Ev::Ref { a, b, t } if a < b => cons.push((a, b, sz(t).0 + 10.0)),
            _ => {}
        }
    }
    cons.sort_by_key(|c| c.1);
    let mut x = vec![0.0f32; n];
    for (i, j, d) in cons {
        let k = x[i] + d - x[j];
        if k > 0.0 {
            x[j..].iter_mut().for_each(|v| *v += k);
        }
    }
    let _ = left_need;
    let mut back = Vec::new(); // latar box, bingkai grup
    let mut mid = Vec::new(); // lifeline, batang aktivasi
    let mut p = Vec::new(); // pesan, catatan, kepala
    let box_title = s.boxes.iter().map(|b| sz(b.0).1).fold(0.0, f32::max);
    let top = if s.boxes.is_empty() {
        0.0
    } else {
        box_title + 4.0
    };
    let draw_head = |p: &mut Vec<Prim>, i: usize, y: f32, foot: bool| {
        let pa = &s.parts[i];
        let (w, h) = dims[i];
        let (lw, lh) = sz(pa.label);
        let cx = x[i];
        let fill = pa.color.unwrap_or(Col::Part);
        let sh = pa.stereo.map_or(0.0, |k| sz(k).1);
        match pa.kind {
            Kind::Participant | Kind::Collections | Kind::Queue => {
                if pa.kind == Kind::Collections {
                    p.push(rect(
                        [cx - w / 2.0 + 4.0, y, w, h - 4.0],
                        2.5,
                        fill,
                        Col::Ink,
                        0.5,
                    ));
                    p.push(rect(
                        [cx - w / 2.0, y + 4.0, w, h - 4.0],
                        2.5,
                        fill,
                        Col::Ink,
                        0.5,
                    ));
                } else {
                    p.push(rect([cx - w / 2.0, y, w, h], 2.5, fill, Col::Ink, 0.5));
                }
                let ty = y + if pa.kind == Kind::Collections {
                    4.0
                } else {
                    0.0
                };
                if let Some(k) = pa.stereo {
                    p.push(Prim::Text(k, cx - sz(k).0 / 2.0, ty + 7.0));
                }
                p.push(Prim::Text(pa.label, cx - lw / 2.0, ty + 7.0 + sh));
            }
            _ => {
                // Figur + label di bawah (kepala) atau di atas (kaki).
                let fig_h = h - lh - 1.0 - sh;
                let (fy, ly) = if foot {
                    (y + lh + 1.0 + sh, y)
                } else {
                    (y, y + fig_h + 1.0)
                };
                figure(p, pa.kind, cx, fy, fill);
                if let Some(k) = pa.stereo {
                    p.push(Prim::Text(k, cx - sz(k).0 / 2.0, ly));
                }
                p.push(Prim::Text(pa.label, cx - lw / 2.0, ly + sh));
            }
        }
    };
    for i in 0..n {
        if s.parts[i].created.is_none() {
            draw_head(&mut p, i, top + hh - dims[i].1, false);
        }
    }
    let life0 = top + hh + 1.0;
    let mut y = life0;
    let mut life_start: Vec<f32> = vec![life0; n];
    let mut life_end: Vec<Option<f32>> = vec![None; n];
    let mut stack: Vec<Vec<(f32, Option<Col>)>> = vec![Vec::new(); n];
    let off = |d: usize| {
        if d == 0 {
            0.0
        } else {
            BAR / 2.0 + (d - 1) as f32 * BAR / 2.0
        }
    };
    let close = |mid: &mut Vec<Prim>, st: &mut Vec<(f32, Option<Col>)>, k: usize, y1: f32| {
        if let Some((y0, c)) = st.pop() {
            let bx = x[k] - BAR / 2.0 + st.len() as f32 * BAR / 2.0;
            mid.push(rect(
                [bx, y0, BAR, (y1 - y0).max(8.0)],
                0.0,
                c.unwrap_or(Col::Paper),
                Col::Ink,
                1.0,
            ));
        }
    };
    let mut gs: Vec<G> = Vec::new();
    let mut seps: Vec<(f32, f32, usize, bool)> = Vec::new(); // (y, tinggi, teks, garis)
    let mut last = y;
    let mut last_msg: Option<(f32, f32, f32)> = None; // (x0, x1, y) pesan terakhir
    let (min_x, max_x) = (
        x.first().copied().unwrap_or(0.0)
            - left_need.max(dims.first().map_or(0.0, |d| d.0 / 2.0))
            - 10.0,
        x.last().copied().unwrap_or(0.0) + right_need + 10.0,
    );
    let ex = |e: End, other: f32| match e {
        End::P(i) => x[i],
        End::Left => min_x,
        End::Right => max_x.max(other + 40.0),
    };
    for (ei, e) in s.evs.iter().enumerate() {
        match *e {
            Ev::Create(i) => {
                // Kepala dibuat di tinggi pesan berikutnya.
                let _ = ei;
                let ny = y + sz(s.parts[i].label).1 + 15.2 - dims[i].1 / 2.0;
                draw_head(&mut p, i, ny, false);
                life_start[i] = ny + dims[i].1;
            }
            Ev::Msg {
                a,
                b,
                t,
                num,
                dotted,
                head: hd,
                tail,
                color,
                act,
                deact,
                destroy,
            } if a != b || !matches!(a, End::P(_)) => {
                let tw = sz(t).0;
                let th = lh(t).max(num.map_or(0.0, lh));
                let ya = y + th + 15.2;
                let pa = match a {
                    End::P(i) => Some(i),
                    _ => None,
                };
                let pb = match b {
                    End::P(i) => Some(i),
                    _ => None,
                };
                let xa0 = ex(a, pb.map_or(0.0, |k| x[k]));
                let xb0 = ex(b, xa0);
                let dir = if xb0 >= xa0 { 1.0 } else { -1.0 };
                if let (true, Some(bi)) = (act, pb) {
                    stack[bi].push((ya, None));
                }
                if let Some(bi) = pb {
                    if s.parts[bi].created.is_some() && life_start[bi] > ya {
                        life_start[bi] = ya;
                    }
                }
                let xa = xa0 + pa.map_or(0.0, |i| dir * off(stack[i].len()));
                let tip = xb0
                    - pb.map_or(0.0, |i| dir * off(stack[i].len()))
                    - dir * if dir > 0.0 { 1.0 } else { -1.0 };
                let c = color.unwrap_or(Col::Ink);
                let line_end = match hd {
                    Head::Filled => tip - dir * 4.0,
                    // Garis berhenti di pusat X.
                    Head::Cross => tip - dir * 11.0,
                    _ => tip,
                };
                let pts = vec![(xa, ya), (line_end, ya)];
                p.push(if dotted {
                    dashed(pts, c, 1.0, (2.0, 2.0))
                } else {
                    line(pts, c, 1.0)
                });
                end_mark(&mut p, hd, (tip, ya), dir, c);
                end_mark(&mut p, tail, (xa, ya), -dir, c);
                // Teks: kiri→kanan mulai 7 setelah asal; kanan→kiri 16 setelah ujung.
                let tx = if dir > 0.0 { xa + 7.0 } else { tip + 16.0 };
                let mut cx = tx;
                if let Some(k) = num {
                    p.push(Prim::Text(k, cx, ya - 1.0 - sz(k).1));
                    cx += sz(k).0 + 4.0;
                }
                p.push(Prim::Text(t, cx, ya - 1.0 - sz(t).1));
                grow(&mut gs, xa.min(tip), xa.max(tip).max(cx + tw));
                last_msg = Some((xa.min(tip), xa.max(tip), ya));
                if deact {
                    if let Some(ai) = pa {
                        close(&mut mid, &mut stack[ai], ai, ya);
                    }
                }
                if destroy {
                    if let Some(bi) = pb {
                        cross(&mut p, x[bi], ya);
                        life_end[bi] = Some(ya);
                    }
                }
                last = ya;
                y = ya;
            }
            Ev::Msg {
                a: End::P(a),
                t,
                num,
                dotted,
                head: hd,
                color,
                act,
                deact,
                ..
            } => {
                let tw = sz(t).0;
                let th = lh(t).max(num.map_or(0.0, lh));
                let y1 = y + th + 15.2;
                let xa = x[a] + off(stack[a].len());
                let y2 = y1 + 13.0;
                if act {
                    stack[a].push((y2, None));
                }
                let xb = x[a] + off(stack[a].len()) + 1.0;
                let c = color.unwrap_or(Col::Ink);
                let pts = vec![
                    (xa, y1),
                    (xa + 42.0, y1),
                    (xa + 42.0, y2),
                    (xb + if hd == Head::Filled { 4.0 } else { 0.0 }, y2),
                ];
                p.push(if dotted {
                    dashed(pts, c, 1.0, (2.0, 2.0))
                } else {
                    line(pts, c, 1.0)
                });
                end_mark(&mut p, hd, (xb, y2), -1.0, c);
                let mut cx = xa + 7.0;
                if let Some(k) = num {
                    p.push(Prim::Text(k, cx, y1 - 1.0 - sz(k).1));
                    cx += sz(k).0 + 4.0;
                }
                p.push(Prim::Text(t, cx, y1 - 1.0 - sz(t).1));
                grow(&mut gs, xa, (xa + 42.0).max(cx + tw));
                last_msg = Some((xa, xa + 42.0, y1));
                if deact {
                    close(&mut mid, &mut stack[a], a, y2);
                }
                last = y2;
                y = y2;
            }
            Ev::Msg { .. } => {}
            Ev::Note {
                a,
                b,
                side,
                t,
                shape,
                color,
                on_msg,
                across,
            } => {
                let (tw, th) = (sz(t).0, lh(t));
                let (w, h) = (tw + 20.0, th + 11.3);
                let ny = if on_msg {
                    last_msg.map_or(y, |m| m.2 - h / 2.0 - 4.0).max(y - h)
                } else {
                    // PlantUML: 13 di bawah garis pesan sebelumnya.
                    y + 13.0
                };
                let (x0, x1) = if across {
                    (min_x + 6.0, max_x - 6.0)
                } else {
                    match side {
                        -1 => {
                            let r = if on_msg {
                                last_msg.map_or(x[a], |m| m.0)
                            } else {
                                x[a] - off(stack[a].len())
                            } - 5.0;
                            (r - w, r)
                        }
                        1 => {
                            let l = if on_msg {
                                last_msg.map_or(x[a], |m| m.1)
                            } else {
                                x[a] + off(stack[a].len())
                            } + 5.0;
                            (l, l + w)
                        }
                        _ if a == b => (x[a] - w / 2.0, x[a] + w / 2.0),
                        _ => {
                            let l = x[a] - 15.0;
                            (l, (x[b] + 15.0).max(l + w))
                        }
                    }
                };
                let fill = color.unwrap_or(Col::Note);
                let r = [x0, ny, x1 - x0, h];
                match shape {
                    1 => p.push(rect(r, 0.0, fill, Col::Ink, 0.5)),
                    2 => p.push(poly(
                        vec![
                            (x0 + 5.0, ny),
                            (x1 - 5.0, ny),
                            (x1, ny + h / 2.0),
                            (x1 - 5.0, ny + h),
                            (x0 + 5.0, ny + h),
                            (x0, ny + h / 2.0),
                        ],
                        fill,
                        Col::Ink,
                        0.5,
                    )),
                    _ => p.extend(
                        super::activity::opale(r, None)
                            .into_iter()
                            .map(|q| recolor(q, fill)),
                    ),
                }
                p.push(Prim::Text(t, x0 + 6.0, ny + 5.65));
                grow(&mut gs, x0, x1);
                if !on_msg || ny + h > y {
                    y = (ny + h).max(y);
                }
            }
            Ev::Act(i, true, c) => stack[i].push((last, c)),
            Ev::Act(i, false, _) => close(&mut mid, &mut stack[i], i, last),
            Ev::Destroy(i) => {
                while !stack[i].is_empty() {
                    close(&mut mid, &mut stack[i], i, last);
                }
                cross(&mut p, x[i], last);
                life_end[i] = Some(last);
            }
            Ev::Group { kind, label, color } => {
                let gy = y + 12.0;
                gs.push(G {
                    y0: gy,
                    x0: f32::MAX,
                    x1: f32::MIN,
                    kind,
                    label,
                    color,
                    elses: Vec::new(),
                });
                y = gy + 18.35 + 2.0;
            }
            Ev::Else(t) => {
                let ey = y + 9.0;
                if let Some(g) = gs.last_mut() {
                    g.elses.push((ey, t));
                }
                y = ey + lh(t) - 2.0;
            }
            Ev::End => end(&mut gs, &mut back, &mut p, &mut y, size, &x),
            Ev::Ref { a, b, t } => {
                let (tw, th) = sz(t);
                let ry = y + 12.0;
                let (x0, x1) = (x[a] - 15.0, (x[b] + 15.0).max(x[a] - 15.0 + tw + 20.0));
                let h = th + 18.35 + 10.0;
                p.push(rect([x0, ry, x1 - x0, h], 0.0, Col::Paper, Col::Text, 1.5));
                let k = 31.0;
                p.push(poly(
                    vec![
                        (x0, ry),
                        (x0 + k, ry),
                        (x0 + k, ry + 8.35),
                        (x0 + k - 10.0, ry + 18.35),
                        (x0, ry + 18.35),
                    ],
                    Col::Group,
                    Col::Text,
                    2.0,
                ));
                // "ref" ditulis sebagai teks tab: pakai label kosong + teks t di tengah.
                p.push(Prim::Text(t, (x0 + x1 - tw) / 2.0, ry + 18.35 + 5.0));
                grow(&mut gs, x0, x1);
                y = ry + h;
            }
            Ev::Sep(t) => {
                let th = lh(t);
                let sy = y + 15.7;
                seps.push((sy, th + 9.2, t, true));
                y = sy + th + 9.2 + 2.0;
            }
            Ev::Delay(t) => {
                let th = lh(t);
                seps.push((y + 10.0, th, t, false));
                for i in 0..n {
                    mid.push(dashed(
                        vec![(x[i], y), (x[i], y + th + 20.0)],
                        Col::Ink,
                        0.5,
                        (1.0, 3.0),
                    ));
                }
                y += th + 20.0;
            }
            Ev::Space(h) => y += h,
        }
    }
    while !gs.is_empty() {
        end(&mut gs, &mut back, &mut p, &mut y, size, &x);
    }
    for (k, st) in stack.iter_mut().enumerate() {
        while !st.is_empty() {
            close(&mut mid, st, k, y + 5.0);
        }
    }
    let foot_y = y + 18.0;
    for i in 0..n {
        let y1 = life_end[i].unwrap_or(foot_y);
        mid.insert(
            0,
            dashed(
                vec![(x[i], life_start[i]), (x[i], y1)],
                Col::Ink,
                0.5,
                (5.0, 5.0),
            ),
        );
        if s.foot && life_end[i].is_none() {
            draw_head(&mut p, i, foot_y, true);
        }
    }
    let h = if s.foot { foot_y + hh } else { foot_y };
    // Box peserta: latar di belakang semuanya.
    for &(t, c, a, b) in &s.boxes {
        let x0 = x[a] - dims[a].0 / 2.0 - 5.0;
        let x1 = x[b] + dims[b].0 / 2.0 + 5.0;
        back.insert(
            0,
            rect(
                [x0, 0.0, x1 - x0, h + 4.0],
                0.0,
                c.unwrap_or(Col::Rgb(0xDDDDDD)),
                Col::Ink,
                0.1,
            ),
        );
        back.push(Prim::Text(t, (x0 + x1 - sz(t).0) / 2.0, 2.0));
    }
    let mut all = back;
    all.append(&mut mid);
    all.append(&mut p);
    // Pemisah & jeda: selebar diagram.
    let mut bb = [f32::MAX, f32::MIN];
    for q in &all {
        let r = q.bounds(size);
        bb = [bb[0].min(r[0]), bb[1].max(r[2])];
    }
    for (sy, sh, t, is_line) in seps {
        let (tw, th) = sz(t);
        let cx = (bb[0] + bb[1]) / 2.0;
        if is_line {
            let my = sy + sh / 2.0;
            all.push(rect(
                [bb[0], my - 1.5, bb[1] - bb[0], 3.0],
                0.0,
                Col::Group,
                Col::None,
                0.0,
            ));
            all.push(line(
                vec![(bb[0], my - 1.5), (bb[1], my - 1.5)],
                Col::Text,
                1.0,
            ));
            all.push(line(
                vec![(bb[0], my + 1.5), (bb[1], my + 1.5)],
                Col::Text,
                1.0,
            ));
            all.push(rect(
                [cx - tw / 2.0 - 6.0, sy, tw + 12.0, sh],
                0.0,
                Col::Group,
                Col::Text,
                2.0,
            ));
            all.push(Prim::Text(t, cx - tw / 2.0, sy + (sh - th) / 2.0));
        } else if tw > 0.0 {
            all.push(Prim::Text(t, cx - tw / 2.0, sy));
        }
    }
    Placed {
        prims: all,
        w: 0.0,
        h,
    }
    .fit(size, 10.0)
}

fn recolor(p: Prim, c: Col) -> Prim {
    match p {
        Prim::Path {
            pts,
            closed,
            fill,
            stroke,
            w,
            dash,
        } => Prim::Path {
            pts,
            closed,
            fill: if fill == Col::Note { c } else { fill },
            stroke,
            w,
            dash,
        },
        t => t,
    }
}

/// Tanda di ujung pesan.
fn end_mark(p: &mut Vec<Prim>, h: Head, tip: Pt, dir: f32, c: Col) {
    match h {
        Head::Filled => p.push(head(tip, (dir, 0.0), c)),
        Head::Open => p.push(open_head(tip, (dir, 0.0), c, 1.0)),
        Head::Half => p.push(line(vec![(tip.0 - dir * 10.0, tip.1 - 4.0), tip], c, 1.0)),
        // PlantUML: X 10×10 setebal 2, berpusat 11 sebelum ujung.
        Head::Cross => {
            let cx = tip.0 - dir * 11.0;
            p.push(line(
                vec![(cx - 5.0, tip.1 - 5.0), (cx + 5.0, tip.1 + 5.0)],
                c,
                2.0,
            ));
            p.push(line(
                vec![(cx - 5.0, tip.1 + 5.0), (cx + 5.0, tip.1 - 5.0)],
                c,
                2.0,
            ));
        }
        Head::Circle => p.push(ellipse(
            [tip.0 - 4.0 - dir * 4.0, tip.1 - 4.0, 8.0, 8.0],
            Col::Paper,
            c,
            1.0,
        )),
        Head::None => {}
    }
}

fn cross(p: &mut Vec<Prim>, x: f32, y: f32) {
    let c = Col::Rgb(0xA80036);
    p.push(line(vec![(x - 9.0, y - 9.0), (x + 9.0, y + 9.0)], c, 2.0));
    p.push(line(vec![(x - 9.0, y + 9.0), (x + 9.0, y - 9.0)], c, 2.0));
}

/// Figur peserta non-kotak dengan puncak di `y`.
fn figure(p: &mut Vec<Prim>, kind: Kind, cx: f32, y: f32, fill: Col) {
    match kind {
        Kind::Actor => {
            p.push(ellipse([cx - 8.0, y, 16.0, 16.0], fill, Col::Ink, 0.5));
            let (neck, arm, hip, foot) = (y + 16.0, y + 24.0, y + 43.0, y + 58.0);
            p.push(line(vec![(cx, neck), (cx, hip)], Col::Ink, 0.5));
            p.push(line(
                vec![(cx - 13.0, arm), (cx + 13.0, arm)],
                Col::Ink,
                0.5,
            ));
            p.push(line(
                vec![(cx - 13.0, foot), (cx, hip), (cx + 13.0, foot)],
                Col::Ink,
                0.5,
            ));
        }
        Kind::Database => {
            // Silinder 36 × 46 dengan elips atas.
            let (x0, x1) = (cx - 18.0, cx + 18.0);
            let mut pts = vec![(x0, y + 10.0)];
            vector::bezier(&mut pts, (x0, y + 10.0), (x0, y), (cx, y), (cx, y));
            vector::bezier(&mut pts, (cx, y), (x1, y), (x1, y + 10.0), (x1, y + 10.0));
            pts.push((x1, y + 36.0));
            vector::bezier(
                &mut pts,
                (x1, y + 36.0),
                (x1, y + 46.0),
                (cx, y + 46.0),
                (cx, y + 46.0),
            );
            vector::bezier(
                &mut pts,
                (cx, y + 46.0),
                (x0, y + 46.0),
                (x0, y + 36.0),
                (x0, y + 36.0),
            );
            p.push(poly(pts, fill, Col::Ink, 0.5));
            let mut arc = vec![(x0, y + 10.0)];
            vector::bezier(
                &mut arc,
                (x0, y + 10.0),
                (x0, y + 20.0),
                (cx, y + 20.0),
                (cx, y + 20.0),
            );
            vector::bezier(
                &mut arc,
                (cx, y + 20.0),
                (x1, y + 20.0),
                (x1, y + 10.0),
                (x1, y + 10.0),
            );
            p.push(line(arc, Col::Ink, 0.5));
        }
        Kind::Boundary => {
            p.push(line(
                vec![(cx - 22.0, y + 4.0), (cx - 22.0, y + 24.0)],
                Col::Ink,
                0.5,
            ));
            p.push(line(
                vec![(cx - 22.0, y + 14.0), (cx - 10.0, y + 14.0)],
                Col::Ink,
                0.5,
            ));
            p.push(ellipse(
                [cx - 10.0, y + 2.0, 24.0, 24.0],
                fill,
                Col::Ink,
                0.5,
            ));
        }
        Kind::Control => {
            p.push(ellipse(
                [cx - 12.0, y + 4.0, 24.0, 24.0],
                fill,
                Col::Ink,
                0.5,
            ));
            p.push(open_head((cx - 2.0, y + 4.0), (-1.0, 0.0), Col::Ink, 0.5));
        }
        Kind::Entity => {
            p.push(ellipse(
                [cx - 12.0, y + 2.0, 24.0, 24.0],
                fill,
                Col::Ink,
                0.5,
            ));
            p.push(line(
                vec![(cx - 12.0, y + 28.0), (cx + 12.0, y + 28.0)],
                Col::Ink,
                0.5,
            ));
        }
        _ => {}
    }
}

fn end(
    gs: &mut Vec<G>,
    back: &mut Vec<Prim>,
    p: &mut Vec<Prim>,
    y: &mut f32,
    size: &[(f32, f32)],
    x: &[f32],
) {
    let Some(mut g) = gs.pop() else { return };
    if g.x0 > g.x1 {
        (g.x0, g.x1) = (
            x.first().copied().unwrap_or(0.0),
            x.last().copied().unwrap_or(0.0),
        );
    }
    let (kw, _) = size[g.kind];
    let x0 = g.x0 - 16.0 + 0.0;
    let x0 = x0.min(g.x0 - 16.0);
    let tab = if kw > 0.0 { kw + 45.0 } else { 0.0 };
    let x1 = (g.x1 + 16.0).max(x0 + tab + size[g.label].0 + 30.0);
    let y1 = *y + 8.0;
    if let Some(c) = g.color {
        back.push(rect([x0, g.y0, x1 - x0, y1 - g.y0], 0.0, c, Col::None, 0.0));
    }
    back.push(rect(
        [x0, g.y0, x1 - x0, y1 - g.y0],
        0.0,
        Col::None,
        Col::Text,
        1.5,
    ));
    if tab > 0.0 {
        let (t0, t1) = (g.y0, g.y0 + 18.35);
        p.push(poly(
            vec![
                (x0, t0),
                (x0 + tab, t0),
                (x0 + tab, t0 + 8.35),
                (x0 + tab - 10.0, t1),
                (x0, t1),
            ],
            Col::Group,
            Col::Text,
            1.5,
        ));
        p.push(Prim::Text(g.kind, x0 + 15.0, g.y0 + 1.5));
    }
    p.push(Prim::Text(g.label, x0 + tab + 15.0, g.y0 + 2.0));
    for (ey, t) in g.elses {
        back.push(dashed(vec![(x0, ey), (x1, ey)], Col::Text, 1.0, (2.0, 2.0)));
        p.push(Prim::Text(t, x0 + 5.0, ey + 2.0));
    }
    grow(gs, x0, x1);
    // PlantUML: elemen berikutnya dihitung dari dasar fragmen + 6.
    *y = y1 + 6.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plantuml() {
        let s = parse("actor User as U\nparticipant \"Web App\" as W\nU -> W ++ : login\nW --> U -- : ok\nalt sukses\n  W -> W : simpan\nelse gagal\n  W ->x U\nend\nnote over U, W : catatan\n== selesai ==\n[-> U : masuk").unwrap();
        assert_eq!(s.parts.len(), 2);
        assert_eq!(s.parts[0].kind, Kind::Actor);
        assert_eq!(s.texts.0[s.parts[1].label].s, "Web App");
        assert!(matches!(
            s.evs[0],
            Ev::Msg {
                a: End::P(0),
                b: End::P(1),
                act: true,
                dotted: false,
                head: Head::Filled,
                ..
            }
        ));
        assert!(matches!(
            s.evs[1],
            Ev::Msg {
                a: End::P(1),
                b: End::P(0),
                deact: true,
                dotted: true,
                ..
            }
        ));
        assert!(matches!(
            s.evs[5],
            Ev::Msg {
                head: Head::Cross,
                ..
            }
        ));
        assert!(matches!(
            s.evs[7],
            Ev::Note {
                a: 0,
                b: 1,
                side: 0,
                ..
            }
        ));
        assert!(matches!(s.evs[9], Ev::Msg { a: End::Left, .. }));
        let pl = place(&s, &vec![(40.0, 16.0); s.texts.0.len()]);
        assert!(pl.w > 0.0 && pl.h > 0.0);
        assert!(parse("class Foo {\n}").is_none());
    }

    #[test]
    fn parse_mermaid() {
        let s = parse("sequenceDiagram\nAlice->>+John: Hello\nJohn-->>-Alice: Hi\nAlice-)John: async\nAlice-xJohn: x").unwrap();
        assert!(matches!(
            s.evs[0],
            Ev::Msg {
                a: End::P(0),
                b: End::P(1),
                act: true,
                head: Head::Filled,
                ..
            }
        ));
        assert!(matches!(
            s.evs[1],
            Ev::Msg {
                a: End::P(1),
                b: End::P(0),
                deact: true,
                dotted: true,
                ..
            }
        ));
        assert!(matches!(
            s.evs[2],
            Ev::Msg {
                head: Head::Open,
                ..
            }
        ));
        assert!(matches!(
            s.evs[3],
            Ev::Msg {
                head: Head::Cross,
                ..
            }
        ));
    }
}
