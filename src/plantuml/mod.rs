//! Diagram PlantUML untuk mode baca, digambar native (tanpa Java/Graphviz).
//!
//! Tiap jenis diagram punya modul sendiri: parse teks → tata letak dalam satuan
//! px PlantUML → daftar [`Prim`] (garis/poligon/teks). `app` hanya mengukur teks
//! dan menggambar primitif. Jenis yang di PlantUML ditata Graphviz memakai
//! [`graph`] (tata letak berlapis gaya `dot`), jadi mirip tetapi tidak identik.

pub mod activity;
pub mod alpha;
pub mod chen;
pub mod colors;
pub mod creole;
pub mod desc;
pub mod ditaa;
mod dot_ns;
pub mod gantt;
pub mod graph;
pub mod json;
pub mod misc;
pub mod nwdiag;
pub mod oldact;
pub mod rail;
pub mod salt;
pub mod sequence;
pub mod timing;
pub mod tree;

use crate::vector::{self, Pt};

// ---------------------------------------------------------------------------
// Warna

/// Warna semantik (dipetakan ke tema) atau RGB eksplisit dari sumber diagram.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Col {
    None,
    /// Garis/tepi (#181818).
    Ink,
    /// Teks (hitam).
    Text,
    /// Latar diagram (putih).
    Paper,
    /// Isi elemen umum (#F1F1F1).
    Box,
    /// Isi peserta sequence (#E2E2F0).
    Part,
    /// Isi catatan (#FEFFDD).
    Note,
    /// Kepala grup / pemisah (#EEEEEE).
    Group,
    /// Garis samar (#DDDDDD).
    Faint,
    Rgb(u32),
}

impl Col {
    /// Nama warna PlantUML (`red`, `#F00`, `#FF0000`, `#line.dashed` diabaikan).
    pub fn parse(s: &str) -> Option<Col> {
        let s = s.trim().trim_start_matches('#');
        let s = s.split(['.', ';', ' ']).next()?;
        let low = s.to_lowercase();
        if low == "transparent" || low == "background" {
            return Some(Col::None);
        }
        if let Some(&(_, v)) = colors::NAMED.iter().find(|c| c.0 == low) {
            return Some(Col::Rgb(v));
        }
        let hex = |t: &str| u32::from_str_radix(t, 16).ok();
        match s.len() {
            1 => hex(s).map(|d| Col::Rgb(d * 0x111111)),
            3 => {
                let v = hex(s)?;
                let (r, g, b) = ((v >> 8) & 15, (v >> 4) & 15, v & 15);
                Some(Col::Rgb((r * 17) << 16 | (g * 17) << 8 | b * 17))
            }
            6 => hex(s).map(Col::Rgb),
            8 => hex(&s[..6]).map(Col::Rgb),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Teks

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Satu blok teks (boleh multibaris dan ber-creole); ukuran dalam px PlantUML.
#[derive(Clone, Debug)]
pub struct Txt {
    pub s: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub color: Col,
    pub align: Align,
}

impl Txt {
    pub fn new(s: &str, size: f32) -> Txt {
        Txt {
            s: s.to_string(),
            size,
            bold: false,
            italic: false,
            mono: false,
            color: Col::Text,
            align: Align::Left,
        }
    }

    pub fn bold(mut self) -> Txt {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Txt {
        self.italic = true;
        self
    }

    pub fn center(mut self) -> Txt {
        self.align = Align::Center;
        self
    }

    pub fn color(mut self, c: Col) -> Txt {
        self.color = c;
        self
    }
}

/// Daftar teks diagram; indeksnya dipakai [`Prim::Text`].
#[derive(Default)]
pub struct Texts(pub Vec<Txt>);

impl Texts {
    pub fn add(&mut self, t: Txt) -> usize {
        self.0.push(t);
        self.0.len() - 1
    }

    /// Teks sumber PlantUML: `\n` literal jadi baris baru.
    pub fn src(&mut self, s: &str, size: f32) -> usize {
        self.add(Txt::new(&unescape(s), size))
    }
}

/// Teks harfiah: markup creole di-escape dengan `~`.
pub fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "*/\"_-~<=".contains(c) {
            out.push('~');
        }
        out.push(c);
    }
    out
}

/// `\n`, `\t`, `<br>` → karakter; tanda kutip pembungkus dibuang.
pub fn unescape(s: &str) -> String {
    let s = s.trim();
    let s = if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    };
    s.replace("\\n", "\n")
        .replace("\\t", "    ")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("%newline()", "\n")
}

// ---------------------------------------------------------------------------
// Primitif

pub enum Prim {
    /// Poligon/garis: `closed` + `fill` untuk bentuk, `stroke` untuk tepi.
    Path {
        pts: Vec<Pt>,
        closed: bool,
        fill: Col,
        stroke: Col,
        w: f32,
        dash: Option<(f32, f32)>,
    },
    /// Teks ke-i dengan pojok kiri atas di (x, y).
    Text(usize, f32, f32),
}

impl Prim {
    pub fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Prim::Path { pts, .. } => pts.iter_mut().for_each(|p| {
                p.0 += dx;
                p.1 += dy;
            }),
            Prim::Text(_, x, y) => {
                *x += dx;
                *y += dy;
            }
        }
    }

    /// Kotak pembatas [x0, y0, x1, y1].
    pub fn bounds(&self, size: &[(f32, f32)]) -> [f32; 4] {
        match self {
            Prim::Path { pts, w, .. } => {
                let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                for p in pts {
                    b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
                }
                [
                    b[0] - w / 2.0,
                    b[1] - w / 2.0,
                    b[2] + w / 2.0,
                    b[3] + w / 2.0,
                ]
            }
            Prim::Text(i, x, y) => [*x, *y, x + size[*i].0, y + size[*i].1],
        }
    }
}

