//! Diagram mermaid untuk mode baca. Hanya flowchart (`graph`/`flowchart`);
//! jenis lain (sequence, gantt, ...) tetap tampil sebagai blok kode.
// ponytail: layout berlapis sederhana (rank jalur terpanjang + satu sapuan
// barycenter), sisi lintas-rank bisa menembus node. Subgraph diabaikan.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Rect,
    Round,
    Diamond,
    Circle,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stroke {
    Solid,
    Dotted,
    Thick,
}

pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub stroke: Stroke,
    pub arrow: bool,
}

pub struct Flow {
    pub horiz: bool,
    pub flip: bool,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

pub fn parse(src: &str) -> Option<Flow> {
    let mut lines = src
        .lines()
        .flat_map(|l| l.split(';'))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let mut head = lines.next()?.split_whitespace();
    if !matches!(head.next()?, "graph" | "flowchart") {
        return None;
    }
    let dir = head.next().unwrap_or("TD");
    let mut f = Flow {
        horiz: matches!(dir, "LR" | "RL"),
        flip: matches!(dir, "BT" | "RL"),
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    for l in lines {
        let kw = l.split_whitespace().next().unwrap_or("");
        if !matches!(
            kw,
            "subgraph"
                | "end"
                | "style"
                | "classDef"
                | "class"
                | "click"
                | "linkStyle"
                | "direction"
        ) {
            f.statement(l);
        }
    }
    (!f.nodes.is_empty()).then_some(f)
}

fn clean(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
}

/// Panah di awal `s`: `-->`, `---`, `-.->`, `==>`, `-- teks -->`, lalu `|teks|` opsional.
fn arrow(s: &str) -> Option<((String, Stroke, bool), &str)> {
    let run = |s: &str| s.find(|c: char| !"-.=<>".contains(c)).unwrap_or(s.len());
    let n = run(s);
    if n < 2 {
        return None;
    }
    let mut op = s[..n].to_owned();
    let mut rest = &s[n..];
    let mut label = String::new();
    if matches!(op.as_str(), "--" | "==" | "-.") {
        let e = ["-->", "---", "==>", "===", ".->", ".-"]
            .iter()
            .filter_map(|p| rest.find(p))
            .min()?;
        label = clean(&rest[..e]);
        let m = run(&rest[e..]);
        op.push_str(&rest[e..e + m]);
        rest = &rest[e + m..];
    }
    rest = rest.trim_start();
    if let Some(r) = rest.strip_prefix('|') {
        let e = r.find('|')?;
        label = clean(&r[..e]);
        rest = &r[e + 1..];
    }
    let stroke = if op.contains('=') {
        Stroke::Thick
    } else if op.contains('.') {
        Stroke::Dotted
    } else {
        Stroke::Solid
    };
    Some(((label, stroke, op.ends_with('>')), rest))
}

impl Flow {
    /// `A[x] & B --> C{y} -->|ya| D`
    fn statement(&mut self, l: &str) -> Option<()> {
        let mut s = l;
        let mut prev: Vec<usize> = Vec::new();
        let mut pending = None;
        loop {
            let mut group = Vec::new();
            loop {
                let (i, rest) = self.node(s)?;
                group.push(i);
                s = rest.trim_start();
                match s.strip_prefix('&') {
                    Some(r) => s = r.trim_start(),
                    None => break,
                }
            }
            if let Some((label, stroke, arrow)) = pending.take() {
                for &from in &prev {
                    for &to in &group {
                        self.edges.push(Edge {
                            from,
                            to,
                            label: String::clone(&label),
                            stroke,
                            arrow,
                        });
                    }
                }
            }
            prev = group;
            let (e, rest) = arrow(s)?;
            pending = Some(e);
            s = rest.trim_start();
        }
    }

    fn node<'a>(&mut self, s: &'a str) -> Option<(usize, &'a str)> {
        let end = s
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(s.len());
        if end == 0 {
            return None;
        }
        let (id, mut rest) = s.split_at(end);
        let mut shaped = None;
        let shapes = [
            ("((", "))", Shape::Circle),
            ("([", "])", Shape::Round),
            ("[[", "]]", Shape::Rect),
            ("[(", ")]", Shape::Round),
            ("{{", "}}", Shape::Diamond),
            ("[", "]", Shape::Rect),
            ("(", ")", Shape::Round),
            ("{", "}", Shape::Diamond),
            (">", "]", Shape::Rect),
        ];
        for (open, close, shape) in shapes {
            if let Some(r) = rest.strip_prefix(open) {
                let e = r.find(close)?;
                shaped = Some((clean(&r[..e]), shape));
                rest = &r[e + close.len()..];
                break;
            }
        }
        let i = self
            .nodes
            .iter()
            .position(|n| n.id == id)
            .unwrap_or_else(|| {
                self.nodes.push(Node {
                    id: id.to_owned(),
                    label: id.to_owned(),
                    shape: Shape::Rect,
                });
                self.nodes.len() - 1
            });
        if let Some((label, shape)) = shaped {
            self.nodes[i].label = label;
            self.nodes[i].shape = shape;
        }
        Some((i, rest))
    }
}

/// Hasil penempatan dalam piksel, (0,0) = pojok kiri atas diagram.
pub struct Placed {
    pub nodes: Vec<[f32; 4]>,
    /// Titik-titik garis siku; titik terakhir = ujung panah.
    pub edges: Vec<Vec<(f32, f32)>>,
    /// Pusat label sisi.
    pub labels: Vec<(f32, f32)>,
    pub w: f32,
    pub h: f32,
}

fn dfs(u: usize, f: &Flow, adj: &[Vec<usize>], st: &mut [u8], back: &mut [bool]) {
    st[u] = 1;
    for &e in &adj[u] {
        let v = f.edges[e].to;
        match st[v] {
            0 => dfs(v, f, adj, st, back),
            1 => back[e] = true,
            _ => {}
        }
    }
    st[u] = 2;
}

/// `size` = ukuran node, `lsize` = ukuran label sisi (w, h). Dihitung di sumbu
/// (silang, utama): utama = arah aliran (y untuk TD, x untuk LR).
pub fn place(f: &Flow, size: &[(f32, f32)], lsize: &[(f32, f32)], gap: f32) -> Placed {
    let n = f.nodes.len();
    let axes = |(w, h): (f32, f32)| if f.horiz { (h, w) } else { (w, h) };
    let sz: Vec<(f32, f32)> = size.iter().map(|&s| axes(s)).collect();

    // Sisi balik (siklus) dikeluarkan dari perhitungan rank.
    let mut adj = vec![Vec::new(); n];
    for (i, e) in f.edges.iter().enumerate() {
        adj[e.from].push(i);
    }
    let (mut st, mut back) = (vec![0u8; n], vec![false; f.edges.len()]);
    for u in 0..n {
        if st[u] == 0 {
            dfs(u, f, &adj, &mut st, &mut back);
        }
    }
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        for (e, b) in f.edges.iter().zip(&back) {
            if !b {
                rank[e.to] = rank[e.to].max(rank[e.from] + 1);
            }
        }
    }

