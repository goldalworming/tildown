//! Font, atlas glyph (sisi CPU) dan layout teks. Tidak menyentuh GPU: glyph
//! baru masuk `uploads`, dan `gpu` menyalinnya ke tekstur sebelum frame dikirim.

use std::collections::HashMap;

use ab_glyph::{point, Font as _, FontVec, PxScale, ScaleFont as _};

use crate::gpu::Rgba;

pub const ATLAS: u32 = 2048;
/// Blok putih di pojok kiri atas atlas: rect polos memakai tekstur yang sama
/// dengan teks, jadi satu draw call cukup.
const WHITE: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Face {
    // IBM Plex (tertanam): antarmuka dan isi catatan.
    Regular,
    Bold,
    Italic,
    Mono,
    Medium,
    Semi,
    // Font sistem bergaya Arial: diagram PlantUML, supaya ukuran teksnya
    // sama dengan PlantUML asli.
    Dia,
    DiaBold,
    DiaItalic,
    DiaMono,
}

impl Face {
    /// Plex: tinggi baris dihitung seperti CSS (kelipatan em).
    pub fn css(self) -> bool {
        (self as usize) < Face::Dia as usize
    }
}

/// IBM Plex, urut sesuai `Face::Regular..=Face::Semi`.
const PLEX: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Italic.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
];

fn plex() -> Result<Vec<FontVec>, String> {
    PLEX.iter()
        .map(|d| FontVec::try_from_vec(d.to_vec()).map_err(|e| e.to_string()))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Style {
    pub face: Face,
    pub px: f32,
    pub color: Rgba,
    pub bg: Option<Rgba>,
    pub under: bool,
    pub strike: bool,
    /// 0 = bukan link, selain itu indeks + 1.
    pub link: u16,
}

impl Style {
    pub fn new(face: Face, px: f32, color: Rgba) -> Style {
        Style {
            face,
            px,
            color,
            bg: None,
            under: false,
            strike: false,
            link: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub left: f32,
    /// Jarak dari baseline ke tepi atas bitmap.
    pub top: f32,
}

pub struct Upload {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

pub struct Fonts {
    faces: Vec<FontVec>,
    glyphs: HashMap<(Face, u16, char), Option<Glyph>>,
    advs: HashMap<(Face, u16, char), f32>,
    /// (x, y, tinggi baris) rak atlas yang sedang diisi.
    pen: (u32, u32, u32),
    pub uploads: Vec<Upload>,
    cleared: bool,
}

fn key_px(px: f32) -> u16 {
    px.round().clamp(1.0, 1000.0) as u16
}

#[cfg(windows)]
const CANDIDATES: [&[&str]; 4] = [
    &["C:/Windows/Fonts/segoeui.ttf", "C:/Windows/Fonts/arial.ttf"],
    &[
        "C:/Windows/Fonts/segoeuib.ttf",
        "C:/Windows/Fonts/arialbd.ttf",
    ],
    &[
        "C:/Windows/Fonts/segoeuii.ttf",
        "C:/Windows/Fonts/ariali.ttf",
    ],
    &["C:/Windows/Fonts/consola.ttf", "C:/Windows/Fonts/cour.ttf"],
];
#[cfg(target_os = "macos")]
const CANDIDATES: [&[&str]; 4] = [
    &["/System/Library/Fonts/Supplemental/Arial.ttf"],
    &["/System/Library/Fonts/Supplemental/Arial Bold.ttf"],
    &["/System/Library/Fonts/Supplemental/Arial Italic.ttf"],
    &["/System/Library/Fonts/Supplemental/Courier New.ttf"],
];
#[cfg(not(any(windows, target_os = "macos", target_arch = "wasm32")))]
const CANDIDATES: [&[&str]; 4] = [
    &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/system/fonts/Roboto-Regular.ttf",
    ],
    &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/system/fonts/Roboto-Bold.ttf",
    ],
    &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
        "/system/fonts/Roboto-Italic.ttf",
    ],
    &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/system/fonts/DroidSansMono.ttf",
    ],
];

