//! Bantuan gambar diagram PlantUML: warna, layout teks creole, dan render PNG
//! di CPU (untuk membandingkan dengan keluaran PlantUML tanpa membuka aplikasi).

use crate::gpu::Rgba;
#[cfg(not(target_arch = "wasm32"))]
use crate::plantuml::{self, Col, Prim};
use crate::text::ATLAS;
use crate::text::{layout, Face, Fonts, Lay, Style};
use crate::vector;

fn hex(v: u32) -> Rgba {
    [
        ((v >> 16) & 255) as f32 / 255.0,
        ((v >> 8) & 255) as f32 / 255.0,
        (v & 255) as f32 / 255.0,
        1.0,
    ]
}

/// Warna PlantUML (palet terang skin bawaan); diagram selalu di atas kertas putih.
pub fn puml_col(c: crate::plantuml::Col) -> Rgba {
    use crate::plantuml::Col;
    match c {
        Col::None => [0.0; 4],
        Col::Ink => hex(0x181818),
        Col::Text => hex(0x000000),
        Col::Paper => hex(0xFFFFFF),
        Col::Box => hex(0xF1F1F1),
        Col::Part => hex(0xE2E2F0),
        Col::Note => hex(0xFEFFDD),
        Col::Group => hex(0xEEEEEE),
        Col::Faint => hex(0xDDDDDD),
        Col::Rgb(v) => hex(v),
    }
}

/// Satu teks diagram (creole) pada skala `k` (px layar per px PlantUML).
pub fn puml_lay(f: &mut Fonts, tx: &crate::plantuml::Txt, k: f32) -> Lay {
    use crate::plantuml::{creole, Align};
    let base = creole::Fmt {
        bold: tx.bold,
        italic: tx.italic,
        mono: tx.mono,
        color: Some(tx.color),
        ..Default::default()
    };
    let (text, runs) = creole::parse(&tx.s, base);
    let runs: Vec<(usize, Style)> = runs
        .iter()
        .map(|(end, fm)| {
            let face = if fm.mono {
                Face::DiaMono
            } else if fm.bold {
                Face::DiaBold
            } else if fm.italic {
                Face::DiaItalic
            } else {
                Face::Dia
            };
            let mut st = Style::new(
                face,
                fm.size.unwrap_or(tx.size) * k,
                puml_col(fm.color.unwrap_or(tx.color)),
            );
            st.under = fm.under;
            st.strike = fm.strike;
            st.bg = fm.back.map(puml_col);
            (*end, st)
        })
        .collect();
    // Tinggi baris PlantUML (SansSerif) ≈ 1,164 em; sesuaikan dengan metrik font kita.
    let (asc, desc) = f.metrics(runs[0].1.face, runs[0].1.px);
    let lh = 1.164 * runs[0].1.px / (asc + desc).max(1.0);
    let mut l = layout(f, &text, &runs, 1e6, lh);
    if tx.align != Align::Left {
        for r in &mut l.rows {
            let rw =
                r.gs.iter()
                    .rev()
                    .find(|g| !g.c.is_whitespace())
                    .map_or(0.0, |g| g.x + g.adv);
            let dx = if tx.align == Align::Center {
                (l.w - rw) / 2.0
            } else {
                l.w - rw
            };
            r.gs.iter_mut().for_each(|g| g.x += dx.max(0.0).round());
        }
    }
    l
}

