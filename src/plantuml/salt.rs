//! Salt (`@startsalt`) — wireframe UI berbasis grid (`salt` PlantUML): teks,
//! input `"..."`, tombol `[..]`, radio `()`, centang `[X]`, droplist `^..^`,
//! pemisah `== .. ~~ --`, grid `{+ {# {! {-`, grup `{^"judul"`, tab `{/`.
// ponytail: tanpa tree `{T`, menu `{*`, scroll `{S`, sprite & ikon `<&x>`
// (ikon dibuang), sel rentang `*`.

use super::*;

#[derive(Debug)]
enum E {
    Text(usize),
    /// (teks, jumlah karakter mentah)
    Input(usize, usize),
    Button(usize, usize),
    Radio(bool, usize),
    Check(bool, usize),
    Drop(usize, usize),
    Sep(char),
    Empty,
    Grid(Grid),
}

#[derive(Debug)]
struct Grid {
    /// ' ' tanpa garis, '+' bingkai luar, '#' semua garis, '!' garis tegak, '-' garis mendatar.
    mode: char,
    rows: Vec<Vec<E>>,
    /// `{^"judul"`
    title: Option<usize>,
    tabs: bool,
}

pub struct Salt {
    texts: Texts,
    root: Grid,
}

fn cell(raw: &str, tx: &mut Texts) -> E {
    let s = raw.trim();
    let t = |tx: &mut Texts, x: &str| tx.add(Txt::new(&strip_icons(x), 12.0));
    if s.is_empty() || s == "." {
        return E::Empty;
    }
    if s.len() >= 2 && s.chars().all(|c| c == '=') {
        return E::Sep('=');
    }
    if s.len() >= 2
        && (s.chars().all(|c| c == '.')
            || s.chars().all(|c| c == '~')
            || s.chars().all(|c| c == '-'))
    {
        return E::Sep(s.chars().next().unwrap());
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let raw = &s[1..s.len() - 1];
        return E::Input(tx.add(Txt::new(&literal(raw.trim()), 12.0)), char_count(raw));
    }
    if let Some(r) = s.strip_prefix("[X]").or(s.strip_prefix("[x]")) {
        return E::Check(true, t(tx, r.trim()));
    }
    if let Some(r) = s.strip_prefix("[ ]").or(s.strip_prefix("[]")) {
        return E::Check(false, t(tx, r.trim()));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let raw = &s[1..s.len() - 1];
        return E::Button(tx.add(Txt::new(&strip_icons(raw.trim()), 12.0)), char_count(raw));
    }
    if let Some(r) = s.strip_prefix("(X)").or(s.strip_prefix("(x)")) {
        return E::Radio(true, t(tx, r.trim()));
    }
    if let Some(r) = s.strip_prefix("( )").or(s.strip_prefix("()")) {
        return E::Radio(false, t(tx, r.trim()));
    }
    if s.starts_with('^') && s.ends_with('^') && s.len() >= 2 {
        let raw = s[1..s.len() - 1].split('^').next().unwrap_or("");
        return E::Drop(t(tx, raw.trim()), char_count(raw));
    }
    E::Text(t(tx, s))
}

