//! `@startditaa` — ASCII art → gambar (gaya ditaa): kotak `+--+` terisi dengan
//! bayangan (sudut `/ \` membulat), garis `- | + = :`, panah `< > ^ v`, teks,
//! tag warna `cRED cBLU cGRE cPNK cBLK cYEL c123`.
// ponytail: hanya kotak persegi (bukan poligon bebas); tag bentuk {d} {s} {io}
// {c} {o} {tr} mengubah bentuk kotak itu.

use super::*;

const CW: f32 = 10.0;
const CH: f32 = 14.0;

pub struct Ditaa {
    texts: Texts,
    grid: Vec<Vec<char>>,
    /// (x0, y0, x1, y1) sel sudut, warna, bulat?, tag bentuk
    boxes: Vec<(usize, usize, usize, usize, Col, bool, String)>,
    /// (teks, kolom, baris, jumlah sel)
    labels: Vec<(usize, usize, usize, usize)>,
}

fn is_h(c: char) -> bool {
    matches!(c, '-' | '=' | '+' | '/' | '\\')
}

fn is_v(c: char) -> bool {
    matches!(c, '|' | ':' | '+' | '/' | '\\')
}

fn corner(c: char) -> bool {
    matches!(c, '+' | '/' | '\\')
}

pub fn parse_lines(lines: &[String]) -> Option<Ditaa> {
    let grid: Vec<Vec<char>> = lines
        .iter()
        .map(|l| l.trim_end().chars().collect())
        .collect();
    if grid.iter().all(|r| r.is_empty()) {
        return None;
    }
    let at = |x: usize, y: usize| grid.get(y).and_then(|r| r.get(x)).copied().unwrap_or(' ');
    let mut d = Ditaa {
        texts: Texts::default(),
        grid: grid.clone(),
        boxes: Vec::new(),
        labels: Vec::new(),
    };
    // Kotak: dari tiap sudut kiri-atas, cari sudut kanan-atas lalu kanan-bawah yang menutup.
    let h = grid.len();
    for y0 in 0..h {
        for x0 in 0..grid[y0].len() {
            if !corner(at(x0, y0)) || !is_h(at(x0 + 1, y0)) || !is_v(at(x0, y0 + 1)) {
                continue;
            }
            let mut x1 = x0 + 1;
            while is_h(at(x1, y0)) && !corner(at(x1, y0)) {
                x1 += 1;
            }
            // Cari sudut kanan-atas yang turun.
            let mut found = None;
            let mut xx = x1;
            while is_h(at(xx, y0)) {
                if corner(at(xx, y0)) && is_v(at(xx, y0 + 1)) {
                    // Turun lewat sambungan `+` sampai sudut yang menutup kotak.
                    let mut y1 = y0 + 1;
                    while is_v(at(xx, y1)) {
                        if corner(at(xx, y1))
                            && corner(at(x0, y1))
                            && (x0 + 1..xx).all(|k| is_h(at(k, y1)))
                            && (y0 + 1..y1).all(|k| is_v(at(x0, k)))
                        {
                            found = Some((xx, y1));
                            break;
                        }
                        y1 += 1;
                    }
                    if found.is_some() {
                        break;
                    }
                }
                xx += 1;
            }
            if let Some((x1, y1)) = found {
                // Kotak terkecil saja (abaikan bila memuat kotak lain persis sama).
                let mut col = Col::Paper;
                for yy in y0 + 1..y1 {
                    let row: String = (x0 + 1..x1).map(|k| at(k, yy)).collect();
                    if let Some(k) = row.find('c') {
                        let tag: String = row[k + 1..].chars().take(3).collect();
                        let c = match tag.as_str() {
                            "RED" => Some(Col::Rgb(0xE32D2D)),
                            "BLU" => Some(Col::Rgb(0x555BFF)),
                            "GRE" => Some(Col::Rgb(0x30A830)),
                            "PNK" => Some(Col::Rgb(0xFFAAAA)),
                            "BLK" => Some(Col::Rgb(0x000000)),
                            "YEL" => Some(Col::Rgb(0xFFFF55)),
                            t if t.len() == 3 && t.chars().all(|c| c.is_ascii_hexdigit()) => {
                                Col::parse(t)
                            }
                            _ => None,
                        };
                        if let Some(c) = c {
                            col = c;
                        }
                    }
                }
                let round = at(x0, y0) == '/' || at(x1, y0) == '\\';
                let mut tag = String::new();
                for yy in y0 + 1..y1 {
                    let row: String = (x0 + 1..x1).map(|k| at(k, yy)).collect();
                    if let (Some(a), Some(b)) = (row.find('{'), row.find('}')) {
                        if a < b {
                            tag = row[a + 1..b].to_string();
                        }
                    }
                }
                d.boxes.push((x0, y0, x1, y1, col, round, tag));
            }
        }
    }
    // Teks: karakter yang bukan bagian garis/kotak, dikelompokkan per baris.
    let mut used = vec![vec![false; 0]; h];
    for y in 0..h {
        used[y] = vec![false; grid[y].len()];
    }
    for y in 0..h {
        let row = &grid[y];
        let mut x = 0;
        while x < row.len() {
            let c = row[x];
            let line_char =
                matches!(c, '-' | '|' | '+' | '=' | ':' | '/' | '\\') && line_like(&grid, x, y);
            let arrow = matches!(c, '<' | '>' | '^' | 'v') && arrow_like(&grid, x, y);
            if c == ' ' || line_char || arrow {
                x += 1;
                continue;
            }
            // Kumpulkan sampai dua spasi atau karakter garis.
            let st = x;
            let mut s = String::new();
            while x < row.len() {
                let c = row[x];
                if c == ' ' && row.get(x + 1).is_none_or(|&n| n == ' ') {
                    break;
                }
                if matches!(c, '|' | '+') && line_like(&grid, x, y) {
                    break;
                }
                s.push(c);
                x += 1;
            }
            let s = s.trim_end();
            // Tag warna disembunyikan.
            let mut shown: String = if s.len() == 4 && s.starts_with('c') {
                String::new()
            } else {
                s.to_string()
            };
            if let (Some(a), Some(b)) = (shown.find('{'), shown.find('}')) {
                if a < b {
                    shown = format!("{}{}", &shown[..a], &shown[b + 1..])
                        .trim()
                        .to_string();
                }
            }
            if !shown.is_empty() {
                let t = d.texts.add(Txt::new(&literal(&shown), 15.0).bold());
                // Rentang sel asli (termasuk tag yang disembunyikan): teks dipusatkan di situ.
                d.labels.push((t, st, y, s.chars().count()));
            }
        }
    }
    Some(d)
}