pub fn rect(r: [f32; 4], rad: f32, fill: Col, stroke: Col, w: f32) -> Prim {
    Prim::Path {
        pts: vector::rrect(r, rad),
        closed: true,
        fill,
        stroke,
        w,
        dash: None,
    }
}

pub fn poly(pts: Vec<Pt>, fill: Col, stroke: Col, w: f32) -> Prim {
    Prim::Path {
        pts,
        closed: true,
        fill,
        stroke,
        w,
        dash: None,
    }
}

pub fn ellipse(r: [f32; 4], fill: Col, stroke: Col, w: f32) -> Prim {
    Prim::Path {
        pts: vector::ellipse(r),
        closed: true,
        fill,
        stroke,
        w,
        dash: None,
    }
}

pub fn line(pts: Vec<Pt>, stroke: Col, w: f32) -> Prim {
    Prim::Path {
        pts,
        closed: false,
        fill: Col::None,
        stroke,
        w,
        dash: None,
    }
}

pub fn dashed(pts: Vec<Pt>, stroke: Col, w: f32, dash: (f32, f32)) -> Prim {
    Prim::Path {
        pts,
        closed: false,
        fill: Col::None,
        stroke,
        w,
        dash: Some(dash),
    }
}

/// Mata panah PlantUML (segitiga bertakik 10 × 8) berujung di `tip`, menghadap `dir`.
pub fn head(tip: Pt, dir: Pt, c: Col) -> Prim {
    let l = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
    let (ux, uy) = (dir.0 / l, dir.1 / l);
    let (nx, ny) = (-uy, ux);
    let at = |back: f32, side: f32| (tip.0 - ux * back + nx * side, tip.1 - uy * back + ny * side);
    poly(
        vec![at(10.0, -4.0), tip, at(10.0, 4.0), at(6.0, 0.0)],
        c,
        c,
        1.0,
    )
}

/// Mata panah terbuka (dua garis) — `->>` sequence, asosiasi berarah, dsb.
pub fn open_head(tip: Pt, dir: Pt, c: Col, w: f32) -> Prim {
    let l = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
    let (ux, uy) = (dir.0 / l, dir.1 / l);
    let (nx, ny) = (-uy, ux);
    let at = |back: f32, side: f32| (tip.0 - ux * back + nx * side, tip.1 - uy * back + ny * side);
    line(vec![at(10.0, -4.0), tip, at(10.0, 4.0)], c, w)
}

pub struct Placed {
    pub prims: Vec<Prim>,
    pub w: f32,
    pub h: f32,
}

