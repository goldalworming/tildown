//! Activity diagram PlantUML (sintaks baru, `activitydiagram3`).
//!
//! Tata letak mengikuti `activitydiagram3/ftile`: tiap pernyataan menjadi tile
//! dengan titik masuk/keluar di x = `ax`; tile disusun vertikal dengan panah 20 px
//! (FtileAssemblySimple). Konstanta diambil dari kode dan SVG keluaran PlantUML.
// ponytail: tanpa goto/label, konektor `(A)`, `floating note`, swimlane di dalam
// cabang (lane mengikuti pernyataan tingkat atas), penggabungan garis (Snake merge).

use super::*;

const HEX: f32 = 12.0; // Hexagon.hexagonHalfSize
const ARROW: f32 = 20.0;
const LINE: Col = Col::Ink;

#[derive(Clone, Copy, PartialEq)]
enum BoxStyle {
    Plain,
    Input,
    Output,
    Procedure,
    Load,
    Save,
    Continuous,
    Task,
}

/// Gaya panah berikutnya: `-[#red,dashed]-> label;`.
#[derive(Clone, Copy, Default)]
struct ArrowStyle {
    label: Option<usize>,
    color: Option<Col>,
    dash: bool,
    hidden: bool,
}

enum St {
    Act {
        t: usize,
        color: Option<Col>,
        style: BoxStyle,
    },
    Start,
    Stop,
    End,
    Kill,
    Break,
    Arrow(ArrowStyle),
    Note {
        t: usize,
        left: bool,
    },
    /// Cabang if: (kondisi, label, isi); `els` = (label, isi).
    If {
        conds: Vec<(usize, usize, Vec<St>)>,
        el: usize,
        els: Vec<St>,
    },
    While {
        cond: usize,
        is: usize,
        body: Vec<St>,
        out: usize,
        backward: Option<usize>,
    },
    Repeat {
        start: Option<usize>,
        body: Vec<St>,
        cond: usize,
        is: usize,
        not: usize,
        backward: Option<usize>,
    },
    Fork {
        branches: Vec<Vec<St>>,
        merge: bool,
    },
    Split {
        branches: Vec<Vec<St>>,
    },
    Switch {
        cond: usize,
        cases: Vec<(usize, Vec<St>)>,
    },
    Group {
        title: usize,
        body: Vec<St>,
        color: Option<Col>,
    },
    Lane(usize),
}

pub struct Activity {
    texts: Texts,
    body: Vec<St>,
    /// Swimlane: (judul, warna).
    lanes: Vec<(usize, Option<Col>)>,
}

impl Diagram for Activity {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        if self.lanes.is_empty() {
            let t = seq(&self.body, size);
            return Placed {
                prims: t.prims,
                w: t.w,
                h: t.h,
            }
            .fit(size, 10.0);
        }
        place_lanes(self, size)
    }
}

// ---------------------------------------------------------------------------
// Parse

/// Isi kurung pertama `(x)` dan sisa sesudahnya.
fn paren(s: &str) -> (&str, &str) {
    let Some(a) = s.find('(') else { return ("", s) };
    let mut d = 0;
    for (i, c) in s[a..].char_indices() {
        match c {
            '(' => d += 1,
            ')' => {
                d -= 1;
                if d == 0 {
                    return (&s[a + 1..a + i], &s[a + i + 1..]);
                }
            }
            _ => {}
        }
    }
    (&s[a + 1..], "")
}

/// `(teks)` setelah kata kunci `kw` di `s`, mis. `is (ya)`.
fn after(s: &str, kw: &str) -> String {
    let low = s.to_lowercase();
    match low.find(kw) {
        Some(i) => paren(&s[i + kw.len()..]).0.to_string(),
        None => String::new(),
    }
}

pub fn parse_lines(lines: &[String]) -> Option<Activity> {
    let ls: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut a = Activity {
        texts: Texts::default(),
        body: Vec::new(),
        lanes: Vec::new(),
    };
    let mut i = 0;
    let (body, end) = a.block(&ls, &mut i)?;
    if !end.is_empty() || i < ls.len() {
        return None;
    }
    let ok = |s: &St| {
        matches!(
            s,
            St::Act { .. }
                | St::If { .. }
                | St::While { .. }
                | St::Repeat { .. }
                | St::Fork { .. }
                | St::Split { .. }
                | St::Switch { .. }
                | St::Start
                | St::Group { .. }
        )
    };
    if !body.iter().any(ok) {
        return None;
    }
    a.body = body;
    Some(a)
}

const ENDS: &[&str] = &[
    "else",
    "elseif",
    "endif",
    "endwhile",
    "end",
    "fork",
    "split",
    "case",
    "endswitch",
    "repeat",
    "}",
    "backward",
    "detach",
];

impl Activity {
    fn label(&mut self, s: &str) -> usize {
        self.texts.src(s, 11.0)
    }

