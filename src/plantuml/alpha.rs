//! Diagram alpha PlantUML: `@startflow` (kotak di grid, arah n/s/e/w),
//! `@startgit` (grafik `git log --graph`), `@startwire` (blok berukuran tetap).
// ponytail: wire tanpa `goto`/`move`/`spot`; git hanya cabang `| \ /` sederhana.

use super::graph::{self, Graph};
use super::*;

// ---------------------------------------------------------------------------
// Flow

pub struct Flow {
    texts: Texts,
    /// (id, teks, kolom, baris)
    boxes: Vec<(String, usize, i32, i32)>,
    links: Vec<(usize, usize)>,
}

pub fn parse_flow(lines: &[String]) -> Option<Flow> {
    let mut f = Flow {
        texts: Texts::default(),
        boxes: Vec::new(),
        links: Vec::new(),
    };
    let mut cur: Option<usize> = None;
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let mut w: Vec<&str> = l.splitn(2, ' ').collect();
        let dir = match w[0] {
            "n" | "s" | "e" | "w" if w.len() > 1 => {
                let d = w[0];
                w = w[1].splitn(2, ' ').collect();
                Some(d)
            }
            _ => None,
        };
        let id = w[0].to_string();
        let label = w.get(1).map(|s| s.trim().trim_matches('"').to_string());
        let (cx, cy) = cur.map_or((0, 0), |k| (f.boxes[k].2, f.boxes[k].3));
        let (nx, ny) = match dir {
            Some("n") => (cx, cy - 1),
            Some("s") => (cx, cy + 1),
            Some("e") => (cx + 1, cy),
            Some("w") => (cx - 1, cy),
            _ => (cx, cy),
        };
        let k = match f.boxes.iter().position(|b| b.0 == id) {
            Some(k) => k,
            None => {
                let t = f
                    .texts
                    .add(Txt::new(&label.clone().unwrap_or(id.clone()), 12.0));
                f.boxes.push((id, t, nx, ny));
                f.boxes.len() - 1
            }
        };
        if let (Some(c), Some(_)) = (cur, dir) {
            f.links.push((c, k));
        }
        cur = Some(k);
    }
    (!f.boxes.is_empty()).then_some(f)
}

impl Diagram for Flow {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let (gx, gy) = (200.0, 70.0);
        let pos = |b: &(String, usize, i32, i32)| (b.2 as f32 * gx, b.3 as f32 * gy);
        let dims: Vec<(f32, f32)> = self
            .boxes
            .iter()
            .map(|b| (size[b.1].0 + 20.0, size[b.1].1 + 12.0))
            .collect();
        let mut p = Vec::new();
        for &(a, b) in &self.links {
            let (pa, pb) = (pos(&self.boxes[a]), pos(&self.boxes[b]));
            let (da, db) = (dims[a], dims[b]);
            let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
            let (s, e) = if dx.abs() > dy.abs() {
                (
                    (pa.0 + dx.signum() * da.0 / 2.0, pa.1),
                    (pb.0 - dx.signum() * db.0 / 2.0, pb.1),
                )
            } else {
                (
                    (pa.0, pa.1 + dy.signum() * da.1 / 2.0),
                    (pb.0, pb.1 - dy.signum() * db.1 / 2.0),
                )
            };
            p.push(line(vec![s, e], Col::Ink, 1.0));
            p.push(ellipse(
                [e.0 - 3.0, e.1 - 3.0, 6.0, 6.0],
                Col::Ink,
                Col::Ink,
                1.0,
            ));
        }
        for (k, b) in self.boxes.iter().enumerate() {
            let (cx, cy) = pos(b);
            let (w, h) = dims[k];
            p.push(rect(
                [cx - w / 2.0, cy - h / 2.0, w, h],
                8.0,
                Col::Box,
                Col::Ink,
                1.0,
            ));
            p.push(Prim::Text(
                b.1,
                cx - size[b.1].0 / 2.0,
                cy - size[b.1].1 / 2.0,
            ));
        }
        Placed {
            prims: p,
            w: 0.0,
            h: 0.0,
        }
        .fit(size, 10.0)
    }
}

// ---------------------------------------------------------------------------
// Git