fn cell(g: &[Vec<char>], x: isize, y: isize) -> char {
    if x < 0 || y < 0 {
        return ' ';
    }
    g.get(y as usize)
        .and_then(|r| r.get(x as usize))
        .copied()
        .unwrap_or(' ')
}

/// Karakter garis yang bersambung dengan karakter garis/panah tetangga.
fn line_like(g: &[Vec<char>], x: usize, y: usize) -> bool {
    let (x, y) = (x as isize, y as isize);
    let c = cell(g, x, y);
    let hn = |ch: char| matches!(ch, '-' | '=' | '+' | '<' | '>' | '/' | '\\');
    let vn = |ch: char| matches!(ch, '|' | ':' | '+' | '^' | 'v' | '/' | '\\');
    match c {
        '-' | '=' => hn(cell(g, x - 1, y)) || hn(cell(g, x + 1, y)),
        '|' | ':' => vn(cell(g, x, y - 1)) || vn(cell(g, x, y + 1)),
        '+' | '/' | '\\' => {
            hn(cell(g, x - 1, y))
                || hn(cell(g, x + 1, y))
                || vn(cell(g, x, y - 1))
                || vn(cell(g, x, y + 1))
        }
        _ => false,
    }
}

fn arrow_like(g: &[Vec<char>], x: usize, y: usize) -> bool {
    let (x, y) = (x as isize, y as isize);
    match cell(g, x, y) {
        '>' => matches!(cell(g, x - 1, y), '-' | '='),
        '<' => matches!(cell(g, x + 1, y), '-' | '='),
        '^' => matches!(cell(g, x, y + 1), '|' | ':'),
        'v' => matches!(cell(g, x, y - 1), '|' | ':'),
        _ => false,
    }
}