    /// Baca pernyataan sampai kata penutup; kembalikan baris penutupnya.
    fn block<'a>(&mut self, ls: &[&'a str], i: &mut usize) -> Option<(Vec<St>, &'a str)> {
        let mut out = Vec::new();
        while *i < ls.len() {
            let l = ls[*i];
            *i += 1;
            let low = l.to_lowercase();
            let w0 = low
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or("");
            // `#warna:aksi;` (sintaks lama) atau `:aksi;`
            if l.starts_with(':')
                || l.starts_with('#') && l.split_once(':').is_some_and(|(c, _)| !c.contains(' '))
            {
                let (color, body) = if l.starts_with('#') {
                    let k = l.find(':').unwrap();
                    (Col::parse(&l[..k]), &l[k + 1..])
                } else {
                    (None, &l[1..])
                };
                let mut t = body.to_string();
                while !ends_action(&t) {
                    t.push('\n');
                    t.push_str(ls.get(*i)?);
                    *i += 1;
                }
                let (t, stereo) = split_action(&t);
                let style = match stereo.as_str() {
                    "input" => BoxStyle::Input,
                    "output" => BoxStyle::Output,
                    "procedure" => BoxStyle::Procedure,
                    "load" => BoxStyle::Load,
                    "save" => BoxStyle::Save,
                    "continuous" => BoxStyle::Continuous,
                    "task" => BoxStyle::Task,
                    _ => BoxStyle::Plain,
                };
                let color = color.or_else(|| stereo_color(&stereo));
                let t = self.texts.src(&t, 12.0);
                out.push(St::Act { t, color, style });
                continue;
            }
            if l.starts_with('|') {
                // |Lane| atau |#warna|Lane|
                let parts: Vec<&str> = l.trim_matches('|').split('|').collect();
                let (color, name) = if parts.len() >= 2 {
                    (Col::parse(parts[0]), parts[1])
                } else {
                    (None, parts[0])
                };
                let name = name.trim();
                let idx = match self
                    .lanes
                    .iter()
                    .position(|(t, _)| self.texts.0[*t].s == name)
                {
                    Some(k) => {
                        if color.is_some() {
                            self.lanes[k].1 = color;
                        }
                        k
                    }
                    None => {
                        let t = self.texts.add(Txt::new(name, 18.0));
                        self.lanes.push((t, color));
                        self.lanes.len() - 1
                    }
                };
                out.push(St::Lane(idx));
                continue;
            }
            if l.starts_with('-') && (l.contains("->") || l.contains("-[")) {
                // -> label;   -[#red,dashed]-> label;
                let mut st = ArrowStyle::default();
                if let (Some(a), Some(b)) = (l.find('['), l.find(']')) {
                    for p in l[a + 1..b].split(',') {
                        let p = p.trim();
                        match p.to_lowercase().as_str() {
                            "dashed" | "dotted" => st.dash = true,
                            "hidden" => st.hidden = true,
                            _ => {
                                if let Some(c) = Col::parse(p) {
                                    st.color = Some(c);
                                }
                            }
                        }
                    }
                }
                let rest = l[l.find('>').map_or(l.len(), |k| k + 1)..]
                    .trim()
                    .trim_end_matches(';')
                    .trim();
                if !rest.is_empty() {
                    st.label = Some(self.label(rest));
                }
                out.push(St::Arrow(st));
                continue;
            }
            match w0 {
                _ if low.starts_with("fork again") || low.starts_with("split again") => {
                    return Some((out, l))
                }
                "start" => out.push(St::Start),
                "stop" => out.push(St::Stop),
                "end" if low == "end" => out.push(St::End),
                "kill" | "detach" => out.push(St::Kill),
                "break" => out.push(St::Break),
                "note" | "floating" => {
                    let left = low.contains("left");
                    let text = match l.split_once(':') {
                        Some((_, t)) => t.to_string(),
                        None => {
                            let mut v = Vec::new();
                            while *i < ls.len() {
                                let x = ls[*i];
                                *i += 1;
                                if matches!(
                                    x.to_lowercase().replace(' ', "").as_str(),
                                    "endnote" | "endrnote" | "endhnote"
                                ) {
                                    break;
                                }
                                v.push(x);
                            }
                            v.join("\n")
                        }
                    };
                    let t = self.texts.src(&text, 13.0);
                    out.push(St::Note { t, left });
                }
                "if" => out.push(self.parse_if(l, ls, i)?),
                "while" => {
                    let (c, rest) = paren(&l[5..]);
                    let cond = self.label(c);
                    let is = self.label(&after(rest, "is"));
                    let (body, mut end) = self.block(ls, i)?;
                    let mut backward = None;
                    if end.to_lowercase().starts_with("backward") {
                        backward = Some(self.backward(end, ls, i)?);
                        let (more, e) = self.block(ls, i)?;
                        if !more.is_empty() {
                            return None;
                        }
                        end = e;
                    }
                    let e = end.to_lowercase();
                    if !(e.starts_with("endwhile") || e.starts_with("end while")) {
                        return None;
                    }
                    let out_l = self.label(paren(end).0);
                    out.push(St::While {
                        cond,
                        is,
                        body,
                        out: out_l,
                        backward,
                    });
                }
                "repeat" if !low.starts_with("repeat while") && !low.starts_with("repeatwhile") => {
                    let start = l
                        .split_once(':')
                        .map(|(_, t)| self.texts.src(t.trim_end_matches(';'), 12.0));
                    let (body, mut end) = self.block(ls, i)?;
                    let mut backward = None;
                    if end.to_lowercase().starts_with("backward") {
                        backward = Some(self.backward(end, ls, i)?);
                        let (more, e) = self.block(ls, i)?;
                        if !more.is_empty() {
                            return None;
                        }
                        end = e;
                    }
                    let e = end.to_lowercase();
                    if !e.starts_with("repeat") {
                        return None;
                    }
                    let (c, rest) = paren(end);
                    let cond = self.label(c);
                    let is = self.label(&after(rest, "is"));
                    let not = self.label(&after(rest, "not"));
                    out.push(St::Repeat {
                        start,
                        body,
                        cond,
                        is,
                        not,
                        backward,
                    });
                }
                "fork" | "split" => {
                    let fork = w0 == "fork";
                    let mut branches = Vec::new();
                    let merge;
                    loop {
                        let (b, end) = self.block(ls, i)?;
                        branches.push(b);
                        let e = end.to_lowercase();
                        if e.starts_with("fork again") || e.starts_with("split again") {
                            continue;
                        }
                        if e.starts_with("end fork")
                            || e.starts_with("end split")
                            || e.starts_with("end merge")
                        {
                            merge = e.starts_with("end merge");
                            break;
                        }
                        return None;
                    }
                    out.push(if fork {
                        St::Fork { branches, merge }
                    } else {
                        St::Split { branches }
                    });
                }
                "switch" => {
                    let cond = self.label(paren(&l[6..]).0);
                    let (pre, mut end) = self.block(ls, i)?;
                    if !pre.is_empty() {
                        return None;
                    }
                    let mut cases = Vec::new();
                    while end.to_lowercase().starts_with("case") {
                        let lb = self.label(paren(end).0);
                        let (b, e) = self.block(ls, i)?;
                        cases.push((lb, b));
                        end = e;
                    }
                    if !end.to_lowercase().starts_with("endswitch") {
                        return None;
                    }
                    out.push(St::Switch { cond, cases });
                }
                "partition" | "group" | "package" | "rectangle" | "card" | "folder" | "frame"
                | "cloud" | "database" | "node" => {
                    let rest = l[w0.len()..].trim().trim_end_matches('{').trim();
                    let mut color = None;
                    let mut name = rest.to_string();
                    if let Some(k) = rest.rfind(" #").or(rest.starts_with('#').then_some(0)) {
                        // `partition Nama #warna` / `partition #warna Nama`
                        let (a, b) = rest.split_at(k);
                        let b = b.trim();
                        let tok = b.split_whitespace().next().unwrap_or("");
                        if let Some(c) = Col::parse(tok) {
                            color = Some(c);
                            name = format!("{} {}", a.trim(), b[tok.len()..].trim())
                                .trim()
                                .to_string();
                        }
                    }
                    let title = self.texts.add(Txt::new(&unescape(&name), 14.0));
                    let (body, end) = self.block(ls, i)?;
                    let e = end.to_lowercase();
                    if !(e == "}"
                        || e.starts_with("end group")
                        || e.starts_with("end partition")
                        || e.starts_with("end"))
                    {
                        return None;
                    }
                    out.push(St::Group { title, body, color });
                }
                _ if l == "}" => return Some((out, l)),
                _ if ENDS.contains(&w0) => return Some((out, l)),
                _ if low.starts_with("end ")
                    || low.starts_with("end fork")
                    || low.starts_with("end split")
                    || low.starts_with("end merge")
                    || low.starts_with("end group") =>
                {
                    return Some((out, l))
                }
                _ if low.starts_with("hide ")
                    || low.starts_with("show ")
                    || low.starts_with("left to right")
                    || low.starts_with("top to bottom")
                    || low.starts_with("skinparam") => {}
                _ => return None,
            }
        }
        Some((out, ""))
    }

    fn backward(&mut self, l: &str, ls: &[&str], i: &mut usize) -> Option<usize> {
        let mut t = l.split_once(':')?.1.to_string();
        while !ends_action(&t) {
            t.push('\n');
            t.push_str(ls.get(*i)?);
            *i += 1;
        }
        let (t, _) = split_action(&t);
        Some(self.texts.src(&t, 12.0))
    }

    /// `if (c) then (l)`; `elseif` menambah cabang berantai.
    fn parse_if(&mut self, l: &str, ls: &[&str], i: &mut usize) -> Option<St> {
        let mut conds = Vec::new();
        let mut head = l;
        loop {
            let k = head.find('(')?;
            let (c, rest) = paren(&head[k..]);
            let low = rest.to_lowercase();
            // `if (a) is (b) then` / `if (a) equals (b) then`
            let c = if low.trim_start().starts_with("is") || low.trim_start().starts_with("equals")
            {
                format!("{} {}", c, paren(rest).0)
            } else {
                c.to_string()
            };
            let cond = self.label(&c);
            let tl = self.label(&after(rest, "then"));
            let (body, end) = self.block(ls, i)?;
            conds.push((cond, tl, body));
            let e = end.to_lowercase();
            if e.starts_with("elseif") || e.starts_with("else if") {
                head = end;
                continue;
            }
            if e.starts_with("else") {
                let el = self.label(paren(end).0);
                let (els, end2) = self.block(ls, i)?;
                let e2 = end2.to_lowercase();
                if !(e2.starts_with("endif") || e2.starts_with("end if")) {
                    return None;
                }
                return Some(St::If { conds, el, els });
            }
            if e.starts_with("endif") || e.starts_with("end if") {
                let el = self.label("");
                return Some(St::If {
                    conds,
                    el,
                    els: Vec::new(),
                });
            }
            return None;
        }
    }
}

