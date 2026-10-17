//! Tesselasi vektor sederhana ke segitiga (untuk diagram): isi poligon (ear
//! clipping) dan garis (quad per ruas). Tepi dihaluskan dengan "bulu" 1 px yang
//! alfanya memudar ke 0 — interpolasi warna per verteks melakukan antialias.
// ponytail: tanpa join miter/bulat (ruas diperpanjang setengah tebal), cukup
// untuk garis 1–3 px.

pub type Pt = (f32, f32);
pub type Rgba = [f32; 4];

/// Segitiga: tiap 3 verteks satu segitiga.
pub type Mesh = Vec<(Pt, Rgba)>;

fn clear(c: Rgba) -> Rgba {
    [c[0], c[1], c[2], 0.0]
}

fn quad(m: &mut Mesh, a: Pt, b: Pt, c: Pt, d: Pt, ca: Rgba, cb: Rgba) {
    // a-b sisi dalam (ca), d-c sisi luar (cb); urutan a b c d melingkar.
    m.extend_from_slice(&[(a, ca), (b, ca), (c, cb), (a, ca), (c, cb), (d, cb)]);
}

fn area2(p: &[Pt]) -> f32 {
    let n = p.len();
    (0..n)
        .map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1)
        .sum()
}

fn cross(o: Pt, a: Pt, b: Pt) -> f32 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn inside(p: Pt, a: Pt, b: Pt, c: Pt) -> bool {
    cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
}

/// Isi poligon sederhana (boleh cekung) dengan tepi halus.
pub fn fill(m: &mut Mesh, poly: &[Pt], c: Rgba) {
    let mut p: Vec<Pt> = poly.to_vec();
    p.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3);
    if p.len() > 2
        && (p[0].0 - p[p.len() - 1].0).abs() < 1e-3
        && (p[0].1 - p[p.len() - 1].1).abs() < 1e-3
    {
        p.pop();
    }
    if p.len() < 3 {
        return;
    }
    if area2(&p) < 0.0 {
        p.reverse(); // jadikan berlawanan jarum jam (dalam koordinat y-turun: searah layar)
    }
    let ring = p.clone();
    // Ear clipping O(n²).
    let mut idx: Vec<usize> = (0..p.len()).collect();
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = idx.len();
        let mut cut = None;
        for i in 0..n {
            let (a, b, cc) = (p[idx[(i + n - 1) % n]], p[idx[i]], p[idx[(i + 1) % n]]);
            if cross(a, b, cc) <= 0.0 {
                continue;
            }
            if idx.iter().all(|&k| {
                let q = p[k];
                q == a || q == b || q == cc || !inside(q, a, b, cc)
            }) {
                cut = Some(i);
                m.extend_from_slice(&[(a, c), (b, c), (cc, c)]);
                break;
            }
        }
        match cut {
            Some(i) => {
                idx.remove(i);
            }
            None => break, // poligon rusak: sisanya dibuang
        }
    }
    if idx.len() == 3 {
        m.extend_from_slice(&[(p[idx[0]], c), (p[idx[1]], c), (p[idx[2]], c)]);
    }
    // Bulu tepi keluar 1 px.
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (nx, ny) = (dy / l * 0.8, -dx / l * 0.8); // normal keluar (poligon berlawanan arah)
        quad(
            m,
            a,
            b,
            (b.0 + nx, b.1 + ny),
            (a.0 + nx, a.1 + ny),
            c,
            clear(c),
        );
    }
}