/// Render semua diagram dalam `src` ke PNG (skala 1 = px PlantUML), berurutan ke bawah.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_png(src: &str, out: &str, k: f32) -> Result<(), String> {
    let mut f = Fonts::load()?;
    let dias = plantuml::parse_all(src).ok_or("diagram tidak dikenali")?;
    let mut pages = Vec::new();
    for dia in dias {
        let lays: Vec<Lay> = dia
            .texts()
            .iter()
            .map(|tx| puml_lay(&mut f, tx, k))
            .collect();
        let size: Vec<(f32, f32)> = lays.iter().map(|l| (l.w / k, l.h / k)).collect();
        let pl = dia.place(&size);
        pages.push((lays, pl));
    }
    let w = pages
        .iter()
        .map(|p| (p.1.w * k).ceil() as usize)
        .max()
        .unwrap_or(1)
        .max(1);
    let h: usize = pages
        .iter()
        .map(|p| (p.1.h * k).ceil() as usize)
        .sum::<usize>()
        .max(1);
    let mut img = vec![[1.0f32, 1.0, 1.0]; w * h];
    let mut atlas = vec![0u8; (ATLAS * ATLAS) as usize];
    let mut oy = 0.0;
    for (lays, pl) in &pages {
        let mut mesh = vector::Mesh::new();
        for q in &pl.prims {
            if let Prim::Path {
                pts,
                closed,
                fill,
                stroke,
                w: lw,
                dash,
            } = q
            {
                let pts: Vec<vector::Pt> = pts.iter().map(|p| (p.0 * k, oy + p.1 * k)).collect();
                if *closed && *fill != Col::None {
                    vector::fill(&mut mesh, &pts, puml_col(*fill));
                }
                if *stroke != Col::None && *lw > 0.0 {
                    vector::stroke(
                        &mut mesh,
                        &pts,
                        *closed,
                        lw * k,
                        puml_col(*stroke),
                        dash.map(|(a, b)| (a * k, b * k)),
                    );
                }
            }
        }
        for t in mesh.chunks(3) {
            raster(&mut img, w, h, [t[0], t[1], t[2]]);
        }
        for q in pl
            .prims
            .iter()
            .filter(|_| std::env::var("PUML_NOTEXT").is_err())
        {
            if let Prim::Text(i, x, y) = q {
                text(
                    &mut img,
                    w,
                    h,
                    &mut f,
                    &mut atlas,
                    &lays[*i],
                    x * k,
                    oy + y * k,
                );
            }
        }
        oy += pl.h * k;
    }
    let bytes: Vec<u8> = img
        .iter()
        .flat_map(|p| p.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    image::save_buffer(out, &bytes, w as u32, h as u32, image::ColorType::Rgb8)
        .map_err(|e| e.to_string())
}

