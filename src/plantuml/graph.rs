//! Tata letak graf berlapis gaya Graphviz `dot` (pengganti Graphviz untuk
//! diagram class/usecase/component/state/...).
//!
//! Langkah: putus siklus (DFS) → rank jalur terpanjang + penyeimbangan → node
//! virtual (sisi panjang, label sisi; rank digandakan bila ada label seperti dot)
//! → urutan per rank (median + transpose) dengan anggota cluster bersebelahan →
//! koordinat x (regresi isotonik dengan jarak minimum + bantalan cluster, lalu
//! penyelarasan kotak cluster lintas rank) → spline Bézier bertangen tegak.
// ponytail: rank & urutan heuristik ala dot; x network simplex hanya untuk graf ≤150 node virtual (sisanya heuristik).

use crate::vector::{self, Pt};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Rect,
    Ellipse,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub w: f32,
    pub h: f32,
    pub shape: Shape,
    /// Cluster terdalam yang memuat node ini.
    pub cluster: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    /// Selisih rank minimum (0 = sejajar, `->` PlantUML; 1 = `-->`; 2 = `--->` ...).
    pub minlen: u32,
    /// Ukuran label di tengah sisi.
    pub label: Option<(f32, f32)>,
    /// Kiri→kanan untuk sisi sejajar (`-right->`); false = bebas.
    pub ordered: bool,
}

#[derive(Clone, Debug)]
pub struct Cluster {
    pub parent: Option<usize>,
    /// Ruang dalam: (atas, kanan, bawah, kiri) — atas memuat judul.
    pub pad: [f32; 4],
    /// Lebar minimum (judul).
    pub min_w: f32,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub clusters: Vec<Cluster>,
    /// `left to right direction`.
    pub lr: bool,
    pub nodesep: f32,
    pub ranksep: f32,
}

pub struct Out {
    /// Pusat tiap node.
    pub pos: Vec<Pt>,
    /// Titik sisi (sudah dihaluskan), dari a ke b.
    pub edges: Vec<Vec<Pt>>,
    /// Pusat label sisi.
    pub labels: Vec<Option<Pt>>,
    /// Kotak cluster [x, y, w, h].
    pub clusters: Vec<[f32; 4]>,
    pub w: f32,
    pub h: f32,
}

fn find(p: &mut [usize], x: usize) -> usize {
    let mut r = x;
    while p[r] != r {
        r = p[r];
    }
    let mut c = x;
    while p[c] != r {
        let n = p[c];
        p[c] = r;
        c = n;
    }
    r
}

/// Regresi isotonik berbobot (pool adjacent violators): z tidak turun, dekat target.
fn pav(t: &[f32], w: &[f32]) -> Vec<f32> {
    let mut blocks: Vec<(f32, f32, usize)> = Vec::new(); // (rerata, bobot, jumlah)
    for i in 0..t.len() {
        blocks.push((t[i], w[i].max(1e-3), 1));
        while blocks.len() > 1 {
            let n = blocks.len();
            if blocks[n - 2].0 <= blocks[n - 1].0 {
                break;
            }
            let (b, a) = (blocks.pop().unwrap(), blocks.pop().unwrap());
            let ww = a.1 + b.1;
            blocks.push(((a.0 * a.1 + b.0 * b.1) / ww, ww, a.2 + b.2));
        }
    }
    blocks
        .iter()
        .flat_map(|b| std::iter::repeat(b.0).take(b.2))
        .collect()
}

/// Network simplex (Gansner dkk., seperti dot): cari r dengan r[h] - r[t] >= len
/// untuk tiap sisi (t, h, len, w) yang meminimalkan Σ w·(r[h] - r[t]).
/// Graf harus asiklik & terhubung. ponytail: nilai potong dihitung ulang naif
/// O(V·E) per pivot; cukup untuk graf kecil (pemanggil membatasi ukuran).
fn simplex(n: usize, es: &[(usize, usize, i64, i64)], max_iter: usize) -> Vec<i64> {
    let mut inc = vec![Vec::new(); n];
    let mut out = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for (i, &(t, h, _, _)) in es.iter().enumerate() {
        inc[t].push(i);
        inc[h].push(i);
        out[t].push(i);
        indeg[h] += 1;
    }
    // Rank awal: jalur terpanjang.
    let mut r = vec![0i64; n];
    let mut q: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    while let Some(v) = q.pop() {
        for &i in &out[v] {
            let (_, h, l, _) = es[i];
            r[h] = r[h].max(r[v] + l);
            indeg[h] -= 1;
            if indeg[h] == 0 {
                q.push(h);
            }
        }
    }
    let slack = |r: &[i64], i: usize| r[es[i].1] - r[es[i].0] - es[i].2;
    // Pohon rentang ketat.
    let mut tree = vec![false; es.len()];
    loop {
        tree.fill(false);
        let mut intree = vec![false; n];
        intree[0] = true;
        let (mut st, mut cnt) = (vec![0], 1);
        while let Some(v) = st.pop() {
            for &i in &inc[v] {
                let o = if es[i].0 == v { es[i].1 } else { es[i].0 };
                if !intree[o] && slack(&r, i) == 0 {
                    intree[o] = true;
                    tree[i] = true;
                    cnt += 1;
                    st.push(o);
                }
            }
        }
        if cnt == n {
            break;
        }
        let Some((s, i)) = (0..es.len())
            .filter(|&i| intree[es[i].0] != intree[es[i].1])
            .map(|i| (slack(&r, i), i))
            .min()
        else {
            return r; // tak terhubung
        };
        let d = if intree[es[i].0] { s } else { -s };
        for v in 0..n {
            if intree[v] {
                r[v] += d;
            }
        }
    }
    for _ in 0..max_iter {
        // Penomoran pasca-urut pohon dari akar 0: subpohon v = {u: low[v] <= lim[u] <= lim[v]}.
        let (mut low, mut lim, mut par) = (vec![0usize; n], vec![0usize; n], vec![usize::MAX; n]);
        let mut seen = vec![false; n];
        let mut next = 0;
        let mut st: Vec<(usize, usize, usize)> = vec![(0, 0, 0)]; // (node, indeks inc, low)
        seen[0] = true;
        while let Some(&mut (v, ref mut k, lo)) = st.last_mut() {
            if *k < inc[v].len() {
                let i = inc[v][*k];
                *k += 1;
                let o = if es[i].0 == v { es[i].1 } else { es[i].0 };
                if tree[i] && !seen[o] {
                    seen[o] = true;
                    par[o] = i;
                    st.push((o, 0, next));
                }
            } else {
                low[v] = lo;
                lim[v] = next;
                next += 1;
                st.pop();
            }
        }
        let inside = |c: usize, u: usize| low[c] <= lim[u] && lim[u] <= lim[c];
        // Nilai potong sisi pohon par[c]: Σ bobot tail-komponen → head-komponen
        // dikurangi arah sebaliknya.
        let cut = |c: usize| -> i64 {
            let sub_is_head = es[par[c]].1 == c;
            es.iter()
                .filter(|&&(a, b, _, _)| inside(c, a) != inside(c, b))
                .map(|&(a, b, _, w)| {
                    let fwd = if sub_is_head {
                        inside(c, b)
                    } else {
                        inside(c, a)
                    };
                    if fwd {
                        w
                    } else {
                        -w
                    }
                })
                .sum()
        };
        // Sisi masuk: dari head-komponen ke tail-komponen, slack terkecil.
        let enter = |r: &[i64], c: usize| -> Option<usize> {
            let sub_is_head = es[par[c]].1 == c;
            (0..es.len())
                .filter(|&j| {
                    let (ia, ib) = (inside(c, es[j].0), inside(c, es[j].1));
                    ia != ib && if sub_is_head { ia } else { ib }
                })
                .min_by_key(|&j| slack(r, j))
        };
        let kids: Vec<usize> = (1..n).filter(|&c| par[c] != usize::MAX).collect();
        let Some(c) = kids.iter().copied().find(|&c| cut(c) < 0) else {
            // Optimal. LR_balance dot: sisi bernilai potong 0 digeser ke tengah
            // kelonggarannya (induk di tengah anak-anak yang seri).
            for &c in &kids {
                if cut(c) != 0 {
                    continue;
                }
                let Some(f) = enter(&r, c) else { continue };
                let d = slack(&r, f);
                if d <= 1 {
                    continue;
                }
                let sh = if es[par[c]].0 == c { -(d / 2) } else { d / 2 };
                for u in 0..n {
                    if inside(c, u) {
                        r[u] += sh;
                    }
                }
            }
            break;
        };
        let Some(f) = enter(&r, c) else {
            break;
        };
        tree[par[c]] = false;
        tree[f] = true;
        // Rank ulang dari pohon.
        let mut seen = vec![false; n];
        seen[0] = true;
        let mut st = vec![0];
        while let Some(v) = st.pop() {
            for &i in &inc[v] {
                let (t, h, l, _) = es[i];
                let o = if t == v { h } else { t };
                if tree[i] && !seen[o] {
                    seen[o] = true;
                    r[o] = if t == v { r[v] + l } else { r[v] - l };
                    st.push(o);
                }
            }
        }
    }
    r
}

/// Titik di tepi node (persegi/elips) pada garis dari pusat ke `toward`.
fn clip(c: Pt, w: f32, h: f32, shape: Shape, toward: Pt) -> Pt {
    let (dx, dy) = (toward.0 - c.0, toward.1 - c.1);
    if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
        return c;
    }
    let t = match shape {
        Shape::Rect => {
            let tx = if dx.abs() > 1e-6 {
                (w / 2.0) / dx.abs()
            } else {
                f32::MAX
            };
            let ty = if dy.abs() > 1e-6 {
                (h / 2.0) / dy.abs()
            } else {
                f32::MAX
            };
            tx.min(ty)
        }
        Shape::Ellipse => {
            let (a, b) = (w / 2.0, h / 2.0);
            1.0 / ((dx * dx) / (a * a) + (dy * dy) / (b * b)).sqrt()
        }
    };
    (c.0 + dx * t.min(1.0), c.1 + dy * t.min(1.0))
}

/// Segmen a→b memotong kotak r [x, y, w, h]? (Liang–Barsky)
fn seg_hits(a: Pt, b: Pt, r: [f32; 4]) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a.0 - r[0]),
        (dx, r[0] + r[2] - a.0),
        (-dy, a.1 - r[1]),
        (dy, r[1] + r[3] - a.1),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t0 < t1
}