/// Garis berderet; `dash` = (panjang garis, panjang celah).
pub fn stroke(m: &mut Mesh, pts: &[Pt], closed: bool, w: f32, c: Rgba, dash: Option<(f32, f32)>) {
    let mut p: Vec<Pt> = pts.to_vec();
    if closed && p.len() > 2 {
        p.push(p[0]);
    }
    let Some((on, off)) = dash else {
        for s in p.windows(2) {
            seg(m, s[0], s[1], w, c);
        }
        return;
    };
    // Putus-putus: pola berlanjut melewati sudut.
    let (mut phase, mut draw) = (on, true);
    for s in p.windows(2) {
        let (a, b) = (s[0], s[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt();
        let mut t = 0.0;
        while t < l {
            let k = phase.min(l - t);
            if draw {
                let (u0, u1) = (t / l, (t + k) / l);
                seg(
                    m,
                    (a.0 + dx * u0, a.1 + dy * u0),
                    (a.0 + dx * u1, a.1 + dy * u1),
                    w,
                    c,
                );
            }
            t += k;
            phase -= k;
            if phase <= 1e-4 {
                draw = !draw;
                phase = if draw { on } else { off };
            }
        }
    }
}

/// Satu ruas setebal `w`, ujung diperpanjang w/2 supaya sudut tertutup.
fn seg(m: &mut Mesh, a: Pt, b: Pt, w: f32, c: Rgba) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = (dx * dx + dy * dy).sqrt();
    if l < 1e-4 {
        return;
    }
    let (ux, uy) = (dx / l, dy / l);
    let h = w / 2.0;
    let (a, b) = (
        (a.0 - ux * h * 0.5, a.1 - uy * h * 0.5),
        (b.0 + ux * h * 0.5, b.1 + uy * h * 0.5),
    );
    // Garis tipis (<1 px) dibuat 1 px dengan alfa sebanding.
    let (h, c) = if w < 1.0 {
        (0.5, [c[0], c[1], c[2], c[3] * w])
    } else {
        (h, c)
    };
    let (nx, ny) = (-uy, ux);
    let core = |s: f32| ((a.0 + nx * s, a.1 + ny * s), (b.0 + nx * s, b.1 + ny * s));
    let (a0, b0) = core(-h + 0.35);
    let (a1, b1) = core(h - 0.35);
    // Inti (rapat penuh) + bulu 0,7 px tiap sisi.
    if h > 0.35 {
        m.extend_from_slice(&[(a0, c), (b0, c), (b1, c), (a0, c), (b1, c), (a1, c)]);
    }
    let (ao, bo) = core(h + 0.35);
    quad(m, a1, b1, bo, ao, c, clear(c));
    let (ao, bo) = core(-h - 0.35);
    quad(m, a0, b0, bo, ao, c, clear(c));
}

// ---------------------------------------------------------------------------
// Bentuk bantu → titik poligon.

/// Elips di dalam kotak [x, y, w, h].
pub fn ellipse(r: [f32; 4]) -> Vec<Pt> {
    let (cx, cy, rx, ry) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0, r[2] / 2.0, r[3] / 2.0);
    let n = ((rx.max(ry) * 0.8) as usize).clamp(16, 96);
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

/// Kotak bersudut bulat (radius `rad`; 0 = siku).
pub fn rrect(r: [f32; 4], rad: f32) -> Vec<Pt> {
    let [x, y, w, h] = r;
    let rad = rad.min(w / 2.0).min(h / 2.0).max(0.0);
    if rad < 0.5 {
        return vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    }
    let mut p = Vec::new();
    let k = 6;
    for (cx, cy, a0) in [
        (x + w - rad, y + rad, -90.0f32),
        (x + w - rad, y + h - rad, 0.0),
        (x + rad, y + h - rad, 90.0),
        (x + rad, y + rad, 180.0),
    ] {
        for i in 0..=k {
            let a = (a0 + 90.0 * i as f32 / k as f32).to_radians();
            p.push((cx + rad * a.cos(), cy + rad * a.sin()));
        }
    }
    p
}

/// Kurva Bézier kubik → titik (tanpa titik awal).
pub fn bezier(out: &mut Vec<Pt>, p0: Pt, p1: Pt, p2: Pt, p3: Pt) {
    let l = ((p3.0 - p0.0).abs() + (p3.1 - p0.1).abs() + (p1.0 - p0.0).abs() + (p2.1 - p1.1).abs())
        .max(1.0);
    let n = ((l / 6.0) as usize).clamp(4, 40);
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push((
            a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_concave() {
        // Bentuk L (cekung): 4 segitiga isi.
        let mut m = Mesh::new();
        let l = [
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
        ];
        fill(&mut m, &l, [1.0; 4]);
        let solid: f32 = m
            .chunks(3)
            .filter(|t| t.iter().all(|v| v.1[3] == 1.0))
            .map(|t| cross(t[0].0, t[1].0, t[2].0).abs() / 2.0)
            .sum();
        assert!((solid - 3.0).abs() < 1e-4, "{solid}");
    }
}
