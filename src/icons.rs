//! Ikon Lucide (ISC, `assets/icons`) → garis untuk di-stroke. Hanya bagian SVG
//! yang dipakai Lucide: `path` (M L H V C S Q A Z), `line`, `polyline`,
//! `rect`, `circle`. Koordinat dalam kotak 24×24.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::vector::{self, Pt};

/// (titik, tertutup?)
pub type Shape = Vec<(Vec<Pt>, bool)>;

const SRC: [(&str, &str); 23] = [
    ("chevron-down", include_str!("../assets/icons/chevron-down.svg")),
    ("chevron-up", include_str!("../assets/icons/chevron-up.svg")),
    ("book-open", include_str!("../assets/icons/book-open.svg")),
    ("circle-alert", include_str!("../assets/icons/circle-alert.svg")),
    ("download", include_str!("../assets/icons/download.svg")),
    ("file-text", include_str!("../assets/icons/file-text.svg")),
    ("link", include_str!("../assets/icons/link.svg")),
    ("link-2-off", include_str!("../assets/icons/link-2-off.svg")),
    ("maximize-2", include_str!("../assets/icons/maximize-2.svg")),
    ("minus", include_str!("../assets/icons/minus.svg")),
    ("mouse-pointer-2", include_str!("../assets/icons/mouse-pointer-2.svg")),
    ("panel-left-close", include_str!("../assets/icons/panel-left-close.svg")),
    ("panel-left-open", include_str!("../assets/icons/panel-left-open.svg")),
    ("pencil", include_str!("../assets/icons/pencil.svg")),
    ("plus", include_str!("../assets/icons/plus.svg")),
    ("refresh-cw", include_str!("../assets/icons/refresh-cw.svg")),
    ("search", include_str!("../assets/icons/search.svg")),
    ("settings", include_str!("../assets/icons/settings.svg")),
    ("share-2", include_str!("../assets/icons/share-2.svg")),
    ("trash-2", include_str!("../assets/icons/trash-2.svg")),
    ("trash", include_str!("../assets/icons/trash.svg")),
    ("undo-2", include_str!("../assets/icons/undo-2.svg")),
    ("x", include_str!("../assets/icons/x.svg")),
];

pub fn get(name: &str) -> &'static Shape {
    static MAP: OnceLock<HashMap<&'static str, Shape>> = OnceLock::new();
    static EMPTY: Shape = Vec::new();
    MAP.get_or_init(|| SRC.iter().map(|(n, s)| (*n, parse(s))).collect())
        .get(name)
        .unwrap_or(&EMPTY)
}

/// Nilai atribut `k="..."` dalam satu tag.
fn attr<'a>(tag: &'a str, k: &str) -> Option<&'a str> {
    let pat = format!(" {k}=\"");
    let a = tag.find(&pat)? + pat.len();
    Some(&tag[a..a + tag[a..].find('"')?])
}