pub struct Git {
    texts: Texts,
    /// (nama, komentar)
    commits: Vec<(usize, usize)>,
    /// (anak, induk)
    edges: Vec<(usize, usize)>,
}

pub fn parse_git(lines: &[String]) -> Option<Git> {
    let grid: Vec<Vec<char>> = lines
        .iter()
        .map(|l| l.trim_end().chars().collect())
        .collect();
    let at = |x: isize, y: usize| -> char {
        if x < 0 {
            return ' ';
        }
        grid.get(y)
            .and_then(|r| r.get(x as usize))
            .copied()
            .unwrap_or(' ')
    };
    let mut g = Git {
        texts: Texts::default(),
        commits: Vec::new(),
        edges: Vec::new(),
    };
    let mut pos: Vec<(usize, usize)> = Vec::new(); // (x, y) tiap commit
    for (y, row) in grid.iter().enumerate() {
        if let Some(x) = row.iter().position(|&c| c == '*') {
            let rest: String = row[x + 1..].iter().collect();
            let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '|' || c == '-');
            let (name, comment) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            let n = g.texts.add(Txt::new(name, 12.0).bold());
            let c = g.texts.add(Txt::new(comment, 11.0));
            g.commits.push((n, c));
            pos.push((x, y));
        }
    }
    if g.commits.is_empty() {
        return None;
    }
    // Ikuti garis ke bawah dari tiap commit sampai bertemu commit lain.
    for (k, &(x, y)) in pos.iter().enumerate() {
        let mut starts: Vec<(isize, usize)> = Vec::new();
        for dx in [-1isize, 0, 1] {
            let c = at(x as isize + dx, y + 1);
            let ok = match (dx, c) {
                (0, '|' | '*') | (1, '\\') | (-1, '/') => true,
                _ => false,
            };
            if ok {
                starts.push((x as isize + dx, y + 1));
            }
        }
        for (mut cx, mut cy) in starts {
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > 500 || cy >= grid.len() {
                    break;
                }
                match at(cx, cy) {
                    '*' => {
                        if let Some(j) = pos.iter().position(|&p| p.0 as isize == cx && p.1 == cy) {
                            if !g.edges.contains(&(k, j)) {
                                g.edges.push((k, j));
                            }
                        }
                        break;
                    }
                    '|' => cy += 1,
                    '\\' => {
                        cx += 1;
                        cy += 1;
                        // `\` diikuti `|` atau `*` satu kolom di kanan.
                        if at(cx, cy) == ' ' {
                            cx += 0;
                        }
                    }
                    '/' => {
                        cx -= 1;
                        cy += 1;
                    }
                    _ => break,
                }
            }
        }
    }
    // Urutkan commit menurut kolom lalu baris: urutan node menentukan urutan kiri-kanan
    // di layout gaya dot, jadi cabang kolom kiri tetap di kiri.
    let mut perm: Vec<usize> = (0..pos.len()).collect();
    perm.sort_by_key(|&k| pos[k]);
    let mut inv = vec![0; perm.len()];
    for (i, &k) in perm.iter().enumerate() {
        inv[k] = i;
    }
    g.commits = perm.iter().map(|&k| g.commits[k]).collect();
    for e in g.edges.iter_mut() {
        *e = (inv[e.0], inv[e.1]);
    }
    Some(g)
}

