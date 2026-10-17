//! Mindmap (`@startmindmap`) dan WBS (`@startwbs`) — tata letak pohon milik
//! PlantUML sendiri (`mindmap`, `wbs`), konstanta dari SVG keluarannya.

use super::*;

#[derive(Clone, Debug)]
struct TNode {
    t: usize,
    level: usize,
    /// -1 kiri, 1 kanan (mindmap); WBS: -1 kiri, 1 kanan, 0 bawah.
    side: i8,
    color: Option<Col>,
    boxless: bool,
    kids: Vec<usize>,
}

pub struct Tree {
    texts: Texts,
    nodes: Vec<TNode>,
    roots: Vec<usize>,
    wbs: bool,
}

impl Diagram for Tree {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        let mut y = 0.0;
        for &r in &self.roots {
            let h = if self.wbs {
                wbs_root(self, r, size, &mut p, y)
            } else {
                mm_root(self, r, size, &mut p, y)
            };
            y += h + 20.0;
        }
        let pl = Placed {
            prims: p,
            w: 0.0,
            h: y,
        }
        .fit(size, if self.wbs { 20.0 } else { 10.0 });
        if self.wbs {
            // Kanvas WBS PlantUML: margin 20 (bawah 3 px lebih sempit, ukuran PNG acuan).
            return Placed { h: pl.h - 3.0, ..pl };
        }
        // Kanvas mindmap PlantUML: kiri 10, atas 20, kanan ≈ 20,9, bawah ≈ 17,6 (ukuran PNG acuan).
        let mut prims = pl.prims;
        prims.iter_mut().for_each(|q| q.shift(0.0, 10.0));
        Placed {
            prims,
            w: pl.w + 10.9,
            h: pl.h + 17.6,
        }
    }
}

pub fn parse_lines(lines: &[String], wbs: bool) -> Option<Tree> {
    let mut t = Tree {
        texts: Texts::default(),
        nodes: Vec::new(),
        roots: Vec::new(),
        wbs,
    };
    let mut stack: Vec<usize> = Vec::new();
    let mut side: i8 = 1;
    let mut i = 0;
    let ls: Vec<&str> = lines.iter().map(|l| l.trim_end()).collect();
    while i < ls.len() {
        let raw = ls[i];
        i += 1;
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        if low == "left side" {
            side = -1;
            continue;
        }
        if low == "right side" {
            side = 1;
            continue;
        }
        if low.starts_with("skinparam") || low.starts_with("<style") || low.starts_with("caption") {
            continue;
        }
        let first = l.chars().next()?;
        if !matches!(first, '*' | '+' | '-' | '#') {
            return None;
        }
        let level = l.chars().take_while(|&c| c == first).count();
        let mut rest = &l[level..];
        let mut color = None;
        if let Some(r) = rest.strip_prefix('[') {
            if let Some(k) = r.find(']') {
                color = Col::parse(&r[..k]);
                rest = &r[k + 1..];
            }
        }
        let boxless = rest.starts_with('_');
        let rest = rest.trim_start_matches('_');
        let mut rest = rest.trim_start_matches(['<', '>']).trim().to_string();
        if rest.starts_with(':') {
            // :multibaris ... ;
            let mut s = rest[1..].to_string();
            while !s.trim_end().ends_with(';') && i < ls.len() {
                s.push('\n');
                s.push_str(ls[i]);
                i += 1;
            }
            rest = s.trim_end().trim_end_matches(';').to_string();
        }
        if let (Some(a), Some(b)) = (rest.rfind("<<"), rest.rfind(">>")) {
            if a < b {
                rest = format!("{}{}", &rest[..a], &rest[b + 2..])
                    .trim()
                    .to_string();
            }
        }
        let node_side = match first {
            '+' => 1,
            '-' => -1,
            _ => side,
        };
        let txt = Txt::new(&unescape(&rest), if wbs { 12.0 } else { 14.0 });
        let ti = t.texts.add(txt);
        let k = t.nodes.len();
        t.nodes.push(TNode {
            t: ti,
            level,
            side: node_side,
            color,
            boxless,
            kids: Vec::new(),
        });
        while let Some(&top) = stack.last() {
            if t.nodes[top].level >= level {
                stack.pop();
            } else {
                break;
            }
        }
        match stack.last() {
            Some(&p) => t.nodes[p].kids.push(k),
            None => t.roots.push(k),
        }
        stack.push(k);
    }
    (!t.roots.is_empty()).then_some(t)
}

