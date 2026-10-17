//! Activity diagram sintaks lama (`(*) --> "A"`, `if "c" then ... endif`,
//! `===B===`) — di PlantUML ditata Graphviz; di sini lewat [`super::graph`].
// ponytail: tanpa `partition`, note; `(*top)` diperlakukan seperti `(*)`.

use super::graph::{self, Graph};
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum K {
    Start,
    End,
    Act,
    Branch,
    Bar,
}

struct Node {
    id: String,
    t: Option<usize>,
    k: K,
}

struct Edge {
    a: usize,
    b: usize,
    label: Option<usize>,
    dir: char,
    len: u32,
    dashed: bool,
}

pub struct OldAct {
    texts: Texts,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

pub fn parse_lines(lines: &[String]) -> Option<OldAct> {
    let mut d = OldAct {
        texts: Texts::default(),
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let mut last: Vec<usize> = Vec::new();
    // Tumpukan if: (belah ketupat, ujung cabang selesai)
    let mut ifs: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut saw_start = false;
    for raw in lines {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        if low.starts_with("if ") {
            // if "cond" then
            let cond = l[3..].trim();
            let cond = cond.strip_suffix("then").unwrap_or(cond).trim();
            let t = d.texts.add(Txt::new(&unescape(cond), 11.0));
            let id = format!("__if{}", d.nodes.len());
            d.nodes.push(Node {
                id,
                t: Some(t),
                k: K::Branch,
            });
            let b = d.nodes.len() - 1;
            for &s in &last {
                d.edges.push(Edge {
                    a: s,
                    b,
                    label: None,
                    dir: ' ',
                    len: 1,
                    dashed: false,
                });
            }
            ifs.push((b, Vec::new()));
            last = vec![b];
            continue;
        }
        if low == "else" || low.starts_with("else ") {
            let (b, ends_v) = ifs.last_mut()?;
            ends_v.extend(last.iter().copied().filter(|&x| d.nodes[x].k != K::End));
            last = vec![*b];
            continue;
        }
        if low == "endif" || low == "end if" {
            let (_, mut ends_v) = ifs.pop()?;
            ends_v.extend(last.iter().copied().filter(|&x| d.nodes[x].k != K::End));
            last = ends_v;
            continue;
        }
        // Panah: [sumber] -->[label] tujuan
        let Some(k) = l.find(['-', '.']).filter(|&k| l[k..].contains('>')) else {
            return None;
        };
        let gt = k + l[k..].find('>')?;
        let src = l[..k].trim();
        let arrow = &l[k..gt + 1];
        let mut rest = l[gt + 1..].trim();
        let mut label = None;
        if let Some(r) = rest.strip_prefix('[') {
            let e = r.find(']')?;
            label = Some(d.texts.add(Txt::new(&unescape(&r[..e]), 11.0)));
            rest = r[e + 1..].trim();
        }
        // `-->[x]` dengan label di antara badan dan tujuan juga: `-[#red]->` dsb.
        let alow = arrow.to_lowercase();
        let dir = ["up", "down", "left", "right"]
            .iter()
            .find(|w| alow.contains(**w))
            .map_or(' ', |w| w.chars().next().unwrap());
        let len = (arrow.matches(['-', '.']).count() as u32)
            .saturating_sub(1)
            .max(1);
        let dashed = arrow.contains('.');
        let mut node_of = |d: &mut OldAct, s: &str, as_src: bool| -> usize {
            let s = s.trim();
            if s.starts_with("(*") {
                let start = as_src && !saw_start;
                if as_src {
                    saw_start = true;
                }
                let _ = start;
                let id = if as_src { "__start" } else { "__end" }.to_string();
                if let Some(i) = d.nodes.iter().position(|n| n.id == id) {
                    return i;
                }
                d.nodes.push(Node {
                    id,
                    t: None,
                    k: if as_src { K::Start } else { K::End },
                });
                return d.nodes.len() - 1;
            }
            if s.starts_with("===") {
                let id = s.trim_matches('=').trim().to_string();
                if let Some(i) = d.nodes.iter().position(|n| n.id == id) {
                    return i;
                }
                d.nodes.push(Node {
                    id,
                    t: None,
                    k: K::Bar,
                });
                return d.nodes.len() - 1;
            }
            // "Teks" as A  |  A  |  "Teks"
            let (text, alias) = match s.split_once(" as ") {
                Some((a, b)) => (a.trim(), Some(b.trim())),
                None => (s, None),
            };
            let id = alias.unwrap_or(text).trim_matches('"').to_string();
            if let Some(i) = d
                .nodes
                .iter()
                .position(|n| n.id == id || n.id == text.trim_matches('"'))
            {
                return i;
            }
            let t = d.texts.add(Txt::new(&unescape(text), 12.0));
            d.nodes.push(Node {
                id,
                t: Some(t),
                k: K::Act,
            });
            d.nodes.len() - 1
        };
        let sources: Vec<usize> = if src.is_empty() {
            last.clone()
        } else {
            vec![node_of(&mut d, src, true)]
        };
        let tgt = node_of(&mut d, rest, false);
        for (i, &s) in sources.iter().enumerate() {
            d.edges.push(Edge {
                a: s,
                b: tgt,
                label: if i == 0 { label } else { None },
                dir,
                len,
                dashed,
            });
        }
        last = vec![tgt];
    }
    (!d.edges.is_empty()).then_some(d)
}

impl Diagram for OldAct {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let dims: Vec<(f32, f32)> = self
            .nodes
            .iter()
            .map(|n| match n.k {
                K::Start => (20.0, 20.0),
                K::End => (22.0, 22.0),
                K::Branch => (24.0, 24.0),
                K::Bar => (80.0, 8.0),
                K::Act => {
                    let (tw, th) = size[n.t.unwrap()];
                    (tw + 20.0, th + 20.0)
                }
            })
            .collect();
        let edges: Vec<graph::Edge> = self
            .edges
            .iter()
            .map(|e| {
                let (a, b, minlen, ordered) = match e.dir {
                    'u' => (e.b, e.a, e.len, false),
                    'l' => (e.b, e.a, 0, true),
                    'r' => (e.a, e.b, 0, true),
                    _ => (e.a, e.b, e.len, false),
                };
                graph::Edge {
                    a,
                    b,
                    minlen,
                    label: e.label.map(|t| size[t]),
                    ordered,
                }
            })
            .collect();
        let g = Graph {
            nodes: dims
                .iter()
                .zip(&self.nodes)
                .map(|(d, n)| graph::Node {
                    w: d.0,
                    h: d.1,
                    shape: if matches!(n.k, K::Start | K::End) {
                        graph::Shape::Ellipse
                    } else {
                        graph::Shape::Rect
                    },
                    cluster: None,
                })
                .collect(),
            edges,
            clusters: Vec::new(),
            lr: false,
            nodesep: 20.0,
            ranksep: 40.0,
        };
        let o = graph::layout(&g);
        let mut p = Vec::new();
        for (ei, e) in self.edges.iter().enumerate() {
            let mut pts = o.edges[ei].clone();
            if pts.len() < 2 {
                continue;
            }
            if matches!(e.dir, 'u' | 'l') {
                pts.reverse();
            }
            let n = pts.len();
            let dir = (pts[n - 1].0 - pts[n - 2].0, pts[n - 1].1 - pts[n - 2].1);
            p.push(if e.dashed {
                dashed(pts.clone(), Col::Ink, 1.0, (7.0, 7.0))
            } else {
                line(pts.clone(), Col::Ink, 1.0)
            });
            p.push(head(pts[n - 1], dir, Col::Ink));
            if let (Some(t), Some(m)) = (e.label, o.labels[ei]) {
                p.push(Prim::Text(t, m.0 - size[t].0 / 2.0, m.1 - size[t].1 / 2.0));
            }
        }
        for (i, n) in self.nodes.iter().enumerate() {
            let (cx, cy) = o.pos[i];
            let (w, h) = dims[i];
            let r = [cx - w / 2.0, cy - h / 2.0, w, h];
            match n.k {
                K::Start => p.push(ellipse(r, Col::Rgb(0x222222), Col::Rgb(0x222222), 1.0)),
                K::End => {
                    p.push(ellipse(r, Col::Paper, Col::Rgb(0x222222), 1.5));
                    p.push(ellipse(
                        [cx - 6.0, cy - 6.0, 12.0, 12.0],
                        Col::Rgb(0x222222),
                        Col::Rgb(0x222222),
                        1.0,
                    ));
                }
                K::Bar => p.push(rect(r, 0.0, Col::Rgb(0x555555), Col::None, 0.0)),
                K::Branch => {
                    p.push(poly(
                        vec![(cx, r[1]), (r[0] + w, cy), (cx, r[1] + h), (r[0], cy)],
                        Col::Box,
                        Col::Ink,
                        0.5,
                    ));
                    if let Some(t) = n.t {
                        let (tw, th) = size[t];
                        p.push(Prim::Text(t, cx - 7.0 - tw, r[1] - th + 2.0));
                    }
                }
                K::Act => {
                    p.push(rect(r, 12.5, Col::Box, Col::Ink, 0.5));
                    let t = n.t.unwrap();
                    p.push(Prim::Text(t, r[0] + 10.0, r[1] + 10.0));
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_activity() {
        let l: Vec<String> = "(*) --> \"Mulai\"\nif \"Valid?\" then\n -->[ya] \"Simpan\"\n --> (*)\nelse\n -->[tidak] \"Tolak\"\nendif\n--> \"Catat\"".lines().map(String::from).collect();
        let a = parse_lines(&l).unwrap();
        assert!(a.nodes.iter().any(|n| n.k == K::Branch));
        assert!(a.nodes.iter().any(|n| n.k == K::End));
        // "Catat" menerima sisi dari cabang Tolak (Simpan berakhir di (*)).
        let catat = a.nodes.iter().position(|n| n.id == "Catat").unwrap();
        assert_eq!(a.edges.iter().filter(|e| e.b == catat).count(), 1);
        let pl = a.place(&vec![(30.0, 14.0); a.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