impl Fonts {
    /// Font sistem. Varian yang tidak ada jatuh ke Regular; tanpa Regular gagal.
    // ponytail: tanpa fallback per-karakter (emoji, CJK tampil sebagai kotak).
    // Tambahkan rantai font cadangan di `glyph`/`adv` bila dibutuhkan.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load() -> Result<Fonts, String> {
        let load = |paths: &[&str]| {
            paths.iter().find_map(|p| {
                std::fs::read(p)
                    .ok()
                    .and_then(|d| FontVec::try_from_vec(d).ok())
            })
        };
        let mut faces = plex()?;
        for c in CANDIDATES {
            // Tanpa font sistem: diagram memakai Plex.
            match load(c).or_else(|| load(CANDIDATES[0])) {
                Some(f) => faces.push(f),
                None => faces.push(FontVec::try_from_vec(PLEX[faces.len() - 6].to_vec()).map_err(|e| e.to_string())?),
            }
        }
        Ok(Fonts::with_faces(faces))
    }

    /// Browser tidak memberi akses ke berkas font sistem: Noto Sans (OFL)
    /// ditanam di wasm.
    #[cfg(target_arch = "wasm32")]
    pub fn load() -> Result<Fonts, String> {
        let faces = [
            &include_bytes!("../web/fonts/NotoSans-Regular.ttf")[..],
            include_bytes!("../web/fonts/NotoSans-Bold.ttf"),
            include_bytes!("../web/fonts/NotoSans-Italic.ttf"),
            include_bytes!("../web/fonts/NotoSansMono-Regular.ttf"),
        ];
        let mut out = plex()?;
        for d in faces {
            out.push(FontVec::try_from_vec(d.to_vec()).map_err(|e| e.to_string())?);
        }
        Ok(Fonts::with_faces(out))
    }

    fn with_faces(faces: Vec<FontVec>) -> Fonts {
        let mut f = Fonts {
            faces,
            glyphs: HashMap::new(),
            advs: HashMap::new(),
            pen: (0, 0, 0),
            uploads: Vec::new(),
            cleared: false,
        };
        f.reset();
        f
    }

    /// `px` = ukuran em (seperti CSS `font-size`). PxScale ab_glyph adalah
    /// tinggi ascent−descent, jadi dikonversi lewat units-per-em.
    fn scale(&self, f: Face, px: u16) -> PxScale {
        let font = &self.faces[f as usize];
        PxScale::from(px as f32 * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0))
    }

    /// Kosongkan atlas (tekstur GPU baru, atau atlas penuh). Glyph dirasterisasi
    /// ulang saat dipakai lagi.
    pub fn reset(&mut self) {
        self.glyphs.clear();
        self.uploads.clear();
        self.uploads.push(Upload {
            x: 0,
            y: 0,
            w: WHITE,
            h: WHITE,
            data: vec![255; (WHITE * WHITE) as usize],
        });
        self.pen = (WHITE + 1, 0, WHITE);
    }

    /// Atlas dikosongkan di tengah frame: frame ini mungkin memakai glyph
    /// lama, jadi gambar ulang sekali lagi.
    pub fn take_cleared(&mut self) -> bool {
        std::mem::take(&mut self.cleared)
    }

    /// (ascent, descent) dalam piksel, keduanya positif.
    pub fn metrics(&self, f: Face, px: f32) -> (f32, f32) {
        let sf = self.faces[f as usize].as_scaled(self.scale(f, key_px(px)));
        (sf.ascent(), -sf.descent())
    }

    pub fn adv(&mut self, f: Face, px: f32, c: char) -> f32 {
        match c {
            '\t' => return 4.0 * self.adv(f, px, ' '),
            '\n' | '\r' => return 0.0,
            _ => {}
        }
        let k = (f, key_px(px), c);
        if let Some(&a) = self.advs.get(&k) {
            return a;
        }
        let font = &self.faces[f as usize];
        let a = font
            .as_scaled(self.scale(f, k.1))
            .h_advance(font.glyph_id(c));
        self.advs.insert(k, a);
        a
    }

    pub fn width(&mut self, f: Face, px: f32, s: &str) -> f32 {
        s.chars().map(|c| self.adv(f, px, c)).sum()
    }

    pub fn glyph(&mut self, f: Face, px: f32, c: char) -> Option<Glyph> {
        let k = (f, key_px(px), c);
        if let Some(&g) = self.glyphs.get(&k) {
            return g;
        }
        let font = &self.faces[f as usize];
        let outline = font.outline_glyph(
            font.glyph_id(c)
                .with_scale_and_position(self.scale(f, k.1), point(0.0, 0.0)),
        );
        let bitmap = outline.map(|o| {
            let b = o.px_bounds();
            let (w, h) = (b.width() as u32, b.height() as u32);
            let mut data = vec![0u8; (w * h) as usize];
            o.draw(|x, y, cov| {
                if let Some(px) = data.get_mut((y * w + x) as usize) {
                    *px = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            });
            (w, h, b.min.x, -b.min.y, data)
        });
        let g = match bitmap {
            Some((w, h, ..)) if w == 0 || h == 0 || w >= ATLAS || h >= ATLAS => None,
            None => None,
            Some((w, h, left, top, data)) => {
                if self.pen.0 + w + 1 > ATLAS {
                    self.pen = (0, self.pen.1 + self.pen.2 + 1, 0);
                }
                if self.pen.1 + h + 1 > ATLAS {
                    // ponytail: atlas penuh → kosongkan semua. Cukup untuk teks
                    // biasa; ganti dengan LRU per rak bila banyak ukuran font dipakai.
                    self.reset();
                    self.cleared = true;
                    self.pen = (0, WHITE + 1, 0);
                }
                let g = Glyph {
                    x: self.pen.0,
                    y: self.pen.1,
                    w,
                    h,
                    left,
                    top,
                };
                self.uploads.push(Upload {
                    x: g.x,
                    y: g.y,
                    w,
                    h,
                    data,
                });
                self.pen.0 += w + 1;
                self.pen.2 = self.pen.2.max(h);
                Some(g)
            }
        };
        self.glyphs.insert(k, g);
        g
    }
}