fn node_size(t: &Tree, k: usize, size: &[(f32, f32)]) -> (f32, f32) {
    let (tw, th) = size[t.nodes[k].t];
    // Tinggi teks versi PlantUML (≈ 1,1654 em per baris), bukan font kita.
    let tx = &t.texts.0[t.nodes[k].t];
    let th = if th > 0.0 {
        tx.size * 1.1654 * tx.s.split('\n').count() as f32
    } else {
        0.0
    };
    (tw + 20.0, th + if t.wbs { 21.11 } else { 21.3 })
}

fn draw_box(t: &Tree, k: usize, r: [f32; 4], p: &mut Vec<Prim>) {
    let n = &t.nodes[k];
    if !n.boxless {
        let rad = if t.wbs { 0.0 } else { 12.5 };
        p.push(rect(r, rad, n.color.unwrap_or(Col::Box), Col::Ink, 1.5));
    }
    p.push(Prim::Text(
        n.t,
        r[0] + 10.0,
        r[1] + if t.wbs { 9.7 } else { 10.0 },
    ));
}

// ---------------------------------------------------------------------------
// Mindmap

/// Tinggi subpohon (anak di satu sisi) termasuk jarak antarsaudara 20.
fn mm_height(t: &Tree, k: usize, size: &[(f32, f32)]) -> f32 {
    let h = node_size(t, k, size).1;
    let kids = &t.nodes[k].kids;
    if kids.is_empty() {
        return h;
    }
    let kh: f32 =
        kids.iter().map(|&c| mm_height(t, c, size)).sum::<f32>() + 20.0 * (kids.len() - 1) as f32;
    h.max(kh)
}

/// Gambar subpohon `k` (bukan akar) dengan tepi dekat di x = `x` dan pusat y `cy`.
fn mm_sub(t: &Tree, k: usize, dir: f32, x: f32, cy: f32, size: &[(f32, f32)], p: &mut Vec<Prim>) {
    let (w, h) = node_size(t, k, size);
    let bx = if dir > 0.0 { x } else { x - w };
    draw_box(t, k, [bx, cy - h / 2.0, w, h], p);
    let kids = &t.nodes[k].kids;
    mm_kids(
        t,
        kids,
        dir,
        if dir > 0.0 { bx + w } else { bx },
        cy,
        size,
        p,
    );
}

fn mm_kids(
    t: &Tree,
    kids: &[usize],
    dir: f32,
    edge_x: f32,
    cy: f32,
    size: &[(f32, f32)],
    p: &mut Vec<Prim>,
) {
    if kids.is_empty() {
        return;
    }
    let hs: Vec<f32> = kids.iter().map(|&c| mm_height(t, c, size)).collect();
    let total: f32 = hs.iter().sum::<f32>() + 20.0 * (kids.len() - 1) as f32;
    let mut y = cy - total / 2.0;
    for (i, &c) in kids.iter().enumerate() {
        let ccy = y + hs[i] / 2.0;
        let x0 = edge_x;
        let x1 = edge_x + dir * 50.0;
        let mut pts = vec![(x0, cy), (x0 + dir * 10.0, cy)];
        vector::bezier(
            &mut pts,
            (x0 + dir * 10.0, cy),
            (x0 + dir * 25.0, cy),
            (x0 + dir * 25.0, ccy),
            (x0 + dir * 40.0, ccy),
        );
        pts.push((x1, ccy));
        p.push(line(pts, Col::Ink, 1.0));
        mm_sub(t, c, dir, x1, ccy, size, p);
        y += hs[i] + 20.0;
    }
}

fn mm_root(t: &Tree, r: usize, size: &[(f32, f32)], p: &mut Vec<Prim>, y0: f32) -> f32 {
    let (w, h) = node_size(t, r, size);
    let kids = &t.nodes[r].kids;
    let right: Vec<usize> = kids
        .iter()
        .copied()
        .filter(|&c| t.nodes[c].side >= 0)
        .collect();
    let left: Vec<usize> = kids
        .iter()
        .copied()
        .filter(|&c| t.nodes[c].side < 0)
        .collect();
    let block = |v: &[usize]| {
        v.iter().map(|&c| mm_height(t, c, size)).sum::<f32>()
            + 20.0 * v.len().saturating_sub(1) as f32
    };
    let total = h.max(block(&right)).max(block(&left));
    let cy = y0 + total / 2.0;
    draw_box(t, r, [0.0, cy - h / 2.0, w, h], p);
    mm_kids(t, &right, 1.0, w, cy, size, p);
    mm_kids(t, &left, -1.0, 0.0, cy, size, p);
    total
}