fn num(tag: &str, k: &str) -> f32 {
    attr(tag, k).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

fn nums(s: &str) -> Vec<f32> {
    // Angka SVG: "1.5.5" = 1.5, .5; "-1-2" = -1, -2.
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<f32>| {
        if let Ok(v) = cur.parse() {
            out.push(v);
        }
        cur.clear();
    };
    for c in s.chars() {
        match c {
            '0'..='9' => cur.push(c),
            '.' if cur.contains('.') => {
                flush(&mut cur, &mut out);
                cur.push('.');
            }
            '.' => cur.push(c),
            '-' => {
                if !cur.is_empty() && !cur.ends_with('e') {
                    flush(&mut cur, &mut out);
                }
                cur.push('-');
            }
            _ => flush(&mut cur, &mut out),
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn parse(svg: &str) -> Shape {
    let mut out = Shape::new();
    for tag in svg.split('<').skip(1) {
        let name = tag.split([' ', '\n', '>', '/']).next().unwrap_or("");
        let tag = &tag.replace(['\n', '\t'], " ");
        match name {
            "path" => path(attr(tag, "d").unwrap_or(""), &mut out),
            "line" => out.push((
                vec![(num(tag, "x1"), num(tag, "y1")), (num(tag, "x2"), num(tag, "y2"))],
                false,
            )),
            "polyline" => {
                let v = nums(attr(tag, "points").unwrap_or(""));
                out.push((v.chunks(2).map(|c| (c[0], c[1])).collect(), false));
            }
            "rect" => {
                let r = [num(tag, "x"), num(tag, "y"), num(tag, "width"), num(tag, "height")];
                out.push((vector::rrect(r, num(tag, "rx")), true));
            }
            "circle" => {
                let (cx, cy, r) = (num(tag, "cx"), num(tag, "cy"), num(tag, "r"));
                out.push((vector::ellipse([cx - r, cy - r, 2.0 * r, 2.0 * r]), true));
            }
            _ => {}
        }
    }
    out
}

fn path(d: &str, out: &mut Shape) {
    // Pecah jadi (perintah, angka).
    let mut cmds: Vec<(char, Vec<f32>)> = Vec::new();
    let mut start = None;
    for (i, c) in d.char_indices() {
        if c.is_ascii_alphabetic() && c != 'e' {
            if let Some((k, ch)) = start {
                cmds.push((ch, nums(&d[k..i])));
            }
            start = Some((i + 1, c));
        }
    }
    if let Some((k, ch)) = start {
        cmds.push((ch, nums(&d[k..])));
    }
    let mut cur: Vec<Pt> = Vec::new();
    let (mut p, mut p0, mut ctrl): (Pt, Pt, Pt) = ((0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
    let flush = |cur: &mut Vec<Pt>, out: &mut Shape, closed: bool| {
        if cur.len() > 1 {
            out.push((std::mem::take(cur), closed));
        }
        cur.clear();
    };
    for (c, a) in cmds {
        let rel = c.is_ascii_lowercase();
        let at = |p: Pt, x: f32, y: f32| if rel { (p.0 + x, p.1 + y) } else { (x, y) };
        match c.to_ascii_uppercase() {
            'M' => {
                flush(&mut cur, out, false);
                for (i, xy) in a.chunks(2).enumerate() {
                    p = at(p, xy[0], xy[1]);
                    if i == 0 {
                        p0 = p;
                    }
                    cur.push(p);
                }
                ctrl = p;
            }
            'L' => {
                for xy in a.chunks(2) {
                    p = at(p, xy[0], xy[1]);
                    cur.push(p);
                }
                ctrl = p;
            }
            'H' => {
                for &x in &a {
                    p = (if rel { p.0 + x } else { x }, p.1);
                    cur.push(p);
                }
                ctrl = p;
            }
            'V' => {
                for &y in &a {
                    p = (p.0, if rel { p.1 + y } else { y });
                    cur.push(p);
                }
                ctrl = p;
            }
            'C' => {
                for k in a.chunks(6) {
                    let (c1, c2, e) = (at(p, k[0], k[1]), at(p, k[2], k[3]), at(p, k[4], k[5]));
                    vector::bezier(&mut cur, p, c1, c2, e);
                    (ctrl, p) = (c2, e);
                }
            }
            'S' => {
                for k in a.chunks(4) {
                    let c1 = (2.0 * p.0 - ctrl.0, 2.0 * p.1 - ctrl.1);
                    let (c2, e) = (at(p, k[0], k[1]), at(p, k[2], k[3]));
                    vector::bezier(&mut cur, p, c1, c2, e);
                    (ctrl, p) = (c2, e);
                }
            }
            'Q' => {
                for k in a.chunks(4) {
                    let (q, e) = (at(p, k[0], k[1]), at(p, k[2], k[3]));
                    let c1 = (p.0 + 2.0 / 3.0 * (q.0 - p.0), p.1 + 2.0 / 3.0 * (q.1 - p.1));
                    let c2 = (e.0 + 2.0 / 3.0 * (q.0 - e.0), e.1 + 2.0 / 3.0 * (q.1 - e.1));
                    vector::bezier(&mut cur, p, c1, c2, e);
                    (ctrl, p) = (q, e);
                }
            }
            'A' => {
                for k in a.chunks(7) {
                    let e = at(p, k[5], k[6]);
                    arc(&mut cur, p, k[0], k[1], k[2], k[3] != 0.0, k[4] != 0.0, e);
                    p = e;
                }
                ctrl = p;
            }
            'Z' => {
                flush(&mut cur, out, true);
                p = p0;
                cur.push(p);
                ctrl = p;
            }
            _ => {}
        }
    }
    flush(&mut cur, out, false);
}

/// Busur elips SVG (parameterisasi titik ujung → pusat, SVG 1.1 F.6.5).
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<Pt>, p: Pt, rx: f32, ry: f32, rot: f32, large: bool, sweep: bool, e: Pt) {
    if out.is_empty() {
        out.push(p);
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-6 || ry < 1e-6 {
        out.push(e);
        return;
    }
    let (sn, cs) = rot.to_radians().sin_cos();
    let (dx, dy) = ((p.0 - e.0) / 2.0, (p.1 - e.1) / 2.0);
    let (x1, y1) = (cs * dx + sn * dy, -sn * dx + cs * dy);
    let l = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if l > 1.0 {
        (rx, ry) = (rx * l.sqrt(), ry * l.sqrt());
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den.max(1e-12)).sqrt();
    if large == sweep {
        k = -k;
    }
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let (cx, cy) = (cs * cx1 - sn * cy1 + (p.0 + e.0) / 2.0, sn * cx1 + cs * cy1 + (p.1 + e.1) / 2.0);
    let ang = |ux: f32, uy: f32| uy.atan2(ux);
    let t0 = ang((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = ang((-x1 - cx1) / rx, (-y1 - cy1) / ry) - t0;
    let tau = std::f32::consts::TAU;
    if sweep && dt < 0.0 {
        dt += tau;
    } else if !sweep && dt > 0.0 {
        dt -= tau;
    }
    let n = ((dt.abs() / tau * 48.0).ceil() as usize).max(2);
    for i in 1..=n {
        let t = t0 + dt * i as f32 / n as f32;
        let (x, y) = (rx * t.cos(), ry * t.sin());
        out.push((cs * x - sn * y + cx, sn * x + cs * y + cy));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn icons_parse() {
        for (n, _) in super::SRC {
            let s = super::get(n);
            assert!(!s.is_empty(), "{n}");
            for (pts, _) in s {
                assert!(pts.iter().all(|p| p.0 > -1.0 && p.0 < 25.0 && p.1 > -1.0 && p.1 < 25.0), "{n}");
            }
        }
        // Lingkaran `search`: r 8 di (11, 11).
        let c = &super::get("search")[0].0;
        assert!(c.iter().all(|p| ((p.0 - 11.0).hypot(p.1 - 11.0) - 8.0).abs() < 0.2));
    }
}
