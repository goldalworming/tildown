//! Diagram railroad: `@startebnf` dan `@startregex` (paket `ebnf`/`regex`
//! PlantUML). Elemen ditata rekursif terhadap garis utama: (lebar, atas, bawah).
// ponytail: EBNF tanpa `-` (pengecualian) dan `n *`; regex tanpa lookaround,
// backreference digambar sebagai kotak biasa.

use super::*;

#[derive(Clone, Debug)]
pub enum R {
    Term(usize),
    NonTerm(usize),
    /// Kelas karakter (kotak putus-putus).
    Class(usize),
    Seq(Vec<R>),
    Alt(Vec<R>),
    Opt(Box<R>),
    /// `{x}`: nol atau lebih (jalur pintas + loop di bawah).
    Rep0(Box<R>),
    /// `x+` / `x{n,m}`: satu atau lebih (loop di atas), label opsional.
    Rep1(Box<R>, Option<usize>),
}

pub struct Rail {
    texts: Texts,
    /// (nama aturan?, diagram)
    rules: Vec<(Option<usize>, R)>,
    ebnf: bool,
}

const BOX_H: f32 = 27.609;
const CLASS_H: f32 = 17.609;

impl Rail {
    fn lw(&self) -> f32 {
        if self.ebnf {
            1.5
        } else {
            1.0
        }
    }

    /// (lebar, atas, bawah) relatif garis utama.
    fn dim(&self, r: &R, size: &[(f32, f32)]) -> (f32, f32, f32) {
        match r {
            R::Term(t) | R::NonTerm(t) => (size[*t].0 + 10.0, BOX_H / 2.0, BOX_H / 2.0),
            R::Class(t) => (size[*t].0 + 10.0, CLASS_H / 2.0, CLASS_H / 2.0),
            R::Seq(v) => {
                if v.is_empty() {
                    return (0.0, 0.0, 0.0);
                }
                let ds: Vec<_> = v.iter().map(|x| self.dim(x, size)).collect();
                (
                    ds.iter().map(|d| d.0).sum::<f32>() + 20.0 * (v.len() - 1) as f32,
                    ds.iter().map(|d| d.1).fold(0.0, f32::max),
                    ds.iter().map(|d| d.2).fold(0.0, f32::max),
                )
            }
            R::Alt(v) => {
                let ds: Vec<_> = v.iter().map(|x| self.dim(x, size)).collect();
                let w = ds.iter().map(|d| d.0).fold(0.0, f32::max) + 48.0;
                let mut below = ds[0].2;
                for d in &ds[1..] {
                    below += 10.0 + d.1 + d.2;
                }
                (w, ds[0].1, below)
            }
            R::Opt(x) => {
                let d = self.dim(x, size);
                (d.0 + 48.0, 0.0f32.max(0.0), 10.0 + d.1 + d.2)
            }
            R::Rep0(x) => {
                let d = self.dim(x, size);
                (d.0 + 78.0, 0.0, 22.0 + d.1 + d.2)
            }
            R::Rep1(x, lab) => {
                let d = self.dim(x, size);
                let lh = lab.map_or(0.0, |l| size[l].1 + 4.0);
                (d.0 + 30.0, d.1 + 7.0 + lh, d.2)
            }
        }
    }

    fn arrow(&self, p: &mut Vec<Prim>, x: f32, y: f32, dir: f32) {
        p.push(poly(
            vec![(x, y - 3.0), (x + dir * 6.0, y), (x, y + 3.0)],
            Col::Ink,
            Col::None,
            0.0,
        ));
    }

    /// Lengkung S dari (x0, y0) ke (x0 + 24, y1).
    fn s_curve(&self, p: &mut Vec<Prim>, x0: f32, y0: f32, y1: f32, dir: f32) {
        let mut pts = vec![(x0, y0)];
        let mid = (y0 + y1) / 2.0;
        let k = dir * 12.0;
        vector::bezier(
            &mut pts,
            (x0, y0),
            (x0 + k * 0.75, y0),
            (x0 + k, y0 + (mid - y0) * 0.3),
            (x0 + k, mid),
        );
        vector::bezier(
            &mut pts,
            (x0 + k, mid),
            (x0 + k, mid + (y1 - mid) * 0.7),
            (x0 + k * 1.25, y1),
            (x0 + 2.0 * k, y1),
        );
        p.push(line(pts, Col::Ink, self.lw()));
    }