impl Placed {
    /// Geser semua primitif supaya kotak pembatas mulai di (m, m); lebar/tinggi ikut.
    pub fn fit(mut self, size: &[(f32, f32)], m: f32) -> Placed {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in &self.prims {
            let q = p.bounds(size);
            b = [
                b[0].min(q[0]),
                b[1].min(q[1]),
                b[2].max(q[2]),
                b[3].max(q[3]),
            ];
        }
        if b[0] > b[2] {
            return Placed {
                prims: self.prims,
                w: 2.0 * m,
                h: 2.0 * m,
            };
        }
        self.prims
            .iter_mut()
            .for_each(|p| p.shift(m - b[0], m - b[1]));
        Placed {
            prims: self.prims,
            w: b[2] - b[0] + 2.0 * m,
            h: b[3] - b[1] + 2.0 * m,
        }
    }
}

// ---------------------------------------------------------------------------
// Diagram

pub trait Diagram {
    fn texts(&self) -> &[Txt];
    /// `size[i]` = (lebar, tinggi) teks ke-i dalam px PlantUML.
    fn place(&self, size: &[(f32, f32)]) -> Placed;
    /// Geser tegak teks judul (PlantUML berbeda per jenis diagram).
    fn title_dy(&self) -> f32 {
        0.0
    }
}

/// Bagian umum semua diagram: title, header, footer, caption, legend.
#[derive(Default)]
pub struct Frame {
    pub title: Option<String>,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub caption: Option<String>,
    /// (posisi `left|right|center` + `top|bottom`, isi).
    pub legend: Option<(String, String)>,
}

/// Diagram + bingkai umum → satu [`Diagram`].
pub struct Framed {
    inner: Box<dyn Diagram>,
    texts: Vec<Txt>,
    /// Indeks teks bingkai (title, header, footer, caption, legend) setelah teks inner.
    idx: [Option<usize>; 5],
    legend_pos: String,
}

impl Framed {
    fn new(inner: Box<dyn Diagram>, f: Frame) -> Framed {
        let mut texts = inner.texts().to_vec();
        let mut idx = [None; 5];
        let mut add = |k: usize, s: &Option<String>, t: fn(&str) -> Txt| {
            if let Some(s) = s {
                texts.push(t(&unescape(s)));
                idx[k] = Some(texts.len() - 1);
            }
        };
        add(0, &f.title, |s| Txt::new(s, 14.0).bold().center());
        add(1, &f.header, |s| {
            let mut t = Txt::new(s, 10.0).color(Col::Rgb(0x888888));
            t.align = Align::Right;
            t
        });
        add(2, &f.footer, |s| {
            Txt::new(s, 10.0).color(Col::Rgb(0x888888)).center()
        });
        add(3, &f.caption, |s| Txt::new(s, 14.0).center());
        let legend = f.legend.as_ref().map(|l| Some(l.1.clone())).unwrap_or(None);
        add(4, &legend, |s| Txt::new(s, 14.0));
        Framed {
            inner,
            texts,
            idx,
            legend_pos: f.legend.map(|l| l.0).unwrap_or_default(),
        }
    }
}

impl Diagram for Framed {
    fn texts(&self) -> &[Txt] {
        &self.texts
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let n = self.inner.texts().len();
        let body = self.inner.place(&size[..n]);
        let sz = |k: usize| self.idx[k].map_or((0.0, 0.0), |i| size[i]);
        let (title, header, footer, caption, legend) = (sz(0), sz(1), sz(2), sz(3), sz(4));
        let lw = if self.idx[4].is_some() {
            legend.0 + 2.0 * 5.0
        } else {
            0.0
        };
        let w = body
            .w
            .max(title.0 + 20.0)
            .max(header.0)
            .max(footer.0)
            .max(caption.0)
            .max(lw + 24.0);
        let mut prims = Vec::new();
        let mut y = 0.0;
        if let Some(i) = self.idx[1] {
            prims.push(Prim::Text(i, w - header.0, y));
            y += header.1;
        }
        if let Some(i) = self.idx[0] {
            // PlantUML: blok judul = tinggi teks + 22,29 (38,61 untuk 14pt).
            prims.push(Prim::Text(i, (w - title.0) / 2.0, y + 12.0 + self.inner.title_dy()));
            y += title.1 + 22.29;
        }
        let top = self.legend_pos.contains("top");
        let leg = |prims: &mut Vec<Prim>, y: &mut f32| {
            if let Some(i) = self.idx[4] {
                let (bw, bh) = (legend.0 + 10.0, legend.1 + 10.0);
                let x = if self.legend_pos.contains("left") {
                    12.0
                } else if self.legend_pos.contains("right") {
                    w - bw - 12.0
                } else {
                    (w - bw) / 2.0
                };
                prims.push(rect(
                    [x, *y + 12.0, bw, bh],
                    7.5,
                    Col::Rgb(0xDDDDDD),
                    Col::Text,
                    1.0,
                ));
                prims.push(Prim::Text(i, x + 5.0, *y + 17.0));
                *y += bh + 24.0;
            }
        };
        if top {
            leg(&mut prims, &mut y);
        }
        let dx = (w - body.w) / 2.0;
        for mut p in body.prims {
            p.shift(dx, y);
            prims.push(p);
        }
        y += body.h;
        if !top {
            leg(&mut prims, &mut y);
        }
        if let Some(i) = self.idx[3] {
            prims.push(Prim::Text(i, (w - caption.0) / 2.0, y + 1.0));
            y += caption.1 + 2.0;
        }
        if let Some(i) = self.idx[2] {
            prims.push(Prim::Text(i, (w - footer.0) / 2.0, y));
            y += footer.1;
        }
        Placed { prims, w, h: y }
    }
}