fn ends_action(t: &str) -> bool {
    let t = t.trim_end();
    let t = match t.rfind("<<") {
        Some(k) if t.ends_with(">>") => t[..k].trim_end(),
        _ => t,
    };
    t.ends_with(';')
}

/// Teks aksi tanpa `;` dan stereotip `<<x>>` di belakang.
fn split_action(t: &str) -> (String, String) {
    let t = t.trim_end();
    let (t, stereo) = match t.rfind("<<") {
        Some(k) if t.ends_with(">>") => (
            t[..k].trim_end(),
            t[k + 2..t.len() - 2].trim().to_lowercase(),
        ),
        _ => (t, String::new()),
    };
    (t.strip_suffix(';').unwrap_or(t).to_string(), stereo)
}

fn stereo_color(s: &str) -> Option<Col> {
    s.strip_prefix('#').and_then(Col::parse)
}

// ---------------------------------------------------------------------------
// Tile

/// Blok tersusun: masuk dan keluar di x = `ax` (atas/bawah); `open` = ada jalur keluar.
struct Tile {
    w: f32,
    h: f32,
    ax: f32,
    open: bool,
    /// Panjang panah masuk dari tile sebelumnya (20; partition 10).
    pre: f32,
    /// Bentuk (digambar dulu) dan garis/teks (sesudahnya).
    back: Vec<Prim>,
    prims: Vec<Prim>,
    /// Titik `break` yang menunggu disambung ke jalur keluar while.
    brk: Vec<Pt>,
}

impl Tile {
    fn empty() -> Tile {
        Tile {
            w: 0.0,
            h: 0.0,
            ax: 0.0,
            open: true,
            pre: ARROW,
            back: Vec::new(),
            prims: Vec::new(),
            brk: Vec::new(),
        }
    }

    fn put(&mut self, t: Tile, dx: f32, dy: f32) {
        let mv = |mut p: Prim| {
            p.shift(dx, dy);
            p
        };
        self.back.extend(t.back.into_iter().map(mv));
        self.prims.extend(t.prims.into_iter().map(mv));
        self.brk
            .extend(t.brk.into_iter().map(|(x, y)| (x + dx, y + dy)));
    }

    /// Lebar minimum `min` di tengah + margin kiri-kanan `m` (cabang if/fork).
    fn branch(self, min: f32, m: f32) -> Tile {
        let w = self.w.max(min);
        let dx = m + (w - self.w) / 2.0;
        let mut o = Tile {
            w: w + 2.0 * m,
            h: self.h,
            ax: self.ax + dx,
            open: self.open,
            ..Tile::empty()
        };
        o.put(self, dx, 0.0);
        o
    }

    fn finish(mut self) -> Tile {
        let mut all = std::mem::take(&mut self.back);
        all.append(&mut self.prims);
        self.prims = all;
        self
    }
}

fn ln(p: &mut Vec<Prim>, pts: &[Pt]) {
    p.push(line(pts.to_vec(), LINE, 1.0));
}

fn down(p: &mut Vec<Prim>, x: f32, y: f32) {
    p.push(head((x, y), (0.0, 1.0), LINE));
}

/// Panah penanda arah di tengah ruas tegak (`emphasizeDirection`).
fn mid_head(p: &mut Vec<Prim>, x: f32, y0: f32, y1: f32) {
    let d = (y1 - y0).signum();
    p.push(head((x, (y0 + y1) / 2.0 + d * 5.0), (0.0, d), LINE));
}

fn only_stop(b: &[St]) -> bool {
    matches!(b, [St::Stop] | [St::End] | [St::Kill])
}

/// Bagian yang sudah dipisahkan menjadi tile + gaya panah ke tile berikutnya.
fn seq(sts: &[St], size: &[(f32, f32)]) -> Tile {
    seq_lanes(sts, size, None).finish()
}

/// `lanes` = (x kiri, lebar) tiap lane: tile tingkat atas ditaruh di tengah lane-nya.
fn seq_lanes(sts: &[St], size: &[(f32, f32)], lanes: Option<&[(f32, f32)]>) -> Tile {
    // Gabungkan Note ke tile sebelumnya, Arrow jadi gaya sambungan.
    let mut items: Vec<(Tile, ArrowStyle, usize)> = Vec::new();
    let mut pending = ArrowStyle::default();
    let mut lane = 0;
    for s in sts {
        match s {
            St::Arrow(a) => {
                pending = *a;
                // Panah di akhir cabang: label dibawa ke sambungan berikut.
            }
            St::Lane(k) => lane = *k,
            St::Note { t, left } => match items.pop() {
                Some((t0, a, l)) => items.push((with_note(t0, *t, *left, size), a, l)),
                None => items.push((note_alone(*t, size), pending, lane)),
            },
            _ => {
                items.push((tile(s, size), pending, lane));
                pending = ArrowStyle::default();
            }
        }
    }
    let ax = items.iter().fold(0.0f32, |a, t| a.max(t.0.ax));
    let w = ax + items.iter().fold(0.0f32, |a, t| a.max(t.0.w - t.0.ax));
    let mut out = Tile {
        w,
        ax,
        ..Tile::empty()
    };
    let mut y = 0.0;
    let mut prev_x = ax;
    let mut first = true;
    for (t, arrow, l) in items {
        let x_here = lanes.map_or(ax, |c| {
            let (lx, lw) = c[l.min(c.len() - 1)];
            lx + (lw - t.w) / 2.0 + t.ax
        });
        if !first {
            let brk = t.h == 0.0 && !t.brk.is_empty();
            let lh = arrow.label.map_or(0.0, |i| size[i].1);
            let len = if brk {
                14.0
            } else if arrow.label.is_some() {
                t.pre + lh + 9.35
            } else {
                t.pre
            };
            if out.open && !arrow.hidden {
                let c = arrow.color.unwrap_or(LINE);
                let mut pts = vec![(prev_x, y)];
                if (prev_x - x_here).abs() > 0.5 {
                    pts.push((prev_x, y + 5.0));
                    pts.push((x_here, y + 5.0));
                }
                pts.push((x_here, y + len));
                out.prims.push(match arrow.dash {
                    true => dashed(pts, c, 1.0, (2.0, 2.0)),
                    false => line(pts, c, 1.0),
                });
                if !brk {
                    out.prims.push(head((x_here, y + len), (0.0, 1.0), c));
                }
                if let Some(i) = arrow.label {
                    out.prims
                        .push(Prim::Text(i, x_here + 4.0, y + (len - size[i].1) / 2.0));
                    out.w = out.w.max(x_here + 4.0 + size[i].0);
                }
            }
            y += len;
        }
        first = false;
        let (h, open, tax) = (t.h, t.open, t.ax);
        out.put(t, x_here - tax, y);
        y += h;
        out.open = open;
        prev_x = x_here;
    }
    out.h = y;
    out
}

/// Note di samping tile (FtileWithNotes): kotak lipat + penunjuk ke tile.
fn with_note(t: Tile, note: usize, left: bool, size: &[(f32, f32)]) -> Tile {
    let (nw, nh) = (size[note].0 + 21.0, size[note].1 + 11.0);
    let h = t.h.max(nh);
    let ty = (h - t.h) / 2.0;
    let ny = (h - nh) / 2.0;
    let my = ty + t.h / 2.0;
    let mut o = Tile {
        w: t.w + nw + 20.0,
        h,
        open: t.open,
        ..Tile::empty()
    };
    let (tx, nx) = if left {
        (nw + 20.0, 0.0)
    } else {
        (0.0, t.w + 20.0)
    };
    o.ax = t.ax + tx;
    o.back.extend(opale(
        [nx, ny, nw, nh],
        if left {
            Some((nx + nw + 20.0, my))
        } else {
            Some((nx - 20.0, my))
        },
    ));
    o.prims.push(Prim::Text(note, nx + 6.0, ny + 5.5));
    o.put(t, tx, ty);
    o
}