/// Polyline → spline Catmull-Rom (Bézier kubik); ujung bertangen ke arah rank.
fn smooth(pts: &[Pt], lr: bool) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let n = pts.len();
    // Semua titik segaris: satu segmen (banyak potongan kecil tampak putus-putus).
    let (a, b) = (pts[0], pts[n - 1]);
    let l = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt().max(1e-3);
    if pts
        .iter()
        .all(|p| ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)).abs() / l < 0.5)
    {
        return vec![a, b];
    }
    let tan = |i: usize| -> Pt {
        let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
        let k = if i == 0 || i == n - 1 { 1.0 } else { 0.5 };
        let t = ((b.0 - a.0) * k, (b.1 - a.1) * k);
        // Ujung: condongkan ke sumbu rank supaya masuk/keluar node tegak.
        if i == 0 || i == n - 1 {
            if lr {
                (t.0, t.1 * 0.7)
            } else {
                (t.0 * 0.7, t.1)
            }
        } else {
            t
        }
    };
    let mut out = vec![pts[0]];
    for i in 0..n - 1 {
        let (a, b) = (pts[i], pts[i + 1]);
        let (ta, tb) = (tan(i), tan(i + 1));
        vector::bezier(
            &mut out,
            a,
            (a.0 + ta.0 / 3.0, a.1 + ta.1 / 3.0),
            (b.0 - tb.0 / 3.0, b.1 - tb.1 / 3.0),
            b,
        );
    }
    out
}

/// Item di satu tingkat cluster: node langsung atau cluster anak.
#[derive(Clone, Copy, PartialEq, Debug)]
enum It {
    N(usize),
    C(usize),
}

/// Fase rank dot (rank.c dot1_rank) untuk graf tanpa cluster: class1 (sisi kembar
/// digabung), decompose, acyclic (DFS membalik sisi balik), lalu network simplex
/// dengan TB_balance per komponen.
/// Urutan pembuatan node di cgraph untuk DOT dari PlantUML (svek): ujung sisi
/// minlen 0 ditulis paling awal, lalu node di luar cluster, lalu node tiap cluster
/// (cluster induk dulu, anggota langsung lalu subcluster). Hasil: urutan node.
fn node_seq(g: &Graph) -> Vec<usize> {
    let n = g.nodes.len();
    let mut ord = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    let mut add = |v: usize, ord: &mut Vec<usize>| {
        if !seen[v] {
            seen[v] = true;
            ord.push(v);
        }
    };
    for e in g.edges.iter().filter(|e| e.minlen == 0 && e.a != e.b) {
        add(e.a, &mut ord);
        add(e.b, &mut ord);
    }
    for v in 0..n {
        if g.nodes[v].cluster.is_none() {
            add(v, &mut ord);
        }
    }
    fn rec(g: &Graph, c: usize, out: &mut Vec<usize>) {
        for v in 0..g.nodes.len() {
            if g.nodes[v].cluster == Some(c) {
                out.push(v);
            }
        }
        for d in 0..g.clusters.len() {
            if g.clusters[d].parent == Some(c) {
                rec(g, d, out);
            }
        }
    }
    let mut cl = Vec::new();
    for c in 0..g.clusters.len() {
        if g.clusters[c].parent.is_none() {
            rec(g, c, &mut cl);
        }
    }
    for v in cl.into_iter().chain(0..n) {
        add(v, &mut ord);
    }
    ord
}

fn dot_rank(g: &Graph, mult: u32) -> Vec<usize> {
    let order = node_seq(g);
    let mut seq = vec![0; g.nodes.len()];
    for (i, &v) in order.iter().enumerate() {
        seq[v] = i;
    }
    let n = g.nodes.len();
    // class1: per node (urutan node), sisi keluar urut menurut kepala (agfstout).
    let mut eord: Vec<usize> = (0..g.edges.len()).collect();
    eord.sort_by_key(|&k| (seq[g.edges[k].a], seq[g.edges[k].b]));
    let (mut et, mut eh, mut ml, mut wt) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut inn: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &k in &eord {
        let e = &g.edges[k];
        if e.a == e.b {
            continue;
        }
        let m = (e.minlen * mult) as i64;
        if let Some(&f) = out[e.a].iter().find(|&&f| eh[f] == e.b) {
            ml[f] = i64::max(ml[f], m);
            wt[f] += 1;
        } else {
            let f = et.len();
            et.push(e.a);
            eh.push(e.b);
            ml.push(m);
            wt.push(1i64);
            out[e.a].push(f);
            inn[e.b].push(f);
        }
    }
    // decompose: DFS bertumpuk (keluar lalu masuk) per komponen.
    let mut comps: Vec<Vec<usize>> = Vec::new();
    let mut mark = vec![false; n];
    for &s0 in &order {
        if mark[s0] {
            continue;
        }
        let mut comp = Vec::new();
        let mut st = vec![s0];
        while let Some(u) = st.pop() {
            if mark[u] {
                continue;
            }
            mark[u] = true;
            comp.push(u);
            for &f in out[u].iter().chain(inn[u].iter()).rev() {
                let o = if et[f] == u { eh[f] } else { et[f] };
                if !mark[o] {
                    st.push(o);
                }
            }
        }
        comps.push(comp);
    }
    // acyclic: dfs, sisi ke node di tumpukan dibalik (gabung bila sisi balik ada).
    let mut mark = vec![false; n];
    let mut onstack = vec![false; n];
    #[allow(clippy::too_many_arguments)]
    fn dfs(
        u: usize,
        out: &mut Vec<Vec<usize>>,
        inn: &mut Vec<Vec<usize>>,
        et: &mut Vec<usize>,
        eh: &mut Vec<usize>,
        ml: &mut Vec<i64>,
        wt: &mut Vec<i64>,
        mark: &mut Vec<bool>,
        onstack: &mut Vec<bool>,
    ) {
        if mark[u] {
            return;
        }
        mark[u] = true;
        onstack[u] = true;
        let mut i = 0;
        while i < out[u].len() {
            let e = out[u][i];
            let w = eh[e];
            if onstack[w] {
                // reverse_edge: hapus (zapinlist = tukar dengan terakhir), lalu gabung
                // ke sisi w→u yang ada atau buat baru.
                let j = out[u].iter().position(|&x| x == e).unwrap();
                out[u].swap_remove(j);
                let j = inn[w].iter().position(|&x| x == e).unwrap();
                inn[w].swap_remove(j);
                if let Some(&f) = out[w].iter().find(|&&f| eh[f] == u) {
                    ml[f] = i64::max(ml[f], ml[e]);
                    wt[f] += wt[e];
                } else {
                    let f = et.len();
                    et.push(w);
                    eh.push(u);
                    ml.push(ml[e]);
                    wt.push(wt[e]);
                    out[w].push(f);
                    inn[u].push(f);
                }
                continue;
            }
            if !mark[w] {
                dfs(w, out, inn, et, eh, ml, wt, mark, onstack);
            }
            i += 1;
        }
        onstack[u] = false;
    }
    for comp in &comps {
        for &u in comp {
            dfs(
                u,
                &mut out,
                &mut inn,
                &mut et,
                &mut eh,
                &mut ml,
                &mut wt,
                &mut mark,
                &mut onstack,
            );
        }
    }
    // rank1: network simplex + TB_balance per komponen.
    let mut rank = vec![0usize; n];
    for comp in &comps {
        let mut idx = vec![usize::MAX; n];
        for (i, &u) in comp.iter().enumerate() {
            idx[u] = i;
        }
        let mut ns = super::dot_ns::Ns::new(comp.len());
        for &u in comp {
            for &f in &out[u] {
                if idx[eh[f]] != usize::MAX {
                    ns.edge(idx[u], idx[eh[f]], ml[f], wt[f]);
                }
            }
        }
        ns.nlist = (0..comp.len()).collect();
        if ns.solve_tb(500, 1_000_000).is_none() {
            return Vec::new();
        }
        for (i, &u) in comp.iter().enumerate() {
            rank[u] = ns.rank[i].max(0) as usize;
        }
    }
    rank
}

