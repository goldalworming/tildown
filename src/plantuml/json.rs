//! `@startjson` / `@startyaml` — tabel bersarang seperti `jsondiagram` PlantUML:
//! kotak sudut 5, baris 21,6, kunci tebal, anak di kanan disambung panah putus.
// ponytail: YAML subset (peta, daftar, skalar, inline [..]/{..}, kutip);
// tanpa anchor/alias, blok `|`/`>`; `#highlight` diabaikan.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

// --- JSON ---

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }

    fn val(&mut self) -> Option<J> {
        self.ws();
        match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        return Some(J::Obj(v));
                    }
                    let k = match self.val()? {
                        J::Str(k) => k,
                        _ => return None,
                    };
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    let x = self.val()?;
                    v.push((k, x));
                    self.ws();
                    if self.s.get(self.i) == Some(&b',') {
                        self.i += 1;
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b']') {
                        self.i += 1;
                        return Some(J::Arr(v));
                    }
                    v.push(self.val()?);
                    self.ws();
                    if self.s.get(self.i) == Some(&b',') {
                        self.i += 1;
                    }
                }
            }
            b'"' => {
                self.i += 1;
                let mut out = Vec::new();
                while let Some(&c) = self.s.get(self.i) {
                    self.i += 1;
                    match c {
                        b'"' => return Some(J::Str(String::from_utf8_lossy(&out).into_owned())),
                        b'\\' => {
                            let e = *self.s.get(self.i)?;
                            self.i += 1;
                            out.push(match e {
                                b'n' => b'\n',
                                b't' => b'\t',
                                _ => e,
                            });
                        }
                        _ => out.push(c),
                    }
                }
                None
            }
            _ => {
                let st = self.i;
                while self.i < self.s.len()
                    && !b",]}".contains(&self.s[self.i])
                    && !(self.s[self.i] as char).is_whitespace()
                {
                    self.i += 1;
                }
                let w = std::str::from_utf8(&self.s[st..self.i]).ok()?;
                Some(match w {
                    "true" => J::Bool(true),
                    "false" => J::Bool(false),
                    "null" => J::Null,
                    "" => return None,
                    _ => J::Num(w.to_string()),
                })
            }
        }
    }
}

pub fn parse_json(src: &str) -> Option<J> {
    let mut p = P {
        s: src.as_bytes(),
        i: 0,
    };
    let v = p.val()?;
    p.ws();
    (p.i >= p.s.len()).then_some(v)
}

// --- YAML (subset) ---

fn scalar(s: &str) -> J {
    let s = s.trim();
    if (s.starts_with('[') && s.ends_with(']')) || (s.starts_with('{') && s.ends_with('}')) {
        if let Some(v) = parse_json(&yaml_inline_to_json(s)) {
            return v;
        }
    }
    let unq = s.trim_matches('"').trim_matches('\'');
    match s {
        "true" | "True" | "yes" => J::Bool(true),
        "false" | "False" | "no" => J::Bool(false),
        "null" | "~" | "" => J::Null,
        _ if s.parse::<f64>().is_ok() => J::Num(s.to_string()),
        _ => J::Str(unq.to_string()),
    }
}

/// `[a, b]` / `{k: v}` YAML → JSON (kutip ditambahkan).
fn yaml_inline_to_json(s: &str) -> String {
    let mut out = String::new();
    let mut tok = String::new();
    let flush = |tok: &mut String, out: &mut String| {
        let t = tok.trim();
        if !t.is_empty() {
            if t.starts_with('"')
                || t.parse::<f64>().is_ok()
                || matches!(t, "true" | "false" | "null")
            {
                out.push_str(t);
            } else {
                out.push('"');
                out.push_str(t.trim_matches('\''));
                out.push('"');
            }
        }
        tok.clear();
    };
    for c in s.chars() {
        match c {
            '[' | ']' | '{' | '}' | ',' => {
                flush(&mut tok, &mut out);
                out.push(c);
            }
            ':' => {
                flush(&mut tok, &mut out);
                out.push(':');
            }
            _ => tok.push(c),
        }
    }
    flush(&mut tok, &mut out);
    out
}