fn note_alone(note: usize, size: &[(f32, f32)]) -> Tile {
    let (nw, nh) = (size[note].0 + 21.0, size[note].1 + 11.0);
    let mut o = Tile {
        w: nw,
        h: nh,
        ax: nw / 2.0,
        ..Tile::empty()
    };
    o.back.extend(opale([0.0, 0.0, nw, nh], None));
    o.prims.push(Prim::Text(note, 6.0, 5.5));
    o
}

/// Bentuk catatan PlantUML: sudut kanan atas dilipat 10, penunjuk opsional.
pub fn opale(r: [f32; 4], to: Option<Pt>) -> Vec<Prim> {
    let [x, y, w, h] = r;
    let mut pts = vec![
        (x, y),
        (x + w - 10.0, y),
        (x + w, y + 10.0),
        (x + w, y + h),
        (x, y + h),
    ];
    if let Some((px, py)) = to {
        let cy = py.clamp(y + 4.0, y + h - 4.0);
        if px < x {
            pts.extend_from_slice(&[(x, cy + 4.0), (px, py), (x, cy - 4.0)]);
        } else {
            pts.splice(3..3, [(x + w, cy - 4.0), (px, py), (x + w, cy + 4.0)]);
        }
    }
    vec![
        poly(pts, Col::Note, LINE, 0.5),
        poly(
            vec![
                (x + w - 10.0, y),
                (x + w - 10.0, y + 10.0),
                (x + w, y + 10.0),
            ],
            Col::Note,
            LINE,
            0.5,
        ),
    ]
}

fn tile(s: &St, size: &[(f32, f32)]) -> Tile {
    match *s {
        St::Act { t, color, style } => action(t, color, style, size),
        St::Start => {
            let mut o = Tile {
                w: 20.0,
                h: 20.0,
                ax: 10.0,
                ..Tile::empty()
            };
            o.back.push(ellipse(
                [0.0, 0.0, 20.0, 20.0],
                Col::Rgb(0x222222),
                Col::Rgb(0x222222),
                1.0,
            ));
            o
        }
        St::Stop => {
            let mut o = Tile {
                w: 22.0,
                h: 22.0,
                ax: 11.0,
                open: false,
                ..Tile::empty()
            };
            o.back.push(ellipse(
                [0.0, 0.0, 22.0, 22.0],
                Col::Paper,
                Col::Rgb(0x222222),
                1.0,
            ));
            o.back.push(ellipse(
                [5.0, 5.0, 12.0, 12.0],
                Col::Rgb(0x222222),
                Col::Rgb(0x222222),
                1.0,
            ));
            o
        }
        St::End => {
            let mut o = Tile {
                w: 20.0,
                h: 20.0,
                ax: 10.0,
                open: false,
                ..Tile::empty()
            };
            o.back.push(ellipse(
                [0.0, 0.0, 20.0, 20.0],
                Col::Paper,
                Col::Rgb(0x222222),
                1.5,
            ));
            let (a, b) = (10.0 - 6.19, 10.0 + 6.19);
            o.prims
                .push(line(vec![(a, a), (b, b)], Col::Rgb(0x222222), 2.5));
            o.prims
                .push(line(vec![(b, a), (a, b)], Col::Rgb(0x222222), 2.5));
            o
        }
        St::Kill => Tile {
            open: false,
            ..Tile::empty()
        },
        St::Break => Tile {
            open: false,
            brk: vec![(0.0, 0.0)],
            ..Tile::empty()
        },
        St::Arrow(_) | St::Note { .. } | St::Lane(_) => Tile::empty(),
        St::If {
            ref conds,
            el,
            ref els,
        } => {
            if conds.len() > 1 {
                return if_long(conds, (el, els), size);
            }
            let (cond, tl, ref then) = conds[0];
            // ConditionalBuilder.create: satu cabang kosong/hanya stop → FtileIfDown.
            let (e1, e2) = (
                then.is_empty() || only_stop(then),
                els.is_empty() || only_stop(els),
            );
            if e2 && !e1 || then.is_empty() && only_stop(els) {
                if_down(cond, (then, tl), (els, el), size)
            } else if e1 && !e2 || els.is_empty() && only_stop(then) {
                if_down(cond, (els, el), (then, tl), size)
            } else {
                if_links(cond, (then, tl), (els, el), size)
            }
        }
        St::While {
            cond,
            is,
            ref body,
            out,
            backward,
        } => while_tile(cond, is, body, out, backward, size),
        St::Repeat {
            start,
            ref body,
            cond,
            is,
            not,
            backward,
        } => repeat_tile(start, body, cond, is, not, backward, size),
        St::Fork {
            ref branches,
            merge,
        } => fork(branches, merge, size),
        St::Split { ref branches } => split(branches, size),
        St::Switch { cond, ref cases } => switch(cond, cases, size),
        St::Group {
            title,
            ref body,
            color,
        } => group_tile(title, body, color, size),
    }
}

fn action(t: usize, color: Option<Col>, style: BoxStyle, size: &[(f32, f32)]) -> Tile {
    let (tw, th) = size[t];
    let extra = if matches!(style, BoxStyle::Plain) {
        0.0
    } else {
        10.0
    };
    let (w, h) = (tw + 20.0 + extra, th + 20.0);
    let fill = color.unwrap_or(Col::Box);
    let mut o = Tile {
        w,
        h,
        ax: w / 2.0,
        ..Tile::empty()
    };
    let r = [0.0, 0.0, w, h];
    let shape = match style {
        BoxStyle::Plain => rect(r, 12.5, fill, LINE, 0.5),
        BoxStyle::Input => poly(
            vec![(0.0, 0.0), (w, 0.0), (w - 10.0, h / 2.0), (w, h), (0.0, h)],
            fill,
            LINE,
            0.5,
        ),
        BoxStyle::Output => poly(
            vec![
                (0.0, 0.0),
                (w - 10.0, 0.0),
                (w, h / 2.0),
                (w - 10.0, h),
                (0.0, h),
            ],
            fill,
            LINE,
            0.5,
        ),
        BoxStyle::Load | BoxStyle::Save => poly(
            vec![(10.0, 0.0), (w, 0.0), (w - 10.0, h), (0.0, h)],
            fill,
            LINE,
            0.5,
        ),
        BoxStyle::Continuous => poly(
            vec![
                (10.0, 0.0),
                (w - 10.0, 0.0),
                (w, h / 2.0),
                (w - 10.0, h),
                (10.0, h),
                (0.0, h / 2.0),
            ],
            fill,
            LINE,
            0.5,
        ),
        BoxStyle::Procedure | BoxStyle::Task => rect(r, 0.0, fill, LINE, 0.5),
    };
    o.back.push(shape);
    if style == BoxStyle::Procedure {
        o.back.push(line(vec![(5.0, 0.0), (5.0, h)], LINE, 0.5));
        o.back
            .push(line(vec![(w - 5.0, 0.0), (w - 5.0, h)], LINE, 0.5));
    }
    let tx = if matches!(
        style,
        BoxStyle::Load | BoxStyle::Save | BoxStyle::Continuous
    ) {
        15.0
    } else {
        10.0
    };
    o.prims.push(Prim::Text(t, tx, 10.0));
    o
}