    /// Sudut seperempat lingkaran (r 12) dari arah mendatar ke tegak.
    fn corner(&self, p: &mut Vec<Prim>, from: Pt, to: Pt) {
        let mut pts = vec![from];
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        // Mendatar dulu lalu tegak, atau sebaliknya: tentukan dari tanda.
        vector::bezier(
            &mut pts,
            from,
            (from.0 + dx * 0.75, from.1),
            (to.0, to.1 - dy * 0.75),
            to,
        );
        p.push(line(pts, Col::Ink, self.lw()));
    }

    fn corner_v(&self, p: &mut Vec<Prim>, from: Pt, to: Pt) {
        // Tegak dulu lalu mendatar.
        let mut pts = vec![from];
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        vector::bezier(
            &mut pts,
            from,
            (from.0, from.1 + dy * 0.75),
            (to.0 - dx * 0.75, to.1),
            to,
        );
        p.push(line(pts, Col::Ink, self.lw()));
    }

    fn draw(&self, r: &R, x: f32, y: f32, size: &[(f32, f32)], p: &mut Vec<Prim>) {
        let lw = self.lw();
        let hl = |p: &mut Vec<Prim>, a: f32, b: f32, yy: f32| {
            if b > a {
                p.push(line(vec![(a, yy), (b, yy)], Col::Ink, lw));
            }
        };
        match r {
            R::Term(t) | R::NonTerm(t) | R::Class(t) => {
                let (w, a, _) = self.dim(r, size);
                let h = 2.0 * a;
                let rr = [x, y - a, w, h];
                match r {
                    R::Term(_) if self.ebnf => p.push(rect(rr, 0.0, Col::None, Col::Ink, 0.5)),
                    R::Class(_) => p.push(Prim::Path {
                        pts: vector::rrect(rr, 0.0),
                        closed: true,
                        fill: Col::None,
                        stroke: Col::Ink,
                        w: 1.0,
                        dash: Some((5.0, 5.0)),
                    }),
                    _ => p.push(rect(rr, 5.0, Col::Box, Col::Ink, 1.5)),
                }
                p.push(Prim::Text(*t, x + 5.0, y - size[*t].1 / 2.0));
            }
            R::Seq(v) => {
                let mut cx = x;
                for (i, it) in v.iter().enumerate() {
                    if i > 0 {
                        hl(p, cx, cx + 20.0, y);
                        cx += 20.0;
                    }
                    self.draw(it, cx, y, size, p);
                    cx += self.dim(it, size).0;
                }
            }
            R::Alt(v) => {
                let (w, _, _) = self.dim(r, size);
                let inner = w - 48.0;
                let ds: Vec<_> = v.iter().map(|x| self.dim(x, size)).collect();
                let mut yy = y;
                let mut prev_d = ds[0].2;
                for (i, it) in v.iter().enumerate() {
                    if i > 0 {
                        yy += prev_d + 10.0 + ds[i].1;
                        prev_d = ds[i].2;
                    }
                    let iw = ds[i].0;
                    if i == 0 {
                        hl(p, x, x + 24.0, y);
                        self.draw(it, x + 24.0, y, size, p);
                        hl(p, x + 24.0 + iw, x + w, y);
                    } else {
                        // Turun di x+12, belok ke item; keluar naik di x+w-12.
                        self.corner_v(p, (x + 12.0, yy - 12.0), (x + 24.0, yy));
                        self.draw(it, x + 24.0, yy, size, p);
                        hl(p, x + 24.0 + iw, x + 24.0 + inner, yy);
                        self.corner(p, (x + 24.0 + inner, yy), (x + w - 12.0, yy - 12.0));
                    }
                }
                if v.len() > 1 {
                    self.corner(p, (x, y), (x + 12.0, y + 12.0));
                    self.corner_v(p, (x + w - 12.0, y + 12.0), (x + w, y));
                    p.push(line(
                        vec![(x + 12.0, y + 12.0), (x + 12.0, yy - 12.0)],
                        Col::Ink,
                        lw,
                    ));
                    p.push(line(
                        vec![(x + w - 12.0, y + 12.0), (x + w - 12.0, yy - 12.0)],
                        Col::Ink,
                        lw,
                    ));
                }
            }
            R::Opt(it) => {
                let (w, _, _) = self.dim(r, size);
                let d = self.dim(it, size);
                hl(p, x, x + w, y);
                self.arrow(p, x + w / 2.0 - 3.0, y, 1.0);
                let iy = y + 10.0 + d.1;
                self.s_curve(p, x, y, iy, 1.0);
                self.draw(it, x + 24.0, iy, size, p);
                self.s_curve(p, x + w, y, iy, -1.0);
            }
            R::Rep0(it) => {
                let (w, _, _) = self.dim(r, size);
                let d = self.dim(it, size);
                hl(p, x, x + w, y);
                self.arrow(p, x + w / 2.0 - 3.0, y, 1.0);
                let iy = y + 22.0 + d.1;
                let ly = y + 15.0;
                self.s_curve(p, x, y, iy, 1.0);
                hl(p, x + 24.0, x + 39.0, iy);
                self.draw(it, x + 39.0, iy, size, p);
                hl(p, x + 39.0 + d.0, x + w - 24.0, iy);
                self.s_curve(p, x + w, y, iy, -1.0);
                // Loop balik di atas item.
                let (lx, rx) = (x + 32.0, x + w - 32.0);
                self.corner_v(p, (lx, iy - 8.0), (lx + 8.0, iy));
                self.corner(p, (lx + 8.0, ly), (lx, ly + 8.0));
                p.push(line(vec![(lx, ly + 8.0), (lx, iy - 8.0)], Col::Ink, lw));
                p.push(line(vec![(lx + 7.0, ly), (rx - 7.0, ly)], Col::Ink, lw));
                self.arrow(p, (lx + rx) / 2.0 + 3.0, ly, -1.0);
                self.corner_v(p, (rx, iy - 8.0), (rx - 8.0, iy));
                self.corner(p, (rx - 8.0, ly), (rx, ly + 8.0));
                p.push(line(vec![(rx, ly + 8.0), (rx, iy - 8.0)], Col::Ink, lw));
            }
            R::Rep1(it, lab) => {
                let d = self.dim(it, size);
                let w = d.0 + 30.0;
                hl(p, x, x + 15.0, y);
                self.draw(it, x + 15.0, y, size, p);
                hl(p, x + 15.0 + d.0, x + w, y);
                let ly = y - d.1 - 7.0;
                let (lx, rx) = (x + 8.0, x + w - 8.0);
                self.corner(p, (lx + 8.0, ly), (lx, ly + 8.0));
                p.push(line(vec![(lx, ly + 8.0), (lx, y - 8.0)], Col::Ink, lw));
                self.corner_v(p, (lx, y - 8.0), (lx + 8.0, y));
                p.push(line(vec![(lx + 7.0, ly), (rx - 7.0, ly)], Col::Ink, lw));
                self.arrow(p, (lx + rx) / 2.0 + 3.0, ly, -1.0);
                self.corner(p, (rx - 8.0, ly), (rx, ly + 8.0));
                p.push(line(vec![(rx, ly + 8.0), (rx, y - 8.0)], Col::Ink, lw));
                self.corner_v(p, (rx, y - 8.0), (rx - 8.0, y));
                if let Some(l) = lab {
                    let (tw, th) = size[*l];
                    p.push(Prim::Text(*l, x + (w - tw) / 2.0, ly - th - 2.0));
                }
            }
        }
    }
}