impl Diagram for Git {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let dims: Vec<(f32, f32)> = self
            .commits
            .iter()
            .map(|c| {
                (
                    size[c.0].0.max(size[c.1].0) + 12.0,
                    size[c.0].1 + size[c.1].1 + 8.0,
                )
            })
            .collect();
        let gr = Graph {
            nodes: dims
                .iter()
                .map(|d| graph::Node {
                    w: d.0,
                    h: d.1,
                    shape: graph::Shape::Rect,
                    cluster: None,
                })
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|&(a, b)| graph::Edge {
                    a,
                    b,
                    minlen: 1,
                    label: None,
                    ordered: false,
                })
                .collect(),
            clusters: Vec::new(),
            lr: false,
            nodesep: 30.0,
            ranksep: 30.0,
        };
        let o = graph::layout(&gr);
        let mut p = Vec::new();
        for (ei, _) in self.edges.iter().enumerate() {
            let pts = o.edges[ei].clone();
            if pts.len() >= 2 {
                let n = pts.len();
                p.push(dashed(pts.clone(), Col::Ink, 1.0, (3.0, 3.0)));
                p.push(head(
                    pts[n - 1],
                    (pts[n - 1].0 - pts[n - 2].0, pts[n - 1].1 - pts[n - 2].1),
                    Col::Ink,
                ));
            }
        }
        for (k, c) in self.commits.iter().enumerate() {
            let (cx, cy) = o.pos[k];
            let (w, h) = dims[k];
            p.push(rect(
                [cx - w / 2.0, cy - h / 2.0, w, h],
                4.0,
                Col::Paper,
                Col::Ink,
                1.5,
            ));
            p.push(Prim::Text(c.0, cx - size[c.0].0 / 2.0, cy - h / 2.0 + 4.0));
            p.push(Prim::Text(
                c.1,
                cx - size[c.1].0 / 2.0,
                cy - h / 2.0 + 4.0 + size[c.0].1,
            ));
        }
        Placed {
            prims: p,
            w: o.w,
            h: o.h,
        }
        .fit(size, 8.0)
    }
}

// ---------------------------------------------------------------------------
// Wire

pub struct Wire {
    texts: Texts,
    /// (nama, teks, x, y, w, h, warna, kedalaman)
    blocks: Vec<(String, usize, f32, f32, f32, f32, Option<Col>)>,
    /// (a, (fx, fy), b, (fx, fy), label?, panah)
    links: Vec<(usize, Pt, usize, Pt, Option<usize>, bool)>,
    prints: Vec<(usize, f32, f32)>,
}

/// `100%`, `50%+10`, `20` → (pecahan, px).
fn coord(s: &str) -> (f32, f32) {
    let s = s.trim();
    match s.split_once('%') {
        Some((a, b)) => (
            a.parse::<f32>().unwrap_or(0.0) / 100.0,
            b.trim().parse().unwrap_or(0.0),
        ),
        None => (0.0, s.parse().unwrap_or(0.0)),
    }
}

pub fn parse_wire(lines: &[String]) -> Option<Wire> {
    let mut w = Wire {
        texts: Texts::default(),
        blocks: Vec::new(),
        links: Vec::new(),
        prints: Vec::new(),
    };
    // Kursor per kolom; anak menjorok di dalam induk.
    let mut col_x = 0.0f32;
    let mut col_w = 0.0f32;
    let mut stack: Vec<(usize, f32, f32)> = Vec::new(); // (indentasi, x, y kursor)
    let mut y = 0.0f32;
    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        let ind = raw.len() - raw.trim_start().len();
        let l = raw.trim();
        if l.chars().all(|c| c == '-') && l.len() >= 2 {
            col_x += col_w + 30.0;
            col_w = 0.0;
            y = 0.0;
            stack.clear();
            continue;
        }
        if let Some(r) = l.strip_prefix("print(") {
            let t = r.trim_end_matches(')').trim_matches('"');
            let tx = w.texts.add(Txt::new(t, 12.0));
            w.prints.push((tx, col_x, y));
            y += 20.0;
            continue;
        }
        if let Some(r) = l.strip_prefix('*') {
            // *Nama[WxH] #warna
            let (name, rest) = r.split_once('[')?;
            let (dim, tail) = rest.split_once(']')?;
            let (bw, bh) = dim.split_once(['x', '*'])?;
            let (bw, bh): (f32, f32) = (bw.trim().parse().ok()?, bh.trim().parse().ok()?);
            let color = tail
                .split_whitespace()
                .find(|t| t.starts_with('#'))
                .and_then(Col::parse);
            while stack.last().is_some_and(|s| s.0 >= ind) {
                stack.pop();
            }
            let (bx, by) = match stack.last_mut() {
                Some(s) => {
                    let p = (s.1, s.2);
                    s.2 += bh + 10.0;
                    p
                }
                None => {
                    let p = (col_x, y);
                    y += bh + 10.0;
                    col_w = col_w.max(bw);
                    p
                }
            };
            let t = w.texts.add(Txt::new(name.trim(), 12.0));
            w.blocks
                .push((name.trim().to_string(), t, bx, by, bw, bh, color));
            // Anak berikutnya di dalam blok ini.
            stack.push((ind, bx + 10.0, by + 20.0));
            continue;
        }
        // A(x,y) --> B(x,y) : pesan
        if let Some(k) = l.find(['-', '=']).filter(|&k| k > 0) {
            let (lhs, rest) = l.split_at(k);
            let arrow_end = rest
                .find(|c: char| c.is_alphanumeric())
                .unwrap_or(rest.len());
            let (arrow, rhs) = rest.split_at(arrow_end);
            let (rhs, msg) = rhs
                .split_once(':')
                .map_or((rhs, None), |(a, b)| (a, Some(b.trim())));
            let end = |s: &str| -> Option<(usize, Pt)> {
                let s = s.trim();
                let (n, xy) = s
                    .split_once('(')
                    .map_or((s, None), |(a, b)| (a, Some(b.trim_end_matches(')'))));
                let k = w.blocks.iter().position(|b| b.0 == n.trim())?;
                let f = xy
                    .and_then(|v| v.split_once(','))
                    .map_or((0.5, 0.5), |(a, b)| {
                        let (ax, _) = coord(a);
                        let (ay, _) = coord(b);
                        (ax, ay)
                    });
                Some((k, f))
            };
            let (a, fa) = end(lhs)?;
            let (b, fb) = end(rhs)?;
            let label = msg
                .filter(|m| !m.is_empty())
                .map(|m| w.texts.add(Txt::new(m, 12.0)));
            w.links.push((a, fa, b, fb, label, arrow.contains('>')));
            continue;
        }
        return None;
    }
    (!w.blocks.is_empty()).then_some(w)
}