impl Diagram for Ditaa {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        let g = &self.grid;
        // Margin ditaa: 2 sel di tiap sisi; garis di tengah sel.
        let (mx, my) = (2.0 * CW, 2.0 * CH);
        let cx = |x: usize| mx + x as f32 * CW + CW / 2.0;
        let cy = |y: usize| my + y as f32 * CH + CH / 2.0;
        // Tepi kotak yang berimpit dengan tepi kotak lain digeser 2 px ke dalam.
        let rects: Vec<[f32; 4]> = self
            .boxes
            .iter()
            .map(|&(x0, y0, x1, y1, ..)| {
                let mut r = [cx(x0), cy(y0), cx(x1), cy(y1)];
                for &(a0, b0, a1, b1, ..) in &self.boxes {
                    let xo = a0 < x1 && x0 < a1;
                    let yo = b0 < y1 && y0 < b1;
                    if xo && b1 == y0 {
                        r[1] += 2.0;
                    }
                    if xo && b0 == y1 {
                        r[3] -= 2.0;
                    }
                    if yo && a1 == x0 {
                        r[0] += 2.0;
                    }
                    if yo && a0 == x1 {
                        r[2] -= 2.0;
                    }
                }
                r
            })
            .collect();
        let shapes: Vec<Vec<Pt>> = self
            .boxes
            .iter()
            .zip(&rects)
            .map(|((.., round, tag), &[x, y, x1, y1])| {
                let (w, h) = (x1 - x, y1 - y);
                match tag.as_str() {
                    "d" => {
                        // Dokumen: tepi bawah gelombang S (turun lalu naik ±4 px).
                        let mut v = vec![(x + w, y + h), (x + w, y), (x, y), (x, y + h)];
                        vector::bezier(
                            &mut v,
                            (x, y + h),
                            (x + w / 3.0, y + h + 13.5),
                            (x + w * 2.0 / 3.0, y + h - 13.5),
                            (x + w, y + h),
                        );
                        v
                    }
                    "io" => vec![(x + 8.0, y), (x + w, y), (x + w - 8.0, y + h), (x, y + h)],
                    "c" => vec![
                        (x + w / 2.0, y),
                        (x + w, y + h / 2.0),
                        (x + w / 2.0, y + h),
                        (x, y + h / 2.0),
                    ],
                    "tr" => vec![(x, y), (x + w, y), (x + w - 8.0, y + h), (x + 8.0, y + h)],
                    "o" => vector::ellipse([x, y, w, h]),
                    "s" => {
                        let mut v = vec![(x, y + 6.0)];
                        vector::bezier(&mut v, (x, y + 6.0), (x, y - 2.0), (x + w, y - 2.0), (x + w, y + 6.0));
                        v.push((x + w, y + h - 6.0));
                        vector::bezier(&mut v, (x + w, y + h - 6.0), (x + w, y + h + 2.0), (x, y + h + 2.0), (x, y + h - 6.0));
                        v
                    }
                    _ => vector::rrect([x, y, w, h], if *round { 6.0 } else { 0.0 }),
                }
            })
            .collect();
        // Bayangan lembut: 5 lapis bergeser 5..1 px, makin dekat makin gelap.
        for k in (1..=5).rev() {
            let v = (167.0 + (k - 1) as f32 * 17.5) as u32;
            let c = Col::Rgb(v << 16 | v << 8 | v);
            for s in &shapes {
                let d = k as f32;
                p.push(poly(s.iter().map(|q| (q.0 + d, q.1 + d)).collect(), c, Col::None, 0.0));
            }
        }
        for (s, b) in shapes.iter().zip(&self.boxes) {
            p.push(poly(s.clone(), b.4, Col::Text, 1.0));
        }
        // Sel di keliling kotak: setengah-segmen di sepanjang keliling tidak digambar.
        let on_perim_h = |x: usize, y: usize, nx: isize| {
            self.boxes.iter().any(|&(x0, y0, x1, y1, ..)| {
                (y == y0 || y == y1) && x >= x0 && x <= x1 && nx >= x0 as isize && nx <= x1 as isize
            })
        };
        let on_perim_v = |x: usize, y: usize, ny: isize| {
            self.boxes.iter().any(|&(x0, y0, x1, y1, ..)| {
                (x == x0 || x == x1) && y >= y0 && y <= y1 && ny >= y0 as isize && ny <= y1 as isize
            })
        };
        let perim = |x: usize, y: usize| {
            self.boxes.iter().any(|&(x0, y0, x1, y1, ..)| {
                ((y == y0 || y == y1) && x >= x0 && x <= x1) || ((x == x0 || x == x1) && y >= y0 && y <= y1)
            })
        };
        let hn = |ch: char| matches!(ch, '-' | '=' | '+' | '<' | '>' | '/' | '\\');
        let vn = |ch: char| matches!(ch, '|' | ':' | '+' | '^' | 'v' | '/' | '\\');
        let is_line = |x: usize, y: usize| {
            let c = cell(g, x as isize, y as isize);
            (matches!(c, '-' | '|' | '+' | '=' | ':' | '/' | '\\') && line_like(g, x, y))
                || (matches!(c, '<' | '>' | '^' | 'v') && arrow_like(g, x, y))
        };
        // Komponen garis (di luar keliling kotak): putus-putus bila memuat `:` / `=`.
        let hgt = g.len();
        let mut comp = vec![vec![usize::MAX; 0]; hgt];
        for y in 0..hgt {
            comp[y] = vec![usize::MAX; g[y].len()];
        }
        let mut dashed_c = Vec::new();
        for y in 0..hgt {
            for x in 0..g[y].len() {
                if comp[y][x] != usize::MAX || !is_line(x, y) || perim(x, y) {
                    continue;
                }
                let id = dashed_c.len();
                let mut dash = false;
                let mut st = vec![(x, y)];
                comp[y][x] = id;
                while let Some((a, b)) = st.pop() {
                    let c = g[b][a];
                    dash |= matches!(c, ':' | '=');
                    let mut nb = Vec::new();
                    if hn(c) {
                        nb.push((a as isize - 1, b as isize));
                        nb.push((a as isize + 1, b as isize));
                    }
                    if vn(c) {
                        nb.push((a as isize, b as isize - 1));
                        nb.push((a as isize, b as isize + 1));
                    }
                    for (na, nbb) in nb {
                        if na < 0 || nbb < 0 {
                            continue;
                        }
                        let (na, nbb) = (na as usize, nbb as usize);
                        if nbb < hgt
                            && na < g[nbb].len()
                            && comp[nbb][na] == usize::MAX
                            && is_line(na, nbb)
                            && !perim(na, nbb)
                        {
                            comp[nbb][na] = id;
                            st.push((na, nbb));
                        }
                    }
                }
                dashed_c.push(dash);
            }
        }
        let dashed_at = |x: usize, y: usize, nx: isize, ny: isize| {
            let c = comp.get(y).and_then(|r| r.get(x)).copied().unwrap_or(usize::MAX);
            let c = if c == usize::MAX && nx >= 0 && ny >= 0 {
                comp.get(ny as usize).and_then(|r| r.get(nx as usize)).copied().unwrap_or(usize::MAX)
            } else {
                c
            };
            dashed_c.get(c).copied().unwrap_or(false)
        };
        // Setengah-segmen garis, lalu digabung jadi ruas lurus (pola putus tak terpotong).
        let mut segs: Vec<(Pt, Pt, bool)> = Vec::new();
        let mut heads = Vec::new();
        for y in 0..hgt {
            for x in 0..g[y].len() {
                if !is_line(x, y) {
                    continue;
                }
                let c = g[y][x];
                let (px, py) = (cx(x), cy(y));
                let (xi, yi) = (x as isize, y as isize);
                if matches!(c, '<' | '>' | '^' | 'v') {
                    // Kepala panah memenuhi selnya (alas di sisi sel, ujung di sisi seberang).
                    let (l, r, t, b) = (px - CW / 2.0, px + CW / 2.0, py - CH / 2.0, py + CH / 2.0);
                    let tri = match c {
                        '>' => vec![(l, t), (r, py), (l, b)],
                        '<' => vec![(r, t), (l, py), (r, b)],
                        '^' => vec![(l, b), (px, t), (r, b)],
                        _ => vec![(l, t), (px, b), (r, t)],
                    };
                    heads.push(poly(tri, Col::Text, Col::None, 0.0));
                    continue;
                }
                for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                    let n = cell(g, xi + dx, yi + dy);
                    let ok = if dy == 0 {
                        hn(c) && hn(n) && !on_perim_h(x, y, xi + dx)
                    } else {
                        vn(c) && vn(n) && !on_perim_v(x, y, yi + dy)
                    };
                    if ok {
                        let e = (px + dx as f32 * CW / 2.0, py + dy as f32 * CH / 2.0);
                        segs.push(((px, py), e, dashed_at(x, y, xi + dx, yi + dy)));
                    }
                }
            }
        }
        // Normalisasi arah lalu gabung yang segaris & bersambung.
        for s in segs.iter_mut() {
            if (s.1 .0, s.1 .1) < (s.0 .0, s.0 .1) {
                std::mem::swap(&mut s.0, &mut s.1);
            }
        }
        // Kunci urut: (mendatar?, garis tetap, awal) — ruas segaris jadi bersebelahan.
        let key = |s: &(Pt, Pt, bool)| {
            if s.0 .1 == s.1 .1 {
                (0, s.0 .1, s.0 .0)
            } else {
                (1, s.0 .0, s.0 .1)
            }
        };
        segs.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap());
        let mut runs: Vec<(Pt, Pt, bool)> = Vec::new();
        for s in segs {
            if let Some(l) = runs.last_mut() {
                let (kl, ks) = (key(l), key(&s));
                if kl.0 == ks.0 && kl.1 == ks.1 && l.2 == s.2 {
                    let (le, ss) = if kl.0 == 0 { (l.1 .0, s.0 .0) } else { (l.1 .1, s.0 .1) };
                    if ss <= le + 0.01 {
                        // Bersambung / tumpang tindih: perpanjang.
                        let longer = if kl.0 == 0 { s.1 .0 > l.1 .0 } else { s.1 .1 > l.1 .1 };
                        if longer {
                            l.1 = s.1;
                        }
                        continue;
                    }
                }
            }
            runs.push(s);
        }
        for (a, b, d) in runs {
            p.push(if d {
                dashed(vec![a, b], Col::Text, 1.0, (6.0, 4.0))
            } else {
                line(vec![a, b], Col::Text, 1.0)
            });
        }
        p.extend(heads);
        for &(t, x, y, n) in &self.labels {
            // Dipusatkan pada sel-sel teks aslinya; garis dasar 3 px di atas tepi bawah sel.
            let x = mx + x as f32 * CW + (n as f32 * CW - size[t].0) / 2.0;
            p.push(Prim::Text(t, x, my + y as f32 * CH - 3.0));
        }
        let w = g.iter().map(|r| r.len()).max().unwrap_or(0) as f32 * CW + 2.0 * mx;
        let h = g.len() as f32 * CH + 2.0 * my;
        Placed { prims: p, w, h }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ditaa_boxes() {
        let l: Vec<String> =
            "+--------+   +-------+\n| Satu   |-->| Dua   |\n|  cRED  |   +-------+\n+--------+"
                .lines()
                .map(String::from)
                .collect();
        let d = parse_lines(&l).unwrap();
        assert_eq!(d.boxes.len(), 2);
        assert_eq!(d.boxes[0].4, Col::Rgb(0xE32D2D));
        assert!(d.labels.iter().any(|l| d.texts.0[l.0].s.contains("Satu")));
        let pl = d.place(&vec![(30.0, 14.0); d.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