/// Kanvas RGB di CPU untuk ekspor PNG (tombol Download di penampil).
/// `Fonts` yang dipakai harus milik kanvas sendiri: glyph baru diambil dari
/// antrean unggah font ke atlas CPU kanvas, bukan ke GPU.
pub struct Canvas {
    img: Vec<[f32; 3]>,
    w: usize,
    h: usize,
    atlas: Vec<u8>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, bg: Rgba) -> Canvas {
        Canvas {
            img: vec![[bg[0], bg[1], bg[2]]; w * h],
            w,
            h,
            atlas: vec![0u8; (ATLAS * ATLAS) as usize],
        }
    }

    pub fn mesh(&mut self, m: &vector::Mesh, dx: f32, dy: f32) {
        let sh = |(p, c): (vector::Pt, Rgba)| ((p.0 + dx, p.1 + dy), c);
        for t in m.chunks(3) {
            raster(&mut self.img, self.w, self.h, [sh(t[0]), sh(t[1]), sh(t[2])]);
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        fill_rect(&mut self.img, self.w, self.h, x, y, w, h, c);
    }

    pub fn text(&mut self, f: &mut Fonts, l: &Lay, x: f32, y: f32) {
        text(&mut self.img, self.w, self.h, f, &mut self.atlas, l, x, y);
    }

    pub fn png(&self) -> Result<Vec<u8>, String> {
        let bytes: Vec<u8> = self
            .img
            .iter()
            .flat_map(|p| p.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect();
        let img = image::RgbImage::from_raw(self.w as u32, self.h as u32, bytes).ok_or("empty image")?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }
}

fn blend(p: &mut [f32; 3], c: Rgba, a: f32) {
    let a = (c[3] * a).clamp(0.0, 1.0);
    for i in 0..3 {
        p[i] = p[i] * (1.0 - a) + c[i] * a;
    }
}

/// Segitiga berwarna per verteks (barisentrik), sampel di tengah piksel.
fn raster(img: &mut [[f32; 3]], w: usize, h: usize, t: [(vector::Pt, Rgba); 3]) {
    let [(a, ca), (b, cb), (c, cc)] = t;
    let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    if area.abs() < 1e-6 {
        return;
    }
    let x0 = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
    let x1 = (a.0.max(b.0).max(c.0).ceil() as usize).min(w);
    let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
    let y1 = (a.1.max(b.1).max(c.1).ceil() as usize).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) / area;
            let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let col: Rgba = std::array::from_fn(|i| ca[i] * w0 + cb[i] * w1 + cc[i] * w2);
            blend(&mut img[y * w + x], col, 1.0);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn text(
    img: &mut [[f32; 3]],
    w: usize,
    h: usize,
    f: &mut Fonts,
    atlas: &mut [u8],
    l: &Lay,
    ox: f32,
    oy: f32,
) {
    for r in &l.rows {
        for g in &r.gs {
            let st = l.styles[g.st as usize];
            if let Some(bg) = st.bg {
                fill_rect(img, w, h, ox + g.x, oy + r.top, g.adv, r.h, bg);
            }
            if g.c.is_whitespace() {
                continue;
            }
            let Some(gl) = f.glyph(st.face, st.px, g.c) else {
                continue;
            };
            for u in f.uploads.drain(..) {
                for yy in 0..u.h {
                    let row = ((u.y + yy) * ATLAS + u.x) as usize;
                    atlas[row..row + u.w as usize]
                        .copy_from_slice(&u.data[(yy * u.w) as usize..((yy + 1) * u.w) as usize]);
                }
            }
            let (gx, gy) = (
                (ox + g.x + gl.left).round() as i64,
                (oy + r.base - gl.top).round() as i64,
            );
            for yy in 0..gl.h as i64 {
                for xx in 0..gl.w as i64 {
                    let (px, py) = (gx + xx, gy + yy);
                    if px < 0 || py < 0 || px >= w as i64 || py >= h as i64 {
                        continue;
                    }
                    let a = atlas[((gl.y as i64 + yy) * ATLAS as i64 + gl.x as i64 + xx) as usize]
                        as f32
                        / 255.0;
                    blend(&mut img[py as usize * w + px as usize], st.color, a);
                }
            }
            if st.under {
                fill_rect(img, w, h, ox + g.x, oy + r.base + 2.0, g.adv, 1.0, st.color);
            }
            if st.strike {
                fill_rect(
                    img,
                    w,
                    h,
                    ox + g.x,
                    oy + r.base - r.h * 0.3,
                    g.adv,
                    1.0,
                    st.color,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(img: &mut [[f32; 3]], w: usize, h: usize, x: f32, y: f32, rw: f32, rh: f32, c: Rgba) {
    for py in (y.round().max(0.0) as usize)..((y + rh).round().max(0.0) as usize).min(h) {
        for px in (x.round().max(0.0) as usize)..((x + rw).round().max(0.0) as usize).min(w) {
            blend(&mut img[py * w + px], c, 1.0);
        }
    }
}
/// Debug: cetak primitif tiap diagram (ukuran teks kira-kira 7 px/huruf).
#[cfg(not(target_arch = "wasm32"))]
pub fn dump(src: &str) {
    for dia in plantuml::parse_all(src).unwrap_or_default() {
        let size: Vec<(f32, f32)> = dia
            .texts()
            .iter()
            .map(|t| (t.s.chars().count() as f32 * t.size * 0.55, t.size * 1.164))
            .collect();
        for p in dia.place(&size).prims {
            match p {
                Prim::Path { pts, closed, fill, stroke, w, dash } => println!("path n={} closed={closed} fill={fill:?} stroke={stroke:?} w={w} dash={dash:?} {:?}", pts.len(), &pts[..pts.len().min(4)]),
                Prim::Text(i, x, y) => println!("text {i} {x:.1},{y:.1} {:?}", dia.texts()[i].s),
            }
        }
    }
}