impl Diagram for Wire {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        for b in &self.blocks {
            p.push(rect(
                [b.2, b.3, b.4, b.5],
                0.0,
                b.6.unwrap_or(Col::Box),
                Col::Ink,
                1.0,
            ));
            p.push(Prim::Text(b.1, b.2 + 4.0, b.3 + 2.0));
        }
        for &(a, fa, b, fb, lab, arrow) in &self.links {
            let (ba, bb) = (&self.blocks[a], &self.blocks[b]);
            let pa = (ba.2 + ba.4 * fa.0, ba.3 + ba.5 * fa.1);
            let pb = (bb.2 + bb.4 * fb.0, bb.3 + bb.5 * fb.1);
            p.push(line(vec![pa, pb], Col::Ink, 1.0));
            if arrow {
                p.push(head(pb, (pb.0 - pa.0, pb.1 - pa.1), Col::Ink));
            }
            if let Some(t) = lab {
                p.push(Prim::Text(
                    t,
                    (pa.0 + pb.0) / 2.0 - size[t].0 / 2.0,
                    (pa.1 + pb.1) / 2.0 - size[t].1 - 2.0,
                ));
            }
        }
        for &(t, x, y) in &self.prints {
            p.push(Prim::Text(t, x, y));
        }
        Placed {
            prims: p,
            w: 0.0,
            h: 0.0,
        }
        .fit(size, 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn alpha_parsers() {
        let f = parse_flow(&l(
            "a \"Mulai\"\ne b \"Proses\"\ns c \"Simpan\"\nw d \"Selesai\"",
        ))
        .unwrap();
        assert_eq!(f.boxes.len(), 4);
        assert_eq!((f.boxes[3].2, f.boxes[3].3), (0, 1));
        let g = parse_git(&l(
            "* a1 Merge\n|\\\n| * c3 Fitur\n* | b2 Bug\n|/\n* 9f Awal",
        ))
        .unwrap();
        assert_eq!(g.commits.len(), 4);
        assert!(g.edges.len() >= 3, "{:?}", g.edges);
        let w = parse_wire(&l("*Server[120x60]\n  *CPU[40x20]\n--\n*Disk[80x40] #lightblue\nServer(100%,50%) --> Disk(0,50%) : data\nprint(\"x\")")).unwrap();
        assert_eq!(w.blocks.len(), 3);
        assert_eq!(w.links.len(), 1);
    }
}