impl Diagram for Rail {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        let mut y = 10.0;
        for (name, r) in &self.rules {
            if let Some(n) = name {
                p.push(Prim::Text(*n, 10.0, y));
                y += size[*n].1 + 6.0;
            }
            let (w, a, b) = self.dim(r, size);
            let my = y + a + 7.0;
            if self.ebnf {
                p.push(ellipse(
                    [10.0, my - 4.0, 8.0, 8.0],
                    Col::None,
                    Col::Ink,
                    2.0,
                ));
                p.push(line(vec![(18.0, my), (40.0, my)], Col::Ink, 1.5));
                self.draw(r, 40.0, my, size, &mut p);
                let ex = 40.0 + w;
                p.push(line(vec![(ex, my), (ex + 26.0, my)], Col::Ink, 1.5));
                self.arrow(&mut p, ex + 11.0, my, 1.0);
                p.push(ellipse(
                    [ex + 26.0, my - 4.0, 8.0, 8.0],
                    Col::Ink,
                    Col::Ink,
                    1.0,
                ));
            } else {
                self.draw(r, 15.0, my, size, &mut p);
            }
            y = my + b + 25.0;
        }
        Placed {
            prims: p,
            w: 0.0,
            h: 0.0,
        }
        .fit(size, 5.0)
    }
}

// --- EBNF ---

