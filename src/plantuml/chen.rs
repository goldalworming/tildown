//! Chen ER (`@startchen`) — entitas (kotak; weak ganda), atribut (elips; key
//! digaris bawahi, derived putus-putus, multi ganda, komposit bersarang), relasi
//! (belah ketupat; identifying ganda), kardinalitas di sisi. Tata letak: graph.
// ponytail: tanpa subclass `->-`/`-<-` (spesialisasi) dan warna.

use super::graph::{self, Graph};
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum K {
    Entity {
        weak: bool,
    },
    Attr {
        key: bool,
        derived: bool,
        multi: bool,
    },
    Rel {
        ident: bool,
    },
}

struct Node {
    id: String,
    t: usize,
    k: K,
}

struct Edge {
    a: usize,
    b: usize,
    label: Option<usize>,
    double: bool,
}

pub struct Chen {
    texts: Texts,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    lr: bool,
}

fn stereo(s: &str) -> (String, String) {
    match (s.find("<<"), s.rfind(">>")) {
        (Some(a), Some(b)) if a < b => (
            format!("{}{}", &s[..a], &s[b + 2..]).trim().to_string(),
            s[a + 2..b].trim().to_lowercase(),
        ),
        _ => (s.trim().to_string(), String::new()),
    }
}

pub fn parse_lines(lines: &[String]) -> Option<Chen> {
    let mut c = Chen {
        texts: Texts::default(),
        nodes: Vec::new(),
        edges: Vec::new(),
        lr: false,
    };
    // Tumpukan pemilik atribut (entitas/relasi/atribut komposit).
    let mut stack: Vec<usize> = Vec::new();
    for raw in lines {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        if low.starts_with("left to right") {
            c.lr = true;
            continue;
        }
        if l == "}" {
            stack.pop();
            continue;
        }
        let opens = l.ends_with('{');
        let body = l.trim_end_matches('{').trim();
        if low.starts_with("entity ") || low.starts_with("relationship ") {
            let is_ent = low.starts_with("entity ");
            let (name, st) = stereo(&body[if is_ent { 7 } else { 13 }..]);
            let (name, _) = name
                .split_once(" as ")
                .map_or((name.as_str(), ""), |(a, b)| (a, b));
            let k = if is_ent {
                K::Entity { weak: st == "weak" }
            } else {
                K::Rel {
                    ident: st == "identifying",
                }
            };
            let id = name.trim().trim_matches('"').to_string();
            let n = c.node(&id, k);
            if opens {
                stack.push(n);
            }
            continue;
        }
        // Relasi: A -1- R, R =N= B, A -(1,N)- R
        if let Some(k) = body.find(['-', '=']) {
            let rest = &body[k..];
            let dbl = rest.starts_with('=');
            let ch = if dbl { '=' } else { '-' };
            if let Some(e) = rest[1..].find(ch) {
                let a = body[..k].trim();
                let lab = rest[1..e + 1].trim();
                let b = rest[e + 2..].trim();
                if !a.is_empty() && !b.is_empty() && !a.contains(' ') && !b.contains(' ') {
                    let ia = c.find_or(a);
                    let ib = c.find_or(b);
                    let label = (!lab.is_empty()).then(|| c.texts.add(Txt::new(lab, 11.0)));
                    c.edges.push(Edge {
                        a: ia,
                        b: ib,
                        label,
                        double: dbl,
                    });
                    continue;
                }
            }
        }
        // Atribut di dalam blok.
        if let Some(&owner) = stack.last() {
            let (name, st) = stereo(body);
            let id = format!("{}.{}", c.nodes[owner].id, name);
            let k = K::Attr {
                key: st == "key",
                derived: st == "derived",
                multi: st == "multi",
            };
            let t = {
                let tx = Txt::new(&unescape(&name), 14.0);
                c.texts.add(if st == "key" {
                    Txt {
                        s: format!("__{}__", tx.s),
                        ..tx
                    }
                } else {
                    tx
                })
            };
            c.nodes.push(Node { id, t, k });
            let n = c.nodes.len() - 1;
            c.edges.push(Edge {
                a: n,
                b: owner,
                label: None,
                double: false,
            });
            if opens {
                stack.push(n);
            }
            continue;
        }
        return None;
    }
    (!c.nodes.is_empty()).then_some(c)
}

impl Chen {
    fn node(&mut self, id: &str, k: K) -> usize {
        if let Some(i) = self.nodes.iter().position(|n| n.id == id) {
            self.nodes[i].k = k;
            return i;
        }
        let t = self.texts.add(Txt::new(&unescape(id), 14.0));
        self.nodes.push(Node {
            id: id.to_string(),
            t,
            k,
        });
        self.nodes.len() - 1
    }