fn strip_icons(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(k) = rest.find("<&") {
        out.push_str(&rest[..k]);
        match rest[k..].find('>') {
            Some(e) => rest = &rest[k + e + 1..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    unescape(&out)
}

/// Pisah sel dengan `|` di luar `"..."`/`[..]`.
fn split_cells(l: &str) -> Vec<&str> {
    let mut v = Vec::new();
    let b = l.as_bytes();
    let (mut q, mut st) = (false, 0);
    for i in 0..b.len() {
        match b[i] {
            b'"' => q = !q,
            b'|' if !q => {
                v.push(&l[st..i]);
                st = i + 1;
            }
            _ => {}
        }
    }
    v.push(&l[st..]);
    v
}

fn parse_grid(ls: &[&str], i: &mut usize, head: &str, tx: &mut Texts) -> Option<Grid> {
    // head = isi setelah `{`: mode + judul/tab pada baris yang sama.
    let h = head.trim();
    let (mode, rest) = match h.chars().next() {
        Some(c @ ('+' | '#' | '!' | '-' | '^' | '/' | '*' | 'T' | 'S')) => {
            (c, h[c.len_utf8()..].trim())
        }
        _ => (' ', h),
    };
    if matches!(mode, '*' | 'T' | 'S') {
        return None;
    }
    let mut g = Grid {
        mode: if mode == '^' || mode == '/' {
            ' '
        } else {
            mode
        },
        rows: Vec::new(),
        title: None,
        tabs: mode == '/',
    };
    let mut first_rest = rest.to_string();
    if mode == '^' {
        // {^"Judul"
        if let (Some(a), Some(b)) = (rest.find('"'), rest.rfind('"')) {
            if a < b {
                g.title = Some(tx.add(Txt::new(&rest[a + 1..b], 12.0)));
                first_rest = rest[b + 1..].to_string();
            }
        }
    }
    // Tab sebaris: `{/ a | b }`
    if !first_rest.trim().is_empty() {
        let r = first_rest.trim();
        if let Some(inner) = r.strip_suffix('}') {
            g.rows.push(
                split_cells(inner)
                    .into_iter()
                    .map(|c| cell(c, tx))
                    .collect(),
            );
            return Some(g);
        }
        g.rows
            .push(split_cells(r).into_iter().map(|c| cell(c, tx)).collect());
    }
    while *i < ls.len() {
        let l = ls[*i].trim();
        *i += 1;
        if l == "}" {
            return Some(g);
        }
        if l.is_empty() {
            continue;
        }
        if let Some(h2) = l.strip_prefix('{') {
            let sub = parse_grid(ls, i, h2, tx)?;
            g.rows.push(vec![E::Grid(sub)]);
            continue;
        }
        g.rows
            .push(split_cells(l).into_iter().map(|c| cell(c, tx)).collect());
    }
    Some(g)
}

pub fn parse_lines(lines: &[String]) -> Option<Salt> {
    let ls: Vec<&str> = lines
        .iter()
        .map(|l| l.as_str())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let mut tx = Texts::default();
    let mut i;
    let first = ls.first()?.trim();
    let root = if let Some(h) = first.strip_prefix('{') {
        i = 1;
        parse_grid(&ls, &mut i, h, &mut tx)?
    } else {
        // Tanpa kurung: satu kolom.
        let rows = ls
            .iter()
            .map(|l| {
                split_cells(l)
                    .into_iter()
                    .map(|c| cell(c, &mut tx))
                    .collect()
            })
            .collect();
        i = ls.len();
        Grid {
            mode: ' ',
            rows,
            title: None,
            tabs: false,
        }
    };
    (i >= ls.len()).then_some(Salt { texts: tx, root })
}

/// Jumlah karakter (`AbstractElementText.getCharNumber`): ikon `<&x>` = 2,
/// tag dibuang, spasi dihitung. Lebar minimal teks = 8 px per karakter.
fn char_count(s: &str) -> usize {
    let mut n = 0;
    let mut rest = s;
    while let Some(k) = rest.find('<') {
        n += rest[..k].chars().count();
        let Some(e) = rest[k..].find('>') else {
            rest = &rest[k..];
            break;
        };
        if rest[k..].starts_with("<&") {
            n += 2;
        }
        rest = &rest[k + e + 1..];
    }
    n + rest.chars().count()
}

impl Salt {
    /// Tinggi blok teks PlantUML (≈ 1,2578 em per baris).
    fn ph(&self, t: usize) -> f32 {
        let tx = &self.texts.0[t];
        tx.size * 1.2578 * tx.s.split('\n').count() as f32
    }

    /// Lebar teks dengan minimal 8 px per karakter (input, tombol, droplist).
    fn tw_min(&self, t: usize, n: usize, size: &[(f32, f32)]) -> f32 {
        size[t].0.max(n as f32 * 8.0)
    }

    /// `getPreferredDimension` tiap elemen PlantUML.
    fn size_of(&self, e: &E, size: &[(f32, f32)]) -> (f32, f32) {
        match e {
            E::Text(t) => (size[*t].0, self.ph(*t)),
            E::Input(t, n) => (self.tw_min(*t, *n, size) + 6.0, self.ph(*t) + 2.0),
            E::Button(t, n) => (self.tw_min(*t, *n, size) + 9.0, self.ph(*t) + 9.0),
            E::Radio(_, t) | E::Check(_, t) => (size[*t].0 + 20.0, self.ph(*t)),
            E::Drop(t, n) => (self.tw_min(*t, *n, size) + 16.0, self.ph(*t) + 4.0),
            E::Sep(_) => (10.0, 6.0),
            E::Empty => (1.0, 1.0),
            E::Grid(g) if g.tabs => {
                // ElementTabBar: tiap tab lebar teks + 2 + 3 + 10.
                let cells = g.rows.iter().flatten();
                let w = cells.clone().map(|c| self.size_of(c, size).0 + 15.0).sum();
                let h = cells.map(|c| self.size_of(c, size).1).fold(0.0, f32::max);
                (w, h)
            }
            E::Grid(g) => self.grid_size(g, size),
        }
    }

    fn title_h(&self, g: &Grid) -> f32 {
        g.title.map_or(0.0, |t| self.ph(t))
    }

    /// ElementPyramid.init: awal kolom & baris (lebar/tinggi elemen + 2).
    fn grid_dims(&self, g: &Grid, size: &[(f32, f32)]) -> (Vec<f32>, Vec<f32>) {
        let ncol = g.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let th = self.title_h(g);
        let mut cw = vec![0.0f32; ncol];
        let mut rh = vec![0.0f32; g.rows.len()];
        for (ri, r) in g.rows.iter().enumerate() {
            for (ci, c) in r.iter().enumerate() {
                let (w, h) = self.size_of(c, size);
                // Pemisah sendirian di barisnya tidak melebarkan kolom.
                if !(matches!(c, E::Sep(_)) && r.len() == 1) {
                    cw[ci] = cw[ci].max(w + 2.0);
                }
                let sup = if ri == 0 { th / 2.0 } else { 0.0 };
                rh[ri] = rh[ri].max(h + sup + 2.0);
            }
        }
        let mut cs = vec![0.0];
        cw.iter().for_each(|w| cs.push(cs.last().unwrap() + w));
        let mut rs = vec![th / 2.0];
        rh.iter().for_each(|h| rs.push(rs.last().unwrap() + h));
        (cs, rs)
    }

    fn grid_size(&self, g: &Grid, size: &[(f32, f32)]) -> (f32, f32) {
        let (cs, rs) = self.grid_dims(g, size);
        (*cs.last().unwrap(), rs.last().unwrap() + self.title_h(g))
    }

    /// Gambar elemen di (x, y) dengan ruang `w`×`h` (dimToUse).
    fn draw(&self, e: &E, x: f32, y: f32, w: f32, h: f32, size: &[(f32, f32)], p: &mut Vec<Prim>) {
        let ink = Col::Text;
        let ee = Col::Rgb(0xEEEEEE);
        match e {
            E::Text(t) => p.push(Prim::Text(*t, x, y)),
            E::Input(t, n) => {
                let (tw, th) = (self.tw_min(*t, *n, size), self.ph(*t));
                p.push(Prim::Text(*t, x + 3.0, y));
                p.push(line(vec![(x + 1.0, y + th), (x + tw + 4.0, y + th)], ink, 1.0));
                for lx in [x + 1.0, x + tw + 4.0] {
                    p.push(line(vec![(lx, y + th - 3.0), (lx, y + th - 1.0)], ink, 1.0));
                }
            }
            E::Button(t, _) => {
                let (bw, bh) = self.size_of(e, size);
                p.push(rect([x + 2.5, y + 2.5, bw - 5.0, bh - 5.0], 5.0, ee, ink, 2.5));
                p.push(Prim::Text(*t, x + (bw - size[*t].0) / 2.0, y + 4.5));
            }
            E::Radio(on, t) => {
                let th = self.ph(*t);
                p.push(ellipse([x + 2.0, y + (th - 10.0) / 2.0, 10.0, 10.0], Col::None, ink, 1.5));
                if *on {
                    p.push(ellipse([x + 5.0, y + (th - 4.0) / 2.0, 4.0, 4.0], ink, ink, 1.5));
                }
                p.push(Prim::Text(*t, x + 20.0, y));
            }
            E::Check(on, t) => {
                let th = self.ph(*t);
                p.push(rect([x + 2.0, y + (th - 10.0) / 2.0, 10.0, 10.0], 0.0, Col::None, ink, 1.5));
                if *on {
                    let (cx, cy) = (x + 3.0, y + 6.0);
                    let pts = [(0.0, 0.0), (3.0, 3.0), (10.0, -6.0), (3.0, 1.0)];
                    p.push(poly(pts.iter().map(|q| (cx + q.0, cy + q.1)).collect(), ink, ink, 1.5));
                }
                p.push(Prim::Text(*t, x + 20.0, y));
            }
            E::Drop(t, _) => {
                let (dw, dh) = self.size_of(e, size);
                p.push(rect([x, y, dw - 1.0, dh - 1.0], 0.0, ee, ink, 1.0));
                p.push(Prim::Text(*t, x + 2.0, y + 2.0));
                let lx = x + dw - 12.0;
                p.push(line(vec![(lx, y), (lx, y + dh - 1.0)], ink, 1.0));
                let th = self.ph(*t);
                p.push(poly(
                    vec![(lx + 3.0, y + 6.0), (lx + 9.0, y + 6.0), (lx + 6.0, y + th - 2.0)],
                    ink,
                    ink,
                    1.0,
                ));
            }
            E::Sep(c) => {
                let gray = Col::Rgb(0xAAAAAA);
                let my = y + h / 2.0;
                match c {
                    '=' => {
                        p.push(line(vec![(x, my - 1.0), (x + w, my - 1.0)], gray, 1.0));
                        p.push(line(vec![(x, my + 1.0), (x + w, my + 1.0)], gray, 1.0));
                    }
                    '.' => p.push(dashed(vec![(x, my), (x + w, my)], gray, 1.0, (1.0, 2.0))),
                    '-' => p.push(line(vec![(x, my), (x + w, my)], gray, 1.0)),
                    _ => p.push(line(vec![(x, my), (x + w, my)], gray, 1.5)),
                }
            }
            E::Empty => {}
            E::Grid(g) if g.tabs => {
                let mut x1 = x;
                for c in g.rows.iter().flatten() {
                    let (tw, th) = self.size_of(c, size);
                    self.draw(c, x1 + 2.0, y, tw, th, size, p);
                    p.push(line(
                        vec![(x1, y + th), (x1, y), (x1 + tw + 5.0, y), (x1 + tw + 5.0, y + th)],
                        ink,
                        1.0,
                    ));
                    p.push(line(vec![(x1 + tw + 5.0, y + th), (x1 + tw + 15.0, y + th)], ink, 1.0));
                    x1 += tw + 15.0;
                }
            }
            E::Grid(g) => self.draw_grid(g, x, y, size, p),
        }
    }

    /// ElementPyramid.drawU + Grid.drawU.
    fn draw_grid(&self, g: &Grid, x: f32, y: f32, size: &[(f32, f32)], p: &mut Vec<Prim>) {
        let (cs, rs) = self.grid_dims(g, size);
        let th = self.title_h(g);
        let ink = Col::Text;
        for (ri, r) in g.rows.iter().enumerate() {
            let sup = if ri == 0 { th / 2.0 } else { 0.0 };
            for (ci, c) in r.iter().enumerate() {
                // Pemisah sendirian di barisnya: selebar seluruh grid.
                let x2 = if matches!(c, E::Sep(_)) && r.len() == 1 {
                    *cs.last().unwrap()
                } else {
                    cs[ci + 1]
                };
                let (w, h) = (x2 - cs[ci] - 1.0, rs[ri + 1] - rs[ri] - 1.0);
                self.draw(c, x + cs[ci] + 1.0, y + rs[ri] + sup + 1.0, w, h, size, p);
            }
        }
        let (x0, x1) = (x, x + cs.last().unwrap());
        let (y0, y1) = (y + rs[0], y + rs.last().unwrap());
        if matches!(g.mode, '+' | '#') || g.title.is_some() {
            p.push(rect([x0, y0, x1 - x0, y1 - y0], 0.0, Col::None, ink, 1.0));
        }
        if matches!(g.mode, '#' | '!') {
            for cx in &cs {
                p.push(line(vec![(x + cx, y0), (x + cx, y1)], ink, 1.0));
            }
        }
        if matches!(g.mode, '#' | '-') {
            for ry in &rs {
                p.push(line(vec![(x0, y + ry), (x1, y + ry)], ink, 1.0));
            }
        }
        if let Some(t) = g.title {
            p.push(rect([x + 6.0, y, size[t].0, th], 0.0, Col::Paper, Col::None, 0.0));
            p.push(Prim::Text(t, x + 6.0, y));
        }
    }
}

impl Diagram for Salt {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        // Margin dokumen 5; ukuran PNG PlantUML dibulatkan ke bawah.
        self.draw_grid(&self.root, 5.0, 5.0, size, &mut p);
        let (w, h) = self.grid_size(&self.root, size);
        Placed {
            prims: p,
            w: (w + 10.0).floor(),
            h: (h + 10.0).floor(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salt_parse() {
        let l: Vec<String> = "{+\n Login | \"budi  \"\n [Batal] | [OK]\n () a | [X] b\n ^Pilih^ | ==\n {^\"Grup\"\n  [ ] x\n }\n}".lines().map(String::from).collect();
        let s = parse_lines(&l).unwrap();
        assert_eq!(s.root.mode, '+');
        assert_eq!(s.root.rows.len(), 5);
        assert!(matches!(s.root.rows[1][0], E::Button(..)));
        assert!(matches!(s.root.rows[2][1], E::Check(true, _)));
        assert!(matches!(s.root.rows[3][0], E::Drop(..)));
        assert!(matches!(&s.root.rows[4][0], E::Grid(g) if g.title.is_some()));
        let pl = s.place(&vec![(30.0, 15.0); s.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