pub fn parse_yaml(lines: &[String]) -> Option<J> {
    let ls: Vec<(usize, &str)> = lines
        .iter()
        .map(|l| (l.len() - l.trim_start().len(), l.trim_end()))
        .filter(|(_, l)| {
            !l.trim().is_empty() && !l.trim_start().starts_with('#') && l.trim() != "---"
        })
        .map(|(ind, l)| (ind, l.trim_start()))
        .collect();
    let mut i = 0;
    let v = yaml_block(&ls, &mut i, 0)?;
    (i >= ls.len()).then_some(v)
}

fn yaml_block(ls: &[(usize, &str)], i: &mut usize, ind: usize) -> Option<J> {
    let (first_ind, first) = *ls.get(*i)?;
    if first_ind < ind {
        return Some(J::Null);
    }
    let ind = first_ind;
    if first.starts_with("- ") || first == "-" {
        let mut arr = Vec::new();
        while *i < ls.len() && ls[*i].0 == ind && (ls[*i].1.starts_with("- ") || ls[*i].1 == "-") {
            let item = ls[*i].1[1..].trim();
            *i += 1;
            if item.is_empty() {
                arr.push(yaml_block(ls, i, ind + 1)?);
            } else if let Some((k, v)) = split_kv(item) {
                // `- k: v` lalu kunci lain pada indentasi lebih dalam.
                let mut obj = vec![(
                    k.to_string(),
                    if v.is_empty() {
                        yaml_block(ls, i, ind + 1)?
                    } else {
                        scalar(v)
                    },
                )];
                if *i < ls.len() && ls[*i].0 > ind {
                    if let J::Obj(more) = yaml_block(ls, i, ind + 1)? {
                        obj.extend(more);
                    }
                }
                arr.push(J::Obj(obj));
            } else {
                arr.push(scalar(item));
            }
        }
        return Some(J::Arr(arr));
    }
    let mut obj = Vec::new();
    while *i < ls.len() && ls[*i].0 == ind {
        let (k, v) = split_kv(ls[*i].1)?;
        *i += 1;
        let val = if v.is_empty() {
            if *i < ls.len() && ls[*i].0 > ind {
                yaml_block(ls, i, ind + 1)?
            } else if *i < ls.len() && ls[*i].0 == ind && ls[*i].1.starts_with('-') {
                yaml_block(ls, i, ind)?
            } else {
                J::Null
            }
        } else {
            scalar(v)
        };
        obj.push((k.to_string(), val));
    }
    Some(J::Obj(obj))
}

fn split_kv(s: &str) -> Option<(&str, &str)> {
    // Titik dua pertama di luar kutip, diikuti spasi/akhir.
    let b = s.as_bytes();
    let mut q = None;
    for k in 0..b.len() {
        match (b[k], q) {
            (b'"' | b'\'', None) => q = Some(b[k]),
            (c, Some(o)) if c == o => q = None,
            (b':', None) if k + 1 == b.len() || b[k + 1] == b' ' => {
                return Some((
                    s[..k].trim().trim_matches('"').trim_matches('\''),
                    s[k + 1..].trim(),
                ));
            }
            _ => {}
        }
    }
    None
}

// --- tata letak ---

/// Satu tabel: baris (kunci?, nilai, anak?).
struct Table {
    /// (kunci?, nilai, anak?, ikon: 0 tidak ada, 1 centang, 2 kotak kosong, 3 null)
    rows: Vec<(Option<usize>, usize, Option<usize>, u8)>,
}

pub struct Json {
    texts: Texts,
    tables: Vec<Table>,
}

pub fn build(v: &J) -> Json {
    let mut d = Json {
        texts: Texts::default(),
        tables: Vec::new(),
    };
    add_table(&mut d, v);
    d
}

