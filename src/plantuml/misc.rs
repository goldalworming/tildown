//! Diagram kecil: `@startchart` (bar/line/area/scatter), `@startfiles` (pohon
//! berkas), `@startboard` (story map), `@starthcl` (tabel seperti JSON),
//! `@startcreole` (teks kaya), `@startdot` (bahasa DOT → [`super::graph`]).
// ponytail: chart tanpa sumbu v2, stack, anotasi; DOT tanpa record/HTML label,
// port, atribut selain label/shape/style/color/rankdir.

use super::graph::{self, Graph};
use super::*;

// ---------------------------------------------------------------------------
// Chart

enum SKind {
    Bar,
    Line,
    Area,
    Scatter,
}

struct Series {
    name: usize,
    kind: SKind,
    data: Vec<f32>,
    color: Col,
}

pub struct Chart {
    texts: Texts,
    cats: Vec<usize>,
    series: Vec<Series>,
    vmin: Option<f32>,
    vmax: Option<f32>,
    vtitle: Option<usize>,
    htitle: Option<usize>,
    legend: bool,
    ticks: Vec<usize>,
    tick_vals: Vec<f32>,
}

const PALETTE: [u32; 6] = [0x3498DB, 0xE74C3C, 0x2ECC71, 0xF39C12, 0x9B59B6, 0x1ABC9C];