#[derive(Clone, Copy, Debug)]
pub struct G {
    /// Offset byte di teks yang di-layout.
    pub i: usize,
    pub c: char,
    pub x: f32,
    pub adv: f32,
    /// Indeks ke `Lay::styles`.
    pub st: u16,
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub top: f32,
    pub h: f32,
    pub base: f32,
    pub w: f32,
    pub gs: Vec<G>,
}

#[derive(Clone, Debug, Default)]
pub struct Lay {
    pub rows: Vec<Row>,
    pub styles: Vec<Style>,
    pub h: f32,
    pub w: f32,
}

/// Bungkus `text` ke lebar `width`. `runs` = (akhir byte, gaya), berurutan,
/// minimal satu (gaya itu juga menentukan tinggi baris kosong). `lh` =
/// pengali tinggi baris. Kata yang lebih panjang dari satu baris dipotong
/// per karakter; spasi boleh menggantung melewati tepi.
pub fn layout(f: &mut Fonts, text: &str, runs: &[(usize, Style)], width: f32, lh: f32) -> Lay {
    let styles: Vec<Style> = runs.iter().map(|r| r.1).collect();
    if styles.is_empty() {
        return Lay::default();
    }
    let mut rows = Vec::new();
    let mut gs: Vec<G> = Vec::new();
    let (mut row_start, mut x, mut brk, mut r) = (0, 0.0f32, None::<usize>, 0);
    for (i, c) in text.char_indices() {
        while r + 1 < runs.len() && i >= runs[r].0 {
            r += 1;
        }
        let st = styles[r];
        if c == '\n' {
            gs.push(G {
                i,
                c,
                x,
                adv: 0.0,
                st: r as u16,
            });
            rows.push(Row {
                start: row_start,
                end: i + 1,
                gs: std::mem::take(&mut gs),
                ..Default::default()
            });
            (row_start, x, brk) = (i + 1, 0.0, None);
            continue;
        }
        let a = f.adv(st.face, st.px, c);
        if x + a > width && !gs.is_empty() && !c.is_whitespace() {
            let rest = match brk {
                Some(b) => gs.split_off(b + 1),
                None => Vec::new(),
            };
            let end = rest.first().map_or(i, |g| g.i);
            rows.push(Row {
                start: row_start,
                end,
                gs: std::mem::take(&mut gs),
                ..Default::default()
            });
            row_start = end;
            let dx = rest.first().map_or(0.0, |g| g.x);
            gs = rest.into_iter().map(|g| G { x: g.x - dx, ..g }).collect();
            x = gs.last().map_or(0.0, |g| g.x + g.adv);
            brk = gs.iter().rposition(|g| g.c == ' ' || g.c == '\t');
        }
        gs.push(G {
            i,
            c,
            x,
            adv: a,
            st: r as u16,
        });
        x += a;
        if c == ' ' || c == '\t' {
            brk = Some(gs.len() - 1);
        }
    }
    rows.push(Row {
        start: row_start,
        end: text.len(),
        gs,
        ..Default::default()
    });

    let sm: Vec<(f32, f32)> = styles.iter().map(|s| f.metrics(s.face, s.px)).collect();
    // Tinggi kotak baris per gaya: Plex seperti CSS (`lh` × em); font diagram
    // `lh` × (ascent + descent), sesuai kalibrasi PlantUML.
    let lb: Vec<f32> = styles
        .iter()
        .zip(&sm)
        .map(|(s, m)| if s.face.css() { s.px * lh } else { (m.0 + m.1) * lh })
        .collect();
    let (mut y, mut w) = (0.0f32, 0.0f32);
    for row in &mut rows {
        let (mut asc, mut desc, mut bh) = (0.0f32, 0.0f32, 0.0f32);
        for g in &row.gs {
            let m = sm[g.st as usize];
            asc = asc.max(m.0);
            desc = desc.max(m.1);
            bh = bh.max(lb[g.st as usize]);
        }
        if row.gs.is_empty() {
            let ri = runs
                .iter()
                .position(|r| row.start < r.0)
                .unwrap_or(runs.len() - 1);
            (asc, desc) = sm[ri];
            bh = lb[ri];
        }
        let th = asc + desc;
        row.h = bh.ceil();
        row.top = y;
        row.base = (y + (row.h - th) / 2.0 + asc).round();
        row.w = row.gs.last().map_or(0.0, |g| g.x + g.adv);
        w = w.max(row.w);
        y += row.h;
    }
    Lay {
        rows,
        styles,
        h: y,
        w,
    }
}