/// Rank tiap node (lihat komentar di `layout`).
fn hier_rank(g: &Graph, paths: &[Vec<usize>], mult: u32) -> Vec<usize> {
    let n = g.nodes.len();
    let nc = g.clusters.len();
    // rel[u] = rank u relatif terhadap cluster terdalam yang sudah diproses.
    let mut rel = vec![0i32; n];
    let mut height = vec![1i32; nc];
    // Urutan: cluster terdalam dulu, akar terakhir.
    let mut order: Vec<Option<usize>> = (0..nc).map(Some).collect();
    order.sort_by_key(|c| std::cmp::Reverse(c.map_or(0, |k| path_len(g, k))));
    order.push(None);
    // Item tingkat L yang memuat node u (None bila u di luar L).
    let item_at = |u: usize, lvl: Option<usize>| -> Option<It> {
        let p = &paths[u];
        match lvl {
            None => Some(p.first().map_or(It::N(u), |&c| It::C(c))),
            Some(l) => {
                let i = p.iter().position(|&c| c == l)?;
                Some(p.get(i + 1).map_or(It::N(u), |&c| It::C(c)))
            }
        }
    };
    let mut item_rank: Vec<(Option<usize>, It, i32)> = Vec::new();
    for lvl in order {
        let mut items: Vec<It> = (0..n)
            .filter(|&u| g.nodes[u].cluster == lvl)
            .map(It::N)
            .collect();
        items.extend((0..nc).filter(|&c| g.clusters[c].parent == lvl).map(It::C));
        let m = items.len();
        if m == 0 {
            continue;
        }
        let idx = |it: It| items.iter().position(|&x| x == it).unwrap();
        // Batasan (a, b, w): R(b) >= R(a) + w.
        let mut cons: Vec<(usize, usize, i32)> = Vec::new();
        let mut eq: Vec<(usize, usize)> = Vec::new();
        for e in &g.edges {
            if e.a == e.b {
                continue;
            }
            let (Some(ia), Some(ib)) = (item_at(e.a, lvl), item_at(e.b, lvl)) else {
                continue;
            };
            if ia == ib {
                continue;
            }
            // off = rank node di dalam itemnya (0 bila item = node).
            let off = |u: usize, it: It| if let It::N(_) = it { 0 } else { rel[u] };
            let (a, b) = (idx(ia), idx(ib));
            if e.minlen == 0 {
                if matches!((ia, ib), (It::N(_), It::N(_))) {
                    eq.push((a, b));
                } else {
                    cons.push((a, b, off(e.a, ia) - off(e.b, ib)));
                }
            } else {
                cons.push((a, b, (e.minlen * mult) as i32 + off(e.a, ia) - off(e.b, ib)));
            }
        }
        // Sejajar node-node: satukan.
        let mut uf: Vec<usize> = (0..m).collect();
        for &(a, b) in &eq {
            let (ra, rb) = (find(&mut uf, a), find(&mut uf, b));
            if ra != rb {
                uf[rb] = ra;
            }
        }
        let rep: Vec<usize> = (0..m).map(|i| find(&mut uf, i)).collect();
        let cons: Vec<(usize, usize, i32)> = cons
            .into_iter()
            .map(|(a, b, w)| (rep[a], rep[b], w))
            .filter(|c| c.0 != c.1)
            .collect();
        // Putus siklus dengan DFS (sisi balik dibalik).
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); m];
        for (k, c) in cons.iter().enumerate() {
            adj[c.0].push(k);
        }
        let mut rev = vec![false; cons.len()];
        let mut st = vec![0u8; m];
        for s0 in 0..m {
            if rep[s0] != s0 || st[s0] != 0 {
                continue;
            }
            let mut stack = vec![(s0, 0usize)];
            st[s0] = 1;
            while let Some(&mut (u, ref mut i)) = stack.last_mut() {
                if *i < adj[u].len() {
                    let k = adj[u][*i];
                    *i += 1;
                    let v = cons[k].1;
                    match st[v] {
                        0 => {
                            st[v] = 1;
                            stack.push((v, 0));
                        }
                        1 => rev[k] = true,
                        _ => {}
                    }
                } else {
                    st[u] = 2;
                    stack.pop();
                }
            }
        }
        let dag: Vec<(usize, usize, i32)> = cons
            .iter()
            .zip(&rev)
            .map(|(c, &r)| if r { (c.1, c.0, c.2.abs().max(1)) } else { *c })
            .collect();
        let mut r = vec![0i32; m];
        let mut indeg = vec![0usize; m];
        let mut out: Vec<Vec<(usize, i32)>> = vec![Vec::new(); m];
        let mut inc: Vec<Vec<(usize, i32)>> = vec![Vec::new(); m];
        for &(a, b, w) in &dag {
            out[a].push((b, w));
            inc[b].push((a, w));
            indeg[b] += 1;
        }
        let mut q: Vec<usize> = (0..m)
            .rev()
            .filter(|&i| rep[i] == i && indeg[i] == 0)
            .collect();
        let mut topo = Vec::new();
        while let Some(u) = q.pop() {
            topo.push(u);
            for &(v, w) in &out[u] {
                r[v] = r[v].max(r[u] + w);
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    q.push(v);
                }
            }
        }
        // Penyeimbangan sederhana (node tanpa masukan ditarik ke penerusnya).
        for _ in 0..6 {
            for &u in topo.iter().rev() {
                let lo = inc[u].iter().map(|&(v, w)| r[v] + w).max();
                let hi = out[u].iter().map(|&(v, w)| r[v] - w).min();
                match (lo, hi) {
                    (None, Some(h)) => r[u] = h,
                    (Some(l), Some(h)) if h > l => {
                        if out[u].len() > inc[u].len() {
                            r[u] = h;
                        } else if inc[u].len() > out[u].len() {
                            r[u] = l;
                        }
                    }
                    _ => {}
                }
            }
        }
        let r: Vec<i32> = (0..m).map(|i| r[rep[i]]).collect();
        let r0 = r.iter().copied().min().unwrap_or(0);
        let mut h = 1;
        for (i, &it) in items.iter().enumerate() {
            let ri = r[i] - r0;
            item_rank.push((lvl, it, ri));
            let ih = match it {
                It::N(_) => 1,
                It::C(c) => height[c],
            };
            h = h.max(ri + ih);
        }
        if let Some(c) = lvl {
            height[c] = h;
            for u in 0..n {
                if let Some(it) = item_at(u, lvl) {
                    let ri = r[idx(it)] - r0;
                    rel[u] = ri + if let It::N(_) = it { 0 } else { rel[u] };
                }
            }
        }
    }
    // Rank absolut: jumlah rank lokal sepanjang jalur cluster.
    let local = |lvl: Option<usize>, it: It| {
        item_rank
            .iter()
            .find(|x| x.0 == lvl && x.1 == it)
            .map_or(0, |x| x.2)
    };
    (0..n)
        .map(|u| {
            let p = &paths[u];
            let mut total = 0;
            let mut lvl = None;
            for &c in p {
                total += local(lvl, It::C(c));
                lvl = Some(c);
            }
            (total + local(lvl, It::N(u))).max(0) as usize
        })
        .collect()
}

fn path_len(g: &Graph, c: usize) -> usize {
    let mut k = Some(c);
    let mut d = 0;
    while let Some(x) = k {
        d += 1;
        k = g.clusters[x].parent;
    }
    d
}