/// Baris sumber yang sudah dibersihkan: komentar `'` dan `/' ... '/` dibuang,
/// bingkai umum (title/header/...) dipisahkan, `skinparam`/`!theme` diabaikan.
pub fn prepare(src: &str) -> (Vec<String>, Frame) {
    let mut out = Vec::new();
    let mut f = Frame::default();
    let mut block: Option<(usize, Vec<String>, String)> = None; // (jenis, baris, posisi)
    let mut in_comment = false;
    let mut skin_depth = 0;
    let mut defs: Vec<(String, String)> = Vec::new();
    let has_end_legend = src
        .lines()
        .any(|l| matches!(l.trim().to_lowercase().as_str(), "endlegend" | "end legend"));
    for raw in src.lines() {
        let mut l = raw.trim().to_string();
        if in_comment {
            if let Some(k) = l.find("'/") {
                in_comment = false;
                l = l[k + 2..].trim().to_string();
            } else {
                continue;
            }
        }
        if let Some(k) = l.find("/'") {
            if let Some(e) = l[k..].find("'/") {
                l = format!("{}{}", &l[..k], &l[k + e + 2..]);
            } else {
                in_comment = true;
                l = l[..k].to_string();
            }
        }
        if l.starts_with('\'') {
            continue;
        }
        let low = l.to_lowercase();
        if skin_depth > 0 {
            skin_depth += l.matches('{').count();
            skin_depth -= l.matches('}').count().min(skin_depth);
            continue;
        }
        if let Some((k, lines, pos)) = &mut block {
            let end = [
                "endheader",
                "endfooter",
                "endtitle",
                "endlegend",
                "endcaption",
                "end header",
                "end footer",
                "end title",
                "end legend",
                "end caption",
            ];
            if end.contains(&low.as_str()) {
                let s = lines.join("\n");
                match k {
                    0 => f.title = Some(s),
                    1 => f.header = Some(s),
                    2 => f.footer = Some(s),
                    3 => f.caption = Some(s),
                    _ => f.legend = Some((pos.clone(), s)),
                }
                block = None;
            } else {
                lines.push(raw.trim_end().to_string());
            }
            continue;
        }
        let w0 = low.split_whitespace().next().unwrap_or("");
        let rest = l.get(w0.len()..).unwrap_or("").trim();
        let rest_s = rest.trim_start_matches(':').trim().to_string();
        let kind = ["title", "header", "footer", "caption", "legend"]
            .iter()
            .position(|k| *k == w0);
        let one_word = [
            "skinparam",
            "!theme",
            "scale",
            "!pragma",
            "hide",
            "show",
            "!include",
            "!includeurl",
            "!import",
            "allowmixing",
            "allow_mixing",
            "autonumber_unused",
            "left",
            "top",
            "newpage",
            "page",
            "skin",
            "!option",
        ];
        match (w0, kind) {
            ("skinparam" | "skin", _) if l.ends_with('{') => skin_depth = 1,
            (_, Some(k))
                if (k == 1 || k == 2)
                    && !rest_s.is_empty()
                    && !(low.starts_with("left header") || low.starts_with("right header")) =>
            {
                if k == 1 {
                    f.header = Some(rest_s)
                } else {
                    f.footer = Some(rest_s)
                }
            }
            (_, Some(4))
                if !has_end_legend && rest_s.chars().all(|c| c.is_alphabetic() || c == ' ') =>
            {
                out.push(raw.trim_end().to_string())
            }
            (_, Some(4)) => {
                if rest.trim().is_empty()
                    || ["left", "right", "top", "bottom", "center"]
                        .iter()
                        .any(|p| rest.to_lowercase().starts_with(p))
                {
                    block = Some((4, Vec::new(), rest.to_lowercase()));
                } else {
                    f.legend = Some((String::new(), rest_s));
                }
            }
            (_, Some(k)) if rest_s.is_empty() => block = Some((k, Vec::new(), String::new())),
            (_, Some(0)) => f.title = Some(rest_s),
            (_, Some(3)) => f.caption = Some(rest_s),
            ("left" | "right" | "center", _)
                if low.contains("header") || low.contains("footer") =>
            {
                let t = l
                    .splitn(3, char::is_whitespace)
                    .nth(2)
                    .unwrap_or("")
                    .to_string();
                if t.is_empty() {
                    block = Some((
                        if low.contains("header") { 1 } else { 2 },
                        Vec::new(),
                        String::new(),
                    ));
                } else if low.contains("header") {
                    f.header = Some(t);
                } else {
                    f.footer = Some(t);
                }
            }
            ("!define" | "!$define", _) => {
                let mut it = rest.splitn(2, char::is_whitespace);
                if let Some(k) = it.next() {
                    defs.push((k.to_string(), it.next().unwrap_or("").trim().to_string()));
                }
            }
            _ if w0.starts_with('$') && l.contains('=') && !l.contains("->") => {
                // `$var = "nilai"` / `!$var = ...`
                let (k, v) = l.split_once('=').unwrap();
                defs.push((
                    k.trim().trim_start_matches('!').to_string(),
                    v.trim().trim_matches('"').to_string(),
                ));
            }
            _ if w0.starts_with("!$") && l.contains('=') => {
                let (k, v) = l.split_once('=').unwrap();
                defs.push((
                    k.trim().trim_start_matches('!').to_string(),
                    v.trim().trim_matches('"').to_string(),
                ));
            }
            _ if one_word.contains(&w0)
                && !(w0 == "hide"
                    || w0 == "show"
                    || w0 == "left"
                    || w0 == "top"
                    || w0 == "page") => {}
            _ if w0.starts_with('!') && !matches!(w0, "!if" | "!else" | "!endif") => {}
            _ => {
                let mut l = raw.trim_end().to_string();
                for (k, v) in &defs {
                    if l.contains(k.as_str()) {
                        l = l.replace(k.as_str(), v);
                    }
                }
                out.push(l);
            }
        }
    }
    (out, f)
}