/// Segi enam kondisi: lebar teks + 2×12, tinggi ≥ 24.
fn hex(p: &mut Vec<Prim>, cond: usize, size: &[(f32, f32)], cx: f32, y: f32) -> (f32, f32) {
    let (cw, ch) = size[cond];
    let (w, h) = (cw + 2.0 * HEX, ch.max(2.0 * HEX));
    let x = cx - w / 2.0;
    p.push(poly(
        vec![
            (x + HEX, y),
            (x + w - HEX, y),
            (x + w, y + h / 2.0),
            (x + w - HEX, y + h),
            (x + HEX, y + h),
            (x, y + h / 2.0),
        ],
        Col::Box,
        LINE,
        0.5,
    ));
    p.push(Prim::Text(cond, cx - cw / 2.0, y + (h - ch) / 2.0));
    (w, h)
}

fn hex_size(cond: usize, size: &[(f32, f32)]) -> (f32, f32) {
    (size[cond].0 + 2.0 * HEX, size[cond].1.max(2.0 * HEX))
}

fn diamond(p: &mut Vec<Prim>, cx: f32, y: f32) {
    p.push(poly(
        vec![
            (cx, y),
            (cx + HEX, y + HEX),
            (cx, y + 2.0 * HEX),
            (cx - HEX, y + HEX),
        ],
        Col::Box,
        LINE,
        0.5,
    ));
}

/// FtileIfDown: cabang `down` lurus ke bawah, cabang lain (kosong/stop) lewat kanan.
fn if_down(
    cond: usize,
    (down_b, dl): (&[St], usize),
    (side, sl): (&[St], usize),
    size: &[(f32, f32)],
) -> Tile {
    let t = seq(down_b, size).branch(30.0, 10.0);
    let (hw, hh) = hex_size(cond, size);
    let (south, east) = (size[dl], size[sl]);
    let cx = (hw / 2.0).max(t.ax);
    let mut o = Tile::empty();
    hex(&mut o.back, cond, size, cx, 0.0);
    let p = &mut o.prims;
    p.push(Prim::Text(dl, cx + 4.0, hh));
    p.push(Prim::Text(sl, cx + hw / 2.0, hh / 2.0 - east.1));
    let ty = hh + south.1 + HEX;
    ln(p, &[(cx, hh), (cx, ty)]);
    down(p, cx, ty);
    let tb = ty + t.h;
    let tx = cx - t.ax;
    let xr = (cx + hw / 2.0 + HEX).max(tx + t.w);
    let my = hh / 2.0;
    let mut w = xr.max(cx + hw / 2.0 + east.0);
    let (h, open);
    if only_stop(side) {
        // optionalStop: stop di kanan, disambung mendatar dari sudut kanan.
        let sx = cx + hw / 2.0 + 22.0f32.max(east.0 + 11.0);
        ln(p, &[(cx + hw / 2.0, my), (sx, my)]);
        p.push(head((sx, my), (1.0, 0.0), LINE));
        o.back.push(ellipse(
            [sx, my - 11.0, 22.0, 22.0],
            Col::Paper,
            Col::Rgb(0x222222),
            1.0,
        ));
        o.back.push(ellipse(
            [sx + 5.0, my - 6.0, 12.0, 12.0],
            Col::Rgb(0x222222),
            Col::Rgb(0x222222),
            1.0,
        ));
        w = w.max(sx + 22.0);
        (h, open) = (tb, t.open);
    } else if t.open {
        let dy = tb + ARROW;
        ln(p, &[(cx, tb), (cx, dy)]);
        down(p, cx, dy);
        diamond(&mut o.back, cx, dy);
        let p = &mut o.prims;
        let dm = dy + HEX;
        ln(
            p,
            &[(cx + hw / 2.0, my), (xr, my), (xr, dm), (cx + HEX, dm)],
        );
        p.push(head((cx + HEX, dm), (-1.0, 0.0), LINE));
        mid_head(p, xr, my, dm);
        (h, open) = (dy + 2.0 * HEX, true);
    } else {
        let hb = tb + 22.0;
        ln(p, &[(cx + hw / 2.0, my), (xr, my), (xr, hb), (cx, hb)]);
        mid_head(p, xr, my, hb);
        (h, open) = (hb, true);
    }
    o.w = w;
    o.h = h;
    o.ax = cx;
    o.open = open;
    o.put(t, tx, ty);
    o
}

/// FtileIfWithLinks: dua cabang berdampingan, keluar dari sudut kiri/kanan segi enam.
fn if_links(
    cond: usize,
    (b1, l1): (&[St], usize),
    (b2, l2): (&[St], usize),
    size: &[(f32, f32)],
) -> Tile {
    let (e1, e2) = (b1.is_empty(), b2.is_empty());
    let (t1, t2) = (
        seq(b1, size).branch(30.0, 10.0),
        seq(b2, size).branch(30.0, 10.0),
    );
    let (hw, hh) = hex_size(cond, size);
    let (lw1, lw2) = (size[l1], size[l2]);
    let inner = (t1.w - t1.ax + t2.ax).max(hw + 20.0);
    let nude = t1.ax + inner + (t2.w - t2.ax);
    let mut cx = t1.ax + inner / 2.0;
    let mut dx = (hw / 2.0 - cx).max(0.0);
    dx += (lw1.0 - (cx + dx - hw / 2.0)).max(0.0);
    cx += dx;
    let w = (dx + nude).max(cx + hw / 2.0 + lw2.0);
    let (x1, x2) = (dx + t1.ax, dx + nude - t2.w + t2.ax);
    let by = hh + 10.0;
    let my = hh / 2.0;
    let mut o = Tile::empty();
    hex(&mut o.back, cond, size, cx, 0.0);
    let p = &mut o.prims;
    p.push(Prim::Text(l1, cx - hw / 2.0 - lw1.0, my - lw1.1));
    p.push(Prim::Text(l2, cx + hw / 2.0, my - lw2.1));
    for (sx, x, e) in [(cx - hw / 2.0, x1, e1), (cx + hw / 2.0, x2, e2)] {
        ln(p, &[(sx, my), (x, my), (x, by)]);
        if !e {
            down(p, x, by);
        }
    }
    let bb = by + t1.h.max(t2.h);
    let outs = [
        (x1, by + t1.h, t1.open, e1, -1.0),
        (x2, by + t2.h, t2.open, e2, 1.0),
    ];
    let h = if t1.open && t2.open {
        let dy = bb + 6.0;
        diamond(&mut o.back, cx, dy);
        let p = &mut o.prims;
        let dm = dy + HEX;
        for (x, y, _, e, sd) in outs {
            let tip = cx + sd * HEX;
            ln(p, &[(x, y), (x, dm), (tip, dm)]);
            p.push(head((tip, dm), (-sd, 0.0), LINE));
            if e {
                mid_head(p, x, y, dm);
            }
        }
        dy + 2.0 * HEX
    } else {
        let h = bb + 6.0;
        for (x, y, open, e, _) in outs {
            if open {
                ln(p, &[(x, y), (x, h), (cx, h)]);
                if e {
                    mid_head(p, x, y, h);
                }
            }
        }
        h
    };
    o.w = w;
    o.h = h;
    o.ax = cx;
    o.open = t1.open || t2.open;
    o.put(t1, dx, by);
    let x2l = dx + nude - t2.w;
    o.put(t2, x2l, by);
    o
}

