//! Font, atlas glyph (sisi CPU) dan layout teks. Tidak menyentuh GPU: glyph
//! baru masuk `uploads`, dan `gpu` menyalinnya ke tekstur sebelum frame dikirim.

use std::collections::HashMap;

use ab_glyph::{point, Font, FontRef, FontVec, GlyphId, PxScale, ScaleFont as _};

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

/// DigitalKhatt (Madina, OFL): huruf Arab, di-shape dengan rustybuzz.
const KHATT: &[u8] = include_bytes!("../assets/fonts/DigitalKhatt-Madina.otf");
/// Huruf Arab tampil lebih kecil dari Latin pada em yang sama: diperbesar.
const AR_SCALE: f32 = 1.3;

/// Huruf yang digambar dengan DigitalKhatt (blok Arab dan bentuk presentasi).
pub fn is_arabic(c: char) -> bool {
    matches!(c as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0x0870..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF)
}

/// Satu glyph hasil shaping: id, cluster (byte relatif kata), advance dan
/// offset dalam piksel (dy ke atas).
#[derive(Clone, Copy)]
struct Shaped {
    gid: u16,
    cl: usize,
    adv: f32,
    dx: f32,
    dy: f32,
}

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
    ar: FontRef<'static>,
    hb: rustybuzz::Face<'static>,
    /// Kata Arab yang sudah di-shape, per ukuran.
    // ponytail: tidak pernah dikosongkan; sebesar kosakata Arab yang pernah tampil.
    shaped: HashMap<(u16, String), Vec<Shaped>>,
    ar_glyphs: HashMap<(u16, u16), Option<Glyph>>,
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
            ar: FontRef::try_from_slice(KHATT).expect("DigitalKhatt"),
            hb: rustybuzz::Face::from_slice(KHATT, 0).expect("DigitalKhatt"),
            shaped: HashMap::new(),
            ar_glyphs: HashMap::new(),
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
        self.ar_glyphs.clear();
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
        let g = self.place(raster(font, font.glyph_id(c), self.scale(f, k.1)));
        self.glyphs.insert(k, g);
        g
    }

    /// Glyph DigitalKhatt menurut id (hasil shaping) untuk teks ukuran `px`.
    fn ar_glyph(&mut self, px: f32, gid: u16) -> Option<Glyph> {
        let k = (key_px(px * AR_SCALE), gid);
        if let Some(&g) = self.ar_glyphs.get(&k) {
            return g;
        }
        let f = &self.ar;
        let scale = PxScale::from(k.0 as f32 * f.height_unscaled() / f.units_per_em().unwrap_or(1000.0));
        let g = self.place(raster(f, GlyphId(gid), scale));
        self.ar_glyphs.insert(k, g);
        g
    }

    /// Glyph untuk `g` di layout (huruf biasa atau hasil shaping) dan geser
    /// (dx, dy ke atas) dari posisinya.
    pub fn glyph_g(&mut self, st: &Style, g: &G) -> Option<(Glyph, f32, f32)> {
        match g.ar {
            Some((0, ..)) => None,
            Some((gid, dx, dy)) => self.ar_glyph(st.px, gid).map(|gl| (gl, dx, dy)),
            None if g.c.is_whitespace() => None,
            None => self.glyph(st.face, st.px, g.c).map(|gl| (gl, 0.0, 0.0)),
        }
    }

    /// Shape satu kata Arab (kanan-ke-kiri); glyph urut visual, kiri ke kanan.
    fn shape_ar(&mut self, px: f32, word: &str) -> Vec<Shaped> {
        let k = (key_px(px * AR_SCALE), word.to_owned());
        if let Some(v) = self.shaped.get(&k) {
            return v.clone();
        }
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(word);
        buf.set_direction(rustybuzz::Direction::RightToLeft);
        let out = rustybuzz::shape(&self.hb, &[], buf);
        let u = k.0 as f32 / self.hb.units_per_em() as f32;
        let v: Vec<Shaped> = out
            .glyph_infos()
            .iter()
            .zip(out.glyph_positions())
            .map(|(i, p)| Shaped {
                gid: i.glyph_id as u16,
                cl: i.cluster as usize,
                adv: p.x_advance as f32 * u,
                dx: p.x_offset as f32 * u,
                dy: p.y_offset as f32 * u,
            })
            .collect();
        self.shaped.insert(k, v.clone());
        v
    }

    /// Simpan bitmap ke atlas.
    fn place(&mut self, bitmap: Option<(u32, u32, f32, f32, Vec<u8>)>) -> Option<Glyph> {
        match bitmap {
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
        }
    }
}

