//! `@startnwdiag` — jaringan sebagai batang mendatar, node sebagai kotak di
//! bawah jaringan pertamanya, garis tegak + alamat ke tiap jaringan (`nwdiag`).
// ponytail: tanpa `group`, bentuk node khusus, `inet`; warna jaringan didukung.

use super::*;

struct Net {
    name: usize,
    addr: Option<usize>,
    color: Option<Col>,
    /// (node, alamat?)
    members: Vec<(usize, Option<usize>)>,
}

struct NNode {
    id: String,
    label: usize,
    color: Option<Col>,
}

pub struct NwDiag {
    texts: Texts,
    nets: Vec<Net>,
    nodes: Vec<NNode>,
}

/// `[address = "x", color = "#red", description = "y"]` → peta atribut.
fn attrs(s: &str) -> Vec<(String, String)> {
    let Some(a) = s.find('[') else {
        return Vec::new();
    };
    let b = s.rfind(']').unwrap_or(s.len());
    s[a + 1..b]
        .split(',')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((
                k.trim().to_lowercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

pub fn parse_lines(lines: &[String]) -> Option<NwDiag> {
    let mut d = NwDiag {
        texts: Texts::default(),
        nets: Vec::new(),
        nodes: Vec::new(),
    };
    let mut cur: Option<usize> = None;
    let mut depth = 0;
    for raw in lines {
        let l = raw.trim().trim_end_matches(';').trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        if low.starts_with("nwdiag") {
            depth += 1;
            continue;
        }
        if low.starts_with("network") {
            let name = l[7..].trim().trim_end_matches('{').trim();
            let t = d.texts.add(Txt::new(&unescape(name), 12.0));
            d.nets.push(Net {
                name: t,
                addr: None,
                color: None,
                members: Vec::new(),
            });
            cur = Some(d.nets.len() - 1);
            continue;
        }
        if l == "}" {
            if cur.take().is_none() {
                depth -= 1;
            }
            continue;
        }
        if low.starts_with("group") || low.starts_with("inet") {
            return None;
        }
        let Some(n) = cur else {
            // Node di luar jaringan (definisi): `web01 [description = ".."]`
            let id = l.split(['[', ' ']).next()?.trim().to_string();
            node(&mut d, &id, &attrs(l));
            continue;
        };
        if let Some((k, v)) = l.split_once('=').filter(|_| !l.contains('[')) {
            match k.trim().to_lowercase().as_str() {
                "address" => {
                    d.nets[n].addr = Some(d.texts.add(Txt::new(v.trim().trim_matches('"'), 12.0)))
                }
                "color" => d.nets[n].color = Col::parse(v.trim().trim_matches('"')),
                _ => {}
            }
            continue;
        }
        let id = l.split(['[', ' ']).next()?.trim().to_string();
        let at = attrs(l);
        let k = node(&mut d, &id, &at);
        let addr = at
            .iter()
            .find(|a| a.0 == "address")
            .map(|a| d.texts.add(Txt::new(&a.1, 11.0)));
        d.nets[n].members.push((k, addr));
    }
    let _ = depth;
    (!d.nets.is_empty()).then_some(d)
}

fn node(d: &mut NwDiag, id: &str, at: &[(String, String)]) -> usize {
    let k = match d.nodes.iter().position(|x| x.id == id) {
        Some(k) => k,
        None => {
            let t = d.texts.add(Txt::new(id, 12.0));
            d.nodes.push(NNode {
                id: id.to_string(),
                label: t,
                color: None,
            });
            d.nodes.len() - 1
        }
    };
    for (a, v) in at {
        match a.as_str() {
            "description" => d.texts.0[d.nodes[k].label] = Txt::new(&unescape(v), 12.0),
            "color" => d.nodes[k].color = Col::parse(v),
            _ => {}
        }
    }
    k
}

impl Diagram for NwDiag {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        // Tinggi blok teks PlantUML (1,2578 em per baris).
        let ph = |t: usize| {
            let x = &self.texts.0[t];
            x.size * 1.2578 * x.s.split('\n').count() as f32
        };
        // Baris grid = jaringan pertama tiap node; kolom = urutan di baris itu.
        let home: Vec<usize> = (0..self.nodes.len())
            .map(|k| {
                self.nets
                    .iter()
                    .position(|n| n.members.iter().any(|m| m.0 == k))
                    .unwrap_or(0)
            })
            .collect();
        let mut col = vec![0usize; self.nodes.len()];
        let mut ncol = vec![0usize; self.nets.len()];
        for k in 0..self.nodes.len() {
            col[k] = ncol[home[k]];
            ncol[home[k]] += 1;
        }
        let link = |k: usize, ni: usize| {
            self.nets
                .get(ni)
                .and_then(|n| n.members.iter().find(|m| m.0 == k))
                .and_then(|m| m.1)
        };
        let dim = |t: Option<usize>| t.map_or((0.0, 0.0), |t| (size[t].0, ph(t)));
        // NServerDraw.naturalDimension.
        let mut cw = vec![0.0f32; ncol.iter().copied().max().unwrap_or(0)];
        let mut rh = vec![50.0f32; self.nets.len()];
        for k in 0..self.nodes.len() {
            let (l1, l2) = (dim(link(k, home[k])), dim(link(k, home[k] + 1)));
            let t = self.nodes[k].label;
            let w = (l1.0 + 20.0).max(size[t].0 + 50.0).max(l2.0 + 20.0);
            let h = l1.1 + l2.1 + 70.0 + ph(t) + 20.0;
            cw[col[k]] = cw[col[k]].max(w);
            rh[home[k]] = rh[home[k]].max(h);
        }
        let mut cs = vec![0.0f32];
        cw.iter().for_each(|w| cs.push(cs.last().unwrap() + w));
        // Label jaringan (nama + alamat) rata kanan; bar di tengah blok label.
        let desc_h = |n: &Net| ph(n.name) + n.addr.map_or(0.0, ph);
        let lab_w = self
            .nets
            .iter()
            .map(|n| size[n.name].0.max(n.addr.map_or(0.0, |a| size[a].0)))
            .fold(0.0, f32::max);
        let x0 = 5.0 + lab_w + 5.0;
        let mut by = vec![5.0 + (desc_h(&self.nets[0]) - 5.0) / 2.0];
        rh.iter().for_each(|h| by.push(by.last().unwrap() + h));
        let md = |ni: usize| if ni % 2 == 0 { 2.0 } else { -2.0 };
        for (ni, net) in self.nets.iter().enumerate() {
            let cols = net.members.iter().map(|m| col[m.0]);
            let (c0, c1) = (cols.clone().min().unwrap_or(0), cols.max().unwrap_or(0));
            let (bx0, bx1) = (x0 + cs[c0], x0 + cs[(c1 + 1).min(cs.len() - 1)].max(cs[c0] + 60.0));
            p.push(rect(
                [bx0, by[ni], bx1 - bx0, 5.0],
                0.0,
                net.color.unwrap_or(Col::Part),
                Col::Ink,
                1.0,
            ));
            let top = by[ni] + 2.5 - desc_h(net) / 2.0;
            p.push(Prim::Text(net.name, x0 - 5.0 - size[net.name].0, top));
            if let Some(a) = net.addr {
                p.push(Prim::Text(a, x0 - 5.0 - size[a].0, top + ph(net.name)));
            }
        }
        let center = |p: &mut Vec<Prim>, t: usize, x: f32, y: f32| {
            p.push(Prim::Text(t, x - size[t].0 / 2.0, y - ph(t) / 2.0));
        };
        for (k, n) in self.nodes.iter().enumerate() {
            let (r, c) = (home[k], col[k]);
            let t = n.label;
            let (bw, bh) = (size[t].0 + 20.0, ph(t) + 20.0);
            let cx = x0 + (cs[c] + cs[c + 1]) / 2.0;
            let alpha = rh[r] / 2.0 - bh / 2.0;
            let y0 = by[r];
            // Tautan ke jaringan utama, lalu ke jaringan lain (selang 9 px).
            let lx = cx + md(r);
            p.push(line(vec![(lx, y0 + 5.0), (lx, y0 + alpha)], Col::Ink, 1.0));
            if let Some(a) = link(k, r) {
                center(&mut p, a, lx, y0 + alpha / 2.0);
            }
            let others: Vec<usize> = (0..self.nets.len())
                .filter(|&o| o != r && self.nets[o].members.iter().any(|m| m.0 == k))
                .collect();
            let mut x = cx - (others.len() as f32 - 1.0) * 4.5;
            for (i, &o) in others.iter().enumerate() {
                let lx = x - md(o);
                p.push(line(vec![(lx, y0 + alpha + bh), (lx, by[o])], Col::Ink, 1.0));
                if let Some(a) = link(k, o) {
                    let dx = if i == 0 && others.len() > 1 { size[a].0 / 2.0 } else { 0.0 };
                    center(&mut p, a, lx - dx, by[o] - alpha / 2.0);
                }
                x += 9.0;
            }
            let b = [cx - bw / 2.0, y0 + alpha, bw, bh];
            p.push(rect(b, 0.0, n.color.unwrap_or(Col::Box), Col::Ink, 0.5));
            p.push(Prim::Text(t, b[0] + 10.0, b[1] + 10.0));
        }
        // Ukuran PNG PlantUML: margin 5, dibulatkan ke atas + 1.
        Placed {
            prims: p,
            w: (x0 + cs.last().unwrap() + 5.0).ceil() + 1.0,
            h: (by.last().unwrap() + 5.0).ceil() + 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nwdiag_parse() {
        let l: Vec<String> = "network dmz {\n address = \"1.x\"\n web01 [address = \"1.1\"];\n}\nnetwork lan {\n web01;\n db;\n}".lines().map(String::from).collect();
        let d = parse_lines(&l).unwrap();
        assert_eq!(d.nets.len(), 2);
        assert_eq!(d.nodes.len(), 2);
        assert!(d.nets[0].addr.is_some());
        let pl = d.place(&vec![(30.0, 14.0); d.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