/// Pecah blok kode menjadi diagram-diagram (`@startXXX ... @endXXX`).
/// None bila ada diagram yang tak dikenali — blok tampil sebagai kode.
pub fn parse_all(src: &str) -> Option<Vec<Box<dyn Diagram>>> {
    let mut out: Vec<Box<dyn Diagram>> = Vec::new();
    let mut cur: Option<(String, Vec<&str>)> = None;
    for l in src.lines() {
        let t = l.trim();
        let low = t.to_lowercase();
        if let Some(rest) = low.strip_prefix("@start") {
            let kind = rest
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or("")
                .to_string();
            cur = Some((kind, Vec::new()));
        } else if low.starts_with("@end") {
            let (kind, lines) = cur.take()?;
            out.push(one(&kind, &lines.join("\n"))?);
        } else if let Some((_, c)) = &mut cur {
            c.push(l);
        }
    }
    if out.is_empty() {
        // Tanpa @start: sequence mermaid atau isi uml langsung.
        let first = src
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        if first == "sequenceDiagram" {
            return Some(vec![Box::new(sequence::parse(src)?)]);
        }
        out.push(one("uml", src)?);
    }
    Some(out)
}

fn one(kind: &str, src: &str) -> Option<Box<dyn Diagram>> {
    let (lines, frame) = prepare(src);
    let inner: Box<dyn Diagram> = match kind {
        "uml" | "" => uml(&lines)?,
        "mindmap" => Box::new(tree::parse_lines(&lines, false)?),
        "wbs" => Box::new(tree::parse_lines(&lines, true)?),
        "gantt" => Box::new(gantt::with_calendar(gantt::parse_lines(&lines)?)),
        "json" => Box::new(json::build(&json::parse_json(
            &lines
                .iter()
                .filter(|l| !l.trim_start().starts_with('#'))
                .cloned()
                .collect::<Vec<_>>()
                .join(
                    "
",
                ),
        )?)),
        "yaml" => Box::new(json::build(&json::parse_yaml(&lines)?)),
        "nwdiag" => Box::new(nwdiag::parse_lines(&lines)?),
        "ebnf" => Box::new(rail::parse_ebnf(&lines)?),
        "regex" => Box::new(rail::parse_regex(&lines)?),
        "salt" => Box::new(salt::parse_lines(&lines)?),
        "chen" => Box::new(chen::parse_lines(&lines)?),
        "chart" => Box::new(misc::parse_chart(&lines)?),
        "files" => Box::new(misc::parse_files(&lines)?),
        "board" => Box::new(misc::parse_board(&lines)?),
        "creole" => Box::new(misc::parse_creole(&lines)?),
        "dot" => Box::new(misc::parse_dot(&lines)?),
        "hcl" => Box::new(json::build(&misc::hcl_to_json(&lines)?)),
        "flow" => Box::new(alpha::parse_flow(&lines)?),
        "git" => Box::new(alpha::parse_git(&lines)?),
        "wire" => Box::new(alpha::parse_wire(&lines)?),
        "ditaa" => Box::new(ditaa::parse_lines(&lines)?),
        _ => return None,
    };
    Some(Box::new(Framed::new(inner, frame)))
}