/// Coverage 8-bit glyph: (lebar, tinggi, kiri, atas dari baseline, data).
fn raster(font: &impl Font, id: GlyphId, scale: PxScale) -> Option<(u32, u32, f32, f32, Vec<u8>)> {
    let o = font.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0)))?;
    let b = o.px_bounds();
    let (w, h) = (b.width() as u32, b.height() as u32);
    let mut data = vec![0u8; (w * h) as usize];
    o.draw(|x, y, cov| {
        if let Some(px) = data.get_mut((y * w + x) as usize) {
            *px = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    });
    Some((w, h, b.min.x, -b.min.y, data))
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
    /// Glyph hasil shaping Arab: (id, dx, dy ke atas). Id 0 = huruf yang
    /// sudah tergambar oleh glyph lain (ligatur, harakat dalam cluster).
    pub ar: Option<(u16, f32, f32)>,
    /// Kata hasil shaping (1, 2, …) — satu unit saat diurutkan kanan-ke-kiri.
    pub w: u32,
    /// Tampil kanan-ke-kiri: cursor di sisi kanan glyph.
    pub r: bool,
}

impl G {
    fn new(i: usize, c: char, x: f32, adv: f32, st: usize) -> G {
        G {
            i,
            c,
            x,
            adv,
            st: st as u16,
            ar: None,
            w: 0,
            r: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub top: f32,
    pub h: f32,
    pub base: f32,
    pub w: f32,
    /// Urut visual (kiri ke kanan), bukan urut teks.
    pub gs: Vec<G>,
    /// Paragraf kanan-ke-kiri (huruf kuat pertamanya Arab).
    pub rtl: bool,
}

impl Row {
    /// Posisi x cursor di depan byte `i` (urut teks) pada baris ini.
    pub fn x_at(&self, i: usize) -> f32 {
        let g = self
            .gs
            .iter()
            .filter(|g| g.i >= i)
            .min_by(|a, b| a.i.cmp(&b.i).then((b.adv > 0.0).cmp(&(a.adv > 0.0))));
        match g {
            Some(g) if g.r => g.x + g.adv,
            Some(g) => g.x,
            None if self.rtl => self.gs.iter().map(|g| g.x).fold(self.w, f32::min),
            None => self.w,
        }
    }

    /// Byte sesudah glyph `g` dalam urut teks.
    fn after(&self, g: &G) -> usize {
        self.gs.iter().map(|h| h.i).filter(|&i| i > g.i).min().unwrap_or(self.end)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Lay {
    pub rows: Vec<Row>,
    pub styles: Vec<Style>,
    pub h: f32,
    pub w: f32,
}

/// Paragraf (sampai `\n`) mulai di `start` berarah kanan-ke-kiri bila huruf
/// kuat pertamanya Arab.
fn para_rtl(text: &str, start: usize) -> bool {
    text[start..]
        .chars()
        .take_while(|&c| c != '\n')
        .find(|&c| is_arabic(c) || c.is_alphabetic())
        .is_some_and(is_arabic)
}

/// Urutkan glyph satu baris secara visual (aturan bidi disederhanakan:
/// kata Arab = kanan-ke-kiri, huruf/angka lain = kiri-ke-kanan, tanda baca
/// dan spasi ikut tetangganya), lalu hitung ulang x. Paragraf kanan-ke-kiri
/// rata kanan bila `width` terbatas.
// ponytail: bukan UBA penuh (tanpa mirroring kurung, angka Arab-Indic dianggap
// huruf Arab). Ganti dengan crate unicode-bidi bila teks campuran rumit salah urut.
fn reorder(gs: &mut Vec<G>, rtl: bool, width: f32) {
    if !rtl && gs.iter().all(|g| g.ar.is_none()) {
        return;
    }
    let nl = (gs.last().is_some_and(|g| g.c == '\n')).then(|| gs.pop()).flatten();
    // Unit: satu kata Arab hasil shaping, atau satu huruf lain.
    let mut units: Vec<(usize, usize, i8)> = Vec::new(); // (awal, akhir, arah 1=R -1=L 0=netral)
    let mut k = 0;
    while k < gs.len() {
        let mut e = k + 1;
        let d = if gs[k].w > 0 {
            while e < gs.len() && gs[e].w == gs[k].w {
                e += 1;
            }
            1
        } else if gs[k].c.is_alphanumeric() {
            -1
        } else {
            0
        };
        units.push((k, e, d));
        k = e;
    }
    let pd = if rtl { 1 } else { -1 };
    let strong: Vec<i8> = units.iter().map(|u| u.2).collect();
    for (n, u) in units.iter_mut().enumerate() {
        if u.2 == 0 {
            let prev = strong[..n].iter().rev().find(|&&d| d != 0);
            let next = strong[n + 1..].iter().find(|&&d| d != 0);
            u.2 = match (prev, next) {
                (Some(a), Some(b)) if a == b => *a,
                _ => pd,
            };
        }
    }
    // Level: R = 1; L = 0 (paragraf kiri-ke-kanan) atau 2. Aturan L2: balik
    // tiap deret unit dengan level ≥ n, dari level tertinggi ke 1.
    let lv: Vec<u8> = units.iter().map(|u| if u.2 == 1 { 1 } else if rtl { 2 } else { 0 }).collect();
    let mut order: Vec<usize> = (0..units.len()).collect();
    for n in (1..=2u8).rev() {
        let mut a = 0;
        while a < order.len() {
            if lv[order[a]] < n {
                a += 1;
                continue;
            }
            let mut b = a;
            while b < order.len() && lv[order[b]] >= n {
                b += 1;
            }
            order[a..b].reverse();
            a = b;
        }
    }
    let mut out = Vec::with_capacity(gs.len() + 1);
    let mut pen = 0.0;
    for &u in &order {
        let (a, b, _) = units[u];
        let x0 = gs[a].x;
        for g in &gs[a..b] {
            out.push(G {
                x: pen + g.x - x0,
                r: lv[u] % 2 == 1,
                ..*g
            });
        }
        pen += gs[a..b].iter().map(|g| g.adv).sum::<f32>();
    }
    let shift = if rtl && width < 1e6 { (width - pen).max(0.0) } else { 0.0 };
    for g in &mut out {
        g.x += shift;
    }
    if let Some(g) = nl {
        out.push(G {
            x: if rtl { shift } else { pen },
            ..g
        });
    }
    *gs = out;
}

/// Bungkus `text` ke lebar `width`. `runs` = (akhir byte, gaya), berurutan,
/// minimal satu (gaya itu juga menentukan tinggi baris kosong). `lh` =
/// pengali tinggi baris. Kata yang lebih panjang dari satu baris dipotong
/// per karakter; spasi boleh menggantung melewati tepi. Kata Arab di-shape
/// utuh dengan DigitalKhatt dan tidak dipotong.
pub fn layout(f: &mut Fonts, text: &str, runs: &[(usize, Style)], width: f32, lh: f32) -> Lay {
    let styles: Vec<Style> = runs.iter().map(|r| r.1).collect();
    if styles.is_empty() {
        return Lay::default();
    }
    let mut rows = Vec::new();
    let mut gs: Vec<G> = Vec::new();
    let (mut row_start, mut x, mut brk, mut r) = (0, 0.0f32, None::<usize>, 0);
    let mut word = 0u32;
    // Pindahkan isi baris sampai titik putus terakhir ke baris baru.
    let wrap = |rows: &mut Vec<Row>, gs: &mut Vec<G>, brk: &mut Option<usize>, row_start: &mut usize, x: &mut f32, i: usize| {
        let rest = match *brk {
            Some(b) => gs.split_off(b + 1),
            None => Vec::new(),
        };
        let end = rest.first().map_or(i, |g| g.i);
        rows.push(Row {
            start: *row_start,
            end,
            gs: std::mem::take(gs),
            ..Default::default()
        });
        *row_start = end;
        let dx = rest.first().map_or(0.0, |g| g.x);
        *gs = rest.into_iter().map(|g| G { x: g.x - dx, ..g }).collect();
        *x = gs.iter().map(|g| g.x + g.adv).fold(0.0, f32::max);
        *brk = gs.iter().rposition(|g| g.c == ' ' || g.c == '\t');
    };
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        while r + 1 < runs.len() && i >= runs[r].0 {
            r += 1;
        }
        let st = styles[r];
        if c == '\n' {
            gs.push(G::new(i, c, x, 0.0, r));
            rows.push(Row {
                start: row_start,
                end: i + 1,
                gs: std::mem::take(&mut gs),
                ..Default::default()
            });
            (row_start, x, brk) = (i + 1, 0.0, None);
            continue;
        }
        if is_arabic(c) {
            // Satu kata: huruf Arab berurutan dalam gaya yang sama.
            let run_end = if r + 1 < runs.len() { runs[r].0 } else { text.len() };
            let mut j = i + c.len_utf8();
            while let Some(&(k, d)) = it.peek() {
                if k >= run_end || !is_arabic(d) {
                    break;
                }
                j = k + d.len_utf8();
                it.next();
            }
            let sh = f.shape_ar(st.px, &text[i..j]);
            let ww: f32 = sh.iter().map(|g| g.adv).sum();
            if x + ww > width && !gs.is_empty() {
                wrap(&mut rows, &mut gs, &mut brk, &mut row_start, &mut x, i);
            }
            word += 1;
            let mut pen = x;
            let mut at: Vec<(usize, f32)> = Vec::new(); // (cluster, x) glyph pertama tiap cluster
            for g in &sh {
                let ch = text[i + g.cl..].chars().next().unwrap_or(' ');
                gs.push(G {
                    ar: Some((g.gid, g.dx, g.dy)),
                    w: word,
                    ..G::new(i + g.cl, ch, pen, g.adv, r)
                });
                if !at.iter().any(|a| a.0 == g.cl) {
                    at.push((g.cl, pen));
                }
                pen += g.adv;
            }
            // Huruf yang tidak memulai cluster (ligatur, harakat): tanpa glyph
            // sendiri, tapi tetap ada supaya teks bisa dipilih dan dicari.
            for (k, ch) in text[i..j].char_indices() {
                if !at.iter().any(|a| a.0 == k) {
                    let px = at.iter().filter(|a| a.0 < k).max_by_key(|a| a.0).map_or(x, |a| a.1);
                    gs.push(G {
                        ar: Some((0, 0.0, 0.0)),
                        w: word,
                        ..G::new(i + k, ch, px, 0.0, r)
                    });
                }
            }
            x = pen;
            continue;
        }
        let a = f.adv(st.face, st.px, c);
        if x + a > width && !gs.is_empty() && !c.is_whitespace() {
            wrap(&mut rows, &mut gs, &mut brk, &mut row_start, &mut x, i);
        }
        gs.push(G::new(i, c, x, a, r));
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

    let mut para = (usize::MAX, false);
    for row in &mut rows {
        let p = text[..row.start].rfind('\n').map_or(0, |k| k + 1);
        if para.0 != p {
            para = (p, para_rtl(text, p));
        }
        row.rtl = para.1;
        reorder(&mut row.gs, row.rtl, width);
    }

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
        if row.rtl {
            bh *= 1.25; // harakat di atas/bawah huruf Arab butuh ruang
        }
        let th = asc + desc;
        row.h = bh.ceil();
        row.top = y;
        row.base = (y + (row.h - th) / 2.0 + asc).round();
        row.w = row.gs.iter().map(|g| g.x + g.adv).fold(0.0, f32::max);
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
            if g.c == '\n' {
                return g.i;
            }
            if g.adv <= 0.0 {
                continue;
            }
            // Glyph kanan-ke-kiri: separuh kiri = sesudahnya dalam urut teks.
            if x < g.x + g.adv / 2.0 {
                return if g.r { r.after(g) } else { g.i };
            }
            if g.r && x < g.x + g.adv {
                return g.i;
            }
        }
        // Baris yang dibungkus berakhir dengan spasi: cursor di depan spasi
        // itu, supaya tetap tampil di baris yang diklik.
        match r.gs.last() {
            Some(g) if k + 1 < self.rows.len() && g.c.is_whitespace() && !r.rtl => g.i,
            _ => r.end,
        }
    }

    /// (x, atas baris, tinggi baris) untuk cursor di `pos`.
    pub fn caret(&self, pos: usize) -> (f32, f32, f32) {
        let n = self.rows.len();
        for (k, r) in self.rows.iter().enumerate() {
            if pos < r.end || k + 1 == n {
                return (r.x_at(pos), r.top, r.h);
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
    fn arabic_is_shaped_and_ordered_right_to_left() {
        let Ok(mut f) = Fonts::load() else { return };
        let st = Style::new(Face::Regular, 16.0, [0.0; 4]);
        let xs = |lay: &Lay, s: &str, text: &str| {
            let i = text.find(s).unwrap();
            lay.rows[0].gs.iter().find(|g| g.i == i).unwrap().x
        };

        // Paragraf Latin: kata Arab berurutan dibaca kanan-ke-kiri.
        let text = "ab سلام عليكم cd";
        let lay = layout(&mut f, text, &[(text.len(), st)], 1000.0, 1.5);
        assert!(lay.rows[0].gs.iter().any(|g| g.ar.is_some_and(|a| a.0 != 0)));
        assert!(xs(&lay, "ab", text) < xs(&lay, "عليكم", text));
        assert!(xs(&lay, "عليكم", text) < xs(&lay, "سلام", text));
        assert!(xs(&lay, "سلام", text) < xs(&lay, "cd", text));
        assert!(!lay.rows[0].rtl);

        // Paragraf Arab: rata kanan, kata Latin di dalamnya tetap kiri-ke-kanan.
        let text = "سلام hello world";
        let lay = layout(&mut f, text, &[(text.len(), st)], 1000.0, 1.5);
        assert!(lay.rows[0].rtl);
        assert!(xs(&lay, "hello", text) < xs(&lay, "world", text));
        assert!(xs(&lay, "world", text) < xs(&lay, "سلام", text));
        assert!((lay.w - 1000.0).abs() < 1.0);

        // Cursor: setiap batas kata bolak-balik caret ↔ klik.
        for text in ["ab سلام عليكم cd", "سلام hello world"] {
            let lay = layout(&mut f, text, &[(text.len(), st)], 1000.0, 1.5);
            for (pos, _) in text.match_indices(' ') {
                for p in [pos, pos + 1] {
                    let (x, top, h) = lay.caret(p);
                    let back = lay.pos_at(x + 0.1, top + h / 2.0);
                    let back2 = lay.pos_at(x - 0.1, top + h / 2.0);
                    assert!(back == p || back2 == p, "{text:?} pos {p}: {back}/{back2}");
                }
            }
        }
    }

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