struct Tok<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tok<'_> {
    fn ws(&mut self) {
        loop {
            while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
                self.i += 1;
            }
            // Komentar (* ... *)
            if self.s[self.i..].starts_with(b"(*") {
                match self.s[self.i + 2..].windows(2).position(|w| w == b"*)") {
                    Some(k) => self.i += k + 4,
                    None => self.i = self.s.len(),
                }
                continue;
            }
            break;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
}

fn ebnf_alt(t: &mut Tok, texts: &mut Texts) -> Option<R> {
    let mut alts = vec![ebnf_seq(t, texts)?];
    while t.peek() == Some(b'|') {
        t.i += 1;
        alts.push(ebnf_seq(t, texts)?);
    }
    Some(if alts.len() == 1 {
        alts.pop().unwrap()
    } else {
        R::Alt(alts)
    })
}

fn ebnf_seq(t: &mut Tok, texts: &mut Texts) -> Option<R> {
    let mut v = Vec::new();
    loop {
        match t.peek() {
            Some(b',') => t.i += 1,
            Some(b'|' | b';' | b'.' | b')' | b']' | b'}') | None => break,
            _ => v.push(ebnf_term(t, texts)?),
        }
    }
    Some(if v.len() == 1 {
        v.pop().unwrap()
    } else {
        R::Seq(v)
    })
}

fn ebnf_term(t: &mut Tok, texts: &mut Texts) -> Option<R> {
    let c = t.peek()?;
    t.i += 1;
    match c {
        b'"' | b'\'' => {
            let st = t.i;
            while t.i < t.s.len() && t.s[t.i] != c {
                t.i += 1;
            }
            let s = std::str::from_utf8(&t.s[st..t.i]).ok()?.to_string();
            t.i += 1;
            Some(R::Term(texts.add(Txt::new(&s, 14.0))))
        }
        b'?' => {
            let st = t.i;
            while t.i < t.s.len() && t.s[t.i] != b'?' {
                t.i += 1;
            }
            let s = std::str::from_utf8(&t.s[st..t.i]).ok()?.trim().to_string();
            t.i += 1;
            Some(R::NonTerm(texts.add(Txt::new(&s, 14.0).italic())))
        }
        b'[' | b'{' | b'(' => {
            let inner = ebnf_alt(t, texts)?;
            let close = match c {
                b'[' => b']',
                b'{' => b'}',
                _ => b')',
            };
            if t.peek() != Some(close) {
                return None;
            }
            t.i += 1;
            Some(match c {
                b'[' => R::Opt(Box::new(inner)),
                b'{' => R::Rep0(Box::new(inner)),
                _ => inner,
            })
        }
        _ => {
            let st = t.i - 1;
            while t.i < t.s.len()
                && ((t.s[t.i] as char).is_alphanumeric() || b"_- ".contains(&t.s[t.i]))
            {
                t.i += 1;
            }
            let s = std::str::from_utf8(&t.s[st..t.i]).ok()?.trim().to_string();
            if s.is_empty() {
                return None;
            }
            Some(R::NonTerm(texts.add(Txt::new(&s, 14.0))))
        }
    }
}

pub fn parse_ebnf(lines: &[String]) -> Option<Rail> {
    let src = lines.join("\n");
    let mut texts = Texts::default();
    let mut rules = Vec::new();
    let mut t = Tok {
        s: src.as_bytes(),
        i: 0,
    };
    while t.peek().is_some() {
        let st = t.i;
        while t.i < t.s.len() && t.s[t.i] != b'=' {
            t.i += 1;
        }
        let name = std::str::from_utf8(&t.s[st..t.i]).ok()?.trim().to_string();
        if t.i >= t.s.len() || name.is_empty() {
            return None;
        }
        t.i += 1;
        let r = ebnf_alt(&mut t, &mut texts)?;
        if matches!(t.peek(), Some(b';' | b'.')) {
            t.i += 1;
        }
        let n = texts.add(Txt::new(&name, 14.0).bold());
        rules.push((Some(n), r));
    }
    (!rules.is_empty()).then_some(Rail {
        texts,
        rules,
        ebnf: true,
    })
}

// --- regex ---

pub fn parse_regex(lines: &[String]) -> Option<Rail> {
    let mut texts = Texts::default();
    let mut rules = Vec::new();
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let c: Vec<char> = l.chars().collect();
        let mut i = 0;
        let r = re_alt(&c, &mut i, &mut texts)?;
        if i < c.len() {
            return None;
        }
        rules.push((None, r));
    }
    (!rules.is_empty()).then_some(Rail {
        texts,
        rules,
        ebnf: false,
    })
}