fn list(s: &str) -> Vec<String> {
    let a = s.find('[').map_or(0, |k| k + 1);
    let b = s.rfind(']').unwrap_or(s.len());
    s[a..b]
        .split(',')
        .map(|x| x.trim().trim_matches('"').to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

pub fn parse_chart(lines: &[String]) -> Option<Chart> {
    let mut c = Chart {
        texts: Texts::default(),
        cats: Vec::new(),
        series: Vec::new(),
        vmin: None,
        vmax: None,
        vtitle: None,
        htitle: None,
        legend: false,
        ticks: Vec::new(),
        tick_vals: Vec::new(),
    };
    for raw in lines {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        let quoted = || l.split('"').nth(1).map(|s| s.to_string());
        if low.starts_with("h-axis") || low.starts_with("x-axis") {
            if let Some(t) = quoted() {
                c.htitle = Some(c.texts.add(Txt::new(&t, 14.0)));
            }
            if l.contains('[') {
                for s in list(l) {
                    let t = c.texts.add(Txt::new(&s, 14.0));
                    c.cats.push(t);
                }
            }
        } else if low.starts_with("v-axis") || low.starts_with("y-axis") {
            if let Some(t) = quoted() {
                c.vtitle = Some(c.texts.add(Txt::new(&t, 14.0)));
            }
            let nums: Vec<f32> = l
                .split(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-'))
                .filter_map(|x| x.parse().ok())
                .collect();
            if l.contains("-->") && nums.len() >= 2 {
                c.vmin = Some(nums[0]);
                c.vmax = Some(nums[nums.len() - 1]);
            }
        } else if let Some(kind) = ["bar", "line", "area", "scatter"]
            .iter()
            .position(|k| low.starts_with(k))
        {
            let name = quoted().unwrap_or_default();
            let data: Vec<f32> = list(l).iter().filter_map(|x| x.parse().ok()).collect();
            let color = l
                .rsplit('#')
                .next()
                .filter(|_| l.contains('#'))
                .and_then(|x| Col::parse(x.split_whitespace().next().unwrap_or("")))
                .unwrap_or(Col::Rgb(PALETTE[c.series.len() % 6]));
            let name = c.texts.add(Txt::new(&name, 14.0));
            let kind = match kind {
                0 => SKind::Bar,
                1 => SKind::Line,
                2 => SKind::Area,
                _ => SKind::Scatter,
            };
            c.series.push(Series {
                name,
                kind,
                data,
                color,
            });
        } else if low.starts_with("legend") {
            c.legend = true;
        } else if low.starts_with("orientation")
            || low.starts_with("stackmode")
            || low.starts_with("annotation")
        {
        } else {
            return None;
        }
    }
    if c.series.is_empty() {
        return None;
    }
    let n = c.series.iter().map(|s| s.data.len()).max().unwrap_or(0);
    while c.cats.len() < n {
        let t = c.texts.add(Txt::new(&(c.cats.len() + 1).to_string(), 14.0));
        c.cats.push(t);
    }
    let vmax = c.vmax.unwrap_or_else(|| {
        c.series
            .iter()
            .flat_map(|s| s.data.iter().copied())
            .fold(0.0, f32::max)
            * 1.1
    });
    let vmin = c.vmin.unwrap_or(0.0);
    for i in 0..=5 {
        let v = vmin + (vmax - vmin) * i as f32 / 5.0;
        let s = if (v - v.round()).abs() < 1e-3 {
            format!("{}", v.round() as i64)
        } else {
            format!("{v:.1}")
        };
        let t = c.texts.add(Txt::new(&s, 14.0));
        c.ticks.push(t);
        c.tick_vals.push(v);
    }
    c.vmin = Some(vmin);
    c.vmax = Some(vmax);
    Some(c)
}

impl Diagram for Chart {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn title_dy(&self) -> f32 {
        8.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        // ChartRenderer: MARGIN 20, AXIS_LABEL_SPACE 40, TITLE_SPACE 30, plot 400×300;
        // ditambah margin dokumen 10.
        let (x0, y0, w, h) = (70.0, 60.0, 400.0, 300.0);
        let ph = |t: usize| self.texts.0[t].size * 1.2578;
        let (vmin, vmax) = (self.vmin.unwrap_or(0.0), self.vmax.unwrap_or(1.0));
        let vy = |v: f32| y0 + h - (v - vmin) / (vmax - vmin).max(1e-6) * h;
        // Sumbu-x di garis nol bila rentang memuat nol.
        let ax = if vmin <= 0.0 && vmax >= 0.0 { vy(0.0) } else { y0 + h };
        let n = self.cats.len().max(1);
        let cw = w / n as f32;
        let cx = |i: usize| x0 + cw * (i as f32 + 0.5);
        p.push(line(vec![(x0, y0), (x0, y0 + h)], Col::Ink, 0.5));
        for (i, &t) in self.ticks.iter().enumerate() {
            let y = vy(self.tick_vals[i]);
            p.push(line(vec![(x0, y), (x0 - 5.0, y)], Col::Ink, 0.5));
            p.push(Prim::Text(t, x0 - 10.0 - size[t].0, y - ph(t) / 2.0));
        }
        if let Some(t) = self.vtitle {
            // ponytail: PlantUML memutar judul -90°; renderer belum bisa memutar
            // teks, jadi mendatar di pusat teks putar itu.
            p.push(Prim::Text(t, x0 - 48.2 - size[t].0 / 2.0, y0 + h / 2.0 - ph(t) / 2.0));
        }
        p.push(line(vec![(x0, ax), (x0 + w, ax)], Col::Ink, 0.5));
        for (i, &t) in self.cats.iter().enumerate() {
            p.push(line(vec![(cx(i), ax), (cx(i), ax + 5.0)], Col::Ink, 0.5));
            p.push(Prim::Text(t, cx(i) - size[t].0 / 2.0, ax + 10.0));
        }
        if let Some(t) = self.htitle {
            p.push(Prim::Text(t, x0 + w / 2.0 - size[t].0 / 2.0, y0 + h + 32.0));
        }
        let bars: Vec<usize> = (0..self.series.len())
            .filter(|&k| matches!(self.series[k].kind, SKind::Bar))
            .collect();
        for (si, s) in self.series.iter().enumerate() {
            match s.kind {
                SKind::Bar => {
                    let bi = bars.iter().position(|&k| k == si).unwrap_or(0);
                    let bw = cw * 0.6 / bars.len().max(1) as f32;
                    for (i, &v) in s.data.iter().enumerate() {
                        let x = cx(i) - cw * 0.3 + bw * bi as f32;
                        let (ya, yb) = (vy(v), ax);
                        p.push(rect(
                            [x, ya.min(yb), bw, (yb - ya).abs()],
                            0.0,
                            s.color,
                            Col::Ink,
                            0.5,
                        ));
                    }
                }
                SKind::Line | SKind::Area => {
                    let pts: Vec<Pt> = s
                        .data
                        .iter()
                        .enumerate()
                        .map(|(i, &v)| (cx(i), vy(v)))
                        .collect();
                    if matches!(s.kind, SKind::Area) && pts.len() > 1 {
                        let mut a = pts.clone();
                        a.push((pts[pts.len() - 1].0, ax));
                        a.push((pts[0].0, ax));
                        p.push(poly(a, s.color, Col::None, 0.0));
                    }
                    p.push(line(pts, s.color, 2.0));
                }
                SKind::Scatter => {
                    for (i, &v) in s.data.iter().enumerate() {
                        p.push(ellipse(
                            [cx(i) - 4.0, vy(v) - 4.0, 8.0, 8.0],
                            s.color,
                            s.color,
                            1.0,
                        ));
                    }
                }
            }
        }
        // Legenda kanan: simbol 12, jarak teks 5, antarbaris tinggi teks + 15.
        let mut lw = 0.0;
        if self.legend {
            let lx = x0 + w + 10.0;
            let mut y = y0;
            for s in &self.series {
                match s.kind {
                    SKind::Line => p.push(line(vec![(lx, y + 6.0), (lx + 12.0, y + 6.0)], s.color, 2.0)),
                    _ => p.push(rect([lx, y, 12.0, 12.0], 0.0, s.color, s.color, 1.0)),
                }
                p.push(Prim::Text(s.name, lx + 17.0, y));
                y += ph(s.name) + 15.0;
                lw = f32::max(lw, size[s.name].0 + 37.0 + 10.0);
            }
        }
        Placed {
            prims: p,
            w: 10.0 + 480.0 + lw + 10.0,
            h: 10.0 + 410.0 + if self.htitle.is_some() { 20.0 } else { 0.0 } + 10.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Files

pub struct Files {
    texts: Texts,
    /// (kedalaman, teks, folder?)
    rows: Vec<(usize, usize, bool)>,
}

pub fn parse_files(lines: &[String]) -> Option<Files> {
    // Bangun pohon dari jalur `/a/b/c`.
    let mut paths: Vec<Vec<String>> = Vec::new();
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if !l.starts_with('/') {
            return None;
        }
        paths.push(
            l.trim_start_matches('/')
                .split('/')
                .map(String::from)
                .collect(),
        );
    }
    let mut f = Files {
        texts: Texts::default(),
        rows: Vec::new(),
    };
    // Urutan: sesuai kemunculan, folder sebelum isinya.
    let mut seen: Vec<Vec<String>> = Vec::new();
    for p in &paths {
        for d in 1..=p.len() {
            let pre = p[..d].to_vec();
            if !seen.contains(&pre) {
                let is_dir = d < p.len() || paths.iter().any(|q| q.len() > d && q[..d] == pre[..]);
                let t = f.texts.add(Txt::new(&p[d - 1], 14.0));
                f.rows.push((d - 1, t, is_dir));
                seen.push(pre);
            }
        }
    }
    // Kelompokkan anak di bawah induknya (urutan stabil per induk).
    (!f.rows.is_empty()).then_some(f)
}

impl Diagram for Files {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        let mut y = 0.0;
        for &(d, t, dir) in &self.rows {
            let x = d as f32 * 18.0;
            let (_, th) = size[t];
            let h = th.max(16.0) + 5.0;
            let iy = y + (h - 12.0) / 2.0;
            if dir {
                p.push(poly(
                    vec![
                        (x, iy),
                        (x + 6.0, iy),
                        (x + 8.0, iy + 2.0),
                        (x + 16.0, iy + 2.0),
                        (x + 16.0, iy + 12.0),
                        (x, iy + 12.0),
                    ],
                    Col::Rgb(0x555555),
                    Col::Rgb(0x333333),
                    0.5,
                ));
            } else {
                p.push(poly(
                    vec![
                        (x + 2.0, iy - 1.0),
                        (x + 10.0, iy - 1.0),
                        (x + 14.0, iy + 3.0),
                        (x + 14.0, iy + 13.0),
                        (x + 2.0, iy + 13.0),
                    ],
                    Col::Rgb(0xDDDDDD),
                    Col::Rgb(0x888888),
                    0.5,
                ));
                for k in 0..3 {
                    let ly = iy + 5.0 + k as f32 * 2.5;
                    p.push(line(
                        vec![(x + 4.0, ly), (x + 12.0, ly)],
                        Col::Rgb(0x888888),
                        0.5,
                    ));
                }
            }
            p.push(Prim::Text(t, x + 18.0, y + (h - th) / 2.0));
            y += h;
        }
        Placed {
            prims: p,
            w: 0.0,
            h: 0.0,
        }
        .fit(size, 8.0)
    }
}

// ---------------------------------------------------------------------------
// Board

pub struct Board {
    texts: Texts,
    /// (kolom, baris, teks)
    cards: Vec<(usize, usize, usize)>,
}

pub fn parse_board(lines: &[String]) -> Option<Board> {
    let mut b = Board {
        texts: Texts::default(),
        cards: Vec::new(),
    };
    let mut col: isize = -1;
    let mut next_row = vec![0usize; 0];
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let depth = l.chars().take_while(|&c| c == '+').count();
        let text = l[depth..].trim();
        if depth == 0 {
            col += 1;
            next_row.push(1);
            let t = b.texts.add(Txt::new(text, 14.0));
            b.cards.push((col as usize, 0, t));
        } else {
            let c = col.max(0) as usize;
            if next_row.is_empty() {
                next_row.push(1);
            }
            // Baris = kedalaman; kartu di baris yang sudah terisi bergeser ke kolom berikut.
            let row = depth;
            let mut cc = c;
            while b.cards.iter().any(|k| k.0 == cc && k.1 == row) {
                cc += 1;
            }
            if cc as isize > col {
                col = cc as isize;
            }
            let t = b.texts.add(Txt::new(text, 14.0));
            b.cards.push((cc, row, t));
        }
    }
    (!b.cards.is_empty()).then_some(b)
}

impl Diagram for Board {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        let (cw, ch) = (127.0, 60.0);
        let rows = self.cards.iter().map(|c| c.1).max().unwrap_or(0) + 1;
        let cols = self.cards.iter().map(|c| c.0).max().unwrap_or(0) + 1;
        let width = cols as f32 * (cw + 16.0);
        for r in 1..rows {
            let y = r as f32 * (ch + 16.0) - 8.0;
            p.push(dashed(
                vec![(0.0, y), (width, y)],
                Col::Rgb(0x555555),
                1.0,
                (4.0, 3.0),
            ));
        }
        for &(c, r, t) in &self.cards {
            let (x, y) = (c as f32 * (cw + 16.0), r as f32 * (ch + 16.0));
            p.push(rect(
                [x + 2.0, y + 2.0, cw, ch],
                0.0,
                Col::Rgb(0x999999),
                Col::None,
                0.0,
            ));
            p.push(rect([x, y, cw, ch], 0.0, Col::Rgb(0xC0C0C0), Col::Ink, 0.5));
            p.push(Prim::Text(t, x + 3.0, y + 2.0));
        }
        let _ = size;
        Placed {
            prims: p,
            w: 0.0,
            h: 0.0,
        }
        .fit(size, 8.0)
    }
}

// ---------------------------------------------------------------------------
// Creole

pub struct Creole {
    texts: Texts,
}

pub fn parse_creole(lines: &[String]) -> Option<Creole> {
    let mut t = Texts::default();
    for l in lines {
        let s = l.trim_end();
        let tx = if let Some(h) = s.strip_prefix("==") {
            Txt::new(h.trim_matches('=').trim(), 16.0).bold()
        } else if let Some(h) = s.strip_prefix('=') {
            Txt::new(h.trim_matches('=').trim(), 20.0).bold()
        } else {
            Txt::new(s, 14.0)
        };
        t.add(tx);
    }
    (!t.0.is_empty()).then_some(Creole { texts: t })
}

impl Diagram for Creole {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        let mut y = 0.0;
        for (i, s) in size.iter().enumerate() {
            p.push(Prim::Text(i, 0.0, y));
            y += s.1.max(8.0);
        }
        Placed {
            prims: p,
            w: 0.0,
            h: 0.0,
        }
        .fit(size, 5.0)
    }
}

// ---------------------------------------------------------------------------
// DOT

#[derive(Clone)]
struct DNode {
    id: String,
    label: usize,
    shape: String,
    style: String,
    color: Option<Col>,
    fill: Option<Col>,
    cluster: Option<usize>,
}

struct DEdge {
    a: usize,
    b: usize,
    label: Option<usize>,
    style: String,
    color: Option<Col>,
}

pub struct Dot {
    texts: Texts,
    nodes: Vec<DNode>,
    edges: Vec<DEdge>,
    clusters: Vec<(Option<usize>, Option<usize>)>,
    directed: bool,
    lr: bool,
}

/// Token DOT: identitas, string berkutip, simbol.
fn dot_tokens(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') || ch == '#' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if ch == '"' {
            let mut t = String::from("\"");
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' && i + 1 < c.len() {
                    t.push(c[i]);
                    i += 1;
                }
                t.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(t);
        } else if ch == '-' && matches!(c.get(i + 1), Some('>') | Some('-')) {
            out.push(format!("-{}", c[i + 1]));
            i += 2;
        } else if "{}[];=,:".contains(ch) {
            out.push(ch.to_string());
            i += 1;
        } else {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
            if i == st {
                i += 1;
            }
            out.push(c[st..i].iter().collect());
        }
    }
    out
}