fn add_table(d: &mut Json, v: &J) -> usize {
    let k = d.tables.len();
    d.tables.push(Table { rows: Vec::new() });
    let items: Vec<(Option<String>, &J)> = match v {
        J::Obj(o) => o.iter().map(|(k, v)| (Some(k.clone()), v)).collect(),
        J::Arr(a) => a.iter().map(|v| (None, v)).collect(),
        other => vec![(None, other)],
    };
    for (key, val) in items {
        let kt = key.map(|k| d.texts.add(Txt::new(&k, 14.0).bold()));
        let (s, child, icon) = match val {
            J::Obj(_) | J::Arr(_) => ("   ".to_string(), Some(add_table(d, val)), 0),
            J::Null => (String::new(), None, 3),
            J::Bool(true) => ("true".to_string(), None, 1),
            J::Bool(false) => ("false".to_string(), None, 2),
            J::Num(n) => (n.clone(), None, 0),
            J::Str(s) => (s.clone(), None, 0),
        };
        let vt = d.texts.add(Txt::new(&s, 14.0));
        d.tables[k].rows.push((kt, vt, child, icon));
    }
    k
}

const RH: f32 = 21.609;

impl Json {
    /// (lebar kolom kunci, lebar tabel, tinggi tabel).
    fn dim(&self, t: usize, size: &[(f32, f32)]) -> (f32, f32, f32) {
        let tb = &self.tables[t];
        let kw = tb
            .rows
            .iter()
            .filter_map(|r| r.0)
            .map(|k| size[k].0 + 10.0)
            .fold(0.0f32, f32::max);
        let vw = tb
            .rows
            .iter()
            .map(|r| {
                size[r.1].0
                    + 10.0
                    + if r.2.is_some() { 12.0 } else { 0.0 }
                    + if r.3 > 0 { 14.0 } else { 0.0 }
            })
            .fold(0.0f32, f32::max);
        (kw, kw + vw.max(20.0), (tb.rows.len().max(1) as f32) * RH)
    }

    /// Tinggi subpohon (tabel + anak bertumpuk berjarak 18).
    fn tree_h(&self, t: usize, size: &[(f32, f32)]) -> f32 {
        let own = self.dim(t, size).2;
        let kids: Vec<usize> = self.tables[t].rows.iter().filter_map(|r| r.2).collect();
        if kids.is_empty() {
            return own;
        }
        let kh: f32 = kids.iter().map(|&c| self.tree_h(c, size)).sum::<f32>()
            + 18.0 * (kids.len() - 1) as f32;
        own.max(kh)
    }