// ---------------------------------------------------------------------------
// WBS

/// Lebar & tinggi subpohon tegak (tingkat ≥ 2): kotak induk lalu anak menjorok.
fn wbs_list(t: &Tree, k: usize, size: &[(f32, f32)]) -> (f32, f32) {
    let (w, h) = node_size(t, k, size);
    let mut tw = w;
    let mut th = h;
    for &c in &t.nodes[k].kids {
        let (cw, ch) = wbs_list(t, c, size);
        tw = tw.max(w / 2.0 + 10.0 + cw);
        th += 15.0 + ch;
    }
    (tw, th)
}

fn wbs_draw_list(t: &Tree, k: usize, x: f32, y: f32, size: &[(f32, f32)], p: &mut Vec<Prim>) {
    let (w, h) = node_size(t, k, size);
    draw_box(t, k, [x, y, w, h], p);
    let lx = x + w / 2.0;
    let mut cy = y + h;
    let mut last_mid = None;
    for &c in &t.nodes[k].kids {
        cy += 15.0;
        let ch = node_size(t, c, size).1;
        let mid = cy + ch / 2.0;
        p.push(line(vec![(lx, mid), (lx + 10.0, mid)], Col::Ink, 1.5));
        wbs_draw_list(t, c, lx + 10.0, cy, size, p);
        cy += wbs_list(t, c, size).1;
        last_mid = Some(mid);
    }
    if let Some(m) = last_mid {
        p.push(line(vec![(lx, y + h), (lx, m)], Col::Ink, 1.5));
    }
}

fn wbs_root(t: &Tree, r: usize, size: &[(f32, f32)], p: &mut Vec<Prim>, y0: f32) -> f32 {
    let (w, h) = node_size(t, r, size);
    let kids = &t.nodes[r].kids;
    if kids.is_empty() {
        draw_box(t, r, [0.0, y0, w, h], p);
        return h;
    }
    let dims: Vec<(f32, f32)> = kids.iter().map(|&c| wbs_list(t, c, size)).collect();
    let total_w: f32 = dims.iter().map(|d| d.0).sum::<f32>() + 20.0 * (kids.len() - 1) as f32;
    let bar = y0 + h + 20.0;
    let mut x = 0.0;
    let mut centers = Vec::new();
    for (i, &c) in kids.iter().enumerate() {
        let cw = node_size(t, c, size).0;
        let cx = x + cw / 2.0;
        p.push(line(vec![(cx, bar), (cx, bar + 20.0)], Col::Ink, 1.5));
        wbs_draw_list(t, c, x, bar + 20.0, size, p);
        centers.push(cx);
        x += dims[i].0 + 20.0;
    }
    let (a, b) = (centers[0], *centers.last().unwrap());
    p.push(line(vec![(a, bar), (b, bar)], Col::Ink, 1.5));
    let rx = (a + b) / 2.0 - w / 2.0;
    p.push(line(
        vec![((a + b) / 2.0, y0 + h), ((a + b) / 2.0, bar)],
        Col::Ink,
        1.5,
    ));
    draw_box(t, r, [rx, y0, w, h], p);
    let _ = total_w;
    h + 40.0 + dims.iter().map(|d| d.1).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mindmap_and_wbs() {
        let l = |s: &str| s.lines().map(String::from).collect::<Vec<_>>();
        let m = parse_lines(&l("* A\n** B\n*** C\nleft side\n** D\n-- E"), false).unwrap();
        assert_eq!(m.roots.len(), 1);
        assert_eq!(m.nodes[0].kids.len(), 3);
        assert_eq!(m.nodes[3].side, -1);
        let pl = m.place(&vec![(30.0, 16.0); m.texts.0.len()]);
        assert!(pl.w > 0.0);
        let w = parse_lines(&l("* P\n** F1\n*** a\n** F2"), true).unwrap();
        let pl = w.place(&vec![(30.0, 14.0); w.texts.0.len()]);
        assert!(pl.h > 0.0);
    }
}