    // Tiap sisi jadi rantai segmen antar-rank yang bersebelahan: sisi yang
    // melompati rank diberi node bayangan (lebar 0) supaya punya jalur sendiri,
    // sisi balik dibalik dulu lalu titiknya dibalik lagi di akhir.
    let mut sz = sz;
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for (i, e) in f.edges.iter().enumerate() {
        let (a, b) = if back[i] {
            (e.to, e.from)
        } else {
            (e.from, e.to)
        };
        let mut ch = vec![a];
        if a != b {
            for r in rank[a] + 1..rank[b] {
                rank.push(r);
                sz.push((0.0, 0.0));
                ch.push(rank.len() - 1);
            }
            ch.push(b);
        }
        chains.push(ch);
    }
    let segs: Vec<(usize, usize)> = chains
        .iter()
        .flat_map(|c| c.windows(2).map(|w| (w[0], w[1])))
        .collect();
    let real = n;
    let n = rank.len();
    let nr = rank.iter().max().map_or(0, |r| r + 1);
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); nr];
    for u in 0..n {
        rows[rank[u]].push(u);
    }
    // Urutan dalam rank: rata-rata posisi induk (barycenter).
    let mut pos = vec![0.0f32; n];
    for r in 0..nr {
        for u in 0..rows[r].len() {
            let v = rows[r][u];
            let ps: Vec<f32> = segs.iter().filter(|s| s.1 == v).map(|s| pos[s.0]).collect();
            pos[v] = if ps.is_empty() {
                u as f32
            } else {
                ps.iter().sum::<f32>() / ps.len() as f32
            };
        }
        rows[r].sort_by(|&a, &b| pos[a].total_cmp(&pos[b]));
        for (i, &v) in rows[r].iter().enumerate() {
            pos[v] = i as f32;
        }
    }

    // Koordinat silang: tiap node ingin berada di rata-rata induknya (rantai
    // bayangan jadi lurus). Satu lintasan dari kiri (hanya mendorong ke kanan)
    // dan satu dari kanan (hanya ke kiri); rata-rata keduanya tetap tidak bertumpuk.
    let lmain = lsize.iter().map(|&s| axes(s).1).fold(0.0, f32::max);
    let gm = gap * 1.5 + lmain;
    let sp = |a: usize, b: usize| {
        if a >= real && b >= real {
            gap * 0.4
        } else {
            gap
        }
    };
    let mut cx = vec![0.0f32; n];
    let mut placed = vec![false; n];
    // Turun (ikut induk), naik (ikut anak), turun lagi: rantai panjang makin lurus.
    for pass in 0..3 {
        let up = pass == 1;
        let order: Vec<usize> = if up {
            (0..nr).rev().collect()
        } else {
            (0..nr).collect()
        };
        for r in order {
            let row = &rows[r];
            let k = row.len();
            let want: Vec<Option<f32>> = row
                .iter()
                .map(|&v| {
                    let ps: Vec<f32> = segs
                        .iter()
                        .filter_map(|s| {
                            if up {
                                (s.0 == v).then_some(s.1)
                            } else {
                                (s.1 == v).then_some(s.0)
                            }
                        })
                        .filter(|&u| placed[u])
                        .map(|u| cx[u])
                        .collect();
                    if ps.is_empty() {
                        placed[v].then_some(cx[v])
                    } else {
                        Some(ps.iter().sum::<f32>() / ps.len() as f32)
                    }
                })
                .collect();
            let d = |i: usize| (sz[row[i]].0 + sz[row[i + 1]].0) / 2.0 + sp(row[i], row[i + 1]);
            let mut l = vec![0.0f32; k];
            for i in 0..k {
                let min = if i == 0 {
                    f32::MIN
                } else {
                    l[i - 1] + d(i - 1)
                };
                l[i] = want[i].unwrap_or(if i == 0 { 0.0 } else { min }).max(min);
            }
            let mut r = vec![0.0f32; k];
            for i in (0..k).rev() {
                let max = if i + 1 == k {
                    f32::MAX
                } else {
                    r[i + 1] - d(i)
                };
                r[i] = want[i].unwrap_or(l[i]).min(max);
            }
            for i in 0..k {
                cx[row[i]] = (l[i] + r[i]) / 2.0;
                placed[row[i]] = true;
            }
        }
    }
    let lo = (0..n)
        .map(|u| cx[u] - sz[u].0 / 2.0)
        .fold(f32::MAX, f32::min);
    let cmax = (0..n)
        .map(|u| cx[u] + sz[u].0 / 2.0 - lo)
        .fold(0.0, f32::max);
    let mut rc = vec![[0.0f32; 4]; n]; // (c0, m0, cw, mh)
    let mut rs = vec![0.0f32; nr]; // awal tiap rank di sumbu utama
    let mut m = 0.0;
    for (r, row) in rows.iter().enumerate() {
        rs[r] = m;
        let mh = row.iter().map(|&u| sz[u].1).fold(0.0, f32::max);
        for &u in row {
            rc[u] = [
                cx[u] - sz[u].0 / 2.0 - lo,
                m + (mh - sz[u].1) / 2.0,
                sz[u].0,
                sz[u].1,
            ];
        }
        m += mh + gm;
    }
    let mtot = (m - gm).max(0.0);

    // Port: segmen keluar/masuk dibagi rata di sepanjang tepi node, urut posisi lawannya.
    let center = |u: usize| rc[u][0] + rc[u][2] / 2.0;
    let port = |u: usize, si: usize, out: bool| {
        if u >= real || matches!(f.nodes[u].shape, Shape::Diamond | Shape::Circle) {
            return center(u); // bayangan, atau hanya ujungnya yang menyentuh tepi kotak
        }
        let end = |i: usize| if out { segs[i].0 } else { segs[i].1 };
        let other = |i: usize| center(if out { segs[i].1 } else { segs[i].0 });
        let mut es: Vec<usize> = (0..segs.len()).filter(|&i| end(i) == u).collect();
        es.sort_by(|&a, &b| other(a).total_cmp(&other(b)));
        let k = es.iter().position(|&i| i == si).unwrap_or(0);
        rc[u][0] + rc[u][2] * (k + 1) as f32 / (es.len() + 1) as f32
    };
    let pc: Vec<(f32, f32)> = segs
        .iter()
        .enumerate()
        .map(|(si, s)| (port(s.0, si, true), port(s.1, si, false)))
        .collect();
    // Belokan mendatar dalam satu celah: yang rentangnya bertumpang tindih diberi
    // tingkat berbeda (pewarnaan interval), yang tidak bertumpang tindih berbagi tingkat.
    let mut lv = vec![0usize; segs.len()];
    let mut used: Vec<Vec<(f32, f32, usize)>> = vec![Vec::new(); nr];
    for (si, s) in segs.iter().enumerate() {
        let (a, b) = pc[si];
        if (a - b).abs() < 0.5 {
            continue;
        }
        let (lo, hi) = (a.min(b) - gap * 0.3, a.max(b) + gap * 0.3);
        let g = &mut used[rank[s.1]];
        while g.iter().any(|&(l, h, v)| v == lv[si] && l < hi && lo < h) {
            lv[si] += 1;
        }
        g.push((lo, hi, lv[si]));
    }
    let jog: Vec<f32> = segs
        .iter()
        .enumerate()
        .map(|(si, s)| {
            let top = used[rank[s.1]]
                .iter()
                .map(|u| u.2)
                .max()
                .unwrap_or(0)
                .max(1) as f32;
            lv[si] as f32 * (gap * 0.35).min(gm * 0.4 / top)
        })
        .collect();
    let mut cext = cmax;
    let mut edges = Vec::new();
    let mut labels = Vec::new();
    let mut si = 0;
    for (i, ch) in chains.iter().enumerate() {
        if ch.len() == 1 {
            // Sisi ke diri sendiri: putaran kecil di sisi kanan diagram.
            cext += gap;
            let a = rc[ch[0]];
            let (m0, m1) = (a[1] + a[3] / 4.0, a[1] + a[3] * 0.75);
            edges.push(vec![
                (a[0] + a[2], m0),
                (cext, m0),
                (cext, m1),
                (a[0] + a[2], m1),
            ]);
            labels.push((cext, a[1] + a[3] / 2.0));
            continue;
        }
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let mut lab = None;
        for w in ch.windows(2) {
            let (a, b) = (rc[w[0]], rc[w[1]]);
            let (ca, cb) = pc[si];
            let mid = rs[rank[w[1]]] - gm / 2.0;
            pts.push((ca, a[1] + a[3]));
            if (ca - cb).abs() >= 0.5 {
                pts.extend([(ca, mid - jog[si]), (cb, mid - jog[si])]);
            }
            pts.push((cb, b[1]));
            lab.get_or_insert((cb, mid)); // di belokan sisi tujuan: cabang dari satu titik tidak bertumpuk
            si += 1;
        }
        pts.dedup();
        if back[i] {
            pts.reverse();
        }
        edges.push(pts);
        labels.push(lab.unwrap_or_default());
    }
    // Label pada celah rank yang sama digeser menjauh supaya tidak bertumpuk.
    let lw = |i: usize| axes(lsize[i]).0;
    let mut order: Vec<usize> = (0..labels.len()).filter(|&i| lw(i) > 0.0).collect();
    order.sort_by(|&a, &b| {
        labels[a]
            .1
            .total_cmp(&labels[b].1)
            .then(labels[a].0.total_cmp(&labels[b].0))
    });
    for k in 1..order.len() {
        let (p, i) = (order[k - 1], order[k]);
        let min = labels[p].0 + (lw(p) + lw(i)) / 2.0 + gap / 4.0;
        if labels[i].1 == labels[p].1 && labels[i].0 < min {
            labels[i].0 = min;
            cext = cext.max(min + lw(i) / 2.0);
        }
    }
    cext += if cext > cmax { gap } else { 0.0 };

    // Kembali ke (x, y), dibalik untuk BT/RL.
    let flip = |m: f32| if f.flip { mtot - m } else { m };
    let pt = |(c, m): (f32, f32)| if f.horiz { (flip(m), c) } else { (c, flip(m)) };
    Placed {
        nodes: rc[..real]
            .iter()
            .map(|&[c, m, cw, mh]| {
                let m = if f.flip { mtot - m - mh } else { m };
                if f.horiz {
                    [m, c, mh, cw]
                } else {
                    [c, m, cw, mh]
                }
            })
            .collect(),
        edges: edges
            .into_iter()
            .map(|p| p.into_iter().map(pt).collect())
            .collect(),
        labels: labels.into_iter().map(pt).collect(),
        w: if f.horiz { mtot } else { cext },
        h: if f.horiz { cext } else { mtot },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flowchart() {
        let f = parse("graph LR\n  A[Mulai] --> B{Oke?}\n  B -->|ya| C((Selesai))\n  B -- tidak --> A; A & C -.- D").unwrap();
        assert!(f.horiz);
        let ids: Vec<_> = f
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.label.as_str(), n.shape))
            .collect();
        assert_eq!(
            ids,
            [
                ("A", "Mulai", Shape::Rect),
                ("B", "Oke?", Shape::Diamond),
                ("C", "Selesai", Shape::Circle),
                ("D", "D", Shape::Rect)
            ]
        );
        let es: Vec<_> = f
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.label.as_str(), e.stroke, e.arrow))
            .collect();
        assert_eq!(
            es,
            [
                (0, 1, "", Stroke::Solid, true),
                (1, 2, "ya", Stroke::Solid, true),
                (1, 0, "tidak", Stroke::Solid, true),
                (0, 3, "", Stroke::Dotted, false),
                (2, 3, "", Stroke::Dotted, false)
            ]
        );
        assert!(parse("sequenceDiagram\n A->>B: hi").is_none());

        // Siklus tidak membuat rank meledak; B di bawah A, sisi balik masuk dari bawah A.
        let p = place(
            &parse("graph TD\nA-->B-->A").unwrap(),
            &[(50.0, 20.0); 2],
            &[(0.0, 0.0); 2],
            10.0,
        );
        let a = p.nodes[0];
        assert!(p.nodes[1][1] > a[1]);
        assert_eq!(p.edges[1].last().unwrap().1, a[1] + a[3]);

        // Sisi A-->C yang melompati B tidak menembus B.
        let p = place(
            &parse("graph TD\nA-->B-->C\nA-->C").unwrap(),
            &[(50.0, 20.0); 3],
            &[(0.0, 0.0); 3],
            10.0,
        );
        let b = p.nodes[1];
        for w in p.edges[2].windows(2) {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            let inside_x = x0.min(x1) < b[0] + b[2] && x0.max(x1) > b[0];
            let inside_y = y0.min(y1) < b[1] + b[3] && y0.max(y1) > b[1];
            assert!(!(inside_x && inside_y), "{:?}", p.edges[2]);
        }
    }
}