fn dq(s: &str) -> String {
    s.strip_prefix('"')
        .unwrap_or(s)
        .replace("\\n", "\n")
        .replace("\\l", "\n")
        .replace("\\\"", "\"")
}

pub fn parse_dot(lines: &[String]) -> Option<Dot> {
    let toks = dot_tokens(&lines.join("\n"));
    let mut d = Dot {
        texts: Texts::default(),
        nodes: Vec::new(),
        edges: Vec::new(),
        clusters: Vec::new(),
        directed: true,
        lr: false,
    };
    let mut i = 0;
    while i < toks.len() && !matches!(toks[i].as_str(), "graph" | "digraph") {
        i += 1;
    }
    d.directed = toks.get(i)? == "digraph";
    while i < toks.len() && toks[i] != "{" {
        i += 1;
    }
    i += 1;
    let mut stack: Vec<Option<usize>> = vec![None];
    while i < toks.len() {
        let t = toks[i].clone();
        i += 1;
        match t.as_str() {
            ";" | "," => continue,
            "}" => {
                stack.pop();
                if stack.is_empty() {
                    break;
                }
                continue;
            }
            "subgraph" => {
                let mut name = String::new();
                if toks.get(i).is_some_and(|x| x != "{") {
                    name = dq(&toks[i]);
                    i += 1;
                }
                if toks.get(i).map(|x| x.as_str()) == Some("{") {
                    i += 1;
                }
                let parent = *stack.last().unwrap();
                let cl = if name.starts_with("cluster") {
                    d.clusters.push((parent, None));
                    Some(d.clusters.len() - 1)
                } else {
                    parent
                };
                stack.push(cl);
                continue;
            }
            "{" => {
                stack.push(*stack.last().unwrap());
                continue;
            }
            "graph" | "node" | "edge" => {
                let at = dot_attrs(&toks, &mut i);
                if t == "graph" {
                    if let Some(v) = at.iter().find(|a| a.0 == "rankdir") {
                        d.lr = v.1 == "LR" || v.1 == "RL";
                    }
                }
                continue;
            }
            _ => {}
        }
        // `key = value` (atribut graf/subgraf)
        if toks.get(i).map(|x| x.as_str()) == Some("=") {
            let v = dq(toks.get(i + 1)?);
            i += 2;
            match t.as_str() {
                "rankdir" => d.lr = v == "LR" || v == "RL",
                "label" => {
                    if let Some(Some(c)) = stack.last() {
                        let lt = d.texts.add(Txt::new(&v, 14.0));
                        d.clusters[*c].1 = Some(lt);
                    }
                }
                _ => {}
            }
            continue;
        }
        // Node atau rantai sisi: a -> b -> c [attr]
        let cl = *stack.last().unwrap();
        let mut chain = vec![d.node(&dq(&t), cl)];
        while matches!(toks.get(i).map(|x| x.as_str()), Some("->") | Some("--")) {
            i += 1;
            let n = toks.get(i)?.clone();
            i += 1;
            chain.push(d.node(&dq(&n), cl));
        }
        let at = if toks.get(i).map(|x| x.as_str()) == Some("[") {
            dot_attrs(&toks, &mut i)
        } else {
            Vec::new()
        };
        if chain.len() == 1 {
            let n = chain[0];
            for (k, v) in at {
                match k.as_str() {
                    "label" => d.texts.0[d.nodes[n].label].s = unescape(&v),
                    "shape" => d.nodes[n].shape = v.to_lowercase(),
                    "style" => d.nodes[n].style = v.to_lowercase(),
                    "color" => d.nodes[n].color = Col::parse(&v),
                    "fillcolor" => d.nodes[n].fill = Col::parse(&v),
                    _ => {}
                }
            }
        } else {
            let label = at
                .iter()
                .find(|a| a.0 == "label")
                .map(|a| d.texts.add(Txt::new(&unescape(&a.1), 14.0)));
            let style = at
                .iter()
                .find(|a| a.0 == "style")
                .map(|a| a.1.to_lowercase())
                .unwrap_or_default();
            let color = at
                .iter()
                .find(|a| a.0 == "color")
                .and_then(|a| Col::parse(&a.1));
            for w in chain.windows(2) {
                d.edges.push(DEdge {
                    a: w[0],
                    b: w[1],
                    label,
                    style: style.clone(),
                    color,
                });
            }
        }
    }
    (!d.nodes.is_empty()).then_some(d)
}