impl Lay {
    fn row_at(&self, y: f32) -> usize {
        self.rows
            .iter()
            .position(|r| y < r.top + r.h)
            .unwrap_or(self.rows.len().saturating_sub(1))
    }

    /// Posisi cursor terdekat dari titik (x, y) relatif terhadap layout.
    pub fn pos_at(&self, x: f32, y: f32) -> usize {
        let k = self.row_at(y);
        let Some(r) = self.rows.get(k) else { return 0 };
        for g in &r.gs {
            if g.c == '\n' || x < g.x + g.adv / 2.0 {
                return g.i;
            }
        }
        // Baris yang dibungkus berakhir dengan spasi: cursor di depan spasi
        // itu, supaya tetap tampil di baris yang diklik.
        match r.gs.last() {
            Some(g) if k + 1 < self.rows.len() && g.c.is_whitespace() => g.i,
            _ => r.end,
        }
    }

    /// (x, atas baris, tinggi baris) untuk cursor di `pos`.
    pub fn caret(&self, pos: usize) -> (f32, f32, f32) {
        let n = self.rows.len();
        for (k, r) in self.rows.iter().enumerate() {
            if pos < r.end || k + 1 == n {
                let x = r.gs.iter().find(|g| g.i >= pos).map_or(r.w, |g| g.x);
                return (x, r.top, r.h);
            }
        }
        (0.0, 0.0, 0.0)
    }

    /// Glyph tepat di bawah titik, bila ada.
    pub fn glyph_at(&self, x: f32, y: f32) -> Option<&G> {
        if y < 0.0 || y >= self.h {
            return None;
        }
        self.rows[self.row_at(y)]
            .gs
            .iter()
            .find(|g| x >= g.x && x < g.x + g.adv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_spaces_and_maps_positions_both_ways() {
        let Ok(mut f) = Fonts::load() else { return }; // tanpa font sistem: lewati
        let st = Style::new(Face::Regular, 16.0, [0.0; 4]);
        let space = f.adv(Face::Regular, 16.0, ' ');
        let text = "aaaa aaaa aaaa"; // huruf sama: lebar tiap kata persis w4
        let w4 = f.width(Face::Regular, 16.0, "aaaa");
        let lay = layout(
            &mut f,
            text,
            &[(text.len(), st)],
            w4 * 2.0 + space * 1.5,
            1.5,
        );
        assert_eq!(lay.rows.len(), 2);
        assert_eq!(
            (lay.rows[0].start, lay.rows[0].end, lay.rows[1].start),
            (0, 10, 10)
        );
        assert_eq!(lay.rows[1].gs[0].x, 0.0);

        // Klik di kanan baris pertama → di depan spasi penutupnya, bukan awal baris 2.
        let r0 = &lay.rows[0];
        assert_eq!(lay.pos_at(1e6, r0.top + 1.0), 9);
        assert_eq!(lay.pos_at(1e6, lay.h - 1.0), text.len());
        for pos in [0, 3, 9, 10, 12, 14] {
            let (x, top, h) = lay.caret(pos);
            assert_eq!(lay.pos_at(x + 0.1, top + h / 2.0), pos, "pos {pos}");
        }

        // Kata yang lebih panjang dari lebar dipotong per karakter, tanpa loop tak berujung.
        let long = "x".repeat(50);
        let lay = layout(&mut f, &long, &[(long.len(), st)], w4, 1.5);
        assert!(lay.rows.len() > 5 && lay.rows.iter().all(|r| !r.gs.is_empty()));

        let lay = layout(&mut f, "", &[(0, st)], 100.0, 1.5);
        assert_eq!(lay.rows.len(), 1);
        assert!(lay.h > 16.0);
        let lay = layout(&mut f, "a\n\nb", &[(4, st)], 100.0, 1.5);
        assert_eq!(lay.rows.len(), 3);
        assert_eq!(lay.caret(2).1, lay.rows[1].top);
    }
}