pub fn layout(g: &Graph) -> Out {
    let n = g.nodes.len();
    let nc = g.clusters.len();
    if n == 0 {
        return Out {
            pos: Vec::new(),
            edges: vec![Vec::new(); g.edges.len()],
            labels: vec![None; g.edges.len()],
            clusters: vec![[0.0; 4]; nc],
            w: 0.0,
            h: 0.0,
        };
    }
    // Orientasi: tata letak selalu atas→bawah; lr ditukar di akhir.
    let dim = |w: f32, h: f32| if g.lr { (h, w) } else { (w, h) };
    let pad_of = |c: usize| {
        let p = g.clusters[c].pad;
        // [atas, kanan, bawah, kiri] dalam orientasi tata letak.
        if g.lr {
            [p[3], p[2], p[1], p[0]]
        } else {
            p
        }
    };
    // Jalur cluster (akar → terdalam).
    let path_of = |c: Option<usize>| {
        let mut v = Vec::new();
        let mut k = c;
        while let Some(x) = k {
            v.push(x);
            k = g.clusters[x].parent;
        }
        v.reverse();
        v
    };
    let lca = |a: &[usize], b: &[usize]| -> Option<usize> {
        let mut r = None;
        for (x, y) in a.iter().zip(b) {
            if x == y {
                r = Some(*x);
            } else {
                break;
            }
        }
        r
    };
    let has_label = g.edges.iter().any(|e| e.label.is_some());
    let mult = if has_label { 2 } else { 1 };
    let ranksep = if has_label {
        g.ranksep / 2.0
    } else {
        g.ranksep
    };

    // Ranking hierarkis seperti dot: isi tiap cluster diranking dulu (rank lokal),
    // lalu cluster menjadi satu blok rank di tingkat induknya. Sisi u->v di tingkat
    // L memberi batasan R(item u) + off(u) + minlen <= R(item v) + off(v).
    let paths: Vec<Vec<usize>> = g.nodes.iter().map(|n| path_of(n.cluster)).collect();
    // Tanpa cluster: fase rank dot apa adanya (dot_rank); dengan cluster: hierarkis.
    let seq_order = node_seq(g);
    let mut seq = vec![0; n];
    for (i, &v) in seq_order.iter().enumerate() {
        seq[v] = i;
    }
    let rank = match g.nodes.iter().all(|nd| nd.cluster.is_none()) {
        true => Some(dot_rank(g, mult)).filter(|r| r.len() == n),
        false => None,
    }
    .unwrap_or_else(|| hier_rank(g, &paths, mult));

    // Node virtual. vs: (w, h), vcl: cluster, vrank.
    let mut vs: Vec<(f32, f32)> = g.nodes.iter().map(|n| dim(n.w, n.h)).collect();
    let mut vcl: Vec<Option<usize>> = g.nodes.iter().map(|n| n.cluster).collect();
    let mut vrank = rank.clone();
    // class2 dot: urutan pembuatan rantai (`proc`, per node ekor, sisi keluar urut
    // kepala; rantai sisi lawan dibuat lebih dulu bila perlu) dan penggabungan sisi
    // kembar tanpa label ke rantai yang sudah ada (`merged`).
    let m = g.edges.len();
    let mut proc: Vec<usize> = Vec::new();
    let mut merged: Vec<Option<usize>> = vec![None; m];
    {
        let root = |merged: &Vec<Option<usize>>, mut k: usize| {
            while let Some(r) = merged[k] {
                k = r;
            }
            k
        };
        let mut made = vec![false; m];
        let mut byt: Vec<usize> = (0..m).collect();
        // build_skeleton dot: sisi skeleton cluster dibuat lebih dulu, jadi dari node
        // ber-cluster, sisi di dalam cluster puncak yang sama mendahului sisi keluar.
        let top = |v: usize| paths[v].first().copied();
        byt.sort_by_key(|&k| {
            let (a, b) = (g.edges[k].a, g.edges[k].b);
            let inter = top(a).is_some() && top(a) != top(b);
            (seq[a], inter, seq[b], k)
        });
        let mut prev: Option<usize> = None;
        for (i, &k) in byt.iter().enumerate() {
            let e = &g.edges[k];
            if i == 0 || g.edges[byt[i - 1]].a != e.a {
                prev = None;
            }
            if made[k] || merged[k].is_some() {
                prev = Some(k);
                continue;
            }
            if let Some(p) = prev {
                let pe = &g.edges[p];
                if pe.b == e.b
                    && (rank[e.a] == rank[e.b] || (e.label.is_none() && pe.label.is_none()))
                {
                    merged[k] = Some(root(&merged, p));
                    continue;
                }
            }
            if e.a == e.b || rank[e.b] >= rank[e.a] {
                made[k] = true;
                proc.push(k);
                prev = Some(k);
                continue;
            }
            // agfindedge (kamus splay cgraph): umumnya sisi lawan yang terakhir dibuat.
            if let Some(j) = (0..m)
                .rev()
                .find(|&j| g.edges[j].a == e.b && g.edges[j].b == e.a)
            {
                if !made[j] && merged[j].is_none() {
                    made[j] = true;
                    proc.push(j);
                }
                if e.label.is_none() && g.edges[j].label.is_none() {
                    merged[k] = Some(root(&merged, j));
                    continue;
                }
            }
            made[k] = true;
            proc.push(k);
            prev = Some(k);
        }
    }
    let mut chains: Vec<(Vec<usize>, bool, Option<usize>)> = Vec::new();
    for (k, e) in g.edges.iter().enumerate() {
        let (a, b) = (e.a, e.b);
        if merged[k].is_some() {
            chains.push((Vec::new(), rank[a] > rank[b], None));
            continue;
        }
        if a == b {
            chains.push((vec![a], false, None));
            continue;
        }
        let (mut u, mut v, mut flip) = (a, b, false);
        if rank[u] > rank[v] {
            (u, v, flip) = (v, u, true);
        }
        let mut c = vec![u];
        let mut lab = None;
        let (ru, rv) = (rank[u], rank[v]);
        let cl = lca(&paths[u], &paths[v]);
        if rv > ru + 1 {
            let mid = ru + (rv - ru) / 2;
            for r in ru + 1..rv {
                let is_lab = e.label.is_some() && r == mid;
                let size = if is_lab {
                    // label_vnode dot: lw = nodesep, rw = lebar label; garis lewat
                    // posisi node, label di kanannya.
                    dim(e.label.unwrap().0, e.label.unwrap().1)
                } else {
                    (0.0, 0.0)
                };
                vs.push(size);
                vcl.push(cl);
                vrank.push(r);
                if is_lab {
                    lab = Some(vs.len() - 1);
                }
                c.push(vs.len() - 1);
            }
        }
        c.push(v);
        chains.push((c, flip, lab));
    }
    // Sisi gabungan memakai rantai induknya; node virtualnya melebar nodesep/2
    // (incr_width) per gabungan.
    let mut extra = vec![0.0f32; vs.len()];
    for k in 0..m {
        if let Some(mut r) = merged[k] {
            while let Some(q) = merged[r] {
                r = q;
            }
            let c = chains[r].0.clone();
            if c.len() > 2 && rank[g.edges[k].a] != rank[g.edges[k].b] {
                for &v in &c[1..c.len() - 1] {
                    extra[v] += ((g.nodesep.round() as i32) / 2) as f32;
                }
            }
            chains[k].0 = c;
        }
    }
    let nr = vrank.iter().copied().max().unwrap_or(0) + 1;
    // Pengisi cluster: rank tanpa anggota di dalam rentang cluster.
    let spacer0 = vs.len();
    let mut span = vec![(usize::MAX, 0usize); nc];
    for v in 0..vs.len() {
        for c in path_of(vcl[v]) {
            span[c] = (span[c].0.min(vrank[v]), span[c].1.max(vrank[v]));
        }
    }
    for c in 0..nc {
        if span[c].0 == usize::MAX {
            continue;
        }
        for r in span[c].0..=span[c].1 {
            let has = (0..vs.len()).any(|v| vrank[v] == r && path_of(vcl[v]).contains(&c));
            if !has {
                vs.push((0.0, 0.0));
                vcl.push(Some(c));
                vrank.push(r);
            }
        }
    }
    let nv = vs.len();
    let is_spacer = |v: usize| v >= spacer0;
    let vpath: Vec<Vec<usize>> = (0..nv).map(|v| path_of(vcl[v])).collect();
    let mut up: Vec<Vec<(usize, f32)>> = vec![Vec::new(); nv];
    let mut dn: Vec<Vec<(usize, f32)>> = vec![Vec::new(); nv];
    // Urutan sisi seperti class2 dot: per node ekor asli (urutan node), lalu urutan
    // sisi — menentukan urutan tetangga dalam BFS urutan awal.
    // agfstout cgraph: sisi keluar urut menurut nomor node kepala, lalu urutan sisi.
    // Bobot sisi seperti tabel class2 dot: node "ordinary" (≥ 2 sisi) 1, singleton
    // (≤ 1 sisi) atau virtual 2, virtual–virtual 4.
    // weight_class dot: jumlah sisi per node (dihitung penuh sebelum rantai dibuat).
    let mut deg = vec![0usize; n];
    for e in &g.edges {
        deg[e.a] += 1;
        deg[e.b] += 1;
    }
    let class = |v: usize| {
        if v >= n {
            2
        } else if deg[v] <= 1 {
            1
        } else {
            0
        }
    };
    let mut xpen: std::collections::HashMap<(usize, usize), i64> = std::collections::HashMap::new();
    for (c, _, _) in proc.iter().map(|&k| &chains[k]) {
        for w in c.windows(2) {
            let (a, b) = (w[0], w[1]);
            if vrank[a] + 1 == vrank[b] {
                let wt = match (class(a), class(b)) {
                    (0, _) | (_, 0) => 1.0,
                    (2, 2) => 4.0,
                    _ => 2.0,
                };
                dn[a].push((b, wt));
                up[b].push((a, wt));
            }
        }
    }
    // merge_chain: bobot & xpenalty rantai induk bertambah.
    for k in 0..m {
        if merged[k].is_none() || chains[k].0.len() < 2 {
            continue;
        }
        for w in chains[k].0.windows(2) {
            let (a, b) = (w[0], w[1]);
            if vrank[a] + 1 != vrank[b] {
                continue;
            }
            if let Some(x) = dn[a].iter_mut().find(|x| x.0 == b) {
                x.1 += 1.0;
            }
            if let Some(x) = up[b].iter_mut().find(|x| x.0 == a) {
                x.1 += 1.0;
            }
            *xpen.entry((a, b)).or_insert(1) += 1;
        }
    }
    let xp = |a: usize, b: usize| *xpen.get(&(a, b)).unwrap_or(&1);

    // Urutan: kunci per node, lalu susun hierarkis per cluster (anggota bersebelahan).
    let same: Vec<(usize, usize, bool)> = g
        .edges
        .iter()
        .filter(|e| e.minlen == 0 && e.a != e.b)
        .map(|e| (e.a, e.b, e.ordered))
        .collect();
    fn rec(
        items: &[usize],
        depth: usize,
        vpath: &[Vec<usize>],
        key: &dyn Fn(usize) -> f32,
    ) -> Vec<usize> {
        // Kelompokkan berdasarkan cluster di kedalaman `depth`.
        let mut groups: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
        for &v in items {
            match vpath[v].get(depth).copied() {
                Some(c) => match groups.iter_mut().find(|g| g.0 == Some(c)) {
                    Some(g) => g.1.push(v),
                    None => groups.push((Some(c), vec![v])),
                },
                None => groups.push((None, vec![v])),
            }
        }
        let mut keyed: Vec<(f32, Vec<usize>)> = groups
            .into_iter()
            .map(|(c, vsx)| {
                let k = vsx.iter().map(|&v| key(v)).sum::<f32>() / vsx.len() as f32;
                let inner = if c.is_some() {
                    rec(&vsx, depth + 1, vpath, key)
                } else {
                    vsx
                };
                (k, inner)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        keyed.into_iter().flat_map(|x| x.1).collect()
    }
    let arrange =
        |layer: &[usize], key: &dyn Fn(usize) -> f32| -> Vec<usize> { rec(layer, 0, &vpath, key) };
    let fix_same = |layer: &mut Vec<usize>| {
        for &(a, b, ordered) in &same {
            let (Some(ia), Some(ib)) = (
                layer.iter().position(|&x| x == a),
                layer.iter().position(|&x| x == b),
            ) else {
                continue;
            };
            if vpath[a] != vpath[b] {
                continue;
            }
            if ordered && ib != ia + 1 || !ordered && ia.abs_diff(ib) > 1 {
                let v = layer.remove(ib);
                let ia = layer.iter().position(|&x| x == a).unwrap();
                let at = if ordered || ib > ia { ia + 1 } else { ia };
                layer.insert(at, v);
            }
        }
    };
    let cross_between = |l1: &[usize], pos: &[usize]| -> usize {
        let es: Vec<(usize, usize, usize)> = l1
            .iter()
            .flat_map(|&u| dn[u].iter().map(move |&(v, _)| (u, v)))
            .map(|(u, v)| (pos[u], pos[v], xp(u, v) as usize))
            .collect();
        let mut c = 0;
        for i in 0..es.len() {
            for j in i + 1..es.len() {
                if (es[i].0 < es[j].0 && es[i].1 > es[j].1)
                    || (es[i].0 > es[j].0 && es[i].1 < es[j].1)
                {
                    c += es[i].2 * es[j].2;
                }
            }
        }
        c
    };
    let total_cross = |layers: &Vec<Vec<usize>>| -> usize {
        let mut pos = vec![0usize; nv];
        for l in layers {
            for (i, &v) in l.iter().enumerate() {
                pos[v] = i;
            }
        }
        layers.iter().map(|l| cross_between(l, &pos)).sum()
    };
    // Urutan node seperti decompose dot: DFS preorder, sisi keluar lalu masuk.
    // `comps`: komponen terhubung (decompose), masing-masing dalam urutan nlist.
    let mut nlist = Vec::with_capacity(nv);
    let mut comps: Vec<Vec<usize>> = Vec::new();
    {
        let mut seen = vec![false; nv];
        for s0 in seq_order.iter().copied().chain(n..nv) {
            if !seen[s0] {
                comps.push(Vec::new());
            }
            let mut st = vec![s0];
            while let Some(u) = st.pop() {
                if seen[u] {
                    continue;
                }
                seen[u] = true;
                nlist.push(u);
                comps.last_mut().unwrap().push(u);
                for &(v, _) in dn[u].iter().chain(up[u].iter()).rev() {
                    if !seen[v] {
                        st.push(v);
                    }
                }
            }
        }
    }
    // Dua urutan awal (pass 0 & 1 dot), ambil yang persilangannya paling sedikit
    // — median + transpose mudah terjebak minimum lokal.
    // mincross dot apa adanya (graf tanpa cluster): reorder median dengan node tanpa
    // tetangga tetap di tempat & seri ditukar saat `reverse`, lalu transpose.
    let l2r = |a: usize, b: usize| {
        same.iter()
            .any(|&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a))
    };
    let transpose = |layers: &mut Vec<Vec<usize>>, reverse: bool| {
        let mut pos = vec![0i64; nv];
        for l in layers.iter() {
            for (i, &v) in l.iter().enumerate() {
                pos[v] = i as i64;
            }
        }
        // in_cross/out_cross dot, berbobot xpenalty.
        let cross = |isup: bool, pos: &[i64], v: usize, w: usize| -> i64 {
            let nb = if isup { &up } else { &dn };
            let key = |s: usize, t: usize| if isup { xp(t, s) } else { xp(s, t) };
            let mut c = 0;
            for &(t2, _) in &nb[w] {
                for &(t1, _) in &nb[v] {
                    if pos[t1] > pos[t2] {
                        c += key(v, t1) * key(w, t2);
                    }
                }
            }
            c
        };
        let mut cand = vec![true; nr];
        loop {
            let mut delta = 0;
            for r in 0..nr {
                if !cand[r] {
                    continue;
                }
                cand[r] = false;
                for i in 0..layers[r].len().saturating_sub(1) {
                    let (v, w) = (layers[r][i], layers[r][i + 1]);
                    if l2r(v, w) {
                        continue;
                    }
                    let (mut c0, mut c1) = (0, 0);
                    if r > 0 {
                        c0 += cross(true, &pos, v, w);
                        c1 += cross(true, &pos, w, v);
                    }
                    if r + 1 < nr {
                        c0 += cross(false, &pos, v, w);
                        c1 += cross(false, &pos, w, v);
                    }
                    if c1 < c0 || (c0 > 0 && reverse && c1 == c0) {
                        layers[r].swap(i, i + 1);
                        pos[v] = i as i64 + 1;
                        pos[w] = i as i64;
                        delta += c0 - c1;
                        cand[r] = true;
                        if r > 0 {
                            cand[r - 1] = true;
                        }
                        if r + 1 < nr {
                            cand[r + 1] = true;
                        }
                    }
                }
            }
            if delta < 1 {
                break;
            }
        }
    };
    let mincross_step = |layers: &mut Vec<Vec<usize>>, iter: usize| {
        let reverse = iter % 4 < 2;
        let down = iter % 2 == 0;
        let rs: Vec<usize> = if down {
            (1..nr).collect()
        } else {
            (0..nr.saturating_sub(1)).rev().collect()
        };
        for r in rs {
            let mut pos = vec![0i64; nv];
            for l in layers.iter() {
                for (i, &v) in l.iter().enumerate() {
                    pos[v] = i as i64;
                }
            }
            let nb = if down { &up } else { &dn };
            let mval: Vec<i64> = (0..nv)
                .map(|v| {
                    let mut xs: Vec<i64> = nb[v].iter().map(|&(u, _)| pos[u] * 256).collect();
                    xs.sort();
                    let j = xs.len();
                    match j {
                        0 => -1,
                        1 => xs[0],
                        2 => (xs[0] + xs[1]) / 2,
                        _ if j % 2 == 1 => xs[j / 2],
                        _ => {
                            let (rm, lm) = (j / 2, j / 2 - 1);
                            let (rs, ls) = (xs[j - 1] - xs[rm], xs[lm] - xs[0]);
                            if ls == rs {
                                (xs[lm] + xs[rm]) / 2
                            } else {
                                (xs[lm] * rs + xs[rm] * ls) / (ls + rs)
                            }
                        }
                    }
                })
                .collect();
            let l = &mut layers[r];
            let mut ep = l.len();
            for _ in 0..l.len() {
                let mut lp = 0;
                while lp < ep {
                    while lp < ep && mval[l[lp]] < 0 {
                        lp += 1;
                    }
                    if lp >= ep {
                        break;
                    }
                    let (mut rp, mut stay) = (lp + 1, false);
                    while rp < ep {
                        if l2r(l[lp], l[rp]) {
                            stay = true;
                            break;
                        }
                        if mval[l[rp]] >= 0 {
                            break;
                        }
                        rp += 1;
                    }
                    if rp >= ep {
                        break;
                    }
                    if !stay {
                        let (p1, p2) = (mval[l[lp]], mval[l[rp]]);
                        if p1 > p2 || (p1 == p2 && reverse) {
                            l.swap(lp, rp);
                        }
                    }
                    lp = rp;
                }
                if !reverse {
                    ep = ep.saturating_sub(1);
                }
            }
        }
        transpose(layers, !reverse);
    };
    // Satu pass mincross dot: MinQuit 8, Convergence .995. `best` berlaku lintas
    // pass (disimpan bila <=). Hasil: jumlah silangan keadaan akhir pass.
    type Best = (Vec<Vec<usize>>, usize);
    let dot_pass = |mut layers: Vec<Vec<usize>>, maxit: usize, best: &mut Best| -> usize {
        let mut cur = total_cross(&layers);
        if cur <= best.1 {
            *best = (layers.clone(), cur);
        }
        let mut trying = 0;
        for iter in 0..maxit {
            if trying >= 8 || cur == 0 {
                break;
            }
            trying += 1;
            mincross_step(&mut layers, iter);
            cur = total_cross(&layers);
            if cur <= best.1 {
                if (cur as f32) < 0.995 * best.1 as f32 {
                    trying = 0;
                }
                *best = (layers.clone(), cur);
            }
        }
        cur
    };
    let dot_mc = vcl.iter().all(|c| c.is_none());
    let run = |rev: bool, nl: &[usize]| -> (Vec<Vec<usize>>, usize) {
        // Urutan awal seperti build_ranks dot: BFS dari node tanpa sisi masuk
        // (pass 0) atau tanpa sisi keluar (pass 1), mengikuti urutan `nlist`.
        let mut layers: Vec<Vec<usize>> = vec![Vec::new(); nr];
        {
            let mut seen = vec![false; nv];
            let starts = nl.iter().copied().filter(|&i| {
                if rev {
                    dn[i].is_empty()
                } else {
                    up[i].is_empty()
                }
            });
            let mut q = std::collections::VecDeque::new();
            for s0 in starts.chain(nl.iter().copied()) {
                if seen[s0] {
                    continue;
                }
                seen[s0] = true;
                q.push_back(s0);
                while let Some(u) = q.pop_front() {
                    layers[vrank[u]].push(u);
                    // enqueue_neighbors dot: hanya sisi keluar (pass 0) / masuk (pass 1).
                    let f = if rev { &up[u] } else { &dn[u] };
                    for &(v, _) in f.iter() {
                        if !seen[v] {
                            seen[v] = true;
                            q.push_back(v);
                        }
                    }
                }
            }
            // Akhir build_ranks dot: transpose bila masih bersilang.
            if dot_mc && total_cross(&layers) > 0 {
                transpose(&mut layers, false);
            }
            // flat_reorder dot: rank bersisi sejajar disusun pasca-urut (kepala sisi
            // dulu, lalu ekornya) lalu dibalik.
            for l in layers.iter_mut() {
                if !same.iter().any(|&(a, _, _)| l.contains(&a)) {
                    continue;
                }
                fn post(
                    v: usize,
                    l: &[usize],
                    same: &[(usize, usize, bool)],
                    seen: &mut Vec<usize>,
                    out: &mut Vec<usize>,
                ) {
                    seen.push(v);
                    for &(a, b, _) in same {
                        if a == v && l.contains(&b) && !seen.contains(&b) {
                            post(b, l, same, seen, out);
                        }
                    }
                    out.push(v);
                }
                let (mut seen, mut out) = (Vec::new(), Vec::new());
                for &v in l.iter() {
                    let has_in = same.iter().any(|&(a, b, _)| b == v && l.contains(&a));
                    if !has_in && !seen.contains(&v) {
                        post(v, l, &same, &mut seen, &mut out);
                    }
                }
                for &v in l.iter() {
                    if !seen.contains(&v) {
                        post(v, l, &same, &mut seen, &mut out);
                    }
                }
                out.reverse();
                *l = out;
            }
            let mut pos0 = vec![0.0f32; nv];
            for l in &layers {
                for (i, &v) in l.iter().enumerate() {
                    pos0[v] = i as f32;
                }
            }
            for l in layers.iter_mut() {
                *l = arrange(l, &|v| pos0[v]);
                fix_same(l);
            }
        }
        if dot_mc {
            return (layers, 0);
        }
        let mut best = layers.clone();
        let mut best_c = total_cross(&layers);
        let big = nv > 150;
        for it in 0..if big { 8 } else { 24 } {
            if best_c == 0 {
                break;
            }
            let mut pos = vec![0.0f32; nv];
            for l in &layers {
                for (i, &v) in l.iter().enumerate() {
                    pos[v] = i as f32;
                }
            }
            let down = it % 2 == 0;
            let rs: Vec<usize> = if down {
                (1..nr).collect()
            } else {
                (0..nr.saturating_sub(1)).rev().collect()
            };
            for r in rs {
                let nb = if down { &up } else { &dn };
                let key = |v: usize| -> f32 {
                    let ns = &nb[v];
                    if ns.is_empty() {
                        return pos[v];
                    }
                    let mut xs: Vec<f32> = ns.iter().map(|&(u, _)| pos[u]).collect();
                    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let m = xs.len();
                    let med = if m % 2 == 1 {
                        xs[m / 2]
                    } else {
                        (xs[m / 2 - 1] + xs[m / 2]) / 2.0
                    };
                    // dot (`reverse` saat iter % 4 < 2): median seri ditukar urutannya.
                    // Median berselisih ≥ 0.5, jadi geseran < 0.25 hanya memecah seri.
                    if it % 4 < 2 {
                        med - 0.2 * pos[v] / nv as f32
                    } else {
                        med
                    }
                };
                let mut l = arrange(&layers[r], &key);
                fix_same(&mut l);
                layers[r] = l;
                for (i, &v) in layers[r].iter().enumerate() {
                    pos[v] = i as f32;
                }
            }
            // transpose: tukar tetangga se-cluster bila mengurangi persilangan lokal.
            if !big {
                let mut ipos = vec![0usize; nv];
                for l in &layers {
                    for (i, &v) in l.iter().enumerate() {
                        ipos[v] = i;
                    }
                }
                for r in 0..nr {
                    for i in 0..layers[r].len().saturating_sub(1) {
                        let (a, b) = (layers[r][i], layers[r][i + 1]);
                        if vpath[a] != vpath[b]
                            || same.iter().any(|&(x, y, o)| o && x == a && y == b)
                        {
                            continue;
                        }
                        let local = |layers: &Vec<Vec<usize>>, ipos: &Vec<usize>| -> usize {
                            let mut c = cross_between(&layers[r], ipos);
                            if r > 0 {
                                c += cross_between(&layers[r - 1], ipos);
                            }
                            c
                        };
                        let before = local(&layers, &ipos);
                        layers[r].swap(i, i + 1);
                        ipos[a] = i + 1;
                        ipos[b] = i;
                        if local(&layers, &ipos) >= before {
                            layers[r].swap(i, i + 1);
                            ipos[a] = i;
                            ipos[b] = i + 1;
                        }
                    }
                }
            }
            let c = total_cross(&layers);
            if c < best_c {
                best_c = c;
                best = layers.clone();
            }
        }
        (best, best_c)
    };
    let layers = if dot_mc {
        // mincross dot: pass 0 (dari sumber), pass 1 (dari sink) hanya bila pass 0
        // masih bersilang, lalu pass 2 dari yang terbaik; transpose akhir.
        // Per komponen (init_mccomp), lalu disambung per rank (merge2).
        let mut all: Vec<Vec<usize>> = vec![Vec::new(); nr];
        for comp in &comps {
            let mut best: Best = (Vec::new(), usize::MAX);
            if dot_pass(run(false, comp).0, 4, &mut best) != 0
                && dot_pass(run(true, comp).0, 4, &mut best) != 0
            {
                dot_pass(best.0.clone(), 24, &mut best);
                if best.1 > 0 {
                    transpose(&mut best.0, false);
                }
            }
            for (r, l) in best.0.into_iter().enumerate() {
                all[r].extend(l);
            }
        }
        let best: Best = (all, 0);
        best.0
    } else {
        // Per komponen seperti dot (cluster puncak dihitung satu kesatuan, termasuk
        // pengisinya), lalu disambung per rank.
        let mut uf: Vec<usize> = (0..nv).collect();
        for u in 0..nv {
            for &(v, _) in &dn[u] {
                let (a, b) = (find(&mut uf, u), find(&mut uf, v));
                uf[a] = b;
            }
        }
        let mut top_rep: Vec<Option<usize>> = vec![None; nc];
        for v in 0..nv {
            if let Some(&c) = vpath[v].first() {
                match top_rep[c] {
                    Some(r) => {
                        let (a, b) = (find(&mut uf, v), find(&mut uf, r));
                        uf[a] = b;
                    }
                    None => top_rep[c] = Some(v),
                }
            }
        }
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for &v in &nlist {
            let r = find(&mut uf, v);
            match groups.iter_mut().find(|g| g.0 == r) {
                Some(g) => g.1.push(v),
                None => groups.push((r, vec![v])),
            }
        }
        let mut all: Vec<Vec<usize>> = vec![Vec::new(); nr];
        for (_, comp) in &groups {
            let (l0, c0) = run(false, comp);
            let (l1, c1) = run(true, comp);
            let l = if c1 <= c0 { l1 } else { l0 };
            for (r, x) in l.into_iter().enumerate() {
                all[r].extend(x);
            }
        }
        all
    };

    // Koordinat x. Jarak antartetangga = setengah lebar + nodesep + bantalan cluster
    // yang dilewati di antara keduanya.
    // Jarak antartetangga seperti make_LR_constraints dot: nodesep, tetapi 5 di
    // rank ganjil (rank label) bila ada label sisi. Pengisi cluster: 10.
    let base_sep = |a: usize, b: usize| {
        if is_spacer(a) || is_spacer(b) {
            10.0
        } else if has_label && vrank[a] % 2 == 1 {
            5.0
        } else {
            g.nodesep
        }
    };
    // Bentang kiri/kanan node dari posisi garisnya (node label asimetris).
    let mut is_lab = vec![false; nv];
    for (_, _, lab) in &chains {
        if let Some(v) = *lab {
            is_lab[v] = true;
        }
    }
    // Node virtual dot (plain_vnode): lw = rw = 1 + nodesep/2; node label:
    // lw = nodesep, rw = lebar label.
    let plain_v = |v: usize| v >= n && !is_spacer(v) && !is_lab[v];
    let vhalf = 1.0 + ((g.nodesep.round() as i32) / 2) as f32;
    let lext = |v: usize| {
        if is_lab[v] {
            g.nodesep
        } else if plain_v(v) {
            vhalf + extra[v]
        } else if v < n {
            // dot membulatkan ukuran node ke poin bulat.
            vs[v].0.round() / 2.0
        } else {
            vs[v].0 / 2.0
        }
    };
    let rext = |v: usize| {
        if is_lab[v] {
            vs[v].0
        } else if plain_v(v) {
            vhalf + extra[v]
        } else if v < n {
            vs[v].0.round() / 2.0
        } else {
            vs[v].0 / 2.0
        }
    };
    // flat_edges dot: sisi sejajar berlabel antara tetangga menambah jarak sebesar
    // label (ED_dist; pada LR tinggi label).
    let flat_dist = |a: usize, b: usize| -> f32 {
        g.edges
            .iter()
            .filter(|e| e.minlen == 0 && ((e.a, e.b) == (a, b) || (e.a, e.b) == (b, a)))
            .filter_map(|e| e.label.map(|l| dim(l.0, l.1).0.round()))
            .fold(0.0, f32::max)
    };
    let sep = |a: usize, b: usize| -> f32 {
        let (pa, pb) = (&vpath[a], &vpath[b]);
        let mut k = 0;
        while k < pa.len() && k < pb.len() && pa[k] == pb[k] {
            k += 1;
        }
        let close: f32 = pa[k..].iter().map(|&c| pad_of(c)[1]).sum();
        let open: f32 = pb[k..].iter().map(|&c| pad_of(c)[3]).sum();
        let gap = if close + open > 0.0 {
            g.nodesep / 2.0
        } else {
            0.0
        };
        rext(a) + lext(b) + base_sep(a, b) + close + open + gap + flat_dist(a, b)
    };
    let mut x = vec![0.0f32; nv];
    for l in &layers {
        let mut cx = 0.0;
        for (i, &v) in l.iter().enumerate() {
            if i > 0 {
                cx += sep(l[i - 1], v);
            }
            x[v] = cx;
        }
    }
    // Graf kecil: x seperti dot — network simplex pada graf bantu (urutan dalam rank
    // = sisi ber-minlen jarak; tiap sisi antar-rank = node bantu ke kedua ujung
    // berbobot omega; anggota cluster diapit node kiri/kanan cluster).
    let use_ns = nv <= 150;
    // Tanpa cluster: graf bantu persis seperti position.c dot (make_LR_constraints
    // lalu make_edge_pairs, urutan GD_nlist) dan network simplex ns.c apa adanya.
    let mut done = false;
    if dot_mc {
        let round = |f: f32| f.round() as i64;
        let mut ns = super::dot_ns::Ns::new(nv);
        for (r, l) in layers.iter().enumerate() {
            let Some(&first) = l.first() else { continue };
            let sepr = if has_label && r % 2 == 1 {
                5.0
            } else {
                g.nodesep
            };
            ns.rank[first] = 0;
            let mut last = 0.0f64;
            for j in 0..l.len() {
                let u = l[j];
                if let Some(&v) = l.get(j + 1) {
                    let width = rext(u) + lext(v) + sepr;
                    ns.edge(u, v, round(width), 0);
                    last = (last + width as f64).trunc();
                    ns.rank[v] = last as i64;
                }
                // Sisi sejajar (catatan) dari u.
                for &(a, b, _) in &same {
                    if a != u || !l.contains(&b) {
                        continue;
                    }
                    let ia = l.iter().position(|&x| x == a).unwrap();
                    let ib = l.iter().position(|&x| x == b).unwrap();
                    let (t0, h0) = if ia < ib { (a, b) } else { (b, a) };
                    let width = rext(t0) + lext(h0);
                    match ns.find(t0, h0) {
                        Some(e0) => ns.raise(e0, round(width + g.nodesep + flat_dist(t0, h0)), 1),
                        None => {
                            ns.edge(t0, h0, round(width), 1);
                        }
                    }
                }
            }
        }
        let mut slack = Vec::new();
        for &u in &nlist {
            for &(v, wt) in &dn[u] {
                let sn = ns.node();
                ns.edge(sn, u, 1, wt as i64);
                ns.edge(sn, v, 1, wt as i64);
                ns.rank[sn] = ns.rank[u].min(ns.rank[v]) - 1;
                slack.push(sn);
            }
        }
        ns.nlist = slack
            .iter()
            .rev()
            .copied()
            .chain(nlist.iter().copied())
            .collect();
        if ns.solve(500, 1_000_000).is_some() {
            for v in 0..nv {
                x[v] = ns.rank[v] as f32;
            }
            done = true;
        }
    }
    if use_ns && !done {
        let mut es: Vec<(usize, usize, i64, i64)> = Vec::new();
        let root = nv;
        let mut m = nv + 1;
        for l in &layers {
            if let Some(&f) = l.first() {
                es.push((root, f, 0, 0));
            }
            for w in l.windows(2) {
                es.push((w[0], w[1], sep(w[0], w[1]).ceil() as i64, 0));
            }
        }
        for u in 0..nv {
            for &(v, wt) in &dn[u] {
                es.push((m, u, 0, wt as i64));
                es.push((m, v, 0, wt as i64));
                m += 1;
            }
        }
        for &(a, b, _) in &same {
            es.push((m, a, 0, 1));
            es.push((m, b, 0, 1));
            m += 1;
        }
        for c in 0..nc {
            let mem: Vec<usize> = (0..nv).filter(|&v| vpath[v].contains(&c)).collect();
            if mem.is_empty() {
                continue;
            }
            let (ln, rn) = (m, m + 1);
            m += 2;
            es.push((root, ln, 0, 0));
            let p = pad_of(c);
            for &v in &mem {
                es.push((ln, v, (lext(v) + p[3]).ceil() as i64, 0));
                es.push((v, rn, (rext(v) + p[1]).ceil() as i64, 0));
            }
            // Cluster rapat, seperti dot.
            es.push((ln, rn, 0, 1));
        }
        let r = simplex(m, &es, 2000);
        for v in 0..nv {
            x[v] = r[v] as f32;
        }
    }
    let iters = if use_ns || done {
        0
    } else if nv > 150 {
        8
    } else {
        20
    };
    for it in 0..iters {
        let down = it % 2 == 0;
        let rs: Vec<usize> = if down {
            (0..nr).collect()
        } else {
            (0..nr).rev().collect()
        };
        for r in rs {
            let l = &layers[r];
            if l.is_empty() {
                continue;
            }
            let mut target = Vec::with_capacity(l.len());
            let mut wts = Vec::with_capacity(l.len());
            // Pusat cluster (untuk pengisi).
            let mut ccen = vec![(0.0f32, 0.0f32); nc];
            for u in 0..nv {
                if !is_spacer(u) {
                    for &c in &vpath[u] {
                        ccen[c].0 += x[u];
                        ccen[c].1 += 1.0;
                    }
                }
            }
            for &v in l {
                if is_spacer(v) {
                    let c = vcl[v].unwrap();
                    target.push(if ccen[c].1 > 0.0 {
                        ccen[c].0 / ccen[c].1
                    } else {
                        x[v]
                    });
                    wts.push(1.0);
                    continue;
                }
                let mut s = 0.0;
                let mut ws = 0.0;
                for &(u, w) in up[v].iter().chain(dn[v].iter()) {
                    s += x[u] * w;
                    ws += w;
                }
                if ws > 0.0 {
                    target.push(s / ws);
                    wts.push(ws);
                } else {
                    target.push(x[v]);
                    wts.push(0.05);
                }
            }
            let mut off = vec![0.0f32; l.len()];
            for i in 1..l.len() {
                off[i] = off[i - 1] + sep(l[i - 1], l[i]);
            }
            let t: Vec<f32> = target.iter().zip(&off).map(|(t, o)| t - o).collect();
            let z = pav(&t, &wts);
            for (i, &v) in l.iter().enumerate() {
                x[v] = z[i] + off[i];
            }
        }
    }
    // Kotak cluster (x) lintas rank; dorong non-anggota keluar sampai rapi.
    let mut corder: Vec<usize> = (0..nc).collect();
    corder.sort_by_key(|&c| std::cmp::Reverse(path_of(Some(c)).len()));
    let cluster_x = |x: &[f32]| -> Vec<(f32, f32)> {
        let mut bx = vec![(f32::MAX, f32::MIN); nc];
        for &c in &corder {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for v in 0..nv {
                if vcl[v] == Some(c) && !is_spacer(v) {
                    lo = lo.min(x[v] - lext(v));
                    hi = hi.max(x[v] + rext(v));
                }
            }
            for d in 0..nc {
                if g.clusters[d].parent == Some(c) && bx[d].0 <= bx[d].1 {
                    lo = lo.min(bx[d].0);
                    hi = hi.max(bx[d].1);
                }
            }
            if lo <= hi {
                let p = pad_of(c);
                let (mut l, mut h) = (lo - p[3], hi + p[1]);
                let mw = if g.lr { 0.0 } else { g.clusters[c].min_w };
                if h - l < mw {
                    let d = (mw - (h - l)) / 2.0;
                    l -= d;
                    h += d;
                }
                bx[c] = (l, h);
            }
        }
        bx
    };
    // Kotak cluster tidak boleh menimpa saudara: per tingkat (terdalam dulu), item
    // (node/virtual langsung + cluster anak) diurutkan menurut tengahnya lalu
    // digeser ke kanan bila rentang rank-nya beririsan dengan item sebelumnya.
    {
        let mut levels: Vec<Option<usize>> = corder.iter().map(|&c| Some(c)).collect();
        levels.push(None);
        for lvl in levels {
            let bx = cluster_x(&x);
            // (l, h, r0, r1, item): item = Ok(node) / Err(cluster)
            let mut items: Vec<(f32, f32, usize, usize, Result<usize, usize>)> = Vec::new();
            for v in 0..nv {
                if vcl[v] == lvl && !is_spacer(v) {
                    items.push((x[v] - lext(v), x[v] + rext(v), vrank[v], vrank[v], Ok(v)));
                }
            }
            for c in 0..nc {
                if g.clusters[c].parent == lvl && bx[c].0 <= bx[c].1 && span[c].0 != usize::MAX {
                    items.push((bx[c].0, bx[c].1, span[c].0, span[c].1, Err(c)));
                }
            }
            items.sort_by(|a, b| {
                (a.0 + a.1)
                    .partial_cmp(&(b.0 + b.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for i in 0..items.len() {
                let (l, _, r0, r1, it) = items[i];
                let mut req = f32::MIN;
                for j in 0..i {
                    let (_, hj, s0, s1, jt) = items[j];
                    if s0 <= r1 && r0 <= s1 {
                        let gap = match (it, jt) {
                            (Ok(a), Ok(b)) => base_sep(b, a),
                            (Ok(a), Err(_)) | (Err(_), Ok(a)) => base_sep(a, a) / 2.0 + 4.0,
                            _ => g.nodesep / 2.0,
                        };
                        req = req.max(hj + gap);
                    }
                }
                if l < req {
                    let d = req - l;
                    match it {
                        Ok(v) => x[v] += d,
                        Err(c) => {
                            for u in 0..nv {
                                if vpath[u].contains(&c) {
                                    x[u] += d;
                                }
                            }
                        }
                    }
                    items[i].0 += d;
                    items[i].1 += d;
                }
            }
        }
    }
    let bx = cluster_x(&x);
    // y per rank + ruang kepala/kaki cluster.
    let mut rh = vec![0.0f32; nr];
    for v in 0..nv {
        rh[vrank[v]] = rh[vrank[v]].max(vs[v].1);
    }
    // Tumpukan bantalan atas di rank r: cluster yang mulai di r (bersarang dijumlah).
    let mut top_stack = vec![0.0f32; nr + 1];
    let mut bot_stack = vec![0.0f32; nr + 1];
    for c in 0..nc {
        if span[c].0 == usize::MAX {
            continue;
        }
        let pth = path_of(Some(c));
        let t: f32 = pth
            .iter()
            .filter(|&&a| span[a].0 == span[c].0)
            .map(|&a| pad_of(a)[0])
            .sum();
        let b: f32 = pth
            .iter()
            .filter(|&&a| span[a].1 == span[c].1)
            .map(|&a| pad_of(a)[2])
            .sum();
        top_stack[span[c].0] = top_stack[span[c].0].max(t);
        bot_stack[span[c].1] = bot_stack[span[c].1].max(b);
    }
    let mut ry = vec![0.0f32; nr];
    let mut yy = top_stack[0];
    for r in 0..nr {
        if r > 0 {
            let gap = ranksep.max(bot_stack[r - 1] + top_stack[r] + 10.0);
            yy += gap;
        }
        ry[r] = yy + rh[r] / 2.0;
        yy += rh[r];
    }
    let total_h = yy + bot_stack[nr - 1];
    let mut pos: Vec<Pt> = (0..nv).map(|v| (x[v], ry[vrank[v]])).collect();
    // Kotak cluster (y) dari anggota.
    let mut cbox = vec![[0.0f32; 4]; nc];
    {
        let mut by = vec![(f32::MAX, f32::MIN); nc];
        for &c in &corder {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for v in 0..nv {
                if vcl[v] == Some(c) && !is_spacer(v) {
                    let hh = if v < n || vs[v].1 > 0.0 {
                        vs[v].1 / 2.0
                    } else {
                        0.0
                    };
                    lo = lo.min(pos[v].1 - hh);
                    hi = hi.max(pos[v].1 + hh);
                }
            }
            for d in 0..nc {
                if g.clusters[d].parent == Some(c) && by[d].0 <= by[d].1 {
                    lo = lo.min(by[d].0);
                    hi = hi.max(by[d].1);
                }
            }
            if lo <= hi {
                let p = pad_of(c);
                by[c] = (lo - p[0], hi + p[2]);
            }
            let (l, h) = bx[c];
            if l <= h && by[c].0 <= by[c].1 {
                cbox[c] = [l, by[c].0, h - l, by[c].1 - by[c].0];
            }
        }
    }
    // Normalisasi ke x >= 0, y >= 0.
    let mut minx = f32::MAX;
    let mut maxx = f32::MIN;
    for v in 0..nv {
        if !is_spacer(v) {
            minx = minx.min(x[v] - lext(v));
            maxx = maxx.max(x[v] + rext(v));
        }
    }
    for b in &cbox {
        if b[2] > 0.0 {
            minx = minx.min(b[0]);
            maxx = maxx.max(b[0] + b[2]);
        }
    }
    let miny = cbox
        .iter()
        .filter(|b| b[3] > 0.0)
        .map(|b| b[1])
        .fold(0.0f32, f32::min);
    for p in pos.iter_mut() {
        p.0 -= minx;
        p.1 -= miny;
    }
    for b in cbox.iter_mut() {
        b[0] -= minx;
        b[1] -= miny;
    }
    let (w, h) = (maxx - minx, total_h - miny);
    // Sisi.
    let tr = |p: Pt| if g.lr { (p.1, p.0) } else { p };
    let mut edges = Vec::with_capacity(g.edges.len());
    let mut looped = Vec::with_capacity(g.edges.len());
    let mut labels = Vec::with_capacity(g.edges.len());
    let mut twins: std::collections::HashMap<(usize, usize), Vec<usize>> = Default::default();
    for (k, (c, _, _)) in chains.iter().enumerate() {
        if c.len() >= 2 {
            let e = &g.edges[k];
            twins
                .entry((e.a.min(e.b), e.a.max(e.b)))
                .or_default()
                .push(k);
        }
    }
    // Rute: buang titik virtual yang tak perlu — garis lurus selama tidak menabrak
    // node lain / cluster asing (dot juga merentang spline selurus mungkin).
    let lab_box: Vec<Option<[f32; 4]>> = chains
        .iter()
        .enumerate()
        .map(|(k, (_, _, lab))| {
            lab.map(|v| {
                let (x, y) = pos[v];
                let ext = vs[v].0;
                let (cx, cy) = tr((x + 3.0 + ext / 2.0, y));
                let (lw, lh) = g.edges[k].label.unwrap();
                [cx - lw / 2.0, cy - lh / 2.0, lw, lh]
            })
        })
        .collect();
    let routed: Vec<Vec<Pt>> = chains
        .iter()
        .enumerate()
        .map(|(k, (c, flip, lab))| {
            let e = &g.edges[k];
            if c.len() < 2 {
                return Vec::new();
            }
            let mut forced: Vec<bool> = c.iter().map(|&v| Some(v) == *lab).collect();
            if *flip {
                forced.reverse();
            }
            let mut pts: Vec<Pt> = c.iter().map(|&v| tr(pos[v])).collect();
            if *flip {
                pts.reverse();
            }
            let last = pts.len() - 1;
            let obst: Vec<[f32; 4]> = (0..n)
                .filter(|&v| v != e.a && v != e.b)
                .map(|v| {
                    let (cx, cy) = tr(pos[v]);
                    let (w, h) = (g.nodes[v].w, g.nodes[v].h);
                    [cx - w / 2.0 - 4.0, cy - h / 2.0 - 4.0, w + 8.0, h + 8.0]
                })
                .chain(
                    (0..nc)
                        .filter(|&cc| {
                            !paths[e.a].contains(&cc)
                                && !paths[e.b].contains(&cc)
                                && cbox[cc][2] > 0.0
                        })
                        .map(|cc| {
                            let b = cbox[cc];
                            let b = if g.lr { [b[1], b[0], b[3], b[2]] } else { b };
                            [b[0] - 4.0, b[1] - 4.0, b[2] + 8.0, b[3] + 8.0]
                        }),
                )
                .chain(
                    lab_box
                        .iter()
                        .enumerate()
                        .filter(|&(kk, _)| kk != k)
                        .filter_map(|(_, b)| {
                            b.map(|b| {
                                if g.lr {
                                    [b[0] - 2.0, b[1] - 10.0, b[2] + 4.0, b[3] + 12.0]
                                } else {
                                    [b[0] - 10.0, b[1] - 2.0, b[2] + 12.0, b[3] + 4.0]
                                }
                            })
                        }),
                )
                .collect();
            let mut simp = vec![pts[0]];
            let mut i = 0;
            while i < last {
                // Titik terjauh yang masih terlihat lurus (string pulling).
                let stop = (i + 1..=last).find(|&j| forced[j]).unwrap_or(last);
                let j = (i + 1..=stop)
                    .rev()
                    .find(|&j| j == i + 1 || obst.iter().all(|r| !seg_hits(pts[i], pts[j], *r)))
                    .unwrap();
                // Masih menabrak (titik berikutnya pun terhalang): belok lewat sudut kotak.
                // Kotak gabungan semua penghalang yang ditabrak.
                let hit = obst.iter().filter(|r| seg_hits(pts[i], pts[j], **r)).fold(
                    None,
                    |u: Option<[f32; 4]>, r| {
                        Some(u.map_or(*r, |u| {
                            let (x0, y0) = (u[0].min(r[0]), u[1].min(r[1]));
                            [
                                x0,
                                y0,
                                (u[0] + u[2]).max(r[0] + r[2]) - x0,
                                (u[1] + u[3]).max(r[1] + r[3]) - y0,
                            ]
                        }))
                    },
                );
                if let Some(r) = hit {
                    let cs = [
                        (r[0], r[1]),
                        (r[0] + r[2], r[1]),
                        (r[0], r[1] + r[3]),
                        (r[0] + r[2], r[1] + r[3]),
                    ];
                    let d = |a: Pt, b: Pt| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                    let clear = |a: Pt, b: Pt| obst.iter().all(|o| !seg_hits(a, b, *o));
                    if let Some(c) = cs
                        .iter()
                        .map(|&c| {
                            (
                                c.0 + if c.0 > r[0] { 2.0 } else { -2.0 },
                                c.1 + if c.1 > r[1] { 2.0 } else { -2.0 },
                            )
                        })
                        .filter(|&c| clear(pts[i], c) && clear(c, pts[j]))
                        .min_by(|&a, &b| {
                            (d(pts[i], a) + d(a, pts[j])).total_cmp(&(d(pts[i], b) + d(b, pts[j])))
                        })
                    {
                        simp.push(c);
                    }
                }
                simp.push(pts[j]);
                i = j;
            }
            // Buang titik yang menyimpang < 3 px dari garis tetangganya (zig-zag kecil
            // antara sudut penghalang dan node virtual membuat spline berkait).
            let mut k = 1;
            while k + 1 < simp.len() {
                let (a, p, b) = (simp[k - 1], simp[k], simp[k + 1]);
                let l = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt().max(1e-3);
                if ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)).abs() / l < 3.0 {
                    simp.remove(k);
                } else {
                    k += 1;
                }
            }
            simp
        })
        .collect();
    for (k, (c, _, _)) in chains.into_iter().enumerate() {
        let e = &g.edges[k];
        if c.len() == 1 {
            // Putaran diri di sisi kanan node.
            let (px, py) = tr(pos[c[0]]);
            let (w, h) = (g.nodes[c[0]].w, g.nodes[c[0]].h);
            let a = (px + w / 2.0, py - h / 6.0);
            let b = (px + w / 2.0, py + h / 6.0);
            let mut pts = vec![a];
            vector::bezier(
                &mut pts,
                a,
                (a.0 + 40.0, a.1 - 25.0),
                (b.0 + 40.0, b.1 + 25.0),
                b,
            );
            edges.push(pts);
            looped.push(true);
            labels.push(e.label.map(|l| (px + w / 2.0 + 34.0 + l.0 / 2.0, py)));
            continue;
        }
        let mut pts = routed[k].clone();
        let last = pts.len() - 1;
        // Sisi kembar antara dua node yang sama: geser sejajar supaya tidak bertumpuk.
        {
            let key = (e.a.min(e.b), e.a.max(e.b));
            let (i, cnt) = twins.get(&key).map_or((0, 1), |v| {
                (v.iter().position(|&x| x == k).unwrap_or(0), v.len())
            });
            // Hanya bila keduanya lurus (rute sama); rute berbeda tak perlu digeser.
            let straight = |kk: usize| routed[kk].len() == 2;
            if cnt > 1 && last == 1 && twins[&key].iter().all(|&kk| straight(kk)) {
                let off =
                    (i as f32 - (cnt - 1) as f32 / 2.0) * 12.0 * if e.a < e.b { 1.0 } else { -1.0 };
                let (dx, dy) = (pts[last].0 - pts[0].0, pts[last].1 - pts[0].1);
                let l = (dx * dx + dy * dy).sqrt().max(1e-6);
                let sh = (-dy / l * off, dx / l * off);
                for q in pts.iter_mut() {
                    *q = (q.0 + sh.0, q.1 + sh.1);
                }
            }
        }
        let (na, nb) = (&g.nodes[e.a], &g.nodes[e.b]);
        let s0 = pts[1.min(last)];
        let s1 = pts[last.saturating_sub(1)];
        pts[0] = clip(pts[0], na.w, na.h, na.shape, s0);
        pts[last] = clip(pts[last], nb.w, nb.h, nb.shape, s1);
        let lp = lab_box[k]
            .map(|b| (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0))
            .or_else(|| {
                e.label.map(|l| {
                    let (a, b) = (pts[0], pts[last]);
                    let m = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                    // Sisi pendek (sejajar): label di atas garis seperti dot; pada LR
                    // "atas" menjadi kiri.
                    if (a.0 - b.0).abs() < (a.1 - b.1).abs() {
                        (m.0 - l.0 / 2.0 - 4.0, m.1)
                    } else {
                        (m.0, m.1 - l.1 / 2.0 - 2.0)
                    }
                })
            });
        edges.push(pts);
        looped.push(false);
        labels.push(lp);
    }
    // Sebar ujung sisi yang berdesakan di satu sisi node (dot memberi tiap sisi
    // titik sambung sendiri).
    {
        let at = |p: Pt| if g.lr { (p.1, p.0) } else { p };
        for v in 0..n {
            let (cx, cy) = at(tr(pos[v]));
            let (w, h) = if g.lr {
                (g.nodes[v].h, g.nodes[v].w)
            } else {
                (g.nodes[v].w, g.nodes[v].h)
            };
            for top in [true, false] {
                // (sisi, ujung awal?, x arah tetangga)
                let mut ends: Vec<(usize, bool, f32)> = Vec::new();
                for (k, e) in g.edges.iter().enumerate() {
                    let pts = &edges[k];
                    if looped[k] || pts.len() < 2 {
                        continue;
                    }
                    for (start, node) in [(true, e.a), (false, e.b)] {
                        let (q, nb) = if start {
                            (pts[0], pts[1])
                        } else {
                            (pts[pts.len() - 1], pts[pts.len() - 2])
                        };
                        let (q, nb) = (at(q), at(nb));
                        if node == v && (q.1 < cy) == top && (q.1 - cy).abs() > h * 0.3 {
                            ends.push((k, start, nb.0 - q.0 + (q.0 - cx) * 4.0));
                        }
                    }
                }
                if ends.len() < 2 {
                    continue;
                }
                ends.sort_by(|a, b| a.2.total_cmp(&b.2));
                let xs: Vec<f32> = ends
                    .iter()
                    .map(|&(k, st, _)| {
                        at(if st {
                            edges[k][0]
                        } else {
                            edges[k][edges[k].len() - 1]
                        })
                        .0
                    })
                    .collect();
                let gap = 12.0f32.min(w * 0.8 / (ends.len() - 1) as f32);
                if xs.windows(2).all(|p| p[1] - p[0] >= gap - 0.5) {
                    continue;
                }
                let mean = xs.iter().sum::<f32>() / xs.len() as f32;
                let span = gap * (ends.len() - 1) as f32;
                // max/min, bukan clamp: galat float bisa membuat batas terbalik (panic).
                let x0 = (mean - span / 2.0)
                    .min(cx + w * 0.4 - span)
                    .max(cx - w * 0.4);
                for (i, &(k, st, _)) in ends.iter().enumerate() {
                    let nx = x0 + gap * i as f32;
                    let ny = match g.nodes[v].shape {
                        Shape::Rect => {
                            if top {
                                cy - h / 2.0
                            } else {
                                cy + h / 2.0
                            }
                        }
                        Shape::Ellipse => {
                            let t = ((nx - cx) / (w / 2.0)).clamp(-1.0, 1.0);
                            let dy = h / 2.0 * (1.0 - t * t).sqrt();
                            if top {
                                cy - dy
                            } else {
                                cy + dy
                            }
                        }
                    };
                    let np = at((nx, ny));
                    let m = edges[k].len() - 1;
                    edges[k][if st { 0 } else { m }] = np;
                }
            }
        }
    }
    let edges: Vec<Vec<Pt>> = edges
        .iter()
        .zip(&looped)
        .map(|(p, &l)| if l { p.clone() } else { smooth(p, g.lr) })
        .collect();
    let pos_out: Vec<Pt> = pos[..n].iter().map(|&p| tr(p)).collect();
    let clusters: Vec<[f32; 4]> = cbox
        .iter()
        .map(|b| if g.lr { [b[1], b[0], b[3], b[2]] } else { *b })
        .collect();
    let (w, h) = if g.lr { (h, w) } else { (w, h) };
    Out {
        pos: pos_out,
        edges,
        labels,
        clusters,
        w,
        h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(w: f32, h: f32, c: Option<usize>) -> Node {
        Node {
            w,
            h,
            shape: Shape::Rect,
            cluster: c,
        }
    }

    fn edge(a: usize, b: usize, minlen: u32) -> Edge {
        Edge {
            a,
            b,
            minlen,
            label: None,
            ordered: false,
        }
    }

    #[test]
    fn ranks_and_clusters() {
        // a → b → c, a → c, d sejajar a; b dan c di cluster 0; e di luar sejajar c.
        let g = Graph {
            nodes: vec![
                node(40.0, 20.0, None),
                node(40.0, 20.0, Some(0)),
                node(40.0, 20.0, Some(0)),
                node(30.0, 20.0, None),
                node(30.0, 20.0, None),
            ],
            edges: vec![
                edge(0, 1, 1),
                edge(1, 2, 1),
                edge(0, 2, 1),
                edge(0, 3, 0),
                edge(2, 0, 1),
                edge(1, 4, 1),
            ],
            clusters: vec![Cluster {
                parent: None,
                pad: [20.0, 10.0, 10.0, 10.0],
                min_w: 0.0,
            }],
            lr: false,
            nodesep: 35.0,
            ranksep: 60.0,
        };
        let o = layout(&g);
        assert!(o.pos[1].1 > o.pos[0].1, "{:?}", o.pos);
        assert!(o.pos[2].1 > o.pos[1].1, "{:?}", o.pos);
        assert!((o.pos[3].1 - o.pos[0].1).abs() < 1e-3, "{:?}", o.pos);
        let c = o.clusters[0];
        let inside = |p: Pt| p.0 > c[0] && p.0 < c[0] + c[2] && p.1 > c[1] && p.1 < c[1] + c[3];
        assert!(inside(o.pos[1]) && inside(o.pos[2]), "{:?} {:?}", o.pos, c);
        assert!((o.pos[4].1 - o.pos[2].1).abs() < 1e-3, "{:?}", o.pos);
        assert!(
            !inside(o.pos[4]) && !inside(o.pos[0]) && !inside(o.pos[3]),
            "{:?} {:?}",
            o.pos,
            c
        );
        assert!(o.edges.iter().all(|e| e.len() >= 2));
    }
}