/// FtileIfLongHorizontal (`elseif`): segi enam berantai mendatar, tiap cabang di
/// bawah segi enamnya, cabang else dari sudut kanan segi enam terakhir.
fn if_long(
    conds: &[(usize, usize, Vec<St>)],
    (el, els): (usize, &[St]),
    size: &[(f32, f32)],
) -> Tile {
    let mut o = Tile::empty();
    let hh = conds
        .iter()
        .map(|c| hex_size(c.0, size).1)
        .fold(0.0, f32::max);
    let mut x = 0.0;
    let mut branches: Vec<(Tile, f32)> = Vec::new(); // (tile, x tengah)
    let mut xs = Vec::new(); // (cx, hw)
    let mut right = 0.0f32;
    for (k, (c, l, body)) in conds.iter().enumerate() {
        let t = seq(body, size).branch(30.0, 10.0);
        let (hw, _) = hex_size(*c, size);
        let cx = (x + hw / 2.0).max(x + t.ax).max(right + t.ax);
        if k > 0 {
            let (pcx, phw): (f32, f32) = xs[k - 1];
            let a = pcx + phw / 2.0;
            ln(&mut o.prims, &[(a, hh / 2.0), (cx - hw / 2.0, hh / 2.0)]);
            o.prims
                .push(head((cx - hw / 2.0, hh / 2.0), (1.0, 0.0), LINE));
        }
        hex(&mut o.back, *c, size, cx, (hh - hex_size(*c, size).1) / 2.0);
        o.prims.push(Prim::Text(*l, cx + 4.0, hh));
        xs.push((cx, hw));
        right = cx - t.ax + t.w;
        x = cx + hw / 2.0 + 20.0;
        branches.push((t, cx));
    }
    let lab = conds.iter().map(|c| size[c.1].1).fold(size[el].1, f32::max);
    let by = hh + lab + 20.9;
    // Cabang else.
    let te = seq(els, size).branch(30.0, 10.0);
    let (lcx, lhw) = *xs.last().unwrap();
    let ex = (lcx + lhw / 2.0 + size[el].0 + 10.0)
        .max(right + te.ax)
        .max(lcx + lhw / 2.0 + HEX);
    ln(
        &mut o.prims,
        &[(lcx + lhw / 2.0, hh / 2.0), (ex, hh / 2.0), (ex, by)],
    );
    o.prims
        .push(Prim::Text(el, lcx + lhw / 2.0, hh / 2.0 - size[el].1));
    if !els.is_empty() {
        down(&mut o.prims, ex, by);
    }
    let (te_w, te_ax) = (te.w, te.ax);
    branches.push((te, ex));
    for &(cx, _) in &xs {
        ln(&mut o.prims, &[(cx, hh), (cx, by)]);
        down(&mut o.prims, cx, by);
    }
    let bottom = by + branches.iter().map(|b| b.0.h).fold(0.0, f32::max);
    let first = xs[0].0;
    let opens: Vec<(f32, f32)> = branches
        .iter()
        .filter(|b| b.0.open)
        .map(|b| (b.1, by + b.0.h))
        .collect();
    let mut h = bottom;
    if !opens.is_empty() {
        let ym = bottom + 20.0;
        for &(bx, y) in &opens {
            ln(&mut o.prims, &[(bx, y), (bx, ym)]);
        }
        let (a, b) = opens
            .iter()
            .fold((first, first), |m, o| (m.0.min(o.0), m.1.max(o.0)));
        ln(&mut o.prims, &[(a, ym), (b, ym)]);
        h = ym;
    }
    o.open = !opens.is_empty();
    o.ax = first;
    let mut w = ex + te_w - te_ax;
    for (t, cx) in branches {
        w = w.max(cx - t.ax + t.w);
        let tx = cx - t.ax;
        o.put(t, tx, by);
    }
    o.w = w.max(ex + size[el].0);
    o.h = h;
    o
}

fn while_tile(
    cond: usize,
    is: usize,
    body: &[St],
    out: usize,
    backward: Option<usize>,
    size: &[(f32, f32)],
) -> Tile {
    let b = seq(body, size);
    let (hw, hh) = hex_size(cond, size);
    let (yes, lo) = (size[is].1, size[out]);
    let left = (hw / 2.0).max(b.ax);
    let gw = (left + hw / 2.0).max(left - b.ax + b.w);
    let cx = left + 2.0 * HEX;
    let mut w = gw + 3.0 * HEX;
    let mut o = Tile::empty();
    hex(&mut o.back, cond, size, cx, 0.0);
    let p = &mut o.prims;
    p.push(Prim::Text(is, cx + 4.0, hh));
    p.push(Prim::Text(out, cx - hw / 2.0 - lo.0, hh / 2.0 - lo.1));
    let by = hh + yes + 2.0 * HEX;
    let yb = by + b.h;
    let h = yb + 2.0 * HEX;
    let my = hh / 2.0;
    ln(p, &[(cx, hh), (cx, by)]);
    if b.h > 0.0 {
        down(p, cx, by);
    }
    if b.open {
        let y1 = yb + HEX;
        if let Some(bw) = backward {
            // Aksi backward di kanan, garis naik melewatinya.
            let (tw, th) = size[bw];
            let (bxw, bxh) = (tw + 20.0, th + 20.0);
            let bx = w;
            let byy = (my + y1) / 2.0 - bxh / 2.0;
            o.back
                .push(rect([bx, byy, bxw, bxh], 12.5, Col::Box, LINE, 0.5));
            let p = &mut o.prims;
            p.push(Prim::Text(bw, bx + 10.0, byy + 10.0));
            let xc = bx + bxw / 2.0;
            ln(p, &[(cx, yb), (cx, y1), (xc, y1), (xc, byy + bxh)]);
            p.push(head((xc, byy + bxh), (0.0, -1.0), LINE));
            ln(p, &[(xc, byy), (xc, my), (cx + hw / 2.0, my)]);
            p.push(head((cx + hw / 2.0, my), (-1.0, 0.0), LINE));
            w = bx + bxw;
        } else {
            ln(
                p,
                &[(cx, yb), (cx, y1), (w, y1), (w, my), (cx + hw / 2.0, my)],
            );
            p.push(head((cx + hw / 2.0, my), (-1.0, 0.0), LINE));
            mid_head(p, w, y1, my);
        }
    }
    let p = &mut o.prims;
    let xo = HEX;
    ln(p, &[(cx - hw / 2.0, my), (xo, my), (xo, h), (cx, h)]);
    mid_head(p, xo, my, h);
    o.w = w;
    o.h = h;
    o.ax = cx;
    let bx = cx - b.ax;
    let mut inner = Tile::empty();
    inner.put(b, bx, by);
    for (x, y) in std::mem::take(&mut inner.brk) {
        ln(&mut o.prims, &[(x, y), (xo, y)]);
        o.prims.push(head((xo, y), (-1.0, 0.0), LINE));
    }
    o.back.append(&mut inner.back);
    o.prims.append(&mut inner.prims);
    o
}