fn dot_attrs(toks: &[String], i: &mut usize) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if toks.get(*i).map(|x| x.as_str()) != Some("[") {
        return out;
    }
    *i += 1;
    while *i < toks.len() && toks[*i] != "]" {
        let k = toks[*i].to_lowercase();
        if toks.get(*i + 1).map(|x| x.as_str()) == Some("=") {
            out.push((k, dq(toks.get(*i + 2).map(|x| x.as_str()).unwrap_or(""))));
            *i += 3;
        } else {
            *i += 1;
        }
    }
    *i += 1;
    out
}

impl Dot {
    fn node(&mut self, id: &str, cl: Option<usize>) -> usize {
        if let Some(k) = self.nodes.iter().position(|n| n.id == id) {
            if self.nodes[k].cluster.is_none() {
                self.nodes[k].cluster = cl;
            }
            return k;
        }
        let label = self.texts.add(Txt::new(id, 14.0));
        self.nodes.push(DNode {
            id: id.to_string(),
            label,
            shape: "ellipse".into(),
            style: String::new(),
            color: None,
            fill: None,
            cluster: cl,
        });
        self.nodes.len() - 1
    }
}

impl Diagram for Dot {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let dims: Vec<(f32, f32)> = self
            .nodes
            .iter()
            .map(|n| {
                // Ukuran bawaan Graphviz: min 54×36, margin 8×4; elips/belah ketupat √2.
                let (tw, th) = size[n.label];
                let (bw, bh) = (tw + 16.0, th + 8.0);
                match n.shape.as_str() {
                    "box" | "rect" | "rectangle" | "square" | "note" | "component" | "folder"
                    | "tab" => (bw.max(54.0), bh.max(36.0)),
                    "plaintext" | "plain" | "none" => (bw, bh),
                    "circle" | "doublecircle" => {
                        let r = (bw.max(bh) * 1.2).max(36.0);
                        (r, r)
                    }
                    "point" => (8.0, 8.0),
                    "diamond" => ((bw * 2.0).max(54.0), (bh * 2.0).max(36.0)),
                    _ => ((bw * 1.414).max(54.0), (bh * 1.414).max(36.0)),
                }
            })
            .collect();
        let g = Graph {
            nodes: self
                .nodes
                .iter()
                .zip(&dims)
                .map(|(n, d)| graph::Node {
                    w: d.0,
                    h: d.1,
                    shape: if matches!(
                        n.shape.as_str(),
                        "box"
                            | "rect"
                            | "rectangle"
                            | "square"
                            | "plaintext"
                            | "plain"
                            | "none"
                            | "note"
                    ) {
                        graph::Shape::Rect
                    } else {
                        graph::Shape::Ellipse
                    },
                    cluster: n.cluster,
                })
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|e| graph::Edge {
                    a: e.a,
                    b: e.b,
                    minlen: 1,
                    label: e.label.map(|t| size[t]),
                    ordered: false,
                })
                .collect(),
            clusters: self
                .clusters
                .iter()
                .map(|c| graph::Cluster {
                    parent: c.0,
                    pad: [c.1.map_or(12.0, |t| size[t].1 + 12.0), 10.0, 10.0, 10.0],
                    min_w: c.1.map_or(0.0, |t| size[t].0 + 20.0),
                })
                .collect(),
            lr: self.lr,
            nodesep: 18.0,
            ranksep: 36.0,
        };
        let o = graph::layout(&g);
        let mut p = Vec::new();
        for (k, c) in self.clusters.iter().enumerate() {
            let r = o.clusters[k];
            p.push(rect(r, 0.0, Col::None, Col::Text, 1.0));
            if let Some(t) = c.1 {
                p.push(Prim::Text(t, r[0] + (r[2] - size[t].0) / 2.0, r[1] + 4.0));
            }
        }
        for (ei, e) in self.edges.iter().enumerate() {
            let pts = o.edges[ei].clone();
            if pts.len() < 2 || e.style.contains("invis") {
                continue;
            }
            let c = e.color.unwrap_or(Col::Text);
            p.push(if e.style.contains("dash") {
                dashed(pts.clone(), c, 1.0, (5.0, 4.0))
            } else if e.style.contains("dot") {
                dashed(pts.clone(), c, 1.0, (1.0, 3.0))
            } else {
                line(
                    pts.clone(),
                    c,
                    if e.style.contains("bold") { 2.0 } else { 1.0 },
                )
            });
            if self.directed {
                let n = pts.len();
                p.push(head(
                    pts[n - 1],
                    (pts[n - 1].0 - pts[n - 2].0, pts[n - 1].1 - pts[n - 2].1),
                    c,
                ));
            }
            if let (Some(t), Some(m)) = (e.label, o.labels[ei]) {
                p.push(Prim::Text(t, m.0 - size[t].0 / 2.0, m.1 - size[t].1 / 2.0));
            }
        }
        for (i, n) in self.nodes.iter().enumerate() {
            let (cx, cy) = o.pos[i];
            let (w, h) = dims[i];
            let r = [cx - w / 2.0, cy - h / 2.0, w, h];
            let stroke = n.color.unwrap_or(Col::Text);
            let fill = if n.style.contains("filled") {
                n.fill.or(n.color).unwrap_or(Col::Rgb(0xD3D3D3))
            } else {
                Col::Paper
            };
            match n.shape.as_str() {
                "box" | "rect" | "rectangle" | "square" | "note" | "component" | "folder"
                | "tab" => p.push(rect(
                    r,
                    if n.style.contains("rounded") {
                        8.0
                    } else {
                        0.0
                    },
                    fill,
                    stroke,
                    1.0,
                )),
                "plaintext" | "plain" | "none" => {}
                "diamond" => p.push(poly(
                    vec![(cx, r[1]), (r[0] + w, cy), (cx, r[1] + h), (r[0], cy)],
                    fill,
                    stroke,
                    1.0,
                )),
                "point" => p.push(ellipse(r, Col::Text, Col::Text, 1.0)),
                "doublecircle" => {
                    p.push(ellipse(r, fill, stroke, 1.0));
                    p.push(ellipse(
                        [r[0] + 4.0, r[1] + 4.0, w - 8.0, h - 8.0],
                        fill,
                        stroke,
                        1.0,
                    ));
                }
                _ => p.push(ellipse(r, fill, stroke, 1.0)),
            }
            if n.shape != "point" {
                let (tw, th) = size[n.label];
                p.push(Prim::Text(n.label, cx - tw / 2.0, cy - th / 2.0));
            }
        }
        Placed {
            prims: p,
            w: o.w,
            h: o.h,
        }
        .fit(size, 6.0)
    }
}