fn re_alt(c: &[char], i: &mut usize, tx: &mut Texts) -> Option<R> {
    let mut alts = vec![re_seq(c, i, tx)?];
    while c.get(*i) == Some(&'|') {
        *i += 1;
        alts.push(re_seq(c, i, tx)?);
    }
    Some(if alts.len() == 1 {
        alts.pop().unwrap()
    } else {
        R::Alt(alts)
    })
}

fn re_seq(c: &[char], i: &mut usize, tx: &mut Texts) -> Option<R> {
    let mut v: Vec<R> = Vec::new();
    let mut lit = String::new();
    let flush = |lit: &mut String, v: &mut Vec<R>, tx: &mut Texts| {
        if !lit.is_empty() {
            v.push(R::Term(tx.add(Txt::new(lit, 14.0))));
            lit.clear();
        }
    };
    while *i < c.len() && c[*i] != '|' && c[*i] != ')' {
        let ch = c[*i];
        *i += 1;
        let atom = match ch {
            '(' => {
                if c.get(*i) == Some(&'?') {
                    *i += 2; // (?: (?= dsb.
                }
                let inner = re_alt(c, i, tx)?;
                if c.get(*i) != Some(&')') {
                    return None;
                }
                *i += 1;
                Some(inner)
            }
            '[' => {
                let st = *i;
                while *i < c.len() && c[*i] != ']' {
                    if c[*i] == '\\' {
                        *i += 1;
                    }
                    *i += 1;
                }
                let s: String = c[st..(*i).min(c.len())].iter().collect();
                *i += 1;
                Some(R::Class(tx.add(Txt::new(&s, 14.0))))
            }
            '\\' => {
                let e = *c.get(*i)?;
                *i += 1;
                Some(R::Term(tx.add(Txt::new(&format!("\\{e}"), 14.0))))
            }
            '^' | '$' | '.' => Some(R::Term(tx.add(Txt::new(&ch.to_string(), 14.0)))),
            _ => {
                // Literal: digabung, kecuali karakter terakhir diberi kuantor.
                let quant_next = matches!(c.get(*i), Some('*' | '+' | '?' | '{'));
                if quant_next {
                    flush(&mut lit, &mut v, tx);
                    Some(R::Term(tx.add(Txt::new(&ch.to_string(), 14.0))))
                } else {
                    lit.push(ch);
                    None
                }
            }
        };
        let Some(mut atom) = atom else { continue };
        flush(&mut lit, &mut v, tx);
        // Kuantor.
        match c.get(*i) {
            Some('*') => {
                *i += 1;
                atom = R::Rep0(Box::new(atom));
            }
            Some('+') => {
                *i += 1;
                atom = R::Rep1(Box::new(atom), None);
            }
            Some('?') => {
                *i += 1;
                atom = R::Opt(Box::new(atom));
            }
            Some('{') => {
                let st = *i;
                while *i < c.len() && c[*i] != '}' {
                    *i += 1;
                }
                let q: String = c[st..=(*i).min(c.len() - 1)].iter().collect();
                *i += 1;
                let lab = tx.add(Txt::new(&q, 12.0));
                let zero = q.starts_with("{0");
                atom = R::Rep1(Box::new(atom), Some(lab));
                if zero {
                    atom = R::Opt(Box::new(atom));
                }
            }
            _ => {}
        }
        if c.get(*i) == Some(&'?') {
            *i += 1; // malas (lazy): diabaikan
        }
        v.push(atom);
    }
    flush(&mut lit, &mut v, tx);
    Some(if v.len() == 1 {
        v.pop().unwrap()
    } else {
        R::Seq(v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ebnf_and_regex() {
        let l = |s: &str| s.lines().map(String::from).collect::<Vec<_>>();
        let e = parse_ebnf(&l(
            "angka = \"0\" | \"1\";\nbil = angka , { angka } ;\nopsi = [ \"-\" ] , bil ;",
        ))
        .unwrap();
        assert_eq!(e.rules.len(), 3);
        assert!(matches!(e.rules[0].1, R::Alt(ref v) if v.len() == 2));
        let r = parse_regex(&l("^[a-z]+(\\d{2,3})?$")).unwrap();
        assert!(matches!(r.rules[0].1, R::Seq(ref v) if v.len() == 4));
        let pl = e.place(&vec![(20.0, 16.0); e.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