    fn find_or(&mut self, id: &str) -> usize {
        match self.nodes.iter().position(|n| n.id == id) {
            Some(i) => i,
            None => self.node(id, K::Entity { weak: false }),
        }
    }
}

impl Diagram for Chen {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let dims: Vec<(f32, f32)> = self
            .nodes
            .iter()
            .map(|n| {
                let (tw, th) = size[n.t];
                match n.k {
                    K::Entity { .. } => (tw + 20.0, th + 21.3),
                    K::Attr { .. } => (tw + 22.0, th + 14.6),
                    K::Rel { .. } => (tw + 57.6, (th + 21.3) * 1.82),
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
                    shape: if matches!(n.k, K::Entity { .. }) {
                        graph::Shape::Rect
                    } else {
                        graph::Shape::Ellipse
                    },
                    cluster: None,
                })
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|e| graph::Edge {
                    a: e.a,
                    b: e.b,
                    // PlantUML: sisi relasi (`-1-`) minlen 2, atribut 1.
                    minlen: if matches!(self.nodes[e.a].k, K::Attr { .. }) {
                        1
                    } else {
                        2
                    },
                    label: e.label.map(|t| size[t]),
                    ordered: false,
                })
                .collect(),
            clusters: Vec::new(),
            lr: self.lr,
            nodesep: 35.0,
            ranksep: 60.0,
        };
        let o = graph::layout(&g);
        let mut p = Vec::new();
        for (ei, e) in self.edges.iter().enumerate() {
            let pts = &o.edges[ei];
            if pts.len() < 2 {
                continue;
            }
            // `=` (partisipasi total): satu garis tebal 2, seperti PlantUML.
            p.push(line(
                pts.clone(),
                Col::Ink,
                if e.double { 2.0 } else { 1.0 },
            ));
            if let (Some(t), Some(m)) = (e.label, o.labels[ei]) {
                p.push(Prim::Text(t, m.0 - size[t].0 / 2.0, m.1 - size[t].1 / 2.0));
            }
        }
        for (i, n) in self.nodes.iter().enumerate() {
            let (cx, cy) = o.pos[i];
            let (w, h) = dims[i];
            let r = [cx - w / 2.0, cy - h / 2.0, w, h];
            match n.k {
                K::Entity { weak } => {
                    p.push(rect(r, 0.0, Col::Box, Col::Ink, 0.5));
                    if weak {
                        p.push(rect(
                            [r[0] + 3.0, r[1] + 3.0, w - 6.0, h - 6.0],
                            0.0,
                            Col::Box,
                            Col::Ink,
                            0.5,
                        ));
                    }
                }
                K::Attr { derived, multi, .. } => {
                    p.push(Prim::Path {
                        pts: vector::ellipse(r),
                        closed: true,
                        fill: Col::Box,
                        stroke: Col::Ink,
                        w: 0.5,
                        dash: derived.then_some((10.0, 10.0)),
                    });
                    if multi {
                        p.push(ellipse(
                            [r[0] + 3.0, r[1] + 3.0, w - 6.0, h - 6.0],
                            Col::Box,
                            Col::Ink,
                            0.5,
                        ));
                    }
                }
                K::Rel { ident } => {
                    let d = |k: f32| {
                        vec![
                            (r[0] + k, cy),
                            (cx, r[1] + k * 0.5),
                            (r[0] + w - k, cy),
                            (cx, r[1] + h - k * 0.5),
                        ]
                    };
                    p.push(poly(d(0.0), Col::Box, Col::Ink, 0.5));
                    if ident {
                        p.push(poly(d(10.0), Col::Box, Col::Ink, 0.5));
                    }
                }
            }
            let (tw, th) = size[n.t];
            p.push(Prim::Text(n.t, cx - tw / 2.0, cy - th / 2.0));
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
    fn chen_parse() {
        let l: Vec<String> = "entity A {\n K <<key>>\n N {\n  X\n }\n}\nrelationship R <<identifying>> {\n}\nentity B <<weak>> {\n}\nA -1- R\nR =N= B".lines().map(String::from).collect();
        let c = parse_lines(&l).unwrap();
        assert_eq!(c.nodes.len(), 6);
        assert_eq!(c.edges.len(), 5);
        assert!(c.edges[4].double);
        let pl = c.place(&vec![(30.0, 16.0); c.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