// ---------------------------------------------------------------------------
// HCL → JSON

pub fn hcl_to_json(lines: &[String]) -> Option<super::json::J> {
    use super::json::J;
    fn block(ls: &[&str], i: &mut usize) -> Option<Vec<(String, J)>> {
        let mut out = Vec::new();
        while *i < ls.len() {
            let l = ls[*i].trim();
            *i += 1;
            if l.is_empty() || l.starts_with('#') || l.starts_with("//") {
                continue;
            }
            if l == "}" {
                return Some(out);
            }
            if let Some((k, v)) = l.split_once('=') {
                let v = v.trim();
                let val = if v == "{" {
                    J::Obj(block(ls, i)?)
                } else if v.starts_with('[') {
                    J::Arr(
                        v.trim_matches(['[', ']'])
                            .split(',')
                            .map(|x| x.trim().trim_matches('"'))
                            .filter(|x| !x.is_empty())
                            .map(|x| J::Str(x.to_string()))
                            .collect(),
                    )
                } else if v == "true" || v == "false" {
                    J::Bool(v == "true")
                } else if v.parse::<f64>().is_ok() {
                    J::Num(v.to_string())
                } else {
                    J::Str(v.trim_matches('"').to_string())
                };
                out.push((k.trim().to_string(), val));
            } else if let Some(head) = l.strip_suffix('{') {
                let name = head
                    .split_whitespace()
                    .map(|w| w.trim_matches('"'))
                    .collect::<Vec<_>>()
                    .join(" ");
                out.push((name, J::Obj(block(ls, i)?)));
            } else {
                return None;
            }
        }
        Some(out)
    }
    let ls: Vec<&str> = lines.iter().map(|l| l.as_str()).collect();
    let mut i = 0;
    // Kepala blok (`resource "a" "b"`) tidak ditampilkan PlantUML: ratakan isinya.
    let top = block(&ls, &mut i)?;
    let mut flat = Vec::new();
    for (k, v) in top {
        match v {
            J::Obj(inner) if k.contains(' ') => flat.extend(inner),
            v => flat.push((k, v)),
        }
    }
    Some(J::Obj(flat))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn misc_parsers() {
        let c = parse_chart(&l("h-axis [A, B]\nv-axis \"U\" 0 --> 40\nbar \"S\" [10, 20] #3498db\nline \"T\" [5, 15]\nlegend right")).unwrap();
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.cats.len(), 2);
        let f = parse_files(&l("/src/a.rs\n/src/b.rs\n/Cargo.toml")).unwrap();
        assert_eq!(f.rows.len(), 4);
        assert!(f.rows[0].2);
        let b = parse_board(&l("A1\n+A2\n++A3\n+A4\nB1\n+B2")).unwrap();
        assert_eq!(b.cards.len(), 6);
        let d = parse_dot(&l("digraph G {\n rankdir=LR;\n a [label=\"Mulai\", shape=box];\n a -> b -> c [label=\"x\"];\n subgraph cluster_1 { label=\"G\"; c; }\n}")).unwrap();
        assert_eq!(d.nodes.len(), 3);
        assert_eq!(d.edges.len(), 2);
        assert!(d.lr);
        assert_eq!(d.nodes[2].cluster, Some(0));
        let h = hcl_to_json(&l(
            "resource \"aws\" \"web\" {\n ami = \"abc\"\n count = 2\n}",
        ))
        .unwrap();
        assert!(matches!(h, super::super::json::J::Obj(ref o) if o.len() == 2));
        let pl = d.place(&vec![(30.0, 16.0); d.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