/// FtileRepeat: belah ketupat masuk di atas, isi, segi enam kondisi di bawah,
/// garis balik naik di kanan.
fn repeat_tile(
    start: Option<usize>,
    body: &[St],
    cond: usize,
    is: usize,
    not: usize,
    backward: Option<usize>,
    size: &[(f32, f32)],
) -> Tile {
    let b = seq(body, size);
    let (hw, hh) = hex_size(cond, size);
    let cx =
        b.ax.max(hw / 2.0)
            .max(start.map_or(0.0, |s| (size[s].0 + 20.0) / 2.0));
    let mut o = Tile::empty();
    // Atas: belah ketupat atau aksi awal `repeat :x;`.
    let (top_h, top_hw) = match start {
        Some(s) => {
            let (tw, th) = size[s];
            o.back.push(rect(
                [cx - (tw + 20.0) / 2.0, 0.0, tw + 20.0, th + 20.0],
                12.5,
                Col::Box,
                LINE,
                0.5,
            ));
            o.prims.push(Prim::Text(s, cx - tw / 2.0, 10.0));
            (th + 20.0, (tw + 20.0) / 2.0)
        }
        None => {
            diamond(&mut o.back, cx, 0.0);
            (2.0 * HEX, HEX)
        }
    };
    let by = top_h + ARROW;
    let p = &mut o.prims;
    ln(p, &[(cx, top_h), (cx, by)]);
    down(p, cx, by);
    let yb = by + b.h;
    let hy = yb + ARROW;
    if b.open {
        ln(p, &[(cx, yb), (cx, hy)]);
        down(p, cx, hy);
    }
    hex(&mut o.back, cond, size, cx, hy);
    let p = &mut o.prims;
    let my = hy + hh / 2.0;
    p.push(Prim::Text(is, cx + hw / 2.0, my - size[is].1));
    p.push(Prim::Text(not, cx + 4.0, hy + hh));
    let mut xr = (cx + hw / 2.0 + HEX).max(cx - b.ax + b.w + HEX);
    let ty = top_h / 2.0;
    match backward {
        Some(bw) => {
            let (tw, th) = size[bw];
            let (bxw, bxh) = (tw + 20.0, th + 20.0);
            let bx = xr;
            let byy = (ty + my) / 2.0 - bxh / 2.0;
            o.back
                .push(rect([bx, byy, bxw, bxh], 12.5, Col::Box, LINE, 0.5));
            let p = &mut o.prims;
            p.push(Prim::Text(bw, bx + 10.0, byy + 10.0));
            let xc = bx + bxw / 2.0;
            ln(p, &[(cx + hw / 2.0, my), (xc, my), (xc, byy + bxh)]);
            p.push(head((xc, byy + bxh), (0.0, -1.0), LINE));
            ln(p, &[(xc, byy), (xc, ty), (cx + top_hw, ty)]);
            p.push(head((cx + top_hw, ty), (-1.0, 0.0), LINE));
            xr = bx + bxw;
        }
        None => {
            ln(
                p,
                &[(cx + hw / 2.0, my), (xr, my), (xr, ty), (cx + top_hw, ty)],
            );
            p.push(head((cx + top_hw, ty), (-1.0, 0.0), LINE));
            mid_head(p, xr, my, ty);
        }
    }
    o.w = xr.max(cx + hw / 2.0 + size[is].0);
    o.h = hy + hh + if size[not].1 > 0.0 { size[not].1 } else { 0.0 };
    o.ax = cx;
    let bx = cx - b.ax;
    o.put(b, bx, by);
    let p = &mut o.prims;
    if size[not].1 > 0.0 {
        ln(p, &[(cx, hy + hh), (cx, o.h)]);
    }
    o
}

/// Fork (batang hitam) — ParallelBuilderFork; `merge` = belah ketupat di bawah.
fn fork(branches: &[Vec<St>], merge: bool, size: &[(f32, f32)]) -> Tile {
    let ts: Vec<Tile> = branches
        .iter()
        .map(|b| seq(b, size).branch(0.0, 14.0))
        .collect();
    let w: f32 = ts.iter().map(|t| t.w).sum::<f32>();
    let mut o = Tile {
        w,
        ax: w / 2.0,
        ..Tile::empty()
    };
    let bar = |p: &mut Vec<Prim>, y: f32| {
        p.push(rect(
            [2.0, y, w - 2.0, 6.0],
            2.5,
            Col::Rgb(0x555555),
            Col::Rgb(0x555555),
            1.0,
        ))
    };
    bar(&mut o.back, 0.0);
    let by = 6.0 + ARROW;
    let maxh = ts.iter().map(|t| t.h).fold(0.0, f32::max);
    let yb = by + maxh + ARROW;
    let mut x = 0.0;
    let any_open = ts.iter().any(|t| t.open);
    let mut anchors = Vec::new();
    for t in ts {
        let ax = x + t.ax;
        let p = &mut o.prims;
        ln(p, &[(ax, 6.0), (ax, by)]);
        down(p, ax, by);
        if t.open {
            let tip = if merge { yb + HEX } else { yb };
            ln(p, &[(ax, by + t.h), (ax, tip)]);
            if !merge {
                down(p, ax, yb);
            }
        }
        anchors.push(ax);
        let tw = t.w;
        o.put(t, x, by);
        x += tw;
    }
    if merge {
        let cx = w / 2.0;
        diamond(&mut o.back, cx, yb);
        let (a, b) = anchors
            .iter()
            .fold((cx, cx), |m, &v| (m.0.min(v), m.1.max(v)));
        ln(&mut o.prims, &[(a, yb + HEX), (cx - HEX, yb + HEX)]);
        ln(&mut o.prims, &[(b, yb + HEX), (cx + HEX, yb + HEX)]);
        o.h = yb + 2.0 * HEX;
    } else {
        if any_open {
            bar(&mut o.back, yb);
        }
        o.h = yb + 6.0;
    }
    o.open = any_open;
    o
}

/// Split: garis mendatar tebal 1,5 di atas dan bawah — ParallelBuilderSplit.
fn split(branches: &[Vec<St>], size: &[(f32, f32)]) -> Tile {
    let ts: Vec<Tile> = branches
        .iter()
        .map(|b| seq(b, size).branch(0.0, 14.0))
        .collect();
    let w: f32 = ts.iter().map(|t| t.w).sum::<f32>();
    let maxh = ts.iter().map(|t| t.h).fold(0.0, f32::max);
    let by = 1.5 + 18.5;
    let yb = by + maxh + ARROW;
    let mut o = Tile { w, ..Tile::empty() };
    let mut x = 0.0;
    let mut anchors = Vec::new();
    let mut opens = Vec::new();
    for t in ts {
        let ax = x + t.ax;
        ln(&mut o.prims, &[(ax, 1.5), (ax, by)]);
        down(&mut o.prims, ax, by);
        if t.open {
            ln(&mut o.prims, &[(ax, by + t.h), (ax, yb)]);
            down(&mut o.prims, ax, yb);
            opens.push(ax);
        }
        anchors.push(ax);
        let tw = t.w;
        o.put(t, x, by);
        x += tw;
    }
    let (a, b) = (anchors[0], *anchors.last().unwrap());
    o.prims.push(line(vec![(a, 0.75), (b, 0.75)], LINE, 1.5));
    o.ax = (a + b) / 2.0;
    if !opens.is_empty() {
        let (a, b) = opens
            .iter()
            .fold((o.ax, o.ax), |m, &v| (m.0.min(v), m.1.max(v)));
        o.prims
            .push(line(vec![(a, yb + 0.75), (b, yb + 0.75)], LINE, 1.5));
    }
    o.open = !opens.is_empty();
    o.h = yb + 1.5;
    o
}