    /// Gambar tabel `t` + anaknya; hasil = sudut kanan-bawah terjauh.
    fn draw(&self, t: usize, x: f32, y: f32, size: &[(f32, f32)], p: &mut Vec<Prim>) -> (f32, f32) {
        let (kw, w, h) = self.dim(t, size);
        p.push(rect([x, y, w, h], 5.0, Col::Box, Col::None, 0.0));
        let rows = &self.tables[t].rows;
        let mut dots = Vec::new();
        for (i, &(k, v, c, icon)) in rows.iter().enumerate() {
            let ry = y + i as f32 * RH;
            if i > 0 {
                p.push(line(vec![(x, ry), (x + w, ry)], Col::Text, 1.0));
            }
            if let Some(k) = k {
                p.push(Prim::Text(k, x + 5.0, ry + (RH - size[k].1) / 2.0));
                p.push(line(vec![(x + kw, ry), (x + kw, ry + RH)], Col::Text, 1.0));
            }
            let ix = x + kw + 5.0;
            let iy = ry + RH / 2.0;
            match icon {
                1 | 2 => {
                    p.push(rect(
                        [ix, iy - 5.0, 10.0, 10.0],
                        1.0,
                        Col::None,
                        Col::Text,
                        1.0,
                    ));
                    if icon == 1 {
                        p.push(line(
                            vec![(ix + 2.0, iy), (ix + 4.5, iy + 3.0), (ix + 8.5, iy - 3.5)],
                            Col::Text,
                            1.3,
                        ));
                    }
                }
                3 => {
                    p.push(line(
                        vec![(ix + 1.0, iy + 5.0), (ix + 9.0, iy - 5.0)],
                        Col::Text,
                        1.0,
                    ));
                    p.push(line(
                        vec![
                            (ix, iy - 5.0),
                            (ix, iy - 1.0),
                            (ix + 3.0, iy - 5.0),
                            (ix + 3.0, iy - 1.0),
                        ],
                        Col::Text,
                        0.8,
                    ));
                    p.push(line(
                        vec![
                            (ix + 6.0, iy + 1.0),
                            (ix + 6.0, iy + 5.0),
                            (ix + 9.0, iy + 5.0),
                        ],
                        Col::Text,
                        0.8,
                    ));
                }
                _ => {}
            }
            let tx = if icon > 0 { ix + 14.0 } else { ix };
            p.push(Prim::Text(v, tx, ry + (RH - size[v].1) / 2.0));
            if let Some(c) = c {
                dots.push((x + w - 11.85, ry + RH / 2.0, c));
            }
        }
        p.push(rect([x, y, w, h], 5.0, Col::None, Col::Text, 1.5));
        let mut ext = (x + w, y + h);
        if dots.is_empty() {
            return ext;
        }
        // Anak bertumpuk, blok dipusatkan pada rerata baris induknya.
        let hs: Vec<f32> = dots.iter().map(|d| self.tree_h(d.2, size)).collect();
        let total: f32 = hs.iter().sum::<f32>() + 18.0 * (dots.len() - 1) as f32;
        let cy = dots.iter().map(|d| d.1).sum::<f32>() / dots.len() as f32;
        // ponytail: blok anak tak naik melewati tepi atas induk (PlantUML menggeser kanvas).
        let mut yy = (cy - total / 2.0).max(y);
        // Kolom anak 37 px di kanan; tiap tabel dipusatkan di kolomnya.
        let colw = dots.iter().map(|d| self.dim(d.2, size).1).fold(0.0, f32::max);
        for (i, &(dx, dy, c)) in dots.iter().enumerate() {
            let (_, cw, ch) = self.dim(c, size);
            let cx = x + w + 37.0 + (colw - cw) / 2.0;
            let top = yy + (hs[i] - ch) / 2.0;
            // ponytail: ujung panah smetana didekati 40% dari titik ke pusat tabel.
            let tgt = (cx, dy + (top + ch / 2.0 - dy) * 0.4);
            p.push(ellipse(
                [dx - 3.0, dy - 3.0, 6.0, 6.0],
                Col::Text,
                Col::Text,
                1.0,
            ));
            let mut pts = vec![(dx, dy), (dx + 13.0, dy)];
            vector::bezier(
                &mut pts,
                (dx + 13.0, dy),
                (dx + 25.0, dy),
                (tgt.0 - 15.0, tgt.1),
                (tgt.0 - 8.0, tgt.1),
            );
            p.push(dashed(pts, Col::Text, 1.0, (3.0, 3.0)));
            p.push(poly(
                vec![
                    (tgt.0, tgt.1),
                    (tgt.0 - 8.5, tgt.1 - 4.0),
                    (tgt.0 - 6.0, tgt.1),
                    (tgt.0 - 8.5, tgt.1 + 4.0),
                ],
                Col::Text,
                Col::None,
                0.0,
            ));
            let e = self.draw(c, cx, top, size, p);
            ext = (ext.0.max(e.0), ext.1.max(e.1));
            yy += hs[i] + 18.0;
        }
        ext
    }
}

impl Diagram for Json {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        // Margin 10; ukuran PNG PlantUML dibulatkan ke bawah.
        let (w, h) = self.draw(0, 10.0, 10.0, size, &mut p);
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
    fn json_yaml() {
        let j = parse_json(r#"{"a": 1, "b": {"c": true}, "d": [1, "x"], "e": null}"#).unwrap();
        assert!(matches!(&j, J::Obj(o) if o.len() == 4));
        let y = parse_yaml(
            &"a: 1\nb:\n  c: true\nd:\n  - 1\n  - x\ne: [1, 2]"
                .lines()
                .map(String::from)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let J::Obj(o) = &y else { panic!() };
        assert_eq!(o[0], ("a".into(), J::Num("1".into())));
        assert!(matches!(&o[2].1, J::Arr(a) if a.len() == 2));
        assert!(matches!(&o[3].1, J::Arr(a) if a.len() == 2));
        let d = build(&j);
        let pl = d.place(&vec![(30.0, 16.0); d.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