/// `@startuml`: coba tiap jenis sesuai urutan deteksi PlantUML.
fn uml(lines: &[String]) -> Option<Box<dyn Diagram>> {
    let first = lines.iter().map(|l| l.trim()).position(|l| !l.is_empty());
    if let Some(k) = first.filter(|&k| lines[k].trim().starts_with("ditaa")) {
        return Some(Box::new(ditaa::parse_lines(&lines[k + 1..])?));
    }
    if let Some(k) = first.filter(|&k| lines[k].trim() == "salt") {
        return Some(Box::new(salt::parse_lines(&lines[k + 1..])?));
    }
    if lines
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .is_some_and(|l| l.starts_with("nwdiag"))
    {
        return Some(Box::new(nwdiag::parse_lines(lines)?));
    }
    if let Some(d) = sequence::parse_lines(lines) {
        return Some(Box::new(d));
    }
    if let Some(d) = timing::parse_lines(lines) {
        return Some(Box::new(d));
    }
    if let Some(d) = activity::parse_lines(lines) {
        return Some(Box::new(d));
    }
    if lines.iter().any(|l| {
        l.trim_start().starts_with("(*")
            || l.contains("--> (*")
            || l.trim_start().starts_with("===")
    }) {
        if let Some(d) = oldact::parse_lines(lines) {
            return Some(Box::new(d));
        }
    }
    if let Some(d) = desc::parse_lines(lines) {
        return Some(Box::new(d));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(Col::parse("#F00"), Some(Col::Rgb(0xFF0000)));
        assert_eq!(Col::parse("red"), Some(Col::Rgb(0xFF0000)));
        assert_eq!(Col::parse("#LightBlue"), Some(Col::Rgb(0xADD8E6)));
        assert_eq!(Col::parse("#a1b2c3"), Some(Col::Rgb(0xA1B2C3)));
    }

    #[test]
    fn prepare_frame() {
        let (l, f) = prepare("title Halo\n' komentar\nskinparam x {\n a b\n}\nA -> B\nlegend right\nisi\nendlegend\nfooter kaki");
        assert_eq!(l, vec!["A -> B"]);
        assert_eq!(f.title.as_deref(), Some("Halo"));
        assert_eq!(f.footer.as_deref(), Some("kaki"));
        assert_eq!(f.legend, Some(("right".into(), "isi".into())));
    }
}