/// Switch: segi enam, cabang case berdampingan, belah ketupat penggabung.
fn switch(cond: usize, cases: &[(usize, Vec<St>)], size: &[(f32, f32)]) -> Tile {
    let ts: Vec<(Tile, usize)> = cases
        .iter()
        .map(|(l, b)| (seq(b, size).branch(0.0, 10.0), *l))
        .collect();
    let (hw, hh) = hex_size(cond, size);
    let lab = cases.iter().map(|c| size[c.0].1).fold(0.0, f32::max);
    let by = hh + lab + 24.77;
    let mut o = Tile::empty();
    let mut x = 0.0;
    let mut anchors = Vec::new();
    let n = ts.len();
    let mut placed = Vec::new();
    for (t, l) in ts {
        let ax = x + t.ax;
        let right = (x + t.w).max(ax + size[l].0 + 10.0);
        anchors.push((ax, l, t.open, t.h));
        placed.push((t, x));
        x = right + 10.0;
    }
    let total = x - 10.0;
    let cx = if n > 0 {
        (anchors[0].0 + anchors[n - 1].0) / 2.0
    } else {
        hw / 2.0
    };
    let shift = (hw / 2.0 - cx).max(0.0);
    let cx = cx + shift;
    hex(&mut o.back, cond, size, cx, 0.0);
    let my = hh / 2.0;
    let maxh = anchors.iter().map(|a| a.3).fold(0.0, f32::max);
    let dy = by + maxh + ARROW;
    let dm = dy + HEX;
    let mut any = false;
    for (k, &(ax, l, open, th)) in anchors.iter().enumerate() {
        let ax = ax + shift;
        let p = &mut o.prims;
        if k == 0 && n > 1 {
            ln(p, &[(cx - hw / 2.0, my), (ax, my), (ax, by)]);
        } else if k == n - 1 && n > 1 {
            ln(p, &[(cx + hw / 2.0, my), (ax, my), (ax, by)]);
        } else {
            let jy = hh + 12.56;
            ln(p, &[(cx, hh), (cx, jy), (ax, jy), (ax, by)]);
        }
        down(p, ax, by);
        p.push(Prim::Text(l, ax, by - 20.0 - size[l].1 / 2.0));
        if open {
            any = true;
            let y = by + th;
            if ax < cx - 0.5 {
                ln(p, &[(ax, y), (ax, dm), (cx - HEX, dm)]);
                p.push(head((cx - HEX, dm), (1.0, 0.0), LINE));
            } else if ax > cx + 0.5 {
                ln(p, &[(ax, y), (ax, dm), (cx + HEX, dm)]);
                p.push(head((cx + HEX, dm), (-1.0, 0.0), LINE));
            } else {
                ln(p, &[(ax, y), (ax, dy)]);
                down(p, cx, dy);
            }
        }
    }
    if any {
        diamond(&mut o.back, cx, dy);
    }
    for (t, x0) in placed {
        o.put(t, x0 + shift, by);
    }
    o.w = (total + shift).max(cx + hw / 2.0);
    o.h = if any { dy + 2.0 * HEX } else { by + maxh };
    o.ax = cx;
    o.open = any;
    o
}

/// Partition/group: bingkai 1,5 dengan tab judul (FtileGroup).
fn group_tile(title: usize, body: &[St], color: Option<Col>, size: &[(f32, f32)]) -> Tile {
    let b = seq(body, size);
    let (tw, th) = size[title];
    let tab_h = th + 4.3;
    let top = tab_h + 17.0;
    let w = (b.w + 20.0).max(tw + 30.0);
    let h = top + b.h + 12.0;
    let mut o = Tile {
        w,
        h,
        ax: 10.0 + b.ax,
        open: b.open,
        pre: 10.0,
        ..Tile::empty()
    };
    o.back.push(rect(
        [0.0, 0.0, w, h],
        0.0,
        color.unwrap_or(Col::None),
        Col::Text,
        1.5,
    ));
    let tx = tw + 10.0;
    o.back.push(line(
        vec![
            (tx + 10.0, 0.0),
            (tx + 10.0, tab_h - 10.0),
            (tx, tab_h),
            (0.0, tab_h),
        ],
        Col::Text,
        1.5,
    ));
    o.prims.push(Prim::Text(title, 3.0, 2.0));
    // Panah menembus bingkai: dari tepi atas ke isi, dari isi ke tepi bawah.
    ln(&mut o.prims, &[(o.ax, 0.0), (o.ax, top)]);
    down(&mut o.prims, o.ax, top);
    if b.open {
        ln(&mut o.prims, &[(o.ax, top + b.h), (o.ax, h)]);
    }
    o.put(b, 10.0, top);
    o
}

// ---------------------------------------------------------------------------
// Swimlane

fn lane_of(sts: &[St]) -> Vec<usize> {
    // Lane tiap pernyataan tingkat atas (yang menghasilkan tile).
    let mut lane = 0;
    let mut v = Vec::new();
    for s in sts {
        match s {
            St::Lane(k) => lane = *k,
            St::Arrow(_) | St::Note { .. } => {}
            _ => v.push(lane),
        }
    }
    v
}

fn place_lanes(a: &Activity, size: &[(f32, f32)]) -> Placed {
    // Lebar lane = tile terlebar di lane itu (atau judul + 10).
    let n = a.lanes.len();
    let lanes = lane_of(&a.body);
    let mut widths: Vec<f32> = a.lanes.iter().map(|(t, _)| size[*t].0 + 10.0).collect();
    let mut k = 0;
    let mut tiles_w = vec![0.0f32; n];
    let mut idx = 0;
    while idx < a.body.len() {
        let s = &a.body[idx];
        if !matches!(s, St::Lane(_) | St::Arrow(_) | St::Note { .. }) {
            let mut t = tile(s, size);
            // Note sesudahnya ikut melebarkan.
            if let Some(St::Note { t: nt, left }) = a.body.get(idx + 1) {
                t = with_note(t, *nt, *left, size);
            }
            let l = lanes.get(k).copied().unwrap_or(0).min(n.saturating_sub(1));
            tiles_w[l] = tiles_w[l].max(t.w);
            k += 1;
        }
        idx += 1;
    }
    for (w, t) in widths.iter_mut().zip(&tiles_w) {
        *w = w.max(t + 20.0);
    }
    let mut xs = Vec::new();
    let mut x = 0.0;
    for w in &widths {
        xs.push((x, *w));
        x += w;
    }
    let total = x;
    let head_h = a.lanes.iter().map(|(t, _)| size[*t].1).fold(0.0, f32::max) + 6.0;
    let t = seq_lanes(&a.body, size, Some(&xs)).finish();
    let mut prims = Vec::new();
    let body_h = t.h + 2.0 * 10.0 + head_h;
    let mut x = 0.0;
    for (k, (title, color)) in a.lanes.iter().enumerate() {
        if let Some(c) = color {
            prims.push(rect([x, 0.0, widths[k], body_h], 0.0, *c, *c, 1.0));
        }
        prims.push(Prim::Text(
            *title,
            x + (widths[k] - size[*title].0) / 2.0,
            2.0,
        ));
        x += widths[k];
    }
    let mut x = 0.0;
    for w in widths.iter().chain(std::iter::once(&0.0)) {
        prims.push(line(vec![(x, 0.0), (x, body_h)], Col::Text, 1.5));
        x += w;
    }
    // seq_lanes memakai xs absolut: tile tingkat atas sudah di x lane-nya.
    for mut p in t.prims {
        p.shift(0.0, head_h + 10.0);
        prims.push(p);
    }
    let _ = total;
    Placed {
        prims,
        w: total,
        h: body_h,
    }
    .fit(size, 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Activity> {
        parse_lines(&s.lines().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn parse_features() {
        let a = parse("start\n:satu\ndua;\n-> lbl;\nif (c?) then (ya)\n  :x;\n  stop\nelseif (d?) then (ya)\n  :y;\nelse (tidak)\n  while (w?) is (ya)\n    if (e?) then (ya)\n      break\n    endif\n    :z;\n  endwhile (tidak)\nendif\nrepeat\n :r;\nrepeat while (lagi?) is (ya) not (tidak)\nfork\n :a;\nfork again\n :b;\nend fork\nswitch (s?)\ncase (1)\n :p;\ncase (2)\n :q;\nendswitch\npartition P {\n :in;\n note right: n\n}\nstop").unwrap();
        assert_eq!(a.texts.0[0].s, "satu\ndua");
        assert!(matches!(&a.body[3], St::If { conds, .. } if conds.len() == 2));
        assert!(matches!(&a.body[4], St::Repeat { .. }));
        assert!(matches!(&a.body[5], St::Fork { branches, .. } if branches.len() == 2));
        assert!(matches!(&a.body[6], St::Switch { cases, .. } if cases.len() == 2));
        let pl = a.place(&vec![(40.0, 16.0); a.texts.0.len()]);
        assert!(pl.w > 0.0 && pl.h > 0.0);
        assert!(parse("A -> B : x").is_none());
    }
}
