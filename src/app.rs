//! Shell winit dan seluruh UI. Satu-satunya tempat yang tahu soal event
//! platform: semuanya diterjemahkan ke perintah `Doc` dan `Store`.
//!
//! Koordinat dalam piksel fisik; ukuran desain (px logis) dikali `s`.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use web_time::{Instant, SystemTime};

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::doc::Doc;
use crate::gpu::{Gpu, Rect, Rgba};
use crate::md::{self, BKind, Kind, Tok};
use crate::puml_render::{puml_col, puml_lay};
use crate::store::{self, Store};
use crate::sync::{self, Sync};
use crate::text::{layout, Face, Fonts, Lay, Row, Style, ATLAS};

const BODY: f32 = 16.0;
const LH: f32 = 1.5;
// Ukuran dalam px CSS desain (× skala layar).
const SIDE_W: f32 = 280.0;
const BAR_H: f32 = 48.0;
/// Baris daftar catatan: 80 + jarak 2.
const ROW_H: f32 = 82.0;
/// Padding kolom konten: atas, bawah, samping.
const TOP: f32 = 48.0;
const BOTTOM: f32 = 96.0;
const GUTTER: f32 = 32.0;
/// Simpan 1 detik setelah berhenti mengetik, paling lambat 5 detik setelah
/// perubahan pertama (PRD §5).
const SAVE_DELAY: Duration = Duration::from_millis(1000);
const SAVE_MAX: Duration = Duration::from_secs(5);
/// Sync: kirim sekian detik setelah simpan; tarik berkala selama jendela aktif.
const SYNC_DELAY: Duration = Duration::from_secs(3);
const SYNC_POLL: Duration = Duration::from_secs(60);

/// User event: GPU siap (web, async) atau jawaban server sync.
pub enum Ev {
    Gpu(Gpu),
    Sync(Result<String, String>),
    /// Tag rilis terbaru di GitHub (desktop).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Update(Result<String, String>),
    /// Exe baru sudah terpasang (Windows); tinggal restart.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Updated(Result<(), String>),
    /// Jawaban `/share` (true) atau `/unshare` untuk catatan id.
    Share(String, bool, Result<String, String>),
    /// Keyboard layar (web): 1 = preedit, 2 = teks, 3 = Enter, 4 = Backspace;
    /// 5 = isi berkas markdown yang di-drop.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Kb(u8, String),
    /// Gambar yang ditempel atau di-drop (web): nama, isi.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Img(String, Vec<u8>),
    /// Gambar selesai di-decode di thread latar (desktop).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Decoded(String, Option<Img>),
    /// Isi gambar `assets/…` selesai dibaca (web, lazy).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Asset(String),
}

/// Sentuhan satu jari di luar penampil: tap = klik, geser = gulir.
struct Tap {
    id: u64,
    start: (f32, f32),
    last: (f32, f32),
    moved: bool,
    t0: Instant,
    /// Ditahan diam ≥ `LONG_PRESS`: jari memilih teks, bukan menggulir.
    held: bool,
    /// Kecepatan gulir jari (px/detik, arah isi) dan waktu gerak terakhir.
    v: f32,
    tl: Instant,
}

/// Gulir beranimasi: sisa jarak roda mouse (dihabiskan halus) dan kecepatan
/// lemparan jari (melambat sendiri). `list` = daftar catatan, bukan isi.
struct Glide {
    list: bool,
    rest: f32,
    v: f32,
    t: Instant,
}

const LONG_PRESS: Duration = Duration::from_millis(450);

/// Ukuran heading editor relatif terhadap isi (# 24, ## 20, ### 18).
fn hscale(n: u8) -> f32 {
    [1.5, 1.25, 1.125, 1.0, 1.0, 1.0][(n.clamp(1, 6) - 1) as usize]
}

/// Tinggi baris editor per jenis (CSS desain: 32/24, 28/20, 26/18, 21/14).
fn ed_lh(k: Kind) -> f32 {
    match k {
        Kind::Heading(1) => 32.0 / 24.0,
        Kind::Heading(2) => 1.4,
        Kind::Heading(3) => 26.0 / 18.0,
        _ => LH,
    }
}

fn hex(v: u32) -> Rgba {
    [
        ((v >> 16) & 255) as f32 / 255.0,
        ((v >> 8) & 255) as f32 / 255.0,
        (v & 255) as f32 / 255.0,
        1.0,
    ]
}

fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

/// Token warna desain (`--bg`, `--side`, ...).
#[derive(Clone, Copy)]
struct Theme {
    dark: bool,
    bg: Rgba,
    side: Rgba,
    fg: Rgba,
    dim: Rgba,
    accent: Rgba,
    /// Sorotan seleksi teks (transparan).
    sel: Rgba,
    code_bg: Rgba,
    border: Rgba,
    /// `--sel`: baris terpilih, opsi aktif.
    row_sel: Rgba,
    err: Rgba,
    /// fg 6% / 11% di atas latar: hover / ditekan.
    hover: Rgba,
}

impl Theme {
    fn new(dark: bool) -> Theme {
        let (bg, side, fg, dim, border, code_bg, accent, row_sel, err) = if dark {
            (
                0x16181B, 0x1C1F23, 0xE6E8EB, 0x9AA1AA, 0x2E333A, 0x24282E, 0x6AA6F0, 0x1E2C3F,
                0xFF7B72,
            )
        } else {
            (
                0xFFFFFF, 0xF6F7F8, 0x1F2328, 0x5F6670, 0xE1E4E8, 0xF1F2F4, 0x1F6FD1, 0xE7EFFA,
                0xC42B1C,
            )
        };
        Theme {
            dark,
            bg: hex(bg),
            side: hex(side),
            fg: hex(fg),
            dim: hex(dim),
            accent: hex(accent),
            sel: alpha(hex(accent), if dark { 0.35 } else { 0.25 }),
            code_bg: hex(code_bg),
            border: hex(border),
            row_sel: hex(row_sel),
            err: hex(err),
            hover: alpha(hex(fg), 0.06),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Editor,
    Search,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Edit = 0,
    Read = 1,
}

/// Semua tombol yang bisa diklik; posisinya dari `App::hits`.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Btn {
    Side,
    Search,
    New,
    Clear,
    Settings,
    Mode,
    Delete,
    ZoomOut,
    ZoomIn,
    ZoomClose,
    /// Penampil: simpan gambar/diagram ke folder Downloads.
    ZoomSave,
    SetClose,
    Tab(u8),
    Theme(u8),
    FsDown,
    FsUp,
    Width(u8),
    OpenIn(u8),
    EmptyTrash,
    Restore(usize),
    Purge(usize),
    MatchPrev,
    MatchNext,
    /// Tab Sync: isian (0 = URL, 1 = token), Connect/Disconnect, Sync now.
    Field(u8),
    SyncConnect,
    SyncNow,
    /// Buat token acak (disalin ke clipboard) / buka halaman deploy server.
    GenToken,
    Deploy,
    /// Cek versi terbaru (desktop) / muat ulang build terbaru (web).
    Update,
    /// Salin kode koneksi (URL + token) untuk perangkat lain / tempel lalu connect.
    CopyCode,
    PasteCode,
    /// Salin URL MCP untuk Claude.
    CopyMcp,
    /// Toolbar: bagikan catatan (link baru), salin link, berhenti membagikan.
    Share,
    CopyLink,
    Unshare,
    /// Menu konteks (klik kanan / tekan lama): 0 Cut, 1 Copy, 2 Paste, 3 Select all.
    Menu(u8),
}

#[cfg(not(target_arch = "wasm32"))]
const RELEASES: &str = "https://github.com/goldalworming/tildown/releases/latest";
#[cfg(not(target_arch = "wasm32"))]
const RELEASES_API: &str = "https://api.github.com/repos/goldalworming/tildown/releases/latest";

/// Tag rilis (`v1.2.3`) lebih baru dari versi aplikasi ini.
/// Windows: unduh exe portable rilis `tag` dan tukar dengan exe yang sedang
/// jalan (exe yang jalan boleh di-rename; yang lama dihapus saat start berikut).
#[cfg(not(target_arch = "wasm32"))]
fn install_update(tag: &str) -> Result<(), String> {
    let url = format!(
        "https://github.com/goldalworming/tildown/releases/download/{tag}/TilDown-{}-portable.exe",
        tag.trim_start_matches('v')
    );
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = ureq::get(&url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(200 << 20).read_to_vec())
        .map_err(|e| e.to_string())?;
    if bin.len() < 1 << 20 || !bin.starts_with(b"MZ") {
        return Err("bad download".into());
    }
    let (new, old) = (exe.with_extension("new"), exe.with_extension("old"));
    std::fs::write(&new, bin).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    std::fs::rename(&new, &exe).map_err(|e| {
        let _ = std::fs::rename(&old, &exe);
        e.to_string()
    })
}

fn newer(tag: &str) -> bool {
    let v = |s: &str| -> Vec<u32> { s.trim_start_matches('v').split('.').map(|x| x.parse().unwrap_or(0)).collect() };
    v(tag) > v(env!("CARGO_PKG_VERSION"))
}

/// Tombol "Deploy to Cloudflare" untuk `server/`.
const DEPLOY_URL: &str =
    "https://deploy.workers.cloudflare.com/?url=https://github.com/goldalworming/tildown/tree/main/server";

/// Pilihan lebar kolom (Narrow / Normal / Wide).
const WIDTHS: [f32; 3] = [600.0, 720.0, 880.0];

/// Isi yang bisa dibuka di penampil: diagram PlantUML ke-n di blok, flowchart
/// Mermaid, atau gambar (`src` = kunci `imgs`).
#[derive(Clone, Copy, PartialEq)]
enum DiaKind {
    Puml(usize),
    Mermaid,
    Image,
}

/// Penampil diagram/gambar layar penuh: roda = zoom di pointer, seret = geser.
struct Zoom {
    src: String,
    kind: DiaKind,
    /// Skala px per unit PlantUML (0 = pas layar); `built` = skala `view`.
    k: f32,
    built: f32,
    pan: (f32, f32),
    grab: bool,
    view: RdView,
    /// Pojok kiri atas diagram di koordinat `view`.
    org: (f32, f32),
    /// Jari di layar: (id, posisi) — 1 jari geser, 2 jari pinch.
    touches: Vec<(u64, (f32, f32))>,
    /// Hasil Download; menggantikan petunjuk di bawah.
    msg: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Editor,
    Search,
    Read,
    /// Thumb scrollbar dipegang; `grab` = jarak pointer dari atas thumb.
    Bar {
        list: bool,
        grab: f32,
    },
}

/// Scrollbar ala brosql: pita genggam 14 px jauh lebih lebar dari garis 3 px
/// yang digambar; didekati atau di-drag, thumb menebal.
struct Bar {
    track: Rect,
    thumb: Rect,
    max: f32,
}

fn bar(area: Rect, total: f32, scroll: f32, s: f32) -> Option<Bar> {
    let max = total - area.h;
    if max <= 0.5 {
        return None;
    }
    let th = (area.h * area.h / total).max(24.0 * s).min(area.h);
    let ty = area.y + (area.h - th) * (scroll / max).clamp(0.0, 1.0);
    let track = Rect::new(area.x + area.w - 14.0 * s, area.y, 14.0 * s, area.h);
    Some(Bar {
        track,
        thumb: Rect::new(track.x, ty, track.w, th),
        max,
    })
}

struct Geo {
    /// Sidebar; lebar 0 bila disembunyikan.
    side: Rect,
    /// Kotak input pencarian; tinggi 0 bila pencarian tertutup.
    search: Rect,
    list: Rect,
    bar: Rect,
    content: Rect,
    col_x: f32,
    col_w: f32,
}

// ---------------------------------------------------------------------------
// Layout editor: satu `Lay` per baris logis, di-cache per isi baris. Mengetik
// di satu baris hanya me-layout ulang baris itu.

struct EdLine {
    start: usize,
    end: usize,
    y: f32,
    lay: Rc<Lay>,
    kind: Kind,
}

#[derive(Default)]
struct EdView {
    key: Option<(u64, u32, u32, bool)>,
    lines: Vec<EdLine>,
    h: f32,
    cache: HashMap<u64, (Rc<Lay>, Kind, bool)>,
}

fn ed_runs(hl: &md::Line, s: f32, t: &Theme) -> Vec<(usize, Style)> {
    let body = BODY * s;
    let (px, face) = match hl.kind {
        Kind::Heading(1) => (body * 1.5, Face::Bold),
        Kind::Heading(n) => (body * hscale(n), Face::Semi),
        Kind::Code | Kind::Fence => (body * 0.875, Face::Mono),
        _ => (body, Face::Regular),
    };
    let fg = if hl.kind == Kind::Quote { t.dim } else { t.fg };
    let base = Style::new(face, px, fg);
    let mut out: Vec<(usize, Style)> = hl
        .toks
        .iter()
        .map(|&(end, tok)| {
            let mut st = base;
            match tok {
                Tok::Text => {}
                Tok::Mark => {
                    st.color = t.dim;
                    match hl.kind {
                        Kind::Heading(_) => st.face = Face::Regular,
                        Kind::Item => {
                            st.face = Face::Mono;
                            st.px = px * 0.875;
                        }
                        _ => {}
                    }
                }
                Tok::Bold | Tok::BoldItalic => st.face = Face::Bold,
                Tok::Italic if face != Face::Bold => st.face = Face::Italic,
                Tok::Italic => {}
                Tok::Code if hl.kind != Kind::Code => {
                    st.face = Face::Mono;
                    st.px = px * 0.875;
                    st.bg = Some(t.code_bg);
                }
                Tok::Code => {}
                Tok::Link => st.color = t.accent,
                Tok::Url => st.color = t.dim,
                Tok::Strike => {
                    st.strike = true;
                    st.color = t.dim;
                }
            }
            (end, st)
        })
        .collect();
    if out.is_empty() {
        out.push((0, base));
    }
    out
}

impl EdView {
    fn build(&mut self, doc: &Doc, f: &mut Fonts, w: f32, s: f32, t: &Theme) {
        let key = (doc.version, w.to_bits(), s.to_bits(), t.dark);
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let mut old = std::mem::take(&mut self.cache);
        self.lines.clear();
        let (mut y, mut start, mut fence) = (0.0, 0, false);
        for line in doc.text.split('\n') {
            let mut hs = DefaultHasher::new();
            (line, fence, key.1, key.2, key.3).hash(&mut hs);
            let k = hs.finish();
            let v = match self.cache.get(&k) {
                Some(v) => v.clone(),
                None => {
                    let v = old.remove(&k).unwrap_or_else(|| {
                        let hl = md::highlight(line, fence);
                        (
                            Rc::new(layout(f, line, &ed_runs(&hl, s, t), w, ed_lh(hl.kind))),
                            hl.kind,
                            hl.fence_after,
                        )
                    });
                    self.cache.insert(k, v.clone());
                    v
                }
            };
            let (lay, kind, fence_after) = v;
            let end = start + line.len();
            let h = lay.h;
            self.lines.push(EdLine {
                start,
                end,
                y,
                lay,
                kind,
            });
            (y, start, fence) = (y + h, end + 1, fence_after);
        }
        self.h = y;
    }

    fn line_of(&self, pos: usize) -> Option<&EdLine> {
        let i = self
            .lines
            .partition_point(|l| l.start <= pos)
            .saturating_sub(1);
        self.lines.get(i)
    }

    /// (x, atas, tinggi) cursor relatif terhadap awal kolom teks.
    fn caret(&self, pos: usize) -> (f32, f32, f32) {
        let Some(l) = self.line_of(pos) else {
            return (0.0, 0.0, BODY * LH);
        };
        let (x, top, h) = l.lay.caret(pos - l.start);
        (x, l.y + top, h)
    }

    fn pos_at(&self, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let i = self
            .lines
            .partition_point(|l| l.y + l.lay.h <= y)
            .min(self.lines.len() - 1);
        let l = &self.lines[i];
        l.start + l.lay.pos_at(x, y - l.y)
    }
}

/// Warna sorotan hasil pencarian (yang aktif lebih tegas).
fn search_hl(t: &Theme, on: bool) -> Rgba {
    match (t.dark, on) {
        (false, false) => alpha(hex(0xFFD54F), 0.45),
        (false, true) => alpha(hex(0xFFB300), 0.75),
        (true, false) => alpha(hex(0xFFD54F), 0.25),
        (true, true) => alpha(hex(0xFFB300), 0.55),
    }
}

/// Kemunculan `terms` (huruf kecil) di deretan karakter `chars`, tanpa
/// memandang huruf besar: (indeks karakter awal, panjang), urut, tidak tumpang tindih.
fn find_terms(chars: impl Iterator<Item = char>, terms: &[String]) -> Vec<(usize, usize)> {
    let low: Vec<char> = chars.map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
    let tcs: Vec<Vec<char>> = terms.iter().map(|t| t.chars().collect()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < low.len() {
        match tcs
            .iter()
            .find(|t| !t.is_empty() && low[i..].starts_with(t))
        {
            Some(t) => {
                out.push((i, t.len()));
                i += t.len();
            }
            None => i += 1,
        }
    }
    out
}

/// Kata pencarian (huruf kecil) dari teks query.
fn terms_of(q: &str) -> Vec<String> {
    q.to_lowercase().split_whitespace().map(String::from).collect()
}

/// Rentang byte kemunculan `terms` di `text`.
fn find_bytes(text: &str, terms: &[String]) -> Vec<Range<usize>> {
    let idx: Vec<usize> = text.char_indices().map(|c| c.0).collect();
    find_terms(text.chars(), terms)
        .into_iter()
        .map(|(i, n)| idx[i]..idx.get(i + n).copied().unwrap_or(text.len()))
        .collect()
}

/// Baris isi (bukan judul) yang memuat kata pencarian, dipotong mendekati
/// kemunculannya: cuplikan daftar saat mencari.
fn match_snippet(text: &str, terms: &[String]) -> Option<String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    lines.next();
    lines.find_map(|l| {
        let r = find_bytes(l, terms).into_iter().next()?;
        let pre = l[..r.start].chars().count();
        let skip = pre.saturating_sub(16);
        let s: String = l.chars().skip(skip).take(160).collect();
        Some(if skip > 0 { format!("…{s}") } else { s })
    })
}

/// Pecah `text` jadi (potongan, cocok?) menurut kemunculan `terms`
/// (huruf kecil), tanpa memandang huruf besar.
fn marks<'a>(text: &'a str, terms: &[String]) -> Vec<(&'a str, bool)> {
    let idx: Vec<(usize, char)> = text.char_indices().collect();
    let mut hit = vec![false; idx.len()];
    for (i, n) in find_terms(text.chars(), terms) {
        hit[i..i + n].iter_mut().for_each(|h| *h = true);
    }
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=idx.len() {
        if i == idx.len() || hit[i] != hit[start] {
            let a = idx[start].0;
            let b = idx.get(i).map_or(text.len(), |x| x.0);
            out.push((&text[a..b], hit[start]));
            start = i;
        }
    }
    out
}

/// Rentang x glyph dengan byte di [lo, hi) pada satu baris. Teks kanan-ke-kiri
/// di tengah teks kiri-ke-kanan bisa terpisah secara visual: diambil
/// selubungnya.
fn xspan(r: &Row, lo: usize, hi: usize) -> (f32, f32) {
    let (a, b) = r
        .gs
        .iter()
        .filter(|g| g.i >= lo && g.i < hi && g.c != '\n')
        .fold((f32::MAX, f32::MIN), |(a, b), g| (a.min(g.x), b.max(g.x + g.adv)));
    if a <= b {
        (a, b)
    } else {
        let x = r.x_at(lo);
        (x, x)
    }
}

// ---------------------------------------------------------------------------
// Mode baca: blok hasil pulldown-cmark, posisi relatif terhadap kolom teks.

enum Deco {
    Fill(Rect, Rgba),
    /// Kotak bersudut bulat (radius).
    Round(Rect, f32, Rgba),
    Icon(&'static str, Rect, Rgba),
    Img(String, Rect),
    /// Segitiga diagram (koordinat absolut dalam view) + kotak pembatas.
    Mesh(Rect, crate::vector::Mesh),
}

#[derive(Default)]
struct RdView {
    key: Option<(u64, u32, u32, bool)>,
    items: Vec<(f32, f32, Lay)>,
    decos: Vec<Deco>,
    checks: Vec<(Rect, Range<usize>, bool)>,
    links: Vec<String>,
    /// Diagram dan gambar yang bisa diklik: (kotak, sumber, jenis).
    dias: Vec<(Rect, String, DiaKind)>,
    h: f32,
    /// Seleksi mode baca: (jangkar, kepala), masing-masing (indeks item, byte).
    /// Ikut hilang saat tampilan dibangun ulang.
    sel: Option<((usize, usize), (usize, usize))>,
}

impl RdView {
    /// Item terdekat dari titik (x, y) dan posisi di dalamnya. Jarak vertikal
    /// didahulukan, supaya drag di sela blok memilih blok di atas/bawahnya.
    fn pos_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let d = |(ix, iy, lay): &(f32, f32, Lay)| {
            (
                (iy - y).max(y - iy - lay.h).max(0.0),
                (ix - x).max(x - ix - lay.w).max(0.0),
            )
        };
        let k = (0..self.items.len())
            .min_by(|&a, &b| d(&self.items[a]).partial_cmp(&d(&self.items[b])).unwrap())?;
        let (ix, iy, lay) = &self.items[k];
        Some((k, lay.pos_at(x - ix, y - iy)))
    }

    /// Rentang byte yang terpilih di item `k`.
    fn sel_range(&self, k: usize) -> Option<(usize, usize)> {
        let (a, b) = self.sel?;
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        (a.0..=b.0).contains(&k).then(|| {
            (
                if k == a.0 { a.1 } else { 0 },
                if k == b.0 { b.1 } else { usize::MAX },
            )
        })
    }

    /// Teks yang tampil (tanpa sintaks markdown). Item di baris visual yang
    /// sama (penanda list, sel tabel) digabung dengan spasi, selain itu baris baru.
    fn selected_text(&self) -> String {
        let mut out = String::new();
        let mut prev_bottom = None;
        for (k, (_, iy, lay)) in self.items.iter().enumerate() {
            let Some((lo, hi)) = self.sel_range(k) else {
                continue;
            };
            if let Some(b) = prev_bottom {
                out.push(if *iy < b { ' ' } else { '\n' });
            }
            let mut cs: Vec<(usize, char)> = lay
                .rows
                .iter()
                .flat_map(|r| &r.gs)
                .filter(|g| g.i >= lo && g.i < hi)
                .map(|g| (g.i, g.c))
                .collect();
            cs.sort_by_key(|g| g.0);
            cs.dedup_by_key(|g| g.0); // harakat/ligatur: beberapa glyph satu huruf
            out.extend(cs.into_iter().map(|g| g.1));
            prev_bottom = Some(iy + lay.h);
        }
        out
    }

    /// Kata (rentang byte) di sekitar byte `i` item `k`. Harakat dan huruf
    /// non-ASCII lain dianggap bagian kata.
    fn word_at(&self, k: usize, i: usize) -> (usize, usize) {
        let mut cs: Vec<(usize, char)> = self.items[k].2.rows.iter().flat_map(|r| &r.gs).map(|g| (g.i, g.c)).collect();
        cs.sort_by_key(|g| g.0);
        cs.dedup_by_key(|g| g.0);
        let w = |c: char| c.is_alphanumeric() || c == '_' || !(c.is_ascii() || c.is_whitespace());
        let end = |j: usize| cs.get(j).map_or_else(|| cs.last().map_or(i, |g| g.0 + g.1.len_utf8()), |g| g.0);
        let Some(n) = cs.iter().position(|g| g.0 >= i).or(cs.len().checked_sub(1)) else {
            return (i, i);
        };
        if !w(cs[n].1) {
            return (cs[n].0, end(n + 1));
        }
        let a = (0..=n).rev().take_while(|&j| w(cs[j].1)).last().unwrap_or(n);
        let b = (n..cs.len()).find(|&j| !w(cs[j].1)).unwrap_or(cs.len());
        (cs[a].0, end(b))
    }

    fn link_at(&self, x: f32, y: f32) -> Option<&str> {
        for (ix, iy, lay) in &self.items {
            if let Some(g) = lay.glyph_at(x - ix, y - iy) {
                let st = lay.styles[g.st as usize];
                if st.link > 0 {
                    return self.links.get(st.link as usize - 1).map(String::as_str);
                }
            }
        }
        None
    }

    fn check_at(&self, x: f32, y: f32, slop: f32) -> Option<(Range<usize>, bool)> {
        self.checks
            .iter()
            .find(|(r, ..)| {
                Rect::new(r.x - slop, r.y - slop, r.w + 2.0 * slop, r.h + 2.0 * slop).contains(x, y)
            })
            .map(|(_, range, done)| (range.clone(), *done))
    }
}

pub(crate) struct Img {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    tex: Option<usize>,
}

/// Gambar `key` di `r` (diunggah ke GPU saat pertama digambar).
fn img_quad(p: &mut P, imgs: &mut HashMap<String, Option<Img>>, key: &str, r: Rect) {
    let Some(Some(img)) = imgs.get_mut(key) else {
        return;
    };
    let id = *img.tex.get_or_insert_with(|| {
        let id = p.g.add_image(img.w, img.h, &img.rgba);
        img.rgba = Vec::new(); // sudah di GPU
        id
    });
    p.g.quad(id, r, [0.0, 0.0, 1.0, 1.0], [1.0; 4], 1.0);
}

/// Berkas gambar lokal `src` (relatif terhadap folder catatan); None = web.
fn img_path(dir: &Path, src: &str) -> Option<PathBuf> {
    if src.contains("://") && !src.starts_with("file://") {
        return None;
    }
    let p = src.trim_start_matches("file://").replace("%20", " ");
    Some(if Path::new(&p).is_absolute() {
        PathBuf::from(p)
    } else {
        dir.join(p)
    })
}

/// Diagram sebagai PNG: skala 2x, tema terang, tepi 32 px.
fn diagram_png(src: &str, kind: DiaKind) -> Result<Vec<u8>, String> {
    // Font sendiri: kanvas CPU butuh atlas glyph sendiri (lihat `Canvas`).
    let mut f = Fonts::load()?;
    let (t, s) = (Theme::new(false), 2.0);
    let k = match kind {
        DiaKind::Puml(_) => 2.0,
        _ => s,
    };
    let (v, r) = zoom_view(&mut f, src, kind, None, k, s, &t).ok_or("diagram not recognized")?;
    let pad = 16.0 * s;
    let (w, h) = ((r.w + 2.0 * pad).ceil() as usize, (r.h + 2.0 * pad).ceil() as usize);
    let mut c = crate::puml_render::Canvas::new(w, h, t.bg);
    let (ox, oy) = (pad - r.x, pad - r.y);
    for d in &v.decos {
        match d {
            Deco::Mesh(_, m) => c.mesh(m, ox, oy),
            Deco::Fill(q, col) | Deco::Round(q, _, col) => c.rect(ox + q.x, oy + q.y, q.w, q.h, *col),
            Deco::Img(..) | Deco::Icon(..) => {}
        }
    }
    for (x, y, lay) in &v.items {
        c.text(&mut f, lay, ox + x, oy + y);
    }
    c.png()
}

/// Simpan ke folder Downloads tanpa menimpa; mengembalikan path-nya.
#[cfg(not(target_arch = "wasm32"))]
fn save_download(name: &str, bytes: &[u8]) -> Result<String, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or("no home folder")?;
    let dir = PathBuf::from(home).join("Downloads");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "png"));
    let mut path = dir.join(name);
    let mut k = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({k}).{ext}"));
        k += 1;
    }
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Web: unduhan browser (masuk ke folder Downloads).
#[cfg(target_arch = "wasm32")]
fn save_download(name: &str, bytes: &[u8]) -> Result<String, String> {
    crate::web::download(name, bytes);
    Ok(format!("Downloads/{name}"))
}

/// Gambar lokal relatif terhadap folder catatan. Gambar web tidak diunduh.
// ponytail: decode sinkron di thread UI saat mode baca dibangun. Pindahkan ke
// thread latar (pola decode worker stylegen) bila gambar besar terasa macet.
/// Isi berkas gambar lokal. Web: hanya `assets/` di OPFS.
fn img_bytes(dir: &Path, src: &str) -> Option<Vec<u8>> {
    if cfg!(target_arch = "wasm32") {
        return disk_asset(dir, src);
    }
    std::fs::read(img_path(dir, src)?).ok()
}

#[cfg(target_arch = "wasm32")]
fn disk_asset(dir: &Path, src: &str) -> Option<Vec<u8>> {
    crate::web::read_bytes(dir, src.trim_start_matches("./"))
}

#[cfg(not(target_arch = "wasm32"))]
fn disk_asset(_: &Path, _: &str) -> Option<Vec<u8>> {
    None
}

/// Penerima hasil decode di thread latar (desktop); belum dipasang = sinkron.
static DECODED: std::sync::OnceLock<Box<dyn Fn(String, Option<Img>) + Send + std::marker::Sync>> =
    std::sync::OnceLock::new();

/// Decode tanpa menahan UI: `None` dulu, gambar menyusul lewat `DECODED`
/// (`Ev::Decoded`). Web tidak punya thread: sinkron (isinya sendiri lazy).
fn decode_bg(dir: &Path, src: &str) -> Option<Img> {
    if cfg!(target_arch = "wasm32") || DECODED.get().is_none() {
        return decode_img(dir, src);
    }
    let (dir, src) = (dir.to_owned(), src.to_owned());
    std::thread::spawn(move || {
        let img = decode_img(&dir, &src);
        if let Some(f) = DECODED.get() {
            f(src, img);
        }
    });
    None
}

fn decode_img(dir: &Path, src: &str) -> Option<Img> {
    let mut img = image::load_from_memory(&img_bytes(dir, src)?).ok()?;
    if img.width() > 1600 || img.height() > 1600 {
        img = img.thumbnail(1600, 1600);
    }
    let img = img.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let mut rgba = img.into_raw();
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8; // premultiply, lihat shader
        }
    }
    Some(Img {
        w,
        h,
        rgba,
        tex: None,
    })
}

fn rich_runs(r: &md::Rich, px: f32, face: Face, color: Rgba, t: &Theme) -> Vec<(usize, Style)> {
    let base = Style::new(face, px, color);
    let mut out: Vec<(usize, Style)> = r
        .spans
        .iter()
        .map(|&(end, inl)| {
            let mut st = base;
            if inl.code {
                st.face = Face::Mono;
                st.px = px * 0.875;
                st.bg = Some(t.code_bg);
            } else if inl.bold {
                st.face = Face::Bold;
            } else if inl.italic && face != Face::Bold {
                st.face = Face::Italic;
            }
            if inl.strike {
                st.strike = true;
                st.color = t.dim;
            }
            if let Some(l) = inl.link {
                st.link = l + 1;
                st.color = t.accent;
                st.under = true;
            }
            (end, st)
        })
        .collect();
    if out.is_empty() {
        out.push((0, base));
    }
    out
}

/// `full` = lebar area konten; diagram boleh melebar ke margin kolom.
/// Ukuran dan jarak mengikuti CSS desain; margin antarblok diciutkan
/// seperti CSS (yang terbesar dari bawah blok atas dan atas blok bawah).
#[allow(clippy::too_many_arguments)]
fn build_read(
    text: &str,
    f: &mut Fonts,
    w: f32,
    full: f32,
    s: f32,
    t: &Theme,
    imgs: &mut HashMap<String, Option<Img>>,
    dir: &Path,
) -> RdView {
    let parsed = md::parse(text);
    let mut v = RdView {
        links: parsed.links,
        ..Default::default()
    };
    let body = BODY * s;
    let lw = s.round().max(1.0);
    let (mut y, mut gap) = (0.0f32, 0.0f32);
    for (bi, b) in parsed.blocks.iter().enumerate() {
        let x0 = b.quote as f32 * 19.0 * s + b.indent as f32 * 24.0 * s;
        let bw = (w - x0).max(40.0 * s);
        let color = if b.quote > 0 { t.dim } else { t.fg };
        let prev_q = bi > 0 && parsed.blocks[bi - 1].quote > 0;
        let next = parsed.blocks.get(bi + 1);
        let next_q = next.is_some_and(|b| b.quote > 0);
        let mut mt = s * match b.kind {
            BKind::Heading(1) => 0.0,
            BKind::Heading(2) => 32.0,
            BKind::Heading(_) | BKind::Rule => 24.0,
            _ => 0.0,
        };
        if b.quote > 0 && !prev_q {
            mt = mt.max(24.0 * s);
        }
        let used = if bi > 0 { gap.max(mt) } else { 0.0 };
        y += used;
        let y0 = y;
        let mut mb = 16.0 * s;
        match &b.kind {
            BKind::Para => {
                let runs = rich_runs(&b.rich, body, Face::Regular, color, t);
                let lay = layout(f, &b.rich.text, &runs, bw, LH);
                y += lay.h;
                v.items.push((x0, y0, lay));
            }
            BKind::Heading(n) => {
                let (px, lh, face, m) = match n {
                    1 => (32.0, 40.0, Face::Bold, 16.0),
                    2 => (24.0, 32.0, Face::Semi, 12.0),
                    3 => (20.0, 28.0, Face::Semi, 8.0),
                    _ => (16.0, 24.0, Face::Semi, 8.0),
                };
                let runs = rich_runs(&b.rich, px * s, face, color, t);
                let lay = layout(f, &b.rich.text, &runs, bw, lh / px);
                y += lay.h;
                v.items.push((x0, y0, lay));
                mb = m * s;
            }
            BKind::Mermaid
                if build_mermaid(&mut v, f, &b.rich.text, x0, &mut y, bw, full - x0, s, t)
                    .is_some() =>
            {
                dia_caption(&mut v, f, x0, bw, y0, &mut y, s, t);
            }
            BKind::Mermaid | BKind::Plantuml
                if build_puml(&mut v, f, &b.rich.text, x0, &mut y, bw, full - x0, s, t)
                    .is_some() =>
            {
                dia_caption(&mut v, f, x0, bw, y0, &mut y, s, t);
            }
            BKind::Code | BKind::Mermaid | BKind::Plantuml => {
                let pad = 16.0 * s;
                let runs = rich_runs(&b.rich, body * 0.875, Face::Mono, t.fg, t);
                let lay = layout(f, &b.rich.text, &runs, bw - 2.0 * pad, 1.5);
                let h = lay.h + 2.0 * pad;
                v.decos.push(Deco::Round(Rect::new(x0, y, bw, h), 6.0 * s, t.code_bg));
                v.items.push((x0 + pad, y + pad, lay));
                y += h;
                mb = 24.0 * s;
            }
            BKind::Item { marker, task } => {
                let done = matches!(task, Some((true, _)));
                let tx = x0 + if task.is_some() { 30.0 } else { 24.0 } * s;
                let mut runs = rich_runs(&b.rich, body, Face::Regular, if done { t.dim } else { color }, t);
                if done {
                    runs.iter_mut().for_each(|r| r.1.strike = true);
                }
                let lay = layout(f, &b.rich.text, &runs, (w - tx).max(40.0 * s), LH);
                let r0 = lay.rows.first().map_or(body * LH, |r| r.h);
                match task {
                    Some((done, range)) => {
                        let bs = (18.0 * s).round();
                        v.checks.push((
                            Rect::new(x0, (y + (r0 - bs) / 2.0).round(), bs, bs),
                            range.clone(),
                            *done,
                        ));
                    }
                    None => {
                        let st = Style::new(Face::Regular, body, color);
                        let m = layout(f, marker, &[(marker.len(), st)], f32::INFINITY, LH);
                        v.items.push(((tx - 8.0 * s - m.w).round(), y, m));
                    }
                }
                y += lay.h;
                v.items.push((tx, y0, lay));
                mb = if matches!(next.map(|b| &b.kind), Some(BKind::Item { .. })) {
                    4.0 * s
                } else {
                    16.0 * s
                };
            }
            BKind::Rule => {
                v.decos.push(Deco::Fill(Rect::new(x0, y, bw, lw), t.border));
                y += lw;
                mb = 24.0 * s;
            }
            BKind::Image { src } => {
                let size = imgs
                    .entry(src.clone())
                    .or_insert_with(|| decode_bg(dir, src))
                    .as_ref()
                    .map(|i| (i.w as f32, i.h as f32));
                match size {
                    Some((iw, ih)) => {
                        let dw = iw.min(bw);
                        let dh = ih * dw / iw.max(1.0);
                        v.decos
                            .push(Deco::Img(src.clone(), Rect::new(x0, y, dw, dh)));
                        v.dias.push((Rect::new(x0, y, dw, dh), src.clone(), DiaKind::Image));
                        y += dh;
                    }
                    None => {
                        let label = format!(
                            "[image: {}]",
                            if b.rich.text.is_empty() {
                                src
                            } else {
                                &b.rich.text
                            }
                        );
                        let lay = layout(
                            f,
                            &label,
                            &[(label.len(), Style::new(Face::Italic, body, t.dim))],
                            bw,
                            LH,
                        );
                        y += lay.h;
                        v.items.push((x0, y0, lay));
                    }
                }
                mb = 24.0 * s;
            }
            BKind::Table { rows } => {
                let ncol = rows.iter().map(Vec::len).max().unwrap_or(0);
                if ncol == 0 {
                    continue;
                }
                let px = body * 0.875;
                let (py, pxx) = (8.0 * s, 12.0 * s);
                // Padding sel: kolom pertama tanpa kiri, terakhir tanpa kanan.
                let pl = |c: usize| if c == 0 { 0.0 } else { pxx };
                let pr = |c: usize| if c + 1 == ncol { 0.0 } else { pxx };
                let face = |ri: usize| if ri == 0 { Face::Semi } else { Face::Regular };
                // Lebar alami dan minimum (kata terpanjang) per kolom.
                let (mut nat, mut min) = (vec![0.0f32; ncol], vec![40.0 * s; ncol]);
                for (ri, row) in rows.iter().enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        let l = layout(f, &cell.text, &rich_runs(cell, px, face(ri), t.fg, t), f32::INFINITY, 20.0 / 14.0);
                        nat[c] = nat[c].max(l.w + pl(c) + pr(c));
                        let word = cell
                            .text
                            .split_whitespace()
                            .map(|w| f.width(face(ri), px, w))
                            .fold(0.0, f32::max);
                        min[c] = min[c]
                            .max((word + pl(c) + pr(c)).min(bw / 3.0))
                            .min(nat[c].max(40.0 * s));
                    }
                }
                // Tabel selebar kolom teks (width: 100%): sisa dibagi menurut lebar alami.
                let (total, tmin): (f32, f32) = (nat.iter().sum(), min.iter().sum());
                let cw: Vec<f32> = if total <= bw {
                    nat.iter().map(|n| n * bw / total.max(1.0)).collect()
                } else {
                    let k = ((bw - tmin) / (total - tmin)).max(0.0);
                    nat.iter().zip(&min).map(|(n, m)| m + (n - m) * k).collect()
                };
                let tw: f32 = cw.iter().sum();
                let empty = md::Rich::default();
                for (ri, row) in rows.iter().enumerate() {
                    let lays: Vec<Lay> = (0..ncol)
                        .map(|c| {
                            let cell = row.get(c).unwrap_or(&empty);
                            let runs = rich_runs(cell, px, face(ri), t.fg, t);
                            layout(f, &cell.text, &runs, cw[c] - pl(c) - pr(c), 20.0 / 14.0)
                        })
                        .collect();
                    let rh = lays.iter().map(|l| l.h).fold(0.0, f32::max) + 2.0 * py;
                    let mut cx = x0;
                    for (c, l) in lays.into_iter().enumerate() {
                        v.items.push((cx + pl(c), y + py, l));
                        cx += cw[c];
                    }
                    y += rh;
                    v.decos.push(Deco::Fill(Rect::new(x0, y - lw, tw, lw), t.border));
                }
                mb = 24.0 * s;
            }
        }
        if b.quote > 0 {
            mb = if next_q { 16.0 * s } else { 24.0 * s };
            // Garis kiri kutipan menyambung melewati jarak antarparagraf.
            let top = if prev_q { y0 - used } else { y0 };
            for q in 0..b.quote {
                v.decos.push(Deco::Fill(
                    Rect::new(q as f32 * 19.0 * s, top, (3.0 * s).round(), y - top),
                    t.border,
                ));
            }
        }
        gap = mb;
    }
    v.h = y;
    v
}

/// Keterangan di bawah diagram + area klik/hover selebar kolom.
#[allow(clippy::too_many_arguments)]
fn dia_caption(
    v: &mut RdView,
    f: &mut Fonts,
    x0: f32,
    bw: f32,
    top: f32,
    y: &mut f32,
    s: f32,
    t: &Theme,
) {
    let cap = "Click diagram to enlarge";
    let st = Style::new(Face::Regular, 12.0 * s, t.dim);
    let lay = layout(f, cap, &[(cap.len(), st)], f32::INFINITY, 16.0 / 12.0);
    let ix = (x0 + (bw - 18.0 * s - lay.w) / 2.0).round();
    v.decos.push(Deco::Icon("maximize-2", Rect::new(ix, *y + 2.0 * s, 12.0 * s, 12.0 * s), t.dim));
    v.items.push((ix + 18.0 * s, *y, lay));
    *y += 16.0 * s + 24.0 * s;
    // Satu diagram di blok ini: seluruh bungkusnya bisa diklik.
    let n = v.dias.iter().filter(|d| d.0.y >= top).count();
    if n == 1 {
        if let Some(d) = v.dias.last_mut() {
            d.0 = Rect::new(x0.min(d.0.x), top, (x0 + bw).max(d.0.x + d.0.w) - x0.min(d.0.x), *y - top);
        }
    }
}

/// Flowchart mermaid; None bila sumbernya bukan flowchart yang dikenali.
fn build_mermaid(
    v: &mut RdView,
    f: &mut Fonts,
    src: &str,
    x0: f32,
    y: &mut f32,
    bw: f32,
    full: f32,
    s: f32,
    t: &Theme,
) -> Option<()> {
    use crate::mermaid::{self, Shape, Stroke};
    let flow = mermaid::parse(src)?;
    let mut px = BODY * s * 0.9;
    let (nl, el, pl) = loop {
        let lay = center_lay;
        let nl: Vec<Lay> = flow
            .nodes
            .iter()
            .map(|n| {
                lay(
                    f,
                    &n.label,
                    px,
                    t.fg,
                    if n.shape == Shape::Diamond { 9.0 } else { 12.0 },
                )
            })
            .collect();
        let el: Vec<Lay> = flow
            .edges
            .iter()
            .map(|e| lay(f, &e.label, px * 0.85, t.dim, 12.0))
            .collect();
        let pad = 0.8 * px;
        let size: Vec<(f32, f32)> = nl
            .iter()
            .zip(&flow.nodes)
            .map(|(l, n)| {
                let (w, h) = (l.w + 2.0 * pad, l.h + 1.2 * pad);
                match n.shape {
                    Shape::Diamond => (2.0 * l.w + pad, 2.0 * l.h + pad), // kotak teks muat di dalam belah ketupat
                    Shape::Circle => (w.max(h) * 1.15, w.max(h) * 1.15),
                    _ => (w, h),
                }
            })
            .collect();
        let lsize: Vec<(f32, f32)> = el
            .iter()
            .map(|l| {
                if l.w > 0.0 {
                    (l.w + 8.0 * s, l.h)
                } else {
                    (0.0, 0.0)
                }
            })
            .collect();
        let pl = mermaid::place(&flow, &size, &lsize, 1.6 * px);
        // Terlalu lebar: perkecil huruf sekali-dua kali, sisanya biarkan terpotong.
        if pl.w <= full || px < BODY * s * 0.6 {
            break (nl, el, pl);
        }
        px = (px * (full / pl.w).max(0.8)).max(BODY * s * 0.55);
    };
    let lw = s.round().max(1.0);
    let ox = (x0 + ((bw - pl.w) / 2.0).max(((bw - full) / 2.0).min(0.0))).round(); // tengah kolom, boleh masuk margin
    let oy = (*y + 24.0 * s).round();
    for (i, e) in flow.edges.iter().enumerate() {
        let pts: Vec<(f32, f32)> = pl.edges[i]
            .iter()
            .map(|&(x, y)| ((ox + x).round(), (oy + y).round()))
            .collect();
        let th = if e.stroke == Stroke::Thick {
            2.0 * lw
        } else {
            lw
        };
        let dash = (e.stroke == Stroke::Dotted).then_some((3.0 * lw, 6.0 * lw));
        for w in pts.windows(2) {
            seg(v, w[0], w[1], th, dash, t.dim);
        }
        if let (true, [.., (px0, py0), (px1, py1)]) = (e.arrow, pts.as_slice()) {
            let sg = |d: f32| if d == 0.0 { 0.0 } else { d.signum() }; // signum(0.0) = 1
            head(v, *px1, *py1, sg(px1 - px0), sg(py1 - py0), s, lw, t.dim);
        }
    }
    for ((n, l), r) in flow.nodes.iter().zip(nl).zip(&pl.nodes) {
        let r = Rect::new(
            (ox + r[0]).round(),
            (oy + r[1]).round(),
            r[2].round(),
            r[3].round(),
        );
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let shape = match n.shape {
            Shape::Rect => crate::vector::rrect([r.x, r.y, r.w, r.h], 6.0 * s),
            Shape::Round => crate::vector::rrect([r.x, r.y, r.w, r.h], r.h / 2.0),
            Shape::Diamond => vec![(cx, r.y), (r.x + r.w, cy), (cx, r.y + r.h), (r.x, cy)],
            Shape::Circle => crate::vector::ellipse([r.x, r.y, r.w, r.h]),
        };
        let mut m = crate::vector::Mesh::new();
        crate::vector::fill(&mut m, &shape, t.bg);
        crate::vector::stroke(&mut m, &shape, true, 1.5 * s, t.dim, None);
        v.decos.push(Deco::Mesh(r, m));
        v.items.push((
            (r.x + (r.w - l.w) / 2.0).round(),
            (r.y + (r.h - l.h) / 2.0).round(),
            l,
        ));
    }
    for (l, &(cx, cy)) in el.into_iter().zip(&pl.labels) {
        if l.w > 0.0 {
            let (lx, ly) = ((ox + cx - l.w / 2.0).round(), (oy + cy - l.h / 2.0).round());
            v.decos.push(Deco::Fill(
                Rect::new(lx - 4.0 * s, ly, l.w + 8.0 * s, l.h),
                t.bg,
            ));
            v.items.push((lx, ly, l));
        }
    }
    v.dias.push((Rect::new(ox, oy, pl.w, pl.h), src.to_string(), DiaKind::Mermaid));
    *y = oy + pl.h + 12.0 * s;
    Some(())
}

/// Tampilan penampil pada skala `k` (PlantUML: px per unit, Mermaid: faktor
/// skala UI, gambar: px layar per px gambar; `isz` = ukuran gambar) + kotaknya.
#[allow(clippy::too_many_arguments)]
fn zoom_view(
    f: &mut Fonts,
    src: &str,
    kind: DiaKind,
    isz: Option<(f32, f32)>,
    k: f32,
    s: f32,
    t: &Theme,
) -> Option<(RdView, Rect)> {
    let mut v = RdView::default();
    match kind {
        DiaKind::Image => {
            let (w, h) = isz?;
            let r = Rect::new(0.0, 0.0, w * k, h * k);
            v.decos.push(Deco::Img(src.to_owned(), r));
            Some((v, r))
        }
        DiaKind::Puml(i) => {
            let dia = crate::plantuml::parse_all(src)?.into_iter().nth(i)?;
            let (lays, pl) = puml_place(f, dia.as_ref(), k);
            puml_emit(&mut v, &lays, &pl, k, 0.0, 0.0, s, t);
            Some((v, Rect::new(0.0, 0.0, pl.w * k, pl.h * k)))
        }
        DiaKind::Mermaid => {
            build_mermaid(&mut v, f, src, 0.0, &mut 0.0, 0.0, f32::INFINITY, k, t)?;
            let r = v.dias.last()?.0;
            Some((v, r))
        }
    }
}

/// Label rata tengah per baris, wrap pada `wrap` em.
fn center_lay(f: &mut Fonts, txt: &str, px: f32, c: Rgba, wrap: f32) -> Lay {
    let mut l = layout(
        f,
        txt,
        &[(txt.len(), Style::new(Face::Regular, px, c))],
        wrap * px,
        1.3,
    );
    for r in &mut l.rows {
        let rw =
            r.gs.iter()
                .rev()
                .find(|g| !g.c.is_whitespace())
                .map_or(0.0, |g| g.x + g.adv);
        let dx = ((l.w - rw) / 2.0).max(0.0).round();
        r.gs.iter_mut().for_each(|g| g.x += dx);
    }
    l
}

/// Garis mendatar/tegak setebal `th`; `dash` = (panjang putus, langkah).
fn seg(
    v: &mut RdView,
    (ax, ay): (f32, f32),
    (bx, by): (f32, f32),
    th: f32,
    dash: Option<(f32, f32)>,
    c: Rgba,
) {
    let (len, horiz) = ((bx - ax).abs() + (by - ay).abs(), ay == by);
    let (dash, step) = dash.unwrap_or((len, len.max(1.0)));
    let mut d = 0.0;
    while d < len {
        let k = dash.min(len - d);
        let r = if horiz {
            Rect::new(ax.min(bx) + d, ay - th / 2.0, k + th / 2.0, th)
        } else {
            Rect::new(ax - th / 2.0, ay.min(by) + d, th, k + th / 2.0)
        };
        v.decos.push(Deco::Fill(r, c));
        d += step;
    }
}

/// Mata panah segitiga penuh berujung di (x, y), menghadap (dx, dy).
fn head(v: &mut RdView, x: f32, y: f32, dx: f32, dy: f32, s: f32, lw: f32, c: Rgba) {
    for k in 0..(9.0 * s).round() as i32 {
        let (cx, cy, hw) = (
            x - dx * k as f32,
            y - dy * k as f32,
            (k as f32 * 0.5).max(lw / 2.0),
        );
        let r = if dx == 0.0 {
            Rect::new(cx - hw, cy - dy.max(0.0), 2.0 * hw, 1.0)
        } else {
            Rect::new(cx - dx.max(0.0), cy - hw, 1.0, 2.0 * hw)
        };
        v.decos.push(Deco::Fill(r, c));
    }
}

/// Diagram PlantUML (semua `@startXXX` dalam blok) atau sequence mermaid;
/// None bila sumbernya tidak dikenali.
fn build_puml(
    v: &mut RdView,
    f: &mut Fonts,
    src: &str,
    x0: f32,
    y: &mut f32,
    bw: f32,
    full: f32,
    s: f32,
    t: &Theme,
) -> Option<()> {
    use crate::plantuml;
    for (idx, dia) in plantuml::parse_all(src)?.iter().enumerate() {
        // 12 px PlantUML ≈ huruf isi 0,9 × BODY.
        let mut k = BODY * s * 0.9 / 12.0;
        let (lays, pl) = loop {
            let (lays, pl) = puml_place(f, dia.as_ref(), k);
            // Terlalu lebar: perkecil, sisanya biarkan terpotong.
            if pl.w * k <= full || k < BODY * s * 0.45 / 12.0 {
                break (lays, pl);
            }
            k = (k * (full / (pl.w * k)).max(0.8)).max(BODY * s * 0.4 / 12.0);
        };
        let (w, h) = (pl.w * k, pl.h * k);
        let ox = (x0 + ((bw - w) / 2.0).max(((bw - full) / 2.0).min(0.0))).round();
        let oy = (*y + 24.0 * s).round();
        puml_emit(v, &lays, &pl, k, ox, oy, s, t);
        v.dias.push((Rect::new(ox, oy, w, h), src.to_string(), DiaKind::Puml(idx)));
        *y = oy + h + 12.0 * s;
    }
    Some(())
}

/// Layout teks diagram pada skala `k` lalu tata letaknya.
fn puml_place(
    f: &mut Fonts,
    dia: &dyn crate::plantuml::Diagram,
    k: f32,
) -> (Vec<Lay>, crate::plantuml::Placed) {
    let lays: Vec<Lay> = dia.texts().iter().map(|tx| puml_lay(f, tx, k)).collect();
    let size: Vec<(f32, f32)> = lays.iter().map(|l| (l.w / k, l.h / k)).collect();
    let pl = dia.place(&size);
    (lays, pl)
}

/// Mesh + teks diagram di (ox, oy) dengan skala `k`.
#[allow(clippy::too_many_arguments)]
fn puml_emit(
    v: &mut RdView,
    lays: &[Lay],
    pl: &crate::plantuml::Placed,
    k: f32,
    ox: f32,
    oy: f32,
    s: f32,
    t: &Theme,
) {
    use crate::plantuml::{Col, Prim};
    use crate::vector;
    let (w, h) = (pl.w * k, pl.h * k);
    let mut mesh = vector::Mesh::new();
    if t.dark {
        vector::fill(
            &mut mesh,
            &vector::rrect([ox, oy, w, h], 6.0 * s),
            hex(0xFFFFFF),
        );
    }
    for q in &pl.prims {
        match q {
            Prim::Path {
                pts,
                closed,
                fill,
                stroke,
                w: lw,
                dash,
            } => {
                let pts: Vec<vector::Pt> =
                    pts.iter().map(|p| (ox + p.0 * k, oy + p.1 * k)).collect();
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
            Prim::Text(i, x, ty) => {
                if lays[*i].w > 0.0 {
                    v.items.push((
                        (ox + x * k).round(),
                        (oy + ty * k).round(),
                        lays[*i].clone(),
                    ));
                }
            }
        }
    }
    v.decos.push(Deco::Mesh(Rect::new(ox, oy, w, h), mesh));
}

// ---------------------------------------------------------------------------
// Menggambar

const SYNC_INTRO: &str = "Sync through your own Cloudflare Worker (free plan): generate a token, \
     deploy the server, then paste the Worker URL or a device code.";

/// Pecah `text` per kata supaya tiap baris muat di lebar `w`.
fn wrap_words(f: &mut Fonts, face: Face, px: f32, text: &str, w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(l) if f.width(face, px, &format!("{l} {word}")) <= w => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// Teks pembuka tab Sync dan tinggi bagiannya (dalam satuan sebelum `s`):
/// baris 22, lalu jarak 20 sebelum isian pertama.
fn sync_intro(f: &mut Fonts, s: f32, bw: f32) -> (Vec<String>, f32) {
    let lines = wrap_words(f, Face::Regular, 14.0 * s, SYNC_INTRO, bw);
    let top = 22.0 * lines.len() as f32 + 20.0;
    (lines, top)
}

struct P<'a> {
    g: &'a mut Gpu,
    f: &'a mut Fonts,
    s: f32,
}

impl P<'_> {
    fn lw(&self) -> f32 {
        self.s.round().max(1.0)
    }

    fn rect(&mut self, r: Rect, c: Rgba) {
        self.g.rect(r, c);
    }

    /// Kotak isi bersudut bulat.
    fn rrect(&mut self, r: Rect, rad: f32, c: Rgba) {
        let mut m = crate::vector::Mesh::new();
        crate::vector::fill(&mut m, &crate::vector::rrect([r.x, r.y, r.w, r.h], rad), c);
        self.g.tris(&m, 0.0, 0.0);
    }

    /// Garis tepi bersudut bulat setebal `w`, di dalam kotak.
    fn rframe(&mut self, r: Rect, rad: f32, w: f32, c: Rgba) {
        let h = w / 2.0;
        let mut m = crate::vector::Mesh::new();
        let pts = crate::vector::rrect([r.x + h, r.y + h, r.w - w, r.h - w], (rad - h).max(0.0));
        crate::vector::stroke(&mut m, &pts, true, w, c, None);
        self.g.tris(&m, 0.0, 0.0);
    }

    /// Ikon Lucide ukuran `size` dengan pojok kiri atas (x, y).
    fn icon(&mut self, name: &str, x: f32, y: f32, size: f32, c: Rgba) {
        let k = size / 24.0;
        let mut m = crate::vector::Mesh::new();
        for (pts, closed) in crate::icons::get(name) {
            let pts: Vec<_> = pts.iter().map(|q| (x + q.0 * k, y + q.1 * k)).collect();
            crate::vector::stroke(&mut m, &pts, *closed, 2.0 * k, c, None);
        }
        self.g.tris(&m, 0.0, 0.0);
    }

    /// Ikon dipusatkan di kotak `r`.
    fn icon_in(&mut self, name: &str, r: Rect, size: f32, c: Rgba) {
        self.icon(name, (r.x + (r.w - size) / 2.0).round(), (r.y + (r.h - size) / 2.0).round(), size, c);
    }

    /// Baseline teks `px` yang dipusatkan tegak di [top, top + h].
    fn base(&mut self, face: Face, px: f32, top: f32, h: f32) -> f32 {
        let (a, d) = self.f.metrics(face, px);
        (top + (h + a - d) / 2.0).round()
    }

    /// Teks satu baris dipusatkan di kotak `r`.
    fn center(&mut self, r: Rect, face: Face, px: f32, t: &str, c: Rgba) {
        let w = self.f.width(face, px, t);
        let b = self.base(face, px, r.y, r.h);
        self.text((r.x + (r.w - w) / 2.0).round(), b, face, px, t, c, w + 1.0);
    }

    /// Satu baris dari potongan (teks, face, warna); "…" bila melebihi `max_w`.
    fn parts(&mut self, x: f32, base: f32, px: f32, parts: &[(&str, Face, Rgba)], max_w: f32) {
        if parts.iter().any(|p| p.0.chars().any(crate::text::needs_shaping)) {
            self.shaped_line(x, base, px, parts, max_w);
            return;
        }
        let total: f32 = parts.iter().map(|(t, f, _)| self.f.width(*f, px, t)).sum();
        let Some(&(_, f0, _)) = parts.first() else { return };
        let limit = if total > max_w {
            max_w - self.f.adv(f0, px, '…')
        } else {
            f32::INFINITY
        };
        let mut cx = x;
        for &(t, face, c) in parts {
            for ch in t.chars() {
                let a = self.f.adv(face, px, ch);
                if cx + a - x > limit {
                    self.glyph(face, px, '…', cx, base, c);
                    return;
                }
                if !ch.is_whitespace() {
                    self.glyph(face, px, ch, cx, base, c);
                }
                cx += a;
            }
        }
    }

    fn glyph(&mut self, face: Face, px: f32, ch: char, x: f32, base: f32, c: Rgba) {
        if let Some(gl) = self.f.glyph(face, px, ch) {
            self.quad_glyph(gl, x, base, c);
        }
    }

    fn quad_glyph(&mut self, gl: crate::text::Glyph, x: f32, base: f32, c: Rgba) {
        {
            let a = ATLAS as f32;
            let r = Rect::new(
                (x + gl.left).round(),
                (base - gl.top).round(),
                gl.w as f32,
                gl.h as f32,
            );
            let uv = [
                gl.x as f32 / a,
                gl.y as f32 / a,
                (gl.x + gl.w) as f32 / a,
                (gl.y + gl.h) as f32 / a,
            ];
            if gl.color {
                self.g.quad(0, r, uv, [1.0, 1.0, 1.0, c[3]], 1.0);
            } else {
                self.g.quad(0, r, uv, c, 0.0);
            }
        }
    }

    /// Satu baris tanpa wrap; dipotong dengan "…" bila melebihi `max_w`.
    fn text(
        &mut self,
        x: f32,
        base: f32,
        face: Face,
        px: f32,
        s: &str,
        c: Rgba,
        max_w: f32,
    ) -> f32 {
        if s.chars().any(crate::text::needs_shaping) {
            return self.shaped_line(x, base, px, &[(s, face, c)], max_w);
        }
        let limit = if self.f.width(face, px, s) > max_w {
            max_w - self.f.adv(face, px, '…')
        } else {
            f32::INFINITY
        };
        let mut cx = x;
        for ch in s.chars() {
            let a = self.f.adv(face, px, ch);
            if cx + a - x > limit {
                self.glyph(face, px, '…', cx, base, c);
                return cx + self.f.adv(face, px, '…');
            }
            if !ch.is_whitespace() {
                self.glyph(face, px, ch, cx, base, c);
            }
            cx += a;
        }
        cx
    }

    /// Satu baris berisi huruf Arab; glyph yang melewati `max_w` tidak digambar.
    // ponytail: dipotong tanpa "…"; tambahkan bila judul Arab panjang terlihat janggal.
    fn shaped_line(&mut self, x: f32, base: f32, px: f32, parts: &[(&str, Face, Rgba)], max_w: f32) -> f32 {
        let mut text = String::new();
        let mut runs = Vec::new();
        for &(t, face, c) in parts {
            text.push_str(t);
            runs.push((text.len(), crate::text::Style::new(face, px, c)));
        }
        let l = crate::text::layout(self.f, &text, &runs, f32::INFINITY, 1.0);
        let Some(r) = l.rows.first() else { return x };
        for g in &r.gs {
            if g.x + g.adv > max_w {
                continue;
            }
            let st = l.styles[g.st as usize];
            if let Some((gl, dx, dy)) = self.f.glyph_g(&st, g) {
                self.quad_glyph(gl, x + g.x + dx, base - dy, st.color);
            }
        }
        x + r.w.min(max_w)
    }

    /// Gambar layout di (x, y); baris di luar [top, bot] dilewati.
    fn lay(&mut self, l: &Lay, x: f32, y: f32, top: f32, bot: f32) {
        let t = self.lw();
        for r in &l.rows {
            let ry = y + r.top;
            if ry + r.h < top {
                continue;
            }
            if ry > bot {
                break;
            }
            for g in &r.gs {
                if let Some(bg) = l.styles[g.st as usize].bg {
                    self.rect(Rect::new(x + g.x, ry + r.h * 0.12, g.adv, r.h * 0.76), bg);
                }
            }
            for g in &r.gs {
                let st = l.styles[g.st as usize];
                if let Some((gl, dx, dy)) = self.f.glyph_g(&st, g) {
                    self.quad_glyph(gl, x + g.x + dx, y + r.base - dy, st.color);
                }
                if st.under {
                    self.rect(Rect::new(x + g.x, y + r.base + 2.0 * t, g.adv, t), st.color);
                }
                if st.strike {
                    self.rect(
                        Rect::new(x + g.x, y + r.base - (st.px * 0.3).round(), g.adv, t),
                        st.color,
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Platform

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn clip_get() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Gambar di clipboard sebagai PNG (screenshot, "Copy image").
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn clip_image_png() -> Option<Vec<u8>> {
    let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())?;
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
fn clip_image_png() -> Option<Vec<u8>> {
    None
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn clip_set(s: &str) {
    if let Ok(mut c) = arboard::Clipboard::new() {
        let _ = c.set_text(s.to_owned());
    }
}

#[cfg(target_os = "android")]
fn clip_get() -> Option<String> {
    None
}

#[cfg(target_os = "android")]
fn clip_set(_: &str) {}

// Web: Ctrl+V sampai lewat event `paste` (lihat `web.rs`, `paste_text`);
// CLIP hanya cadangan untuk tempel di dalam aplikasi.
#[cfg(target_arch = "wasm32")]
thread_local!(static CLIP: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) });

#[cfg(target_arch = "wasm32")]
fn clip_get() -> Option<String> {
    Some(CLIP.with_borrow(String::clone)).filter(|s| !s.is_empty())
}

#[cfg(target_arch = "wasm32")]
fn clip_remember(s: &str) {
    CLIP.set(s.to_owned());
}

#[cfg(not(target_arch = "wasm32"))]
fn clip_remember(_: &str) {}

#[cfg(target_arch = "wasm32")]
fn clip_set(s: &str) {
    CLIP.set(s.to_owned());
    crate::web::clip_write(s);
}

/// Hanya http(s) dan mailto: link di catatan tidak boleh menjalankan berkas lokal.
fn open_url(u: &str) {
    if !(u.starts_with("http://") || u.starts_with("https://") || u.starts_with("mailto:")) {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    return crate::web::open_url(u);
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(not(any(windows, target_os = "macos", target_arch = "wasm32")))]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(not(target_arch = "wasm32"))]
    let _ = cmd.arg(u).spawn();
}

/// Tombol edit yang sama untuk editor dan kotak cari.
fn edit_common(doc: &mut Doc, ev: &KeyEvent, ctrl: bool, shift: bool, multiline: bool) -> bool {
    match &ev.logical_key {
        Key::Named(NamedKey::Backspace) if ctrl => doc.delete_word_back(),
        Key::Named(NamedKey::Backspace) => doc.backspace(),
        Key::Named(NamedKey::Delete) if ctrl => doc.delete_word_fwd(),
        Key::Named(NamedKey::Delete) => doc.delete(),
        Key::Named(NamedKey::ArrowLeft) if ctrl => doc.word_left(shift),
        Key::Named(NamedKey::ArrowLeft) => doc.move_left(shift),
        Key::Named(NamedKey::ArrowRight) if ctrl => doc.word_right(shift),
        Key::Named(NamedKey::ArrowRight) => doc.move_right(shift),
        Key::Character(c) if ctrl => match c.to_lowercase().as_str() {
            "a" => doc.select_all(),
            "c" if !doc.selected_text().is_empty() => clip_set(doc.selected_text()),
            "x" if !doc.selected_text().is_empty() => {
                clip_set(doc.selected_text());
                doc.insert("");
            }
            "v" => {
                if let Some(t) = clip_get() {
                    let t = if multiline {
                        t.replace("\r\n", "\n")
                    } else {
                        t.replace(['\r', '\n'], " ")
                    };
                    doc.insert(&t);
                }
            }
            "z" if shift => {
                doc.redo();
            }
            "z" => {
                doc.undo();
            }
            "y" => {
                doc.redo();
            }
            _ => return false,
        },
        _ if ctrl => return false,
        // `text` sudah memperhitungkan layout keyboard dan dead key.
        _ => match ev.text.as_deref() {
            Some(t) if !t.chars().any(char::is_control) => doc.insert(t),
            _ => return false,
        },
    }
    true
}

// ---------------------------------------------------------------------------

pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    fonts: Option<Fonts>,
    err: Option<String>,
    store: Store,
    /// Indeks catatan yang terbuka di `store.notes`.
    cur: usize,
    doc: Doc,
    saved_ver: u64,
    save_due: Option<Instant>,
    dirty_since: Option<Instant>,
    status: String,
    search: Doc,
    /// Hasil pencarian: indeks ke `store.notes`, terbaru di atas.
    res: Vec<usize>,
    /// Jumlah hasil persis di depan `res`; sisanya hasil fuzzy.
    exact: usize,
    list_scroll: f32,
    /// Pointer di atas pita scrollbar [konten, daftar].
    bar_hot: [bool; 2],
    focus: Focus,
    mode: Mode,
    scroll: [f32; 2],
    ed: EdView,
    rd: RdView,
    imgs: HashMap<String, Option<Img>>,
    mods: ModifiersState,
    mouse: (f32, f32),
    drag: Option<Drag>,
    zoom: Option<Zoom>,
    /// Diagram yang ditekan di mode baca; dibuka bila dilepas tanpa digeser.
    dia_press: Option<(usize, (f32, f32))>,
    clicks: (Instant, u32, (f32, f32)),
    /// Kolom yang dipertahankan saat naik/turun baris.
    want_x: Option<f32>,
    preedit: String,
    ime_area: (i32, i32, u32),
    theme: Theme,
    /// Pilihan tema: 0 = ikut sistem, 1 = terang, 2 = gelap.
    theme_pref: u8,
    sys_dark: bool,
    /// Ukuran huruf isi (px CSS, 12..=24) dan lebar kolom (`WIDTHS`).
    fs: f32,
    col: f32,
    /// Mode saat membuka catatan dari daftar.
    open_in: Mode,
    side_open: bool,
    search_open: bool,
    /// Kemunculan pencarian yang aktif di catatan (untuk ‹ ›).
    match_idx: usize,
    /// Dialog Settings: Some(0) = General, Some(1) = Trash.
    settings: Option<u8>,
    trash: Vec<store::Trashed>,
    /// Di bawah pointer: tombol, baris daftar, blok diagram (untuk hover).
    hot: (Option<Btn>, Option<usize>, Option<usize>),
    /// `hits()` / `base_hits()` untuk frame yang sedang digambar (font
    /// dipinjam painter selama menggambar).
    frame_hits: Vec<(Rect, Btn)>,
    frame_base: Vec<(Rect, Btn)>,
    s: f32,
    size: (f32, f32),
    words: (u64, usize),
    icon: CursorIcon,
    /// Jalan kembali ke event loop: GPU web (async) dan jawaban sync.
    proxy: Option<winit::event_loop::EventLoopProxy<Ev>>,
    /// Sync ke server sendiri (`sync.rs`), disimpan di berkas `.sync`.
    sync: Sync,
    sync_due: Option<Instant>,
    sync_busy: bool,
    /// Hasil sync terakhir: waktu, atau pesan galat.
    sync_last: Option<Result<SystemTime, String>>,
    /// Isian tab Sync [URL, token] dan yang sedang diketik.
    sync_in: [Doc; 2],
    sync_edit: Option<usize>,
    /// Petunjuk sesudah Generate (menggantikan status sementara).
    sync_msg: Option<&'static str>,
    /// Cek update: sedang berjalan / tag rilis terbaru.
    upd_busy: bool,
    upd: Option<Result<String, String>>,
    /// Exe baru sudah terpasang: tombol jadi Restart.
    upd_ready: bool,
    share_busy: bool,
    glide: Option<Glide>,
    /// Menu konteks terbuka di titik ini; `true` = dari sentuhan (tampil di atas jari).
    menu: Option<(f32, f32, bool)>,
    /// Pesan biasa di toolbar (bukan galat), hilang saat mengetik/pindah catatan.
    notice: String,
    focused: bool,
    tap: Option<Tap>,
    /// Keyboard layar sedang dibutuhkan (web; lihat `kb_wanted`).
    kb_on: bool,
    /// Kedalaman lapisan yang terakhir dilaporkan ke riwayat browser (web).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    nav: u32,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run() -> Result<(), String> {
    let fonts = Fonts::load()?;
    let dir = store::default_dir();
    let store =
        Store::open(dir.clone()).map_err(|e| format!("folder catatan {}: {e}", dir.display()))?;
    let el = EventLoop::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let mut app = App::new(store, fonts);
    app.proxy = Some(el.create_proxy());
    // Sisa update sebelumnya (exe lama yang sudah diganti).
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
    // Mode screenshot butuh gambar di frame pertama: tetap sinkron.
    if std::env::var_os("MARKDOWN_SHOT").is_none() {
        let p = el.create_proxy();
        _ = DECODED.set(Box::new(move |src, img| _ = p.send_event(Ev::Decoded(src, img))));
    }
    el.set_control_flow(ControlFlow::Wait); // tidur sampai ada event: CPU 0% saat diam
    el.run_app(&mut app).map_err(|e| e.to_string())?;
    app.err.map_or(Ok(()), Err)
}

/// Web: muat OPFS dulu (async), lalu serahkan ke event loop browser. Kembali
/// segera; galat dicatat ke console.
#[cfg(target_arch = "wasm32")]
pub fn run() -> Result<(), String> {
    use winit::platform::web::EventLoopExtWebSys;
    std::panic::set_hook(Box::new(|i| crate::web::log_error(&i.to_string())));
    wasm_bindgen_futures::spawn_local(async {
        let start = async {
            crate::web::preload().await?;
            let store = Store::open(PathBuf::from("/")).map_err(|e| e.to_string())?;
            let mut app = App::new(store, Fonts::load()?);
            let el = EventLoop::with_user_event()
                .build()
                .map_err(|e| e.to_string())?;
            let proxy = el.create_proxy();
            let kb = proxy.clone();
            crate::web::kb_init(move |k, s| _ = kb.send_event(Ev::Kb(k, s)));
            let im = proxy.clone();
            crate::web::img_init(move |n, b| _ = im.send_event(Ev::Img(n, b)));
            let ld = proxy.clone();
            crate::web::on_asset(move |n| _ = ld.send_event(Ev::Asset(n)));
            app.proxy = Some(proxy);
            el.spawn_app(app);
            Ok::<_, String>(())
        };
        if let Err(e) = start.await {
            crate::web::log_error(&format!("TilDown: {e}"));
        }
    });
    Ok(())
}

impl App {
    fn new(mut store: Store, fonts: Fonts) -> App {
        let st = store.load_state();
        let last = st
            .get("last")
            .and_then(|id| store.notes.iter().position(|n| &n.id == id));
        let cur = match last.or_else(|| store.filter("").0.first().copied()) {
            Some(i) => i,
            None => store.create(""),
        };
        let get = |k: &str| st.get(k).map(String::as_str);
        let mode = if get("mode") == Some("read") {
            Mode::Read
        } else {
            Mode::Edit
        };
        let num = |k: &str, d: f32| get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        let sync = Sync::load(&store.read_file(".sync").unwrap_or_default());
        let mut app = App {
            window: None,
            gpu: None,
            fonts: Some(fonts),
            err: None,
            store,
            cur,
            doc: Doc::default(),
            saved_ver: 0,
            save_due: None,
            dirty_since: None,
            status: String::new(),
            search: Doc::default(),
            res: Vec::new(),
            exact: 0,
            list_scroll: 0.0,
            bar_hot: [false; 2],
            focus: Focus::Editor,
            mode,
            scroll: [0.0; 2],
            ed: EdView::default(),
            rd: RdView::default(),
            imgs: HashMap::new(),
            mods: ModifiersState::empty(),
            mouse: (0.0, 0.0),
            drag: None,
            zoom: None,
            dia_press: None,
            clicks: (Instant::now(), 0, (0.0, 0.0)),
            want_x: None,
            preedit: String::new(),
            ime_area: (0, 0, 0),
            theme: Theme::new(false),
            theme_pref: num("theme", 0.0).clamp(0.0, 2.0) as u8,
            sys_dark: false,
            fs: num("fs", 16.0).clamp(12.0, 24.0),
            col: num("width", 720.0),
            open_in: if get("open") == Some("read") {
                Mode::Read
            } else {
                Mode::Edit
            },
            side_open: get("side") != Some("0"),
            search_open: false,
            match_idx: 0,
            settings: None,
            trash: Vec::new(),
            hot: (None, None, None),
            frame_hits: Vec::new(),
            frame_base: Vec::new(),
            s: 1.0,
            size: (1.0, 1.0),
            words: (u64::MAX, 0),
            icon: CursorIcon::Default,
            proxy: None,
            sync_due: sync.on().then(Instant::now),
            sync_busy: false,
            sync_last: None,
            sync_in: [Doc::new(&sync.url), Doc::new(&sync.token)],
            sync_edit: None,
            sync_msg: None,
            upd_busy: false,
            upd: None,
            upd_ready: false,
            share_busy: false,
            nav: 0,
            glide: None,
            menu: None,
            notice: String::new(),
            focused: true,
            tap: None,
            kb_on: false,
            sync,
        };
        app.load(cur);
        app.refilter();
        app
    }

    fn fail(&mut self, el: &ActiveEventLoop, e: String) {
        self.err = Some(e);
        el.exit();
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn set_title(&self) {
        if let Some(w) = &self.window {
            w.set_title(&format!("{} — TilDown", store::title_of(&self.doc.text)));
        }
    }

    // --- catatan --------------------------------------------------------

    fn load(&mut self, i: usize) {
        self.cur = i;
        self.notice.clear();
        self.doc = Doc::new(&self.store.notes[i].text);
        self.doc.set_cursor(0, false);
        self.saved_ver = self.doc.version;
        (self.save_due, self.dirty_since) = (None, None);
        self.scroll = [0.0; 2];
        self.want_x = None;
        self.preedit.clear();
        self.ed = EdView::default();
        self.rd = RdView::default();
        for (_, img) in self.imgs.drain() {
            if let (Some(Img { tex: Some(t), .. }), Some(g)) = (img, self.gpu.as_mut()) {
                g.free_image(t);
            }
        }
        self.words.0 = u64::MAX;
        self.set_title();
    }

    fn refilter(&mut self) {
        (self.res, self.exact) = self.store.filter(&self.search.text);
    }

    fn open_note(&mut self, mut i: usize) {
        if i == self.cur {
            return;
        }
        self.save_now();
        let c = &self.store.notes[self.cur];
        // Catatan baru yang ditinggalkan kosong tidak meninggalkan jejak.
        if !c.on_disk && c.text.trim().is_empty() {
            self.store.notes.remove(self.cur);
            if i > self.cur {
                i -= 1;
            }
        }
        self.load(i);
        self.refilter();
        // HP: daftar menutupi layar; catatan yang dibuka harus kelihatan.
        if self.size.0 > 1.0 && self.narrow() {
            self.side_open = false;
        }
    }

    fn new_note(&mut self, text: &str) {
        let i = self.store.create(text);
        self.open_note(i);
        self.doc.set_cursor(self.doc.text.len(), false);
        (self.focus, self.mode) = (Focus::Editor, Mode::Edit);
        if !text.trim().is_empty() {
            self.saved_ver = u64::MAX; // belum di disk: paksa simpan
            self.changed();
        }
    }

    /// Berkas markdown yang di-drop → catatan baru (aslinya tidak diubah).
    fn import_note(&mut self, text: &str) {
        self.new_note(&text.trim_start_matches('\u{feff}').replace("\r\n", "\n"));
        self.redraw();
    }

    /// Simpan gambar ke `assets/` folder catatan lalu sisipkan tautannya.
    fn add_image(&mut self, name: &str, bytes: &[u8]) {
        match self.store.add_asset(name, bytes) {
            Ok(rel) => {
                let md = format!("![]({rel})\n");
                if self.mode == Mode::Edit {
                    let c = self.doc.cursor;
                    let nl = if c == 0 || self.doc.text[..c].ends_with('\n') { "" } else { "\n" };
                    self.doc.insert(&format!("{nl}{md}"));
                    self.reveal();
                } else {
                    // Mode baca: di akhir catatan.
                    let end = self.doc.text.len();
                    let nl = if end == 0 || self.doc.text.ends_with('\n') { "" } else { "\n" };
                    self.doc.replace_range(end, end, &format!("{nl}{md}"));
                }
                self.changed();
            }
            Err(e) => self.status = format!("Couldn't add image: {e}"),
        }
    }

    /// Tempel dari clipboard sistem (web, lewat event `paste`).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn paste_text(&mut self, t: String) {
        clip_remember(&t);
        let multi = self.settings.is_none() && self.focus == Focus::Editor;
        let t = if multi { t.replace("\r\n", "\n") } else { t.replace(['\r', '\n'], " ") };
        self.soft_key(2, t);
    }

    fn paste_image(&mut self) -> bool {
        let Some(png) = clip_image_png() else {
            return false;
        };
        self.add_image("pasted.png", &png);
        true
    }

    fn delete_note(&mut self) {
        if let Err(e) = self.store.trash(self.cur) {
            self.status = format!("Couldn't delete: {e}");
            return;
        }
        self.sync_soon();
        self.refilter();
        let i = match self.res.first() {
            Some(&i) => i,
            None => self.store.create(""),
        };
        self.load(i);
        self.refilter();
    }

    fn save_now(&mut self) {
        self.save_due = None;
        if self.doc.version == self.saved_ver {
            return;
        }
        self.store.set_text(self.cur, &self.doc.text);
        match self.store.save(self.cur) {
            Ok(()) => {
                self.saved_ver = self.doc.version;
                self.dirty_since = None;
                self.status.clear();
                self.notice.clear();
                self.sync_soon();
            }
            Err(e) => {
                self.status = format!("Couldn't save: {e}");
                self.save_due = Some(Instant::now() + SAVE_MAX); // coba lagi
            }
        }
        self.refilter();
    }

    fn save_state(&self) {
        let n = &self.store.notes[self.cur];
        let last = if n.on_disk { n.id.as_str() } else { "" };
        let m = |m: Mode| if m == Mode::Read { "read" } else { "edit" };
        self.store.save_state(&[
            ("last", last),
            ("mode", m(self.mode)),
            ("theme", &self.theme_pref.to_string()),
            ("fs", &self.fs.to_string()),
            ("width", &self.col.to_string()),
            ("open", m(self.open_in)),
            ("side", if self.side_open { "1" } else { "0" }),
        ]);
    }

    /// Terapkan pilihan tema (sistem/terang/gelap).
    fn apply_theme(&mut self) {
        let dark = match self.theme_pref {
            0 => self.sys_dark,
            1 => false,
            _ => true,
        };
        self.theme = Theme::new(dark);
    }

    /// Skala konten: skala layar × ukuran huruf pilihan.
    fn cs(&self) -> f32 {
        self.s * self.fs / 16.0
    }

    /// Dipanggil setiap kali `doc` mungkin berubah.
    fn changed(&mut self) {
        if self.doc.version != self.saved_ver {
            let now = Instant::now();
            let since = *self.dirty_since.get_or_insert(now);
            self.save_due = Some((now + SAVE_DELAY).min(since + SAVE_MAX));
        }
        self.set_title();
    }

    /// Jendela kembali aktif: ambil perubahan dari luar (Syncthing, editor lain).
    fn rescan(&mut self) {
        let id = self.store.notes[self.cur].id.clone();
        let dirty = self.doc.version != self.saved_ver;
        let Ok(changed) = self.store.rescan(dirty.then_some(id.as_str())) else {
            return;
        };
        self.reload_cur(&id, &changed);
    }

    /// Sesudah catatan berubah dari luar: cari lagi catatan `id` yang terbuka
    /// dan muat ulang bila isinya berubah; kalau hilang, buka yang teratas.
    fn reload_cur(&mut self, id: &str, changed: &[String]) {
        match self.store.notes.iter().position(|n| n.id == id) {
            Some(i) => {
                self.cur = i;
                if changed.iter().any(|c| c == id) {
                    let (a, c) = (self.doc.anchor, self.doc.cursor);
                    let t = self.store.notes[i].text.clone();
                    self.doc.set_text(&t);
                    self.saved_ver = self.doc.version;
                    self.doc.set_selection(a, c);
                }
            }
            None => {
                self.refilter();
                let i = match self.res.first() {
                    Some(&i) => i,
                    None => self.store.create(""),
                };
                self.load(i);
            }
        }
        self.refilter();
    }

    // --- sync -----------------------------------------------------------

    /// Ada perubahan lokal: kirim sebentar lagi.
    fn sync_soon(&mut self) {
        if self.sync.on() {
            let t = Instant::now() + SYNC_DELAY;
            self.sync_due = Some(self.sync_due.map_or(t, |d| d.min(t)));
        }
    }

    fn sync_start(&mut self) {
        if !self.sync.on() || self.sync_busy {
            return;
        }
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.save_now();
        self.sync_due = None;
        self.sync_busy = true;
        let body = self.sync.request(&self.store);
        sync::post(self.sync.endpoint(), self.sync.token.clone(), body, move |r| {
            let _ = proxy.send_event(Ev::Sync(r));
        });
        self.redraw();
    }

    fn sync_done(&mut self, r: Result<String, String>) {
        self.sync_busy = false;
        if !self.sync.on() {
            return; // diputus selagi menunggu
        }
        self.save_now();
        let id = self.store.notes[self.cur].id.clone();
        let now = Instant::now();
        let mut next = self.focused.then(|| now + SYNC_POLL);
        match r.and_then(|body| self.sync.apply(&mut self.store, &body)) {
            Ok(a) => {
                self.sync_last = Some(Ok(SystemTime::now()));
                // Masih ada halaman berikut, atau ada versi lokal/salinan
                // konflik yang perlu dikirim.
                if a.more || a.conflicts > 0 {
                    next = Some(now);
                }
                if let Err(e) = self.store.write_file(".sync", &self.sync.dump()) {
                    self.sync_last = Some(Err(e.to_string()));
                }
                self.reload_cur(&id, &a.changed);
            }
            Err(e) => self.sync_last = Some(Err(e)),
        }
        self.sync_due = [self.sync_due, next].into_iter().flatten().min();
        self.redraw();
    }

    /// Tombol Connect / Disconnect.
    fn sync_connect(&mut self) {
        self.sync_msg = None;
        if self.sync.on() {
            self.sync = Sync::default();
            self.sync_due = None;
        } else {
            let (url, token) = (self.sync_in[0].text.clone(), self.sync_in[1].text.clone());
            self.sync.connect(&url, &token);
            if !self.sync.on() {
                self.sync_last = Some(Err("enter the server URL and token".into()));
                return;
            }
            self.sync_in[0] = Doc::new(&self.sync.url);
            self.sync_edit = None;
            self.sync_due = Some(Instant::now());
        }
        self.sync_last = None;
        let _ = self.store.write_file(".sync", &self.sync.dump());
    }

    /// Isian berisi kode koneksi (`<url>#<token>`): pecah ke dua isian lalu
    /// langsung connect.
    fn sync_split(&mut self) {
        if self.sync.on() {
            return;
        }
        let code = self.sync_in.iter().find_map(|d| {
            let (u, t) = d.text.trim().split_once('#')?;
            Some((u.to_owned(), t.to_owned()))
        });
        if let Some((u, t)) = code {
            self.sync_in = [Doc::new(&u), Doc::new(&t)];
            self.sync_connect();
        }
    }

    fn paste_code(&mut self, t: String) {
        if t.contains('#') {
            self.sync_in[0] = Doc::new(t.trim());
            self.sync_split();
        } else {
            self.sync_last = Some(Err("the clipboard has no connection code".into()));
        }
        self.redraw();
    }

    /// Ikon dan label tombol share.
    fn share_look(&self, b: Btn) -> (&'static str, &'static str) {
        match b {
            Btn::CopyLink => ("link", "Copy link"),
            Btn::Unshare => ("link-2-off", if self.share_busy { "Stopping…" } else { "Unshare" }),
            _ => ("share-2", if self.share_busy { "Sharing…" } else { "Share" }),
        }
    }

    /// Bagikan catatan ini (link baru; link lama, kalau ada, mati) atau
    /// berhenti membagikan. Isi link selalu versi terakhir yang ter-sync.
    fn share(&mut self, on: bool) {
        if self.share_busy {
            return;
        }
        let Some(proxy) = self.proxy.clone() else { return };
        self.save_now();
        if on && !self.store.notes[self.cur].on_disk {
            self.status = "Write something before sharing".into();
            return;
        }
        let id = self.store.notes[self.cur].id.clone();
        self.share_busy = true;
        self.sync_start(); // isi catatan sampai di server secepatnya
        let body = serde_json::json!({ "note": id }).to_string();
        sync::post(self.sync.share_endpoint(on), self.sync.token.clone(), body, move |r| {
            let _ = proxy.send_event(Ev::Share(id, on, r));
        });
    }

    fn share_done(&mut self, id: String, on: bool, r: Result<String, String>) {
        self.share_busy = false;
        let sid = r.and_then(|b| {
            let v: serde_json::Value = serde_json::from_str(&b).map_err(|e| e.to_string())?;
            Ok(v["sid"].as_str().unwrap_or_default().to_owned())
        });
        match sid {
            Ok(sid) if on && !sid.is_empty() => {
                self.sync.shares.insert(id.clone(), sid);
                if let Some(u) = self.sync.share_url(&id) {
                    clip_set(&u);
                }
                self.notice = "Link copied. Anyone with the link can read this note.".into();
            }
            Ok(_) if !on => {
                self.sync.shares.remove(&id);
                self.notice = "Sharing stopped. The old link no longer works.".into();
            }
            Ok(_) => self.status = "Couldn't share: bad reply".into(),
            // Server lama tanpa /share.
            Err(e) if e.starts_with("404") => self.status = "Update your sync server to share notes".into(),
            Err(e) => self.status = format!("Couldn't share: {e}"),
        }
    }

    fn update_label(&self) -> &'static str {
        match &self.upd {
            _ if cfg!(target_arch = "wasm32") => "Update",
            _ if self.upd_ready => "Restart",
            Some(Ok(tag)) if self.upd_busy && newer(tag) => "Downloading…",
            _ if self.upd_busy => "Checking…",
            Some(Ok(tag)) if newer(tag) && cfg!(windows) => "Update",
            Some(Ok(tag)) if newer(tag) => "Download",
            _ => "Check for updates",
        }
    }

    /// Web: muat ulang build terbaru. Desktop: tanya GitHub rilis terbaru;
    /// kalau sudah ada yang lebih baru: Windows mengunduh dan memasang exe-nya
    /// sendiri (lalu Restart), macOS/Linux membuka halaman unduh.
    fn update(&mut self) {
        #[cfg(target_arch = "wasm32")]
        crate::web::update_app();
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.upd_busy {
                return;
            }
            if self.upd_ready {
                self.save_now();
                self.save_state();
                if let Ok(exe) = std::env::current_exe() {
                    if std::process::Command::new(exe).spawn().is_ok() {
                        std::process::exit(0);
                    }
                }
                return;
            }
            let Some(proxy) = self.proxy.clone() else { return };
            if let Some(Ok(tag)) = &self.upd {
                if newer(tag) {
                    if !cfg!(windows) {
                        return open_url(RELEASES);
                    }
                    let tag = tag.clone();
                    self.upd_busy = true;
                    std::thread::spawn(move || {
                        let _ = proxy.send_event(Ev::Updated(install_update(&tag)));
                    });
                    return;
                }
            }
            self.upd_busy = true;
            std::thread::spawn(move || {
                let r = ureq::get(RELEASES_API)
                    .header("Accept", "application/vnd.github+json")
                    .call()
                    .and_then(|mut r| r.body_mut().read_to_string())
                    .map_err(|e| e.to_string())
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string()))
                    .and_then(|v| v["tag_name"].as_str().map(str::to_owned).ok_or_else(|| "no release".into()))
                    .map_err(|_| "couldn't check".to_owned());
                let _ = proxy.send_event(Ev::Update(r));
            });
        }
    }

    fn sync_status(&self) -> (String, Rgba) {
        let t = self.theme;
        if self.sync_busy {
            return ("Syncing…".into(), t.dim);
        }
        if let Some(m) = self.sync_msg {
            return (m.into(), t.accent);
        }
        match (&self.sync_last, self.sync.on()) {
            (Some(Err(e)), _) => (format!("Sync failed: {e}"), t.err),
            (_, false) => ("Not connected".into(), t.dim),
            (Some(Ok(w)), _) => (format!("Last synced {}", store::ago(*w, SystemTime::now())), t.dim),
            (None, true) => ("Connected".into(), t.dim),
        }
    }

    // --- tata letak -----------------------------------------------------

    fn geo(&self) -> Geo {
        let s = self.s;
        let (w, h) = self.size;
        let sw = if !self.side_open {
            0.0
        } else if self.narrow() {
            w // HP: daftar menutupi layar, catatan dibuka dari situ
        } else {
            (SIDE_W * s).min(w * 0.45).round()
        };
        let bh = (BAR_H * s).round();
        let fh = if self.query().is_empty() { 0.0 } else { (40.0 * s).round() };
        let content = Rect::new(sw, bh, (w - sw).max(1.0), (h - bh - fh).max(1.0));
        let col_w = (self.col * s)
            .min(content.w - 2.0 * GUTTER * s)
            .max(40.0 * s)
            .round();
        // Sidebar: kepala 48, [pencarian 36 + 12], daftar, [hasil 40], Settings 53.
        let search = if self.search_open {
            Rect::new(12.0 * s, bh, sw - 24.0 * s, (36.0 * s).round())
        } else {
            Rect::new(12.0 * s, bh, sw - 24.0 * s, 0.0)
        };
        let ly = if self.search_open {
            search.y + search.h + (12.0 * s).round()
        } else {
            bh
        };
        let foot = (53.0 * s).round() + if self.query().is_empty() { 0.0 } else { (40.0 * s).round() };
        Geo {
            side: Rect::new(0.0, 0.0, sw, h),
            search,
            list: Rect::new(0.0, ly, sw, (h - ly - foot).max(1.0)),
            bar: Rect::new(sw, 0.0, w - sw, bh),
            col_x: (content.x + (content.w - col_w) / 2.0).round(),
            col_w,
            content,
        }
    }

    /// Kata kunci pencarian yang aktif (kosong bila pencarian tertutup).
    fn query(&self) -> &str {
        if self.search_open {
            self.search.text.trim()
        } else {
            ""
        }
    }

    fn tw(&mut self, face: Face, px: f32, t: &str) -> f32 {
        self.fonts.as_mut().map_or(0.0, |f| f.width(face, px, t))
    }

    /// Kotak dialog Settings (560×600, di tengah).
    fn dialog(&self) -> Rect {
        let (s, (w, h)) = (self.s, self.size);
        let (dw, dh) = ((560.0 * s).min(w - 32.0 * s), (600.0 * s).min(h - 32.0 * s));
        Rect::new(((w - dw) / 2.0).round(), ((h - dh) / 2.0).round(), dw.round(), dh.round())
    }

    /// Tombol yang tampil sekarang beserta kotaknya. Satu sumber untuk
    /// menggambar, hover, dan klik.
    fn hits(&mut self) -> Vec<(Rect, Btn)> {
        let s = self.s;
        let w = self.size.0;
        let r = |x: f32, y: f32, bw: f32, bh: f32| {
            Rect::new(x.round(), y.round(), (bw * s).round(), (bh * s).round())
        };
        let mut v = Vec::new();
        if self.settings.is_some() {
            return self.settings_hits();
        }
        if self.zoom.is_none() {
            return self.base_hits();
        }
        {
            let bs = [Btn::ZoomSave, Btn::ZoomOut, Btn::ZoomIn, Btn::ZoomClose];
            for (i, b) in bs.into_iter().enumerate() {
                v.push((r(w - (16.0 + 36.0 + (3 - i) as f32 * 44.0) * s, 16.0 * s, 36.0, 36.0), b));
            }
        }
        v
    }

    /// Tombol sidebar dan toolbar.
    fn base_hits(&mut self) -> Vec<(Rect, Btn)> {
        let s = self.s;
        let h = self.size.1;
        let w = self.size.0;
        let r = |x: f32, y: f32, bw: f32, bh: f32| {
            Rect::new(x.round(), y.round(), (bw * s).round(), (bh * s).round())
        };
        let mut v = self.menu_hits();
        let geo = self.geo();
        if self.side_open {
            let sw = geo.side.w;
            v.push((r(sw - 72.0 * s, 8.0 * s, 32.0, 32.0), Btn::Search));
            v.push((r(sw - 40.0 * s, 8.0 * s, 32.0, 32.0), Btn::New));
            if !self.search.text.is_empty() && self.search_open {
                v.push((r(sw - 42.0 * s, geo.search.y + 6.0 * s, 24.0, 24.0), Btn::Clear));
            }
            v.push((r(8.0 * s, h - 44.0 * s, SIDE_W - 16.0, 36.0), Btn::Settings));
        }
        if self.side_open && self.narrow() {
            return v; // isi tertutup daftar: tanpa toolbar
        }
        v.push((r(geo.bar.x + 8.0 * s, 8.0 * s, 32.0, 32.0), Btn::Side));
        if !self.query().is_empty() {
            let y = geo.content.y + geo.content.h + 4.0 * s;
            v.push((r(w - 84.0 * s, y, 32.0, 32.0), Btn::MatchPrev));
            v.push((r(w - 48.0 * s, y, 32.0, 32.0), Btn::MatchNext));
        }
        // Kanan: [ikon 16 + 6 + label], padding 10 / 12.
        let px = 14.0 * s;
        let mut x = w - 16.0 * s;
        let mut bs = vec![(Btn::Delete, "Delete"), (Btn::Mode, self.mode_label())];
        if self.sync.on() {
            match self.sync.shares.contains_key(&self.store.notes[self.cur].id) {
                true => bs.extend([Btn::Unshare, Btn::CopyLink].map(|b| (b, self.share_look(b).1))),
                false => bs.push((Btn::Share, self.share_look(Btn::Share).1)),
            }
        }
        for (b, label) in bs {
            // Layar HP: tombol share cukup ikon.
            let icon_only = self.narrow() && matches!(b, Btn::Share | Btn::CopyLink | Btn::Unshare);
            let bw = if icon_only { 32.0 * s } else { (28.0 + 16.0) * s + self.tw(Face::Medium, px, label) };
            x -= bw;
            v.push((Rect::new(x.round(), (8.0 * s).round(), bw.round(), (32.0 * s).round()), b));
            x -= 4.0 * s;
        }
        v
    }

    /// Layar selebar HP (< 640 px CSS).
    fn narrow(&self) -> bool {
        self.size.0 < 640.0 * self.s
    }

    fn mode_label(&self) -> &'static str {
        if self.mode == Mode::Edit {
            "Read"
        } else {
            "Edit"
        }
    }

    /// Tombol dialog Settings (koordinat layar, sudah dikurangi scroll isi).
    fn settings_hits(&mut self) -> Vec<(Rect, Btn)> {
        let s = self.s;
        let d = self.dialog();
        let mut v = vec![(
            Rect::new(d.x + d.w - 44.0 * s, d.y + 12.0 * s, (32.0 * s).round(), (32.0 * s).round()),
            Btn::SetClose,
        )];
        // Tab: label + (lencana jumlah untuk Trash), jarak 24.
        let px = 14.0 * s;
        let mut x = d.x + 24.0 * s;
        for (i, label) in ["General", "Trash", "Sync"].iter().enumerate() {
            let mut tw = self.tw(Face::Medium, px, label);
            if i == 1 && !self.trash.is_empty() {
                tw += 8.0 * s + self.badge_w();
            }
            v.push((Rect::new(x, d.y + 56.0 * s, tw, 40.0 * s), Btn::Tab(i as u8)));
            x += tw + 24.0 * s;
        }
        let (bx, bw) = (d.x + 24.0 * s, d.w - 48.0 * s);
        let mut y = d.y + (56.0 + 41.0 + 24.0) * s;
        let row = |v: &mut Vec<(Rect, Btn)>, y: f32, n: usize, f: &dyn Fn(usize) -> Btn| {
            let cw = (bw - 8.0 * s * (n - 1) as f32) / n as f32;
            for i in 0..n {
                let x = bx + i as f32 * (cw + 8.0 * s);
                v.push((Rect::new(x.round(), y.round(), cw.round(), (36.0 * s).round()), f(i)));
            }
        };
        if self.settings == Some(0) {
            // Bagian: judul 20 + jarak 8 + kontrol 36; antarbagian 24.
            let sec = 88.0 * s;
            row(&mut v, y + 28.0 * s, 3, &|i| Btn::Theme(i as u8));
            y += sec;
            let q = |x: f32| Rect::new(x.round(), (y + 28.0 * s).round(), (36.0 * s).round(), (36.0 * s).round());
            v.push((q(bx), Btn::FsDown));
            v.push((q(bx + 116.0 * s), Btn::FsUp));
            y += sec;
            row(&mut v, y + 28.0 * s, 3, &|i| Btn::Width(i as u8));
            y += sec;
            row(&mut v, y + 28.0 * s, 2, &|i| Btn::OpenIn(i as u8));
            // Sebaris dengan teks versi (kotak 20 di y + 28).
            let uw = (self.tw(Face::Medium, px, self.update_label()) + 32.0 * s).round();
            v.push((Rect::new(bx + bw - uw, (y + sec + 20.0 * s).round(), uw, (36.0 * s).round()), Btn::Update));
        } else if self.settings == Some(2) {
            // Teks pembuka 2 baris, lalu dua isian (judul 20 + 8 + 36) — token
            // dulu, lalu URL — masing-masing dengan tombol di kanan, lalu tombol.
            let top = self.fonts.as_mut().map_or(64.0, |f| sync_intro(f, s, bw).1);
            let fy = |k: f32| (y + (top + 28.0) * s + k * 88.0 * s).round();
            let on = self.sync.on();
            let aw = if on { 0.0 } else { (112.0 * s).round() };
            let fh = (36.0 * s).round();
            for (k, (f, a)) in [(1u8, Btn::GenToken), (0, Btn::Deploy)].into_iter().enumerate() {
                v.push((Rect::new(bx, fy(k as f32), bw - aw, fh), Btn::Field(f)));
                if !on {
                    v.push((Rect::new(bx + bw - aw + 8.0 * s, fy(k as f32), aw - 8.0 * s, fh), a));
                }
            }
            let by = y + (top + 2.0 * 88.0 + 8.0) * s;
            if self.sync.on() {
                row(&mut v, by, 3, &|i| [Btn::SyncConnect, Btn::SyncNow, Btn::CopyCode][i]);
                row(&mut v, by + 44.0 * s, 1, &|_| Btn::CopyMcp);
            } else {
                row(&mut v, by, 2, &|i| [Btn::SyncConnect, Btn::PasteCode][i]);
            }
        } else {
            if !self.trash.is_empty() {
                let ew = self.tw(Face::Medium, px, "Empty trash") + 24.0 * s;
                v.push((Rect::new(bx + bw - ew, y + 2.0 * s, ew, 32.0 * s), Btn::EmptyTrash));
            }
            // Baris: 12 + 20 + 2 + 16 + 12 + garis 1.
            let mut ry = y + (36.0 + 16.0 + 1.0) * s;
            let rw = self.tw(Face::Medium, px, "Restore") + (20.0 + 22.0) * s;
            for i in 0..self.trash.len() {
                let cy = ry + 31.0 * s;
                v.push((Rect::new(bx + bw - 44.0 * s - rw, cy - 16.0 * s, rw, 32.0 * s), Btn::Restore(i)));
                v.push((Rect::new(bx + bw - 32.0 * s, cy - 16.0 * s, 32.0 * s, 32.0 * s), Btn::Purge(i)));
                ry += 63.0 * s;
            }
        }
        v
    }

    fn badge_w(&mut self) -> f32 {
        let n = self.trash.len().to_string();
        (self.tw(Face::Regular, 12.0 * self.s, &n) + 12.0 * self.s).max(20.0 * self.s)
    }

    /// Jalankan tombol.
    fn click(&mut self, b: Btn) {
        let (cx, cy) = (self.size.0 / 2.0, self.size.1 / 2.0);
        match b {
            Btn::Side => self.side_open = !self.side_open,
            Btn::Search => {
                if self.search_open {
                    self.close_search();
                } else {
                    self.open_search();
                }
            }
            Btn::New => self.new_note(""),
            Btn::Clear => {
                self.search = Doc::default();
                self.refilter();
                self.focus = Focus::Search;
            }
            Btn::Settings => self.open_settings(0),
            Btn::Mode => self.toggle_mode(),
            Btn::Delete => self.delete_note(),
            Btn::Menu(i) => self.menu_do(i),
            Btn::Share => self.share(true),
            Btn::Unshare => self.share(false),
            Btn::CopyLink => {
                if let Some(u) = self.sync.share_url(&self.store.notes[self.cur].id) {
                    clip_set(&u);
                    self.notice = "Link copied".into();
                }
            }
            Btn::ZoomOut => self.zoom_by(1.0 / 1.2, cx, cy),
            Btn::ZoomIn => self.zoom_by(1.2, cx, cy),
            Btn::ZoomSave => self.download_view(),
            Btn::ZoomClose => {
                self.zoom = None;
                self.set_icon(CursorIcon::Default);
            }
            Btn::SetClose => self.settings = None,
            Btn::Tab(i) => self.open_settings(i),
            Btn::Theme(i) => {
                self.theme_pref = i;
                self.apply_theme();
            }
            Btn::FsDown => self.fs = (self.fs - 1.0).max(12.0),
            Btn::FsUp => self.fs = (self.fs + 1.0).min(24.0),
            Btn::Width(i) => self.col = WIDTHS[i as usize],
            Btn::OpenIn(i) => self.open_in = if i == 0 { Mode::Edit } else { Mode::Read },
            Btn::Update => self.update(),
            Btn::EmptyTrash => {
                for t in std::mem::take(&mut self.trash) {
                    let _ = self.store.purge(&t.name);
                }
            }
            Btn::Restore(i) => {
                if let Some(t) = self.trash.get(i) {
                    match self.store.restore(&t.name) {
                        Ok(k) => {
                            self.refilter();
                            self.open_note(k);
                            self.sync_soon();
                        }
                        Err(e) => self.status = format!("Couldn't restore: {e}"),
                    }
                }
                self.trash = self.store.trashed();
            }
            Btn::Field(i) => {
                let d = &mut self.sync_in[i as usize];
                d.set_cursor(d.text.len(), false);
                self.sync_edit = Some(i as usize);
            }
            Btn::SyncConnect => self.sync_connect(),
            Btn::GenToken => {
                let tok = sync::new_token();
                clip_set(&tok);
                self.sync_in[1] = Doc::new(&tok);
                self.sync_msg = Some("Token copied. Paste it when Cloudflare asks for TOKEN.");
            }
            Btn::Deploy => open_url(DEPLOY_URL),
            Btn::SyncNow => self.sync_due = Some(Instant::now()),
            Btn::PasteCode => {
                #[cfg(target_arch = "wasm32")]
                crate::web::clip_read(); // lanjut di `Ev::Kb(7, …)`
                #[cfg(not(target_arch = "wasm32"))]
                self.paste_code(clip_get().unwrap_or_default());
            }
            Btn::CopyMcp => {
                clip_set(&self.sync.mcp_url());
                self.sync_msg = Some("Copied. In Claude: Settings → Connectors → Add custom connector.");
            }
            Btn::CopyCode => {
                clip_set(&self.sync.code());
                self.sync_msg = Some("Code copied. On another device, press Paste code.");
            }
            Btn::MatchPrev => self.goto_match(-1),
            Btn::MatchNext => self.goto_match(1),
            Btn::Purge(i) => {
                if let Some(t) = self.trash.get(i) {
                    let _ = self.store.purge(&t.name);
                }
                self.trash = self.store.trashed();
            }
        }
        if matches!(b, Btn::Theme(_) | Btn::FsDown | Btn::FsUp | Btn::Width(_) | Btn::OpenIn(_) | Btn::Side) {
            self.save_state();
            self.clamp_scroll();
        }
    }

    /// Kemunculan pencarian di catatan aktif: mode edit = rentang byte teks;
    /// mode baca = (item, glyph awal, jumlah glyph) di tampilan baca.
    fn ed_matches(&self) -> Vec<Range<usize>> {
        find_bytes(&self.doc.text, &terms_of(self.query()))
    }

    fn rd_matches(&self) -> Vec<(usize, usize, usize)> {
        let terms = terms_of(self.query());
        if terms.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (k, (_, _, lay)) in self.rd.items.iter().enumerate() {
            let gs = lay.rows.iter().flat_map(|r| &r.gs).map(|g| g.c);
            out.extend(find_terms(gs, &terms).into_iter().map(|(i, n)| (k, i, n)));
        }
        out
    }

    fn match_count(&mut self) -> usize {
        self.layout_views();
        if self.mode == Mode::Edit {
            self.ed_matches().len()
        } else {
            self.rd_matches().len()
        }
    }

    /// Pindah ke kemunculan berikut/sebelumnya dan gulir ke sana.
    fn goto_match(&mut self, d: isize) {
        let n = self.match_count();
        if n == 0 {
            return;
        }
        self.match_idx = (self.match_idx as isize + d).rem_euclid(n as isize) as usize;
        self.reveal_match();
    }

    fn reveal_match(&mut self) {
        let n = self.match_count();
        if n == 0 {
            return;
        }
        let i = self.match_idx.min(n - 1);
        let view = self.view_h();
        let y = if self.mode == Mode::Edit {
            let m = self.ed_matches()[i].clone();
            self.doc.set_selection(m.start, m.end);
            self.ed.caret(m.start).1
        } else {
            let (k, g, _) = self.rd_matches()[i];
            let (_, iy, lay) = &self.rd.items[k];
            let mut seen = 0;
            let row = lay.rows.iter().find(|r| {
                seen += r.gs.len();
                seen > g
            });
            iy + row.map_or(0.0, |r| r.top)
        };
        // Taruh di sepertiga atas layar.
        self.scroll[self.mode as usize] = y - view / 3.0;
        self.clamp_scroll();
    }

    fn open_settings(&mut self, tab: u8) {
        self.settings = Some(tab);
        self.sync_edit = (tab == 2 && !self.sync.on()).then_some(1);
        self.trash = self.store.trashed();
    }

    fn open_search(&mut self) {
        self.side_open = true;
        self.search_open = true;
        self.focus = Focus::Search;
        self.search.select_all();
    }

    /// Buka pencarian berisi `q` (klik tag: daftar catatan bertag sama,
    /// kursor di akhir untuk menambah kata).
    fn search_for(&mut self, q: &str) {
        self.open_search();
        self.search = Doc::new(q);
        self.list_scroll = 0.0;
        self.refilter();
    }

    fn close_search(&mut self) {
        self.search_open = false;
        self.search = Doc::default();
        self.refilter();
        self.focus = Focus::Editor;
    }

    /// Bangun ulang layout mode yang aktif    /// Bangun ulang layout mode yang aktif bila dokumen/ukuran berubah.
    fn layout_views(&mut self) {
        let geo = self.geo();
        let cs = self.cs();
        let Some(f) = self.fonts.as_mut() else { return };
        match self.mode {
            Mode::Edit => self.ed.build(&self.doc, f, geo.col_w, cs, &self.theme),
            Mode::Read => {
                let key = Some((
                    self.doc.version,
                    geo.content.w.to_bits() ^ geo.col_w.to_bits().rotate_left(13),
                    cs.to_bits(),
                    self.theme.dark,
                ));
                if self.rd.key != key {
                    self.rd = build_read(
                        &self.doc.text,
                        f,
                        geo.col_w,
                        (geo.content.w - 2.0 * GUTTER * self.s).max(geo.col_w),
                        cs,
                        &self.theme,
                        &mut self.imgs,
                        &self.store.dir,
                    );
                    self.rd.key = key;
                }
            }
        }
    }

    /// Tinggi area konten yang terlihat untuk teks (tanpa padding atas).
    fn view_h(&self) -> f32 {
        self.geo().content.h - (TOP + 24.0) * self.s
    }

    fn clamp_scroll(&mut self) {
        self.layout_views();
        let m = self.mode as usize;
        let h = if self.mode == Mode::Edit {
            self.ed.h
        } else {
            self.rd.h
        };
        let max = h + (TOP + BOTTOM) * self.s - self.geo().content.h;
        self.scroll[m] = self.scroll[m].clamp(0.0, max.max(0.0));
    }

    /// Gulir seperlunya supaya cursor terlihat.
    fn reveal(&mut self) {
        if self.mode != Mode::Edit {
            return;
        }
        self.layout_views();
        let (_, top, h) = self.ed.caret(self.doc.cursor);
        let view = self.view_h();
        let sc = &mut self.scroll[0];
        if top < *sc {
            *sc = top;
        } else if top + h > *sc + view {
            *sc = top + h - view;
        }
        self.clamp_scroll();
    }

    fn reveal_row(&mut self) {
        let Some(k) = self.res.iter().position(|&i| i == self.cur) else {
            return;
        };
        let (rh, lh) = (ROW_H * self.s, self.geo().list.h);
        let y = k as f32 * rh;
        if y < self.list_scroll {
            self.list_scroll = y;
        } else if y + rh > self.list_scroll + lh {
            self.list_scroll = y + rh - lh;
        }
    }

    fn word_count(&mut self) -> usize {
        if self.words.0 != self.doc.version {
            self.words = (self.doc.version, self.doc.text.split_whitespace().count());
        }
        self.words.1
    }

    fn ed_origin(&self, geo: &Geo) -> (f32, f32) {
        (
            geo.col_x,
            geo.content.y + TOP * self.s - self.scroll[self.mode as usize],
        )
    }

    fn search_pos(&mut self, geo: &Geo, x: f32) -> usize {
        let px = 14.0 * self.s;
        let mut cx = geo.search.x + 36.0 * self.s;
        let Some(f) = self.fonts.as_mut() else {
            return 0;
        };
        for (i, c) in self.search.text.char_indices() {
            let a = f.adv(Face::Regular, px, c);
            if x < cx + a / 2.0 {
                return i;
            }
            cx += a;
        }
        self.search.text.len()
    }

    // --- input ----------------------------------------------------------

    fn key(&mut self, ev: KeyEvent) {
        if ev.state != ElementState::Pressed || !self.preedit.is_empty() {
            return;
        }
        // AltGr di Windows dilaporkan sebagai Ctrl+Alt; tanpa `!alt_key`,
        // karakter seperti '@' di layout Jerman/Polandia akan tertelan.
        if self.settings.is_some() {
            let m = self.mods;
            let ctrl = (m.control_key() && !m.alt_key()) || m.super_key();
            match (&ev.logical_key, self.sync_edit.filter(|_| self.settings == Some(2))) {
                (Key::Named(NamedKey::Escape), _) => self.settings = None,
                (Key::Named(NamedKey::Enter), Some(_)) => self.sync_connect(),
                (Key::Named(NamedKey::Tab), Some(i)) => self.sync_edit = Some(1 - i),
                (_, Some(i)) => {
                    edit_common(&mut self.sync_in[i], &ev, ctrl, m.shift_key(), false);
                    self.sync_split();
                }
                _ => {}
            }
            self.redraw();
            return;
        }
        if self.zoom.is_some() {
            let (cx, cy) = (self.size.0 / 2.0, self.size.1 / 2.0);
            match &ev.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.zoom = None;
                    self.set_icon(CursorIcon::Default);
                }
                Key::Character(c) if c == "+" || c == "=" => self.zoom_by(1.2, cx, cy),
                Key::Character(c) if c == "-" => self.zoom_by(1.0 / 1.2, cx, cy),
                Key::Character(c) if c == "0" => {
                    if let Some(z) = &mut self.zoom {
                        z.k = 0.0;
                    }
                }
                _ => {}
            }
            self.redraw();
            return;
        }
        if ev.logical_key == Key::Named(NamedKey::F3) && !self.query().is_empty() {
            self.goto_match(if self.mods.shift_key() { -1 } else { 1 });
            self.redraw();
            return;
        }
        let m = self.mods;
        let (ctrl, shift) = (
            (m.control_key() && !m.alt_key()) || m.super_key(),
            m.shift_key(),
        );
        if let (true, Key::Character(c)) = (ctrl, &ev.logical_key) {
            match c.to_lowercase().as_str() {
                // Clipboard tanpa teks tapi berisi gambar: simpan lalu tautkan.
                "v" if self.focus == Focus::Editor
                    && clip_get().is_none_or(|t| t.is_empty())
                    && self.paste_image() => {}
                "n" => self.new_note(""),
                "f" | "l" => self.open_search(),
                "\\" => self.click(Btn::Side),
                "e" => self.toggle_mode(),
                "s" => self.save_now(),
                _ => self.key_focused(&ev, ctrl, shift),
            }
        } else {
            self.key_focused(&ev, ctrl, shift);
        }
        self.redraw();
    }

    fn key_focused(&mut self, ev: &KeyEvent, ctrl: bool, shift: bool) {
        match (self.focus, self.mode) {
            (Focus::Search, _) => self.key_search(ev, ctrl, shift),
            (Focus::Editor, Mode::Edit) => self.key_editor(ev, ctrl, shift),
            (Focus::Editor, Mode::Read) => self.key_read(ev, ctrl),
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = if self.mode == Mode::Edit {
            Mode::Read
        } else {
            Mode::Edit
        };
        self.focus = Focus::Editor;
        self.reveal();
    }

    fn search_enter(&mut self) {
        let q = self.search.text.trim().to_owned();
        // Hanya hasil persis yang dibuka Enter: judul baru yang kebetulan
        // mirip catatan lain tetap jadi catatan baru (pola NV).
        if !q.is_empty() {
            match self.res.first().filter(|_| self.exact > 0) {
                Some(&i) => self.open_note(i),
                None => {
                    self.search = Doc::default();
                    self.new_note(&format!("{q}\n"));
                }
            }
        }
        self.focus = Focus::Editor;
    }

    /// Keyboard layar (web): teks/preedit lewat jalur IME, Enter dan Backspace
    /// ke isian yang aktif.
    fn soft_key(&mut self, k: u8, text: String) {
        match k {
            1 => self.ime(Ime::Preedit(text, None)),
            2 => {
                self.preedit.clear();
                if !text.is_empty() {
                    self.ime(Ime::Commit(text));
                }
            }
            3 | 4 => {
                let enter = k == 3;
                if self.settings.is_some() {
                    match self.sync_edit.filter(|_| self.settings == Some(2)) {
                        Some(_) if enter => self.sync_connect(),
                        Some(i) => _ = self.sync_in[i].backspace(),
                        None => {}
                    }
                } else if self.focus == Focus::Search {
                    if enter {
                        self.search_enter();
                    } else {
                        self.search.backspace();
                        self.refilter();
                    }
                } else if self.mode == Mode::Edit {
                    if enter {
                        self.enter();
                    } else {
                        self.doc.backspace();
                    }
                    self.want_x = None;
                    self.changed();
                    self.reveal();
                }
            }
            _ => {}
        }
        self.redraw();
    }

    /// Ada isian teks yang aktif: keyboard layar boleh muncul saat diketuk.
    fn kb_wanted(&self) -> bool {
        if self.settings.is_some() {
            self.settings == Some(2) && self.sync_edit.is_some()
        } else {
            self.zoom.is_none() && (self.focus == Focus::Search || self.mode == Mode::Edit)
        }
    }

    fn key_search(&mut self, ev: &KeyEvent, ctrl: bool, shift: bool) {
        match &ev.logical_key {
            Key::Named(NamedKey::Enter) => self.search_enter(),
            Key::Named(NamedKey::Escape) => self.close_search(),
            Key::Named(k @ (NamedKey::ArrowDown | NamedKey::ArrowUp)) => {
                let k = match (self.res.iter().position(|&i| i == self.cur), k) {
                    (None, _) => 0,
                    (Some(p), NamedKey::ArrowDown) => (p + 1).min(self.res.len().saturating_sub(1)),
                    (Some(p), _) => p.saturating_sub(1),
                };
                if let Some(&i) = self.res.get(k) {
                    self.open_note(i);
                    self.reveal_row();
                }
            }
            Key::Named(NamedKey::Home) => self.search.set_cursor(0, shift),
            Key::Named(NamedKey::End) => self.search.set_cursor(self.search.text.len(), shift),
            _ => {
                let v = self.search.version;
                if edit_common(&mut self.search, ev, ctrl, shift, false) && self.search.version != v
                {
                    self.refilter();
                    self.list_scroll = 0.0;
                    self.match_idx = 0;
                }
            }
        }
    }

    fn key_editor(&mut self, ev: &KeyEvent, ctrl: bool, shift: bool) {
        let mut vertical = false;
        match &ev.logical_key {
            Key::Named(
                k @ (NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::PageUp
                | NamedKey::PageDown),
            ) => {
                self.vertical(*k, shift);
                vertical = true;
            }
            Key::Named(NamedKey::Home) if ctrl => self.doc.set_cursor(0, shift),
            Key::Named(NamedKey::End) if ctrl => self.doc.set_cursor(self.doc.text.len(), shift),
            Key::Named(k @ (NamedKey::Home | NamedKey::End)) => {
                self.layout_views();
                let (_, top, h) = self.ed.caret(self.doc.cursor);
                let x = if *k == NamedKey::Home {
                    0.0
                } else {
                    f32::INFINITY
                };
                let p = self.ed.pos_at(x, top + h / 2.0);
                self.doc.set_cursor(p, shift);
            }
            Key::Named(NamedKey::Enter) => self.enter(),
            Key::Named(NamedKey::Tab) => self.doc.insert("\t"),
            Key::Named(NamedKey::Escape) => return,
            _ => {
                if !edit_common(&mut self.doc, ev, ctrl, shift, true) {
                    return;
                }
            }
        }
        if !vertical {
            self.want_x = None;
        }
        self.changed();
        self.reveal();
    }

    /// Enter melanjutkan list; Enter di item kosong mengakhiri list.
    fn enter(&mut self) {
        let (a, b) = self.doc.selection();
        if a == b {
            let ls = self.doc.line_start(a);
            if let Some((plen, next)) = md::continue_list(&self.doc.text[ls..a]) {
                if a - ls == plen {
                    self.doc.set_selection(ls, a);
                    self.doc.insert("");
                } else {
                    self.doc.insert(&format!("\n{next}"));
                }
                return;
            }
        }
        self.doc.insert("\n");
    }

    fn vertical(&mut self, k: NamedKey, shift: bool) {
        self.layout_views();
        let (x, top, h) = self.ed.caret(self.doc.cursor);
        let x = *self.want_x.get_or_insert(x);
        let page = self.view_h().max(h);
        let y = match k {
            NamedKey::ArrowUp => top - 1.0,
            NamedKey::ArrowDown => top + h + 1.0,
            NamedKey::PageUp => top - page,
            _ => top + h + page,
        };
        let p = if y < 0.0 {
            0
        } else if y >= self.ed.h {
            self.doc.text.len()
        } else {
            self.ed.pos_at(x, y)
        };
        self.doc.set_cursor(p, shift);
    }

    fn key_read(&mut self, ev: &KeyEvent, ctrl: bool) {
        let step = BODY * LH * self.s;
        let d = match &ev.logical_key {
            Key::Character(c) if ctrl => {
                match c.to_lowercase().as_str() {
                    "a" if !self.rd.items.is_empty() => {
                        self.rd.sel = Some(((0, 0), (self.rd.items.len() - 1, usize::MAX)))
                    }
                    "c" => {
                        let t = self.rd.selected_text();
                        if !t.is_empty() {
                            clip_set(&t);
                        }
                    }
                    _ => {}
                }
                return;
            }
            Key::Named(NamedKey::ArrowUp) => -step,
            Key::Named(NamedKey::ArrowDown) => step,
            Key::Named(NamedKey::PageUp) => -self.view_h(),
            Key::Named(NamedKey::PageDown) | Key::Named(NamedKey::Space) => self.view_h(),
            Key::Named(NamedKey::Home) => -f32::INFINITY,
            Key::Named(NamedKey::End) => f32::INFINITY,
            _ => return,
        };
        self.scroll[1] = (self.scroll[1] + d).clamp(-1.0, 1e9);
        self.clamp_scroll();
    }

    fn ime(&mut self, ime: Ime) {
        match ime {
            Ime::Preedit(s, _) => self.preedit = s,
            Ime::Commit(s) => {
                self.preedit.clear();
                if self.settings.is_some() {
                    if let Some(i) = self.sync_edit.filter(|_| self.settings == Some(2)) {
                        self.sync_in[i].insert(&s);
                        self.sync_split();
                    }
                    self.redraw();
                    return;
                }
                match (self.focus, self.mode) {
                    (Focus::Search, _) => {
                        self.search.insert(&s);
                        self.refilter();
                    }
                    (Focus::Editor, Mode::Edit) => {
                        self.doc.insert(&s);
                        self.changed();
                        self.reveal();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        self.redraw();
    }

    fn press(&mut self) {
        let (x, y) = self.mouse;
        if let Some(b) = self.btn_at(x, y) {
            self.click(b);
            self.redraw();
            return;
        }
        if self.settings.is_some() {
            // Klik di luar dialog menutupnya.
            if !self.dialog().contains(x, y) {
                self.settings = None;
            }
            self.redraw();
            return;
        }
        if let Some(z) = &mut self.zoom {
            z.grab = true;
            self.set_icon(CursorIcon::Grabbing);
            return;
        }
        // Klik di luar menu konteks hanya menutupnya.
        if self.menu.take().is_some() {
            self.redraw();
            return;
        }
        self.dia_press = None;
        let geo = self.geo();
        let shift = self.mods.shift_key();
        let now = Instant::now();
        let (t0, n, (x0, y0)) = self.clicks;
        let near = (x - x0).abs() < 4.0 * self.s && (y - y0).abs() < 4.0 * self.s;
        let n = if now - t0 < Duration::from_millis(450) && near {
            n + 1
        } else {
            1
        };
        self.clicks = (now, n, (x, y));

        // Pita scrollbar didahulukan dari baris/teks di bawahnya.
        self.layout_views();
        for list in [false, true] {
            if let Some(b) = self.bar(&geo, list).filter(|b| b.track.contains(x, y)) {
                let t = b.thumb;
                let grab = if y >= t.y && y < t.y + t.h {
                    y - t.y
                } else {
                    t.h / 2.0
                };
                self.drag = Some(Drag::Bar { list, grab });
                self.drag_bar(list, grab, y);
                self.redraw();
                return;
            }
        }

        if geo.search.h > 0.0 && geo.search.contains(x, y) {
            self.focus = Focus::Search;
            let p = self.search_pos(&geo, x);
            if n >= 2 {
                self.search.select_all();
            } else {
                self.search.set_cursor(p, shift);
                self.drag = Some(Drag::Search);
            }
        } else if geo.list.contains(x, y) {
            if let Some(&i) = self.row_at(x, y).and_then(|k| self.res.get(k)) {
                if i != self.cur {
                    self.open_note(i);
                    self.mode = self.open_in;
                    self.match_idx = 0;
                    self.reveal_match();
                }
                self.focus = Focus::Editor;
                if self.narrow() {
                    self.side_open = false;
                }
            }
        } else if geo.content.contains(x, y) {
            self.focus = Focus::Editor;
            self.layout_views();
            let (ox, oy) = self.ed_origin(&geo);
            let (rx, ry) = (x - ox, y - oy);
            match self.mode {
                Mode::Edit => {
                    let p = self.ed.pos_at(rx, ry);
                    match n {
                        1 => self.doc.set_cursor(p, shift),
                        2 => self.doc.select_word(p),
                        _ => self.doc.select_line(p),
                    }
                    self.want_x = None;
                    self.drag = Some(Drag::Editor);
                }
                Mode::Read => {
                    if let Some((r, done)) = self.rd.check_at(rx, ry, 4.0 * self.s) {
                        self.doc
                            .replace_range(r.start, r.end, if done { "[ ]" } else { "[x]" });
                        self.changed();
                    } else if let Some(u) = self.rd.link_at(rx, ry) {
                        match u.strip_prefix("tag:") {
                            Some(t) => {
                                let q = format!("{t} ");
                                self.search_for(&q);
                            }
                            None => open_url(u),
                        }
                    } else if let Some(p) = self.rd.pos_at(rx, ry) {
                        self.dia_press = self
                            .rd
                            .dias
                            .iter()
                            .position(|d| d.0.contains(rx, ry))
                            .map(|i| (i, (x, y)));
                        let anchor = self.rd.sel.filter(|_| shift).map_or(p, |s| s.0);
                        self.rd.sel = Some((anchor, p));
                        self.drag = Some(Drag::Read);
                    }
                }
            }
        }
        self.redraw();
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        let (mx, my) = self.mouse;
        self.mouse = (x, y);
        let hot_btn = self.btn_at(x, y);
        if let Some(z) = &mut self.zoom {
            if z.grab {
                z.pan = (z.pan.0 + x - mx, z.pan.1 + y - my);
                self.redraw();
            } else {
                self.set_icon(if hot_btn.is_some() { CursorIcon::Pointer } else { CursorIcon::Grab });
            }
            self.set_hot((hot_btn, None, None));
            return;
        }
        if self.settings.is_some() {
            self.set_icon(if hot_btn.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
            self.set_hot((hot_btn, None, None));
            return;
        }
        let geo = self.geo();
        match self.drag {
            Some(Drag::Editor) => {
                self.layout_views();
                let (ox, oy) = self.ed_origin(&geo);
                let p = self.ed.pos_at(x - ox, y - oy);
                self.doc.set_cursor(p, true);
                self.redraw();
            }
            Some(Drag::Search) => {
                let p = self.search_pos(&geo, x);
                self.search.set_cursor(p, true);
                self.redraw();
            }
            Some(Drag::Read) => {
                self.layout_views();
                let (ox, oy) = self.ed_origin(&geo);
                if let (Some(p), Some(s)) = (self.rd.pos_at(x - ox, y - oy), self.rd.sel.as_mut()) {
                    s.1 = p;
                }
                self.redraw();
            }
            Some(Drag::Bar { list, grab }) => {
                self.drag_bar(list, grab, y);
                self.redraw();
            }
            None => {}
        }
        let on_bar =
            [false, true].map(|l| self.bar(&geo, l).is_some_and(|b| b.track.contains(x, y)));
        if on_bar != self.bar_hot {
            self.bar_hot = on_bar;
            self.redraw(); // thumb menebal/menipis
        }
        let (ox, oy) = self.ed_origin(&geo);
        let in_content = geo.content.contains(x, y) && !on_bar[0];
        let read_hit = self.mode == Mode::Read
            && in_content
            && (self.rd.link_at(x - ox, y - oy).is_some()
                || self.rd.check_at(x - ox, y - oy, 4.0 * self.s).is_some());
        let row = (!on_bar[1] && geo.list.contains(x, y))
            .then(|| self.row_at(x, y))
            .flatten()
            .filter(|&k| k < self.res.len());
        let row_hit = row.is_some();
        let dia = (self.mode == Mode::Read && in_content)
            .then(|| self.rd.dias.iter().position(|d| d.0.contains(x - ox, y - oy)))
            .flatten();
        self.set_hot((hot_btn, row, dia));
        let dia_hit = self.mode == Mode::Read
            && in_content
            && self.rd.dias.iter().any(|d| d.0.contains(x - ox, y - oy));
        let icon = if hot_btn.is_some() {
            CursorIcon::Pointer
        } else if dia_hit && !read_hit {
            CursorIcon::ZoomIn
        } else if (geo.search.h > 0.0 && geo.search.contains(x, y)) || (in_content && !read_hit) {
            CursorIcon::Text
        } else if row_hit || read_hit {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };
        self.set_icon(icon);
    }

    fn btn_at(&mut self, x: f32, y: f32) -> Option<Btn> {
        self.hits().into_iter().find(|h| h.0.contains(x, y)).map(|h| h.1)
    }

    /// Baris daftar di titik (x, y), tanpa sela antarbaris.
    fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let (s, l) = (self.s, self.geo().list);
        let dy = y - l.y + self.list_scroll;
        let k = (dy / (ROW_H * s)) as usize;
        (x >= 8.0 * s && x < l.w - 8.0 * s && dy >= 0.0 && dy - k as f32 * ROW_H * s < 80.0 * s).then_some(k)
    }

    fn set_hot(&mut self, hot: (Option<Btn>, Option<usize>, Option<usize>)) {
        if hot != self.hot {
            self.hot = hot;
            self.redraw();
        }
    }

    fn set_icon(&mut self, icon: CursorIcon) {
        if icon != self.icon {
            self.icon = icon;
            if let Some(w) = &self.window {
                w.set_cursor(icon);
            }
        }
    }

    fn release(&mut self) {
        self.drag = None;
        if let Some(z) = &mut self.zoom {
            z.grab = false;
            self.set_icon(CursorIcon::Grab);
            return;
        }
        // Klik (tanpa geser) pada diagram: buka penampil.
        let Some((i, (x0, y0))) = self.dia_press.take() else {
            return;
        };
        let (x, y) = self.mouse;
        if (x - x0).abs() > 4.0 * self.s || (y - y0).abs() > 4.0 * self.s {
            return;
        }
        self.open_viewer(i);
    }

    /// Debug tangkapan layar: buka layar tertentu tanpa input.
    fn shot_ui(&mut self, ui: &str) {
        for part in ui.split(',') {
            match part.split_once(':').unwrap_or((part, "")) {
                ("search", q) => {
                    self.open_search();
                    self.search.insert(q);
                    self.refilter();
                }
                ("settings", _) => self.open_settings(0),
                ("trash", _) => self.open_settings(1),
                ("sync", _) => self.open_settings(2),
                ("dark", _) => {
                    self.theme_pref = 2;
                    self.apply_theme();
                }
                ("noside", _) => self.side_open = false,
                ("error", m) => self.status = m.to_owned(),
                ("viewer", _) => {
                    self.mode = Mode::Read;
                    self.layout_views();
                    self.open_viewer(0);
                }
                ("scroll", v) => {
                    self.layout_views();
                    self.scroll[self.mode as usize] = v.parse().unwrap_or(0.0);
                    self.clamp_scroll();
                }
                ("menu", xy) => {
                    if let Some((x, y)) = xy.split_once('x') {
                        self.layout_views();
                        self.mouse_move(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                        self.right_click();
                    }
                }
                ("hover", xy) => {
                    if let Some((x, y)) = xy.split_once('x') {
                        let (x, y) = (x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                        self.layout_views();
                        self.mouse_move(x, y);
                    }
                }
                _ => {}
            }
        }
    }

    /// Buka diagram ke-`i` mode baca di penampil layar penuh.
    fn open_viewer(&mut self, i: usize) {
        let Some((_, src, kind)) = self.rd.dias.get(i) else {
            return;
        };
        self.zoom = Some(Zoom {
            src: src.clone(),
            kind: *kind,
            k: 0.0,
            built: -1.0,
            pan: (0.0, 0.0),
            grab: false,
            view: RdView::default(),
            org: (0.0, 0.0),
            touches: Vec::new(),
            msg: None,
        });
        self.rd.sel = None;
        self.set_icon(CursorIcon::Grab);
        self.redraw();
    }

    /// Satu jari: diam lalu lepas = klik di situ; bergeser > 10 px = gulir
    /// daftar/isi di bawah titik awal (jari ke bawah = isi ikut turun).
    fn touch_ui(&mut self, id: u64, phase: TouchPhase, pos: (f32, f32)) {
        match phase {
            TouchPhase::Started if self.tap.is_none() => {
                self.glide = None; // jari menahan lemparan yang sedang jalan
                let now = Instant::now();
                self.tap = Some(Tap { id, start: pos, last: pos, moved: false, t0: now, held: false, v: 0.0, tl: now });
            }
            TouchPhase::Moved => {
                let slop = 10.0 * self.s;
                let Some(tp) = self.tap.as_mut().filter(|tp| tp.id == id) else {
                    return;
                };
                if tp.held {
                    return self.mouse_move(pos.0, pos.1); // perluas seleksi
                }
                tp.moved |= (pos.0 - tp.start.0).hypot(pos.1 - tp.start.1) > slop;
                let (d, start, moved) = ((pos.0 - tp.last.0, pos.1 - tp.last.1), tp.start, tp.moved);
                tp.last = pos;
                let now = Instant::now();
                let dt = (now - tp.tl).as_secs_f32().max(0.001);
                tp.v = 0.7 * (-d.1 / dt) + 0.3 * tp.v;
                tp.tl = now;
                if moved {
                    if let Some(list) = self.scroll_area(start.0, start.1) {
                        self.scroll_by(list, -d.1);
                        self.redraw();
                    }
                }
            }
            TouchPhase::Ended if self.tap.as_ref().is_some_and(|tp| tp.id == id) => {
                let tp = self.tap.take();
                // Lepas sambil bergerak: isi terus meluncur.
                if let Some(t) = tp.as_ref().filter(|t| t.moved && !t.held && t.tl.elapsed() < Duration::from_millis(80)) {
                    if let (Some(list), true) = (self.scroll_area(t.start.0, t.start.1), t.v.abs() > 100.0) {
                        self.glide = Some(Glide { list, rest: 0.0, v: t.v.clamp(-8000.0, 8000.0), t: Instant::now() });
                    }
                }
                if tp.as_ref().is_some_and(|tp| tp.held) {
                    self.release();
                    if let Some(m) = &mut self.menu {
                        (m.0, m.1) = pos; // menu di dekat jari terakhir
                    }
                } else if !tp.is_some_and(|tp| tp.moved) {
                    self.mouse_move(pos.0, pos.1);
                    self.press();
                    self.release();
                }
                self.redraw();
            }
            TouchPhase::Cancelled => self.tap = None,
            _ => {}
        }
    }

    /// Tekan lama di isi catatan: pilih kata di bawah jari; geser untuk
    /// memperluas, lalu tombol Copy muncul.
    fn long_press(&mut self) {
        let Some(tp) = self.tap.as_mut() else { return };
        tp.held = true;
        let (x, y) = tp.start;
        let geo = self.geo();
        if self.settings.is_some() || !geo.content.contains(x, y) || (self.side_open && self.narrow()) {
            return;
        }
        self.mouse_move(x, y);
        match self.mode {
            Mode::Edit => {
                self.clicks = (Instant::now(), 1, (x, y)); // press berikut = klik ganda: pilih kata
                self.press();
            }
            Mode::Read => {
                self.layout_views();
                let (ox, oy) = self.ed_origin(&geo);
                if let Some((k, i)) = self.rd.pos_at(x - ox, y - oy) {
                    let (a, b) = self.rd.word_at(k, i);
                    self.rd.sel = Some(((k, a), (k, b)));
                    self.drag = Some(Drag::Read);
                }
            }
        }
        self.menu = Some((x, y, true));
        self.redraw();
    }

    /// Lapisan yang bisa ditutup tombol back: catatan terbuka di atas daftar
    /// (layar HP), Settings, penampil.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn nav_depth(&self) -> u32 {
        (self.narrow() && !self.side_open) as u32 + self.settings.is_some() as u32 + self.zoom.is_some() as u32
    }

    /// Tombol back: tutup lapisan teratas.
    fn back(&mut self) {
        self.menu = None;
        if self.zoom.is_some() {
            self.click(Btn::ZoomClose);
        } else if self.settings.is_some() {
            self.click(Btn::SetClose);
        } else if self.narrow() && !self.side_open {
            self.save_now();
            self.side_open = true;
        }
    }

    /// Klik kanan di isi catatan: taruh kursor di situ (kecuali di dalam
    /// seleksi), lalu buka menu konteks.
    fn right_click(&mut self) {
        let (x, y) = self.mouse;
        let geo = self.geo();
        if self.settings.is_some() || self.zoom.is_some() || !geo.content.contains(x, y) {
            return;
        }
        if !self.has_selection() {
            self.layout_views();
            let (ox, oy) = self.ed_origin(&geo);
            match self.mode {
                Mode::Edit => {
                    let p = self.ed.pos_at(x - ox, y - oy);
                    self.doc.set_cursor(p, false);
                    self.focus = Focus::Editor;
                }
                Mode::Read => self.rd.sel = None,
            }
        }
        self.menu = Some((x, y, false));
        self.redraw();
    }

    /// Isi menu konteks: (aksi, label, pintasan).
    fn menu_items(&self) -> Vec<(u8, &'static str, &'static str)> {
        let sel = self.has_selection();
        let mut v = Vec::new();
        if self.mode == Mode::Edit && sel {
            v.push((0, "Cut", "Ctrl+X"));
        }
        if sel {
            v.push((1, "Copy", "Ctrl+C"));
        }
        if self.mode == Mode::Edit {
            v.push((2, "Paste", "Ctrl+V"));
        }
        v.push((3, "Select all", "Ctrl+A"));
        if self.sync.on() {
            v.push((4, "Copy note reference", "")); // untuk Claude: "baca tildown:…"
        }
        v
    }

    fn menu_hits(&mut self) -> Vec<(Rect, Btn)> {
        let Some((x, y, touch)) = self.menu else { return Vec::new() };
        let s = self.s;
        let items = self.menu_items();
        let (iw, ih) = ((180.0 * s).round(), (36.0 * s).round());
        let h = ih * items.len() as f32 + 8.0 * s;
        // Di dalam jendela; sentuhan: di atas jari supaya tidak tertutup.
        let x = x.min(self.size.0 - iw - 8.0 * s).max(8.0 * s);
        let y = if touch { y - h - 24.0 * s } else { y };
        let y = y.min(self.size.1 - h - 8.0 * s).max(8.0 * s);
        items
            .iter()
            .enumerate()
            .map(|(k, it)| (Rect::new(x.round(), (y + 4.0 * s + k as f32 * ih).round(), iw, ih), Btn::Menu(it.0)))
            .collect()
    }

    fn menu_do(&mut self, i: u8) {
        self.menu = None;
        match (i, self.mode) {
            (0, Mode::Edit) => {
                clip_set(self.doc.selected_text());
                self.doc.insert("");
                self.changed();
            }
            (1, Mode::Edit) => clip_set(self.doc.selected_text()),
            (1, Mode::Read) => clip_set(&self.rd.selected_text()),
            (2, _) => {
                // Web: clipboard dibaca async, lanjut di `Ev::Kb(6, …)` (paste_text).
                #[cfg(target_arch = "wasm32")]
                crate::web::clip_paste();
                #[cfg(not(target_arch = "wasm32"))]
                if !self.paste_image() {
                    if let Some(t) = clip_get() {
                        self.doc.insert(&t.replace("\r\n", "\n"));
                        self.changed();
                    }
                }
            }
            (4, _) => {
                let id = &self.store.notes[self.cur].id;
                clip_set(&format!("tildown:{id} ({})", store::title_of(&self.doc.text)));
                self.notice = "Note reference copied. Paste it in Claude.".into();
            }
            (3, Mode::Edit) => self.doc.select_all(),
            (3, Mode::Read) => {
                let n = self.rd.items.len();
                self.rd.sel = (n > 0).then_some(((0, 0), (n - 1, usize::MAX)));
            }
            _ => {}
        }
    }

    fn paint_menu(&mut self, p: &mut P, hits: &[(Rect, Btn)]) {
        let (t, s) = (self.theme, self.s);
        let items: Vec<(Rect, u8)> = hits
            .iter()
            .filter_map(|h| match h.1 {
                Btn::Menu(i) => Some((h.0, i)),
                _ => None,
            })
            .collect();
        let (Some(first), Some(last)) = (items.first(), items.last()) else { return };
        let r = Rect::new(first.0.x, first.0.y - 4.0 * s, first.0.w, last.0.y + last.0.h - first.0.y + 8.0 * s);
        p.rrect(Rect::new(r.x, r.y + 2.0 * s, r.w, r.h), 8.0 * s, alpha(t.fg, 0.12)); // bayangan
        p.rrect(r, 8.0 * s, t.bg);
        p.rframe(r, 8.0 * s, p.lw(), t.border);
        let names = self.menu_items();
        for (ir, i) in items {
            let Some(&(_, label, keys)) = names.iter().find(|n| n.0 == i) else { continue };
            let hot = Rect::new(ir.x + 4.0 * s, ir.y, ir.w - 8.0 * s, ir.h);
            self.hover_bg(p, hot, Btn::Menu(i));
            let base = p.base(Face::Regular, 14.0 * s, ir.y, ir.h);
            p.text(ir.x + 14.0 * s, base, Face::Regular, 14.0 * s, label, t.fg, ir.w);
            if !self.narrow() {
                let kw = p.f.width(Face::Regular, 13.0 * s, keys);
                p.text(ir.x + ir.w - 14.0 * s - kw, base, Face::Regular, 13.0 * s, keys, t.dim, kw + 1.0);
            }
        }
    }

    /// Ada teks terpilih di editor / mode baca.
    fn has_selection(&self) -> bool {
        match self.mode {
            Mode::Edit => self.doc.selection().0 != self.doc.selection().1,
            Mode::Read => self.rd.sel.is_some_and(|(a, b)| a != b),
        }
    }

    /// Download di penampil: berkas gambar asli, atau diagram dirender ulang
    /// 2x di atas putih.
    fn download_view(&mut self) {
        let Some(z) = &self.zoom else { return };
        let (src, kind) = (z.src.clone(), z.kind);
        let r = match kind {
            DiaKind::Image => img_bytes(&self.store.dir, &src)
                .ok_or_else(|| "not a local image".to_owned())
                .map(|b| {
                    let name = src.rsplit(['/', '\\']).next().filter(|n| !n.is_empty());
                    (name.unwrap_or("image.png").to_owned(), b)
                }),
            _ => {
                let title: String = store::title_of(&self.doc.text)
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '-' { c } else { ' ' })
                    .collect();
                let title = title.split_whitespace().collect::<Vec<_>>().join("-");
                let name = if title.is_empty() { "diagram.png".into() } else { format!("{title}-diagram.png") };
                diagram_png(&src, kind).map(|b| (name, b))
            }
        };
        let msg = match r.and_then(|(name, b)| save_download(&name, &b)) {
            Ok(path) => format!("Saved to {path}"),
            Err(e) => format!("Couldn't download: {e}"),
        };
        if let Some(z) = &mut self.zoom {
            z.msg = Some(msg);
        }
    }

    /// Sentuhan di penampil: 1 jari geser, 2 jari pinch (zoom + geser).
    fn touch(&mut self, t: Touch) {
        let pos = (t.location.x as f32, t.location.y as f32);
        // Desktop: Windows juga mengirim klik tiruan untuk sentuhan, jadi
        // tap/gulir sendiri hanya di web.
        if self.zoom.is_none() && cfg!(target_arch = "wasm32") {
            return self.touch_ui(t.id, t.phase, pos);
        }
        if t.phase == TouchPhase::Started
            && self.zoom.as_ref().is_some_and(|z| z.touches.is_empty())
        {
            if let Some(b) = self.btn_at(pos.0, pos.1) {
                self.click(b);
                self.redraw();
                return;
            }
        }
        let Some(z) = &mut self.zoom else { return };
        let old = z.touches.clone();
        match t.phase {
            TouchPhase::Started => z.touches.push((t.id, pos)),
            TouchPhase::Moved => {
                if let Some(e) = z.touches.iter_mut().find(|e| e.0 == t.id) {
                    e.1 = pos;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => z.touches.retain(|e| e.0 != t.id),
        }
        if t.phase != TouchPhase::Moved || old.len() != z.touches.len() {
            return;
        }
        // Titik tengah & jarak dua jari pertama, sebelum dan sesudah.
        let mid = |v: &[(u64, (f32, f32))]| {
            let n = v.len().min(2) as f32;
            let c = v.iter().take(2).fold((0.0, 0.0), |a, e| (a.0 + e.1 .0 / n, a.1 + e.1 .1 / n));
            let d = if v.len() >= 2 {
                ((v[0].1 .0 - v[1].1 .0).powi(2) + (v[0].1 .1 - v[1].1 .1).powi(2)).sqrt()
            } else {
                0.0
            };
            (c, d)
        };
        let ((c0, d0), (c1, d1)) = (mid(&old), mid(&z.touches));
        z.pan = (z.pan.0 + c1.0 - c0.0, z.pan.1 + c1.1 - c0.1);
        if d0 > 1.0 && d1 > 1.0 {
            self.zoom_by(d1 / d0, c1.0, c1.1);
        }
        self.redraw();
    }

    /// Zoom penampil dengan faktor `f`; titik (x, y) tetap di tempat.
    fn zoom_by(&mut self, f: f32, x: f32, y: f32) {
        let s = self.s;
        if let Some(z) = self.zoom.as_mut().filter(|z| z.k > 0.0) {
            // Batas: huruf 12 px antara ~3 dan ~240 px layar.
            let k = (z.k * f).clamp(0.25 * s, 20.0 * s);
            let r = k / z.k;
            z.pan = (x - (x - z.pan.0) * r, y - (y - z.pan.1) * r);
            z.k = k;
            self.redraw();
        }
    }

    /// Scrollbar konten (`list` = false) atau daftar catatan. Panggil setelah `layout_views`.
    fn bar(&self, geo: &Geo, list: bool) -> Option<Bar> {
        if list {
            bar(
                geo.list,
                self.res.len() as f32 * ROW_H * self.s + 14.0 * self.s,
                self.list_scroll,
                self.s,
            )
        } else {
            if self.side_open && self.narrow() {
                return None; // isi tertutup daftar
            }
            let h = if self.mode == Mode::Edit {
                self.ed.h
            } else {
                self.rd.h
            };
            bar(
                geo.content,
                h + (TOP + BOTTOM) * self.s,
                self.scroll[self.mode as usize],
                self.s,
            )
        }
    }

    /// Thumb mengikuti pointer di `y`; klik di jalur membuat thumb lompat ke pointer.
    fn drag_bar(&mut self, list: bool, grab: f32, y: f32) {
        let geo = self.geo();
        let Some(b) = self.bar(&geo, list) else {
            return;
        };
        let travel = (b.track.h - b.thumb.h).max(1.0);
        let v = ((y - grab - b.track.y) / travel).clamp(0.0, 1.0) * b.max;
        if list {
            self.list_scroll = v;
        } else {
            self.scroll[self.mode as usize] = v;
        }
    }

    fn wheel(&mut self, d: MouseScrollDelta) {
        if self.settings.is_some() {
            return;
        }
        if self.zoom.is_some() {
            let n = match d {
                MouseScrollDelta::LineDelta(_, y) => y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
            };
            let (x, y) = self.mouse;
            self.zoom_by(1.2f32.powf(n), x, y);
            return;
        }
        // Roda mouse (per baris, atau loncatan besar dari browser): dianimasikan.
        // Touchpad (piksel kecil-kecil, sudah halus): langsung.
        let (dy, smooth) = match d {
            MouseScrollDelta::LineDelta(_, y) => (-y * 3.0 * BODY * LH * self.s, true),
            MouseScrollDelta::PixelDelta(p) => (-p.y as f32, p.y.abs() >= 50.0),
        };
        let (x, y) = self.mouse;
        let Some(list) = self.scroll_area(x, y) else { return };
        if smooth {
            let rest = self.glide.as_ref().filter(|g| g.list == list).map_or(0.0, |g| g.rest);
            self.glide = Some(Glide { list, rest: rest + dy, v: 0.0, t: Instant::now() });
        } else {
            self.glide = None;
            self.scroll_by(list, dy);
        }
        self.redraw();
    }

    /// Bagian yang digulir di titik (x, y): `Some(true)` daftar, `Some(false)` isi.
    fn scroll_area(&self, x: f32, y: f32) -> Option<bool> {
        let geo = self.geo();
        if geo.list.contains(x, y) && self.side_open {
            Some(true)
        } else {
            geo.content.contains(x, y).then_some(false)
        }
    }

    /// Gulir daftar / isi sejauh `dy`; false bila sudah mentok.
    fn scroll_by(&mut self, list: bool, dy: f32) -> bool {
        if list {
            let geo = self.geo();
            let max = (self.res.len() as f32 * ROW_H * self.s + 14.0 * self.s - geo.list.h).max(0.0);
            let old = self.list_scroll;
            self.list_scroll = (old + dy).clamp(0.0, max);
            self.list_scroll != old
        } else {
            let m = self.mode as usize;
            let old = self.scroll[m];
            self.scroll[m] += dy;
            self.clamp_scroll();
            self.scroll[m] != old
        }
    }

    /// Satu langkah animasi gulir (tiap frame).
    fn glide_step(&mut self) {
        let Some(g) = &mut self.glide else { return };
        let now = Instant::now();
        let dt = (now - g.t).as_secs_f32().min(0.05);
        g.t = now;
        // Roda: habiskan sisa jarak dengan ease-out ~80 ms.
        let mut d = if g.rest.abs() < 0.5 { g.rest } else { g.rest * (1.0 - (-dt / 0.08).exp()) };
        g.rest -= d;
        // Lemparan: melambat eksponensial (τ 0,35 detik), seperti gulir HP.
        d += g.v * dt;
        g.v *= (-dt / 0.35).exp();
        if g.v.abs() < 15.0 {
            g.v = 0.0;
        }
        let (list, done) = (g.list, g.rest == 0.0 && g.v == 0.0);
        if !self.scroll_by(list, d) && d != 0.0 || done {
            self.glide = None; // mentok di ujung, atau sudah berhenti
        }
    }

    // --- menggambar -----------------------------------------------------

    fn render(&mut self) {
        self.glide_step();
        self.layout_views();
        self.frame_hits = self.hits();
        self.frame_base = self.base_hits();
        let Some(mut g) = self.gpu.take() else { return };
        let Some(mut f) = self.fonts.take() else {
            self.gpu = Some(g);
            return;
        };
        g.begin();
        self.paint(&mut P {
            g: &mut g,
            f: &mut f,
            s: self.s,
        });
        g.finish(self.theme.bg, &mut f.uploads);
        if std::env::var_os("MARKDOWN_SHOT").is_some() {
            self.redraw(); // debug: terus menggambar sampai frame disimpan
        }
        let again = f.take_cleared();
        (self.gpu, self.fonts) = (Some(g), Some(f));
        if again || self.glide.is_some() {
            self.redraw();
        }
        self.update_ime_area();
    }

    fn update_ime_area(&mut self) {
        let geo = self.geo();
        let (x, y, h) = match (self.focus, self.mode) {
            (Focus::Editor, Mode::Edit) => {
                let (ox, oy) = self.ed_origin(&geo);
                let (cx, cy, ch) = self.ed.caret(self.doc.cursor);
                (ox + cx, oy + cy, ch)
            }
            (Focus::Search, _) => {
                let p = self.search.cursor;
                let x = match self.fonts.as_mut() {
                    Some(f) => f.width(Face::Regular, 14.0 * self.s, &self.search.text[..p]),
                    None => 0.0,
                };
                (geo.search.x + 36.0 * self.s + x, geo.search.y, geo.search.h)
            }
            _ => return,
        };
        let area = (x as i32, y as i32, h as u32);
        if area != self.ime_area {
            self.ime_area = area;
            if let Some(w) = &self.window {
                w.set_ime_cursor_area(
                    PhysicalPosition::new(area.0, area.1),
                    PhysicalSize::new(2, area.2.max(1)),
                );
            }
        }
    }

    fn paint(&mut self, p: &mut P) {
        if self.zoom.is_some() {
            return self.paint_zoom(p);
        }
        let geo = self.geo();
        let hits = self.frame_base.clone();
        let (t, s) = (self.theme, self.s);
        match self.mode {
            Mode::Edit => self.paint_editor(p, &geo),
            Mode::Read => self.paint_read(p, &geo),
        }
        p.g.set_clip(None);
        self.paint_bar(p, &geo, &hits);
        if !self.query().is_empty() {
            self.paint_matches(p, &geo, &hits);
        }
        if self.side_open {
            self.paint_side(p, &geo, &hits);
        }

        // Scrollbar paling akhir supaya tidak tertutup baris terpilih.
        let lw = p.lw();
        for list in [false, true] {
            if list && !self.side_open {
                continue;
            }
            let Some(b) = self.bar(&geo, list) else {
                continue;
            };
            let held = matches!(self.drag, Some(Drag::Bar { list: l, .. }) if l == list);
            let w = if held || self.bar_hot[list as usize] {
                8.0 * s
            } else {
                4.0 * s
            };
            let x = b.track.x + b.track.w - w - 3.0 * s;
            p.rrect(
                Rect::new(x.round(), b.thumb.y.round(), w.round(), b.thumb.h.round()),
                w / 2.0,
                if held || self.bar_hot[list as usize] { t.dim } else { t.border },
            );
        }
        let _ = lw;
        self.paint_menu(p, &hits);
        if self.settings.is_some() {
            let hits = self.frame_hits.clone();
            self.paint_settings(p, &hits);
        }
    }

    /// Latar tombol saat hover.
    fn hover_bg(&self, p: &mut P, r: Rect, b: Btn) {
        if self.hot.0 == Some(b) {
            p.rrect(r, 6.0 * self.s, self.theme.hover);
        }
    }

    fn paint_bar(&mut self, p: &mut P, geo: &Geo, hits: &[(Rect, Btn)]) {
        let (t, s, b) = (self.theme, self.s, geo.bar);
        let lw = p.lw();
        p.rect(b, t.bg);
        p.rect(Rect::new(b.x, b.y + b.h - lw, b.w, lw), t.border);
        for &(r, btn) in hits {
            let hot = self.hot.0 == Some(btn);
            match btn {
                Btn::Side => {
                    self.hover_bg(p, r, btn);
                    let name = if self.side_open { "panel-left-close" } else { "panel-left-open" };
                    p.icon_in(name, r, 18.0 * s, t.fg);
                }
                Btn::Mode | Btn::Delete => {
                    let del = btn == Btn::Delete;
                    let col = if del && hot { t.err } else { t.fg };
                    if hot {
                        p.rrect(r, 6.0 * s, if del { alpha(t.err, 0.10) } else { t.hover });
                    }
                    let (icon, label) = match (del, self.mode) {
                        (true, _) => ("trash-2", "Delete"),
                        (false, Mode::Edit) => ("book-open", "Read"),
                        (false, Mode::Read) => ("pencil", "Edit"),
                    };
                    p.icon(icon, r.x + 10.0 * s, (r.y + 8.0 * s).round(), 16.0 * s, col);
                    let base = p.base(Face::Medium, 14.0 * s, r.y, r.h);
                    p.text(r.x + 32.0 * s, base, Face::Medium, 14.0 * s, label, col, f32::INFINITY);
                }
                Btn::Share | Btn::CopyLink | Btn::Unshare => {
                    self.hover_bg(p, r, btn);
                    let (icon, label) = self.share_look(btn);
                    let col = if btn == Btn::CopyLink { t.accent } else { t.fg };
                    if r.w <= 32.0 * s {
                        p.icon_in(icon, r, 16.0 * s, col);
                    } else {
                        p.icon(icon, r.x + 10.0 * s, (r.y + 8.0 * s).round(), 16.0 * s, col);
                        let base = p.base(Face::Medium, 14.0 * s, r.y, r.h);
                        p.text(r.x + 32.0 * s, base, Face::Medium, 14.0 * s, label, col, f32::INFINITY);
                    }
                }
                _ => {}
            }
        }
        // Jumlah kata, pesan, atau galat; berhenti sebelum tombol kanan.
        let x = (b.x + 52.0 * s).round();
        let iy = ((b.h - 16.0 * s) / 2.0).round();
        let right = hits
            .iter()
            .filter(|h| h.0.y < b.y + b.h && h.0.x > x)
            .fold(b.x + b.w, |m, h| m.min(h.0.x));
        let max_w = (right - x - 32.0 * s).max(0.0);
        if !self.status.is_empty() {
            p.icon("circle-alert", x, iy, 16.0 * s, t.err);
            let base = p.base(Face::Medium, 14.0 * s, 0.0, b.h);
            let st = self.status.clone();
            p.text(x + 24.0 * s, base, Face::Medium, 14.0 * s, &st, t.err, max_w);
        } else if !self.notice.is_empty() {
            p.icon("link", x, iy, 16.0 * s, t.accent);
            let base = p.base(Face::Regular, 14.0 * s, 0.0, b.h);
            let st = self.notice.clone();
            p.text(x + 24.0 * s, base, Face::Regular, 14.0 * s, &st, t.fg, max_w);
        } else {
            let n = self.word_count();
            let label = format!("{n} {}", if n == 1 { "word" } else { "words" });
            p.icon("file-text", x, iy, 16.0 * s, t.dim);
            let base = p.base(Face::Regular, 14.0 * s, 0.0, b.h);
            p.text(x + 24.0 * s, base, Face::Regular, 14.0 * s, &label, t.dim, max_w);
        }
    }

    /// Bilah bawah saat mencari: "2 of 5" dan tombol ˄ ˅.
    fn paint_matches(&mut self, p: &mut P, geo: &Geo, hits: &[(Rect, Btn)]) {
        let (t, s) = (self.theme, self.s);
        let c = geo.content;
        let r = Rect::new(c.x, c.y + c.h, c.w, self.size.1 - c.y - c.h);
        p.rect(r, t.bg);
        p.rect(Rect::new(r.x, r.y, r.w, p.lw()), t.border);
        let n = self.match_count();
        let label = match n {
            0 => "No results".to_owned(),
            1 => "1 result".to_owned(),
            _ => format!("{} of {n} results", self.match_idx.min(n - 1) + 1),
        };
        p.center(r, Face::Regular, 12.0 * s, &label, t.dim);
        for &(br, b) in hits {
            let icon = match b {
                Btn::MatchPrev => "chevron-up",
                Btn::MatchNext => "chevron-down",
                _ => continue,
            };
            self.hover_bg(p, br, b);
            p.icon_in(icon, br, 18.0 * s, if n > 0 { t.fg } else { t.dim });
        }
    }

    fn paint_side(&mut self, p: &mut P, geo: &Geo, hits: &[(Rect, Btn)]) {
        let (t, s) = (self.theme, self.s);
        let (sw, h) = (geo.side.w, self.size.1);
        let lw = p.lw();
        p.rect(geo.side, t.side);
        p.rect(Rect::new(sw - lw, 0.0, lw, h), t.border);
        let base = p.base(Face::Semi, 14.0 * s, 0.0, (BAR_H * s).round());
        p.text(16.0 * s, base, Face::Semi, 14.0 * s, "All notes", t.fg, sw - 100.0 * s);
        for &(r, b) in hits {
            match b {
                Btn::Search => {
                    if self.search_open {
                        p.rrect(r, 6.0 * s, t.row_sel);
                    } else {
                        self.hover_bg(p, r, b);
                    }
                    let c = if self.search_open { t.accent } else { t.fg };
                    p.icon_in("search", r, 18.0 * s, c);
                }
                Btn::New => {
                    self.hover_bg(p, r, b);
                    p.icon_in("plus", r, 18.0 * s, t.fg);
                }
                Btn::Settings => {
                    self.hover_bg(p, r, b);
                    p.icon("settings", r.x + 10.0 * s, (r.y + 10.0 * s).round(), 16.0 * s, t.fg);
                    let base = p.base(Face::Medium, 14.0 * s, r.y, r.h);
                    p.text(r.x + 36.0 * s, base, Face::Medium, 14.0 * s, "Settings", t.fg, r.w);
                }
                _ => {}
            }
        }
        if self.search_open {
            self.paint_search(p, geo, hits);
        }
        self.paint_list(p, geo);
        let fy = (h - 53.0 * s).round();
        p.rect(Rect::new(0.0, fy, sw - lw, lw), t.border);
        if !self.query().is_empty() {
            let ry = fy - (40.0 * s).round();
            p.rect(Rect::new(0.0, ry, sw - lw, lw), t.border);
            let n = self.res.len();
            let label = format!("{n} {}", if n == 1 { "result" } else { "results" });
            p.center(Rect::new(0.0, ry, sw, 40.0 * s), Face::Regular, 12.0 * s, &label, t.dim);
        }
    }

    fn paint_search(&self, p: &mut P, geo: &Geo, hits: &[(Rect, Btn)]) {
        let (t, s, r) = (self.theme, self.s, geo.search);
        let px = 14.0 * s;
        let lw = p.lw();
        p.rrect(r, 6.0 * s, t.bg);
        let edge = if self.focus == Focus::Search { t.accent } else { t.border };
        p.rframe(r, 6.0 * s, lw, edge);
        p.icon("search", r.x + 12.0 * s, (r.y + 10.0 * s).round(), 16.0 * s, t.dim);
        let (asc, desc) = p.f.metrics(Face::Regular, px);
        let base = (r.y + (r.h + asc - desc) / 2.0).round();
        let tx = r.x + 36.0 * s;
        p.g.set_clip(Some(Rect::new(tx - 2.0 * s, r.y, r.w - 72.0 * s + 4.0 * s, r.h)));
        let q = &self.search.text;
        if q.is_empty() && self.preedit.is_empty() {
            p.text(tx, base, Face::Regular, px, "Search notes…", t.dim, r.w - 72.0 * s);
        } else {
            let (a, b) = self.search.selection();
            if a != b {
                let x1 = tx + p.f.width(Face::Regular, px, &q[..a]);
                let x2 = tx + p.f.width(Face::Regular, px, &q[..b]);
                p.rect(Rect::new(x1, base - asc, x2 - x1, asc + desc), t.sel);
            }
            p.text(tx, base, Face::Regular, px, q, t.fg, f32::INFINITY);
        }
        if self.focus == Focus::Search {
            let cx = (tx + p.f.width(Face::Regular, px, &q[..self.search.cursor])).round();
            if !self.preedit.is_empty() {
                let w = p.f.width(Face::Regular, px, &self.preedit);
                p.rect(Rect::new(cx, base - asc, w, asc + desc), t.bg);
                p.text(cx, base, Face::Regular, px, &self.preedit, t.fg, f32::INFINITY);
                p.rect(Rect::new(cx, base + 2.0 * lw, w, lw), t.fg);
            }
            p.rect(Rect::new(cx, base - asc, (1.5 * s).round().max(1.0), asc + desc), t.accent);
        }
        p.g.set_clip(None);
        if let Some(&(cr, _)) = hits.iter().find(|h| h.1 == Btn::Clear) {
            let hot = self.hot.0 == Some(Btn::Clear);
            if hot {
                p.rrect(cr, 4.0 * s, t.hover);
            }
            p.icon_in("x", cr, 14.0 * s, if hot { t.fg } else { t.dim });
        }
    }

    fn paint_list(&self, p: &mut P, geo: &Geo) {
        let (t, s, l) = (self.theme, self.s, geo.list);
        let rh = ROW_H * s;
        p.g.set_clip(Some(l));
        let terms = terms_of(self.query());
        let now = SystemTime::now();
        let first = (self.list_scroll / rh) as usize;
        for (k, &ni) in self.res.iter().enumerate().skip(first) {
            let y = (l.y + k as f32 * rh - self.list_scroll).round();
            if y > l.y + l.h {
                break;
            }
            let r = Rect::new(8.0 * s, y, l.w - 16.0 * s, (80.0 * s).round());
            if ni == self.cur {
                p.rrect(r, 6.0 * s, t.row_sel);
            } else if self.hot.1 == Some(k) {
                p.rrect(r, 6.0 * s, alpha(t.fg, 0.05));
            }
            let n = &self.store.notes[ni];
            // Catatan yang sedang diedit: pakai teks editor, supaya daftar ikut berubah saat mengetik.
            let (title, snip) = if ni == self.cur {
                (
                    store::title_of(&self.doc.text),
                    store::snippet_of(&self.doc.text),
                )
            } else {
                (n.title(), n.snippet())
            };
            let text = if ni == self.cur { &self.doc.text } else { &n.text };
            let snip = match_snippet(text, &terms).unwrap_or(snip);
            let (x, w) = (r.x + 12.0 * s, r.w - 24.0 * s);
            let tp: Vec<_> = marks(title, &terms)
                .into_iter()
                .map(|(x, m)| (x, Face::Semi, if m { t.accent } else { t.fg }))
                .collect();
            let b = p.base(Face::Semi, 14.0 * s, y + 10.0 * s, 20.0 * s);
            p.parts(x, b, 14.0 * s, &tp, w);
            let sp: Vec<_> = marks(&snip, &terms)
                .into_iter()
                .map(|(x, m)| if m { (x, Face::Semi, t.accent) } else { (x, Face::Regular, t.dim) })
                .collect();
            let b = p.base(Face::Regular, 14.0 * s, y + 32.0 * s, 20.0 * s);
            p.parts(x, b, 14.0 * s, &sp, w);
            let b = p.base(Face::Regular, 12.0 * s, y + 54.0 * s, 16.0 * s);
            p.text(x, b, Face::Regular, 12.0 * s, &store::ago(n.mtime, now), t.dim, w);
        }
        if self.res.is_empty() {
            let q = self.query();
            let msg = if q.is_empty() {
                "No notes yet".to_owned()
            } else {
                format!("No notes match “{q}”")
            };
            p.center(Rect::new(12.0 * s, l.y + 32.0 * s, l.w - 24.0 * s, 20.0 * s), Face::Regular, 14.0 * s, &msg, t.dim);
        }
        p.g.set_clip(None);
    }

    fn paint_settings(&mut self, p: &mut P, hits: &[(Rect, Btn)]) {
        let (t, s) = (self.theme, self.s);
        let (w, h) = self.size;
        let d = self.dialog();
        let lw = p.lw();
        p.rect(Rect::new(0.0, 0.0, w, h), [10.0 / 255.0, 12.0 / 255.0, 14.0 / 255.0, 0.36]);
        // Bayangan lembut: beberapa lapis tipis.
        for i in 1..=8 {
            let e = i as f32 * 4.0 * s;
            p.rrect(
                Rect::new(d.x - e, d.y - e + 16.0 * s, d.w + 2.0 * e, d.h + 2.0 * e),
                10.0 * s + e,
                [0.0, 0.0, 0.0, 0.012],
            );
        }
        p.rrect(d, 10.0 * s, t.bg);
        p.rframe(d, 10.0 * s, lw, t.border);
        let b = p.base(Face::Semi, 16.0 * s, d.y, 56.0 * s);
        p.text(d.x + 24.0 * s, b, Face::Semi, 16.0 * s, "Settings", t.fg, d.w);
        p.rect(Rect::new(d.x, d.y + 96.0 * s, d.w, lw), t.border);
        let foot = d.y + d.h - 40.0 * s;
        p.rect(Rect::new(d.x, foot, d.w, lw), t.border);
        let b = p.base(Face::Regular, 12.0 * s, foot, 40.0 * s);
        p.text(d.x + 24.0 * s, b, Face::Regular, 12.0 * s, "Press Esc to close", t.dim, d.w);

        let px = 14.0 * s;
        let (bx, bw) = (d.x + 24.0 * s, d.w - 48.0 * s);
        let y0 = d.y + (56.0 + 41.0 + 24.0) * s;
        p.g.set_clip(Some(Rect::new(d.x, d.y + 97.0 * s, d.w, foot - d.y - 97.0 * s)));
        if self.settings == Some(0) {
            let labels = ["Theme", "Text size", "Line width", "Open notes in", "Version"];
            for (i, l) in labels.iter().enumerate() {
                let b = p.base(Face::Semi, px, y0 + i as f32 * 88.0 * s, 20.0 * s);
                p.text(bx, b, Face::Semi, px, l, t.fg, bw);
            }
            let fy = y0 + 88.0 * s + 28.0 * s;
            let fs = format!("{} px", self.fs);
            p.center(Rect::new(bx + 44.0 * s, fy, 64.0 * s, 36.0 * s), Face::Mono, px, &fs, t.fg);
            let vy = y0 + 4.0 * 88.0 * s + 28.0 * s;
            let b = p.base(Face::Regular, px, vy, 20.0 * s);
            let mut v = concat!("TilDown v", env!("CARGO_PKG_VERSION")).to_owned();
            let mut vc = t.dim;
            match &self.upd {
                Some(Ok(tag)) if self.upd_ready => {
                    v += &format!(" · {tag} installed, restart to use it");
                    vc = t.accent;
                }
                Some(Ok(tag)) if newer(tag) => {
                    v += &format!(" · {tag} available");
                    vc = t.accent;
                }
                Some(Ok(_)) => v += " · up to date",
                Some(Err(e)) => v += &format!(" · {e}"),
                None => {}
            }
            let uw = hits.iter().find(|h| h.1 == Btn::Update).map_or(0.0, |h| h.0.w);
            p.text(bx, b, Face::Regular, px, &v, vc, bw - uw - 8.0 * s);
        } else if self.settings == Some(2) {
            let (intro, top) = sync_intro(p.f, s, bw);
            for (k, l) in intro.iter().enumerate() {
                let b = p.base(Face::Regular, px, y0 + k as f32 * 22.0 * s, 20.0 * s);
                p.text(bx, b, Face::Regular, px, l, t.dim, bw);
            }
            for (k, l) in ["Token", "Server URL"].iter().enumerate() {
                let b = p.base(Face::Semi, px, y0 + (top + k as f32 * 88.0) * s, 20.0 * s);
                p.text(bx, b, Face::Semi, px, l, t.fg, bw);
            }
            let (msg, c) = self.sync_status();
            let claude = if self.sync.on() { 44.0 } else { 0.0 }; // baris "Connect Claude"
            let b = p.base(Face::Regular, px, y0 + (top + 2.0 * 88.0 + 8.0 + 36.0 + 16.0 + claude) * s, 20.0 * s);
            p.text(bx, b, Face::Regular, px, &msg, c, bw);
        } else if self.trash.is_empty() {
            let cy = (y0 + foot) / 2.0;
            p.icon_in("trash", Rect::new(d.x, cy - 40.0 * s, d.w, 24.0 * s), 24.0 * s, t.dim);
            p.center(Rect::new(d.x, cy - 8.0 * s, d.w, 20.0 * s), Face::Regular, px, "Trash is empty", t.dim);
            let b = p.base(Face::Regular, px, y0 + 2.0 * s, 32.0 * s);
            p.text(bx, b, Face::Regular, px, "Notes are deleted permanently after 30 days.", t.dim, bw);
        } else {
            let b = p.base(Face::Regular, px, y0 + 2.0 * s, 32.0 * s);
            p.text(bx, b, Face::Regular, px, "Notes are deleted permanently after 30 days.", t.dim, bw - 120.0 * s);
            let now = SystemTime::now();
            let mut ry = y0 + 52.0 * s;
            p.rect(Rect::new(bx, ry, bw, lw), t.border);
            ry += lw;
            for tr in &self.trash {
                let b = p.base(Face::Semi, px, ry + 12.0 * s, 20.0 * s);
                p.text(bx, b, Face::Semi, px, &tr.title, t.fg, bw - 160.0 * s);
                let b = p.base(Face::Regular, 12.0 * s, ry + 34.0 * s, 16.0 * s);
                let when = format!("Deleted {}", store::ago(tr.when, now));
                p.text(bx, b, Face::Regular, 12.0 * s, &when, t.dim, bw - 160.0 * s);
                ry += 62.0 * s;
                p.rect(Rect::new(bx, ry, bw, lw), t.border);
                ry += lw;
            }
        }
        p.g.set_clip(None);
        let n = self.trash.len().to_string();
        let badge = (p.f.width(Face::Regular, 12.0 * s, &n) + 12.0 * s).max(20.0 * s);
        for &(r, b) in hits {
            let hot = self.hot.0 == Some(b);
            let opt = |p: &mut P, r: Rect, on: bool, label: &str| {
                p.rrect(r, 6.0 * s, if on { t.row_sel } else { t.bg });
                p.rframe(r, 6.0 * s, lw, if on { t.accent } else if hot { t.dim } else { t.border });
                let face = if on { Face::Semi } else { Face::Regular };
                p.center(r, face, px, label, if on { t.accent } else { t.fg });
            };
            match b {
                Btn::SetClose => {
                    self.hover_bg(p, r, b);
                    p.icon_in("x", r, 18.0 * s, if hot { t.fg } else { t.dim });
                }
                Btn::Tab(i) => {
                    let on = self.settings == Some(i);
                    let c = if on || hot { t.fg } else { t.dim };
                    let label = ["General", "Trash", "Sync"][i as usize];
                    let bl = p.base(Face::Medium, px, r.y, r.h);
                    let lx = p.text(r.x, bl, Face::Medium, px, label, c, f32::INFINITY);
                    if on {
                        p.rect(Rect::new(r.x, r.y + r.h - 2.0 * s, r.w, 2.0 * s), t.accent);
                    }
                    if i == 1 && !self.trash.is_empty() {
                        let br = Rect::new(lx + 8.0 * s, r.y + 10.0 * s, badge, 20.0 * s);
                        p.rrect(br, 10.0 * s, t.code_bg);
                        p.center(br, Face::Regular, 12.0 * s, &self.trash.len().to_string(), t.dim);
                    }
                }
                Btn::Theme(i) => opt(p, r, self.theme_pref == i, ["System", "Light", "Dark"][i as usize]),
                Btn::Width(i) => opt(p, r, self.col == WIDTHS[i as usize], ["Narrow", "Normal", "Wide"][i as usize]),
                Btn::OpenIn(i) => opt(p, r, (self.open_in == Mode::Read) == (i == 1), ["Edit", "Read"][i as usize]),
                Btn::Field(i) => {
                    let on = self.sync_edit == Some(i as usize);
                    p.rframe(r, 6.0 * s, lw, if on { t.accent } else if hot { t.dim } else { t.border });
                    let d = &self.sync_in[i as usize];
                    // Token disamarkan.
                    let show = |x: &str| if i == 1 { "•".repeat(x.chars().count()) } else { x.to_owned() };
                    let (tx, tw) = (r.x + 12.0 * s, r.w - 24.0 * s);
                    let bl = p.base(Face::Regular, px, r.y, r.h);
                    if d.text.is_empty() {
                        let hint = ["https://tildown-sync.<you>.workers.dev", "Press Generate, or paste your TOKEN"];
                        p.text(tx, bl, Face::Regular, px, hint[i as usize], t.dim, tw);
                    } else {
                        p.text(tx, bl, Face::Regular, px, &show(&d.text), t.fg, tw);
                    }
                    if on {
                        let cx = tx + p.f.width(Face::Regular, px, &show(&d.text[..d.cursor]));
                        let cw = (1.5 * s).round().max(1.0);
                        p.rect(Rect::new(cx.min(tx + tw).round(), r.y + 9.0 * s, cw, r.h - 18.0 * s), t.fg);
                    }
                }
                Btn::Update => {
                    self.hover_bg(p, r, b);
                    p.rframe(r, 6.0 * s, lw, if hot { t.dim } else { t.border });
                    p.center(r, Face::Medium, px, self.update_label(), t.fg);
                }
                Btn::GenToken | Btn::Deploy => {
                    self.hover_bg(p, r, b);
                    p.rframe(r, 6.0 * s, lw, if hot { t.dim } else { t.border });
                    p.center(r, Face::Medium, px, if b == Btn::Deploy { "Deploy ↗" } else { "Generate" }, t.fg);
                }
                Btn::SyncConnect | Btn::SyncNow | Btn::CopyCode | Btn::PasteCode | Btn::CopyMcp => {
                    let label = match b {
                        Btn::CopyMcp => "Connect Claude (copy link)",
                        Btn::SyncNow => "Sync now",
                        Btn::CopyCode => "Copy code",
                        Btn::PasteCode => "Paste code",
                        _ if self.sync.on() => "Disconnect",
                        _ => "Connect",
                    };
                    opt(p, r, b == Btn::SyncConnect && !self.sync.on(), label);
                }
                Btn::FsDown | Btn::FsUp => {
                    p.rframe(r, 6.0 * s, lw, if hot { t.dim } else { t.border });
                    p.icon_in(if b == Btn::FsUp { "plus" } else { "minus" }, r, 16.0 * s, t.fg);
                }
                Btn::EmptyTrash => {
                    if hot {
                        p.rrect(r, 6.0 * s, alpha(t.err, 0.10));
                    }
                    p.center(r, Face::Medium, px, "Empty trash", t.err);
                }
                Btn::Restore(_) => {
                    self.hover_bg(p, r, b);
                    p.icon("undo-2", r.x + 10.0 * s, (r.y + 8.0 * s).round(), 16.0 * s, t.fg);
                    let bl = p.base(Face::Medium, px, r.y, r.h);
                    p.text(r.x + 32.0 * s, bl, Face::Medium, px, "Restore", t.fg, r.w);
                }
                Btn::Purge(_) => {
                    if hot {
                        p.rrect(r, 6.0 * s, alpha(t.err, 0.10));
                    }
                    p.icon_in("trash-2", r, 16.0 * s, if hot { t.err } else { t.dim });
                }
                _ => {}
            }
        }
        p.g.set_clip(None);
    }

    fn paint_editor(&self, p: &mut P, geo: &Geo) {
        let (t, s, c) = (self.theme, self.s, geo.content);
        p.g.set_clip(Some(c));
        let (ox, oy) = self.ed_origin(geo);
        let (a, b) = self.doc.selection();
        let (top, bot) = (c.y, c.y + c.h);
        let first = self.ed.lines.partition_point(|l| oy + l.y + l.lay.h < top);
        let code = |l: &EdLine| matches!(l.kind, Kind::Code | Kind::Fence);
        let matches = self.ed_matches();
        let cur_m = self.match_idx.min(matches.len().saturating_sub(1));
        let (hl, hl_on) = (search_hl(&t, false), search_hl(&t, true));
        let mut run: Option<f32> = None;
        for (i, l) in self.ed.lines.iter().enumerate() {
            if code(l) {
                let y0 = *run.get_or_insert(l.y);
                if self.ed.lines.get(i + 1).is_none_or(|n| !code(n)) {
                    run = None;
                    let r = Rect::new(ox - 16.0 * s, oy + y0, geo.col_w + 32.0 * s, l.y + l.lay.h - y0);
                    if r.y + r.h >= top && r.y <= bot {
                        p.rrect(r, 6.0 * s, t.code_bg);
                    }
                }
            }
        }
        for l in &self.ed.lines[first..] {
            let y = oy + l.y;
            if y > bot {
                break;
            }
            if a < b && a <= l.end && b > l.start {
                let n = l.lay.rows.len();
                for (k, r) in l.lay.rows.iter().enumerate() {
                    let lo = a.saturating_sub(l.start).max(r.start);
                    let hi = (b - l.start).min(r.end);
                    let nl = b > l.end && k + 1 == n; // newline ikut terpilih
                    if lo < hi || (nl && lo <= hi) {
                        let (x1, x2) = xspan(r, lo, hi);
                        let x2 = x2 + if nl { 6.0 * s } else { 0.0 };
                        p.rect(Rect::new(ox + x1, y + r.top, x2 - x1, r.h), t.sel);
                    }
                }
            }
            for (mi, m) in matches.iter().enumerate() {
                if m.end <= l.start || m.start > l.end {
                    continue;
                }
                for r in &l.lay.rows {
                    let lo = m.start.saturating_sub(l.start).max(r.start);
                    let hi = (m.end - l.start).min(r.end);
                    if lo < hi {
                        let (x1, x2) = xspan(r, lo, hi);
                        let c = if mi == cur_m { hl_on } else { hl };
                        p.rect(Rect::new(ox + x1, y + r.top, x2 - x1, r.h), c);
                    }
                }
            }
            p.lay(&l.lay, ox, y, top, bot);
        }
        if self.doc.text.is_empty() {
            let base = self
                .ed
                .lines
                .first()
                .and_then(|l| l.lay.rows.first())
                .map_or(BODY * s, |r| r.base);
            p.text(
                ox,
                oy + base,
                Face::Regular,
                BODY * s,
                "Start writing…",
                t.dim,
                geo.col_w,
            );
        }
        if self.focus == Focus::Editor {
            let (cx, cy, ch) = self.ed.caret(self.doc.cursor);
            let inset = ch * (1.0 - 1.0 / LH) / 2.0;
            let cr = Rect::new(
                (ox + cx).round(),
                (oy + cy + inset).round(),
                (2.0 * s).round().max(1.0),
                (ch - 2.0 * inset).round(),
            );
            if !self.preedit.is_empty() {
                let px = BODY * s;
                let (asc, desc) = p.f.metrics(Face::Regular, px);
                let base = (cr.y + (cr.h + asc - desc) / 2.0).round();
                let w = p.f.width(Face::Regular, px, &self.preedit);
                // ponytail: preedit IME ditimpa di atas teks, bukan disisipkan ke layout.
                p.rect(Rect::new(cr.x, cr.y, w + 4.0 * s, cr.h), t.bg);
                p.text(
                    cr.x + 2.0 * s,
                    base,
                    Face::Regular,
                    px,
                    &self.preedit,
                    t.fg,
                    f32::INFINITY,
                );
                p.rect(
                    Rect::new(cr.x, base + 2.0 * p.lw(), w + 4.0 * s, p.lw()),
                    t.fg,
                );
            }
            p.rect(cr, t.accent);
        }
        p.g.set_clip(None);
    }

    fn paint_zoom(&mut self, p: &mut P) {
        let (t, s) = (self.theme, self.s);
        let (w, h) = self.size;
        let Some(z) = &mut self.zoom else { return };
        let isz = match self.imgs.get(&z.src) {
            Some(Some(i)) => Some((i.w as f32, i.h as f32)),
            _ => None,
        };
        if z.k <= 0.0 {
            // Pas layar (maks. 2x ukuran baca), di tengah.
            let k0 = match z.kind {
                DiaKind::Puml(_) => BODY * s * 0.9 / 12.0,
                DiaKind::Mermaid => s,
                DiaKind::Image => 1.0,
            };
            let Some((_, r)) = zoom_view(p.f, &z.src, z.kind, isz, k0, s, &t) else {
                self.zoom = None;
                return;
            };
            let m = 24.0 * s;
            let q = ((w - 2.0 * m) / r.w).min((h - 2.0 * m) / r.h).clamp(0.2, 2.0);
            z.k = k0 * q;
            z.pan = ((w - r.w * q) / 2.0, (h - r.h * q) / 2.0);
        }
        if z.built != z.k {
            let Some((v, r)) = zoom_view(p.f, &z.src, z.kind, isz, z.k, s, &t) else {
                self.zoom = None;
                return;
            };
            (z.view, z.org, z.built) = (v, (r.x, r.y), z.k);
        }
        let (ox, oy) = ((z.pan.0 - z.org.0).round(), (z.pan.1 - z.org.1).round());
        for d in &z.view.decos {
            match d {
                Deco::Mesh(_, m) => p.g.tris(m, ox, oy),
                Deco::Fill(r, c) => p.rect(Rect::new(ox + r.x, oy + r.y, r.w, r.h), *c),
                Deco::Img(key, r) => img_quad(p, &mut self.imgs, key, Rect::new(ox + r.x, oy + r.y, r.w, r.h)),
                Deco::Round(..) | Deco::Icon(..) => {}
            }
        }
        for (x, y, lay) in &z.view.items {
            p.lay(lay, ox + x, oy + y, 0.0, h);
        }
        let lw = p.lw();
        for (r, b) in self.frame_hits.clone() {
            let hot = self.hot.0 == Some(b);
            p.rrect(r, 6.0 * s, if hot { t.side } else { t.bg });
            p.rframe(r, 6.0 * s, lw, if hot { t.dim } else { t.border });
            let name = match b {
                Btn::ZoomSave => "download",
                Btn::ZoomOut => "minus",
                Btn::ZoomIn => "plus",
                _ => "x",
            };
            p.icon_in(name, r, 18.0 * s, t.fg);
        }
        let px = 12.0 * s;
        let hint = self.zoom.as_ref().and_then(|z| z.msg.clone());
        let hint = hint.as_deref().unwrap_or("Scroll/pinch: zoom · Drag: pan · 0: fit · Esc: close");
        let tw = p.f.width(Face::Regular, px, hint);
        let (pw, ph) = ((tw + 48.0 * s).round(), (30.0 * s).round());
        let r = Rect::new(((w - pw) / 2.0).round(), (h - 24.0 * s - ph).round(), pw, ph);
        p.rrect(r, ph / 2.0, t.side);
        p.rframe(r, ph / 2.0, lw, t.border);
        p.icon("mouse-pointer-2", r.x + 14.0 * s, (r.y + (ph - 12.0 * s) / 2.0).round(), 12.0 * s, t.dim);
        let base = p.base(Face::Regular, px, r.y, r.h);
        p.text(r.x + 34.0 * s, base, Face::Regular, px, hint, t.dim, tw + 1.0);
    }

    fn paint_read(&mut self, p: &mut P, geo: &Geo) {
        let (t, c) = (self.theme, geo.content);
        p.g.set_clip(Some(c));
        let (ox, oy) = self.ed_origin(geo);
        let (top, bot) = (c.y, c.y + c.h);
        let visible = |r: &Rect| oy + r.y + r.h >= top && oy + r.y <= bot;
        let s = self.s;
        if let Some((r, ..)) = self.hot.2.and_then(|i| self.rd.dias.get(i)) {
            p.rrect(Rect::new(ox + r.x, oy + r.y, r.w, r.h), 6.0 * s, alpha(t.fg, 0.03));
        }
        for d in &self.rd.decos {
            match d {
                Deco::Fill(r, col) if visible(r) => {
                    p.rect(Rect::new(ox + r.x, oy + r.y, r.w, r.h), *col)
                }
                Deco::Round(r, rad, col) if visible(r) => {
                    p.rrect(Rect::new(ox + r.x, oy + r.y, r.w, r.h), *rad, *col)
                }
                Deco::Icon(name, r, col) if visible(r) => p.icon(name, ox + r.x, oy + r.y, r.w, *col),
                Deco::Mesh(r, m) if visible(r) => p.g.tris(m, ox, oy),
                Deco::Img(key, r) if visible(r) => {
                    img_quad(p, &mut self.imgs, key, Rect::new(ox + r.x, oy + r.y, r.w, r.h))
                }
                _ => {}
            }
        }
        let rm = self.rd_matches();
        let cur_m = self.match_idx.min(rm.len().saturating_sub(1));
        let (hl, hl_on) = (search_hl(&t, false), search_hl(&t, true));
        for (k, (x, y, lay)) in self.rd.items.iter().enumerate() {
            if oy + y + lay.h >= top && oy + y <= bot {
                if let Some((lo, hi)) = self.rd.sel_range(k) {
                    for r in &lay.rows {
                        let (a, b) = (lo.max(r.start), hi.min(r.end));
                        if a < b {
                            let (x1, x2) = xspan(r, a, b);
                            p.rect(Rect::new(ox + x + x1, oy + y + r.top, x2 - x1, r.h), t.sel);
                        }
                    }
                }
                for (mi, &(mk, g, len)) in rm.iter().enumerate() {
                    if mk != k {
                        continue;
                    }
                    let c = if mi == cur_m { hl_on } else { hl };
                    let mut gi = 0;
                    for r in &lay.rows {
                        for gl in &r.gs {
                            if gi >= g && gi < g + len {
                                p.rect(Rect::new(ox + x + gl.x, oy + y + r.top, gl.adv, r.h), c);
                            }
                            gi += 1;
                        }
                    }
                }
                p.lay(lay, ox + x, oy + y, top, bot);
            }
        }
        for (r, _, done) in &self.rd.checks {
            let rr = Rect::new(ox + r.x, oy + r.y, r.w, r.h);
            if *done {
                p.rrect(rr, 4.0 * s, t.accent);
                // Centang putih.
                let k = r.w / 18.0;
                let pts: Vec<_> = [(4.5, 9.5), (7.5, 12.5), (13.5, 5.5)]
                    .iter()
                    .map(|q: &(f32, f32)| (rr.x + q.0 * k, rr.y + q.1 * k))
                    .collect();
                let mut m = crate::vector::Mesh::new();
                crate::vector::stroke(&mut m, &pts, false, 2.0 * k, hex(0xFFFFFF), None);
                p.g.tris(&m, 0.0, 0.0);
            } else {
                p.rframe(rr, 4.0 * s, 1.5 * s, t.dim);
            }
        }
        p.g.set_clip(None);
    }
}

impl ApplicationHandler<Ev> for App {
    // Dipanggil saat start, dan di Android setiap kali aplikasi kembali ke
    // depan: surface dibuat di sini, bukan di `run`.
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let window = match &self.window {
            Some(w) => w.clone(),
            None => {
                let attrs = Window::default_attributes().with_title("TilDown");
                // 1100×720, tapi tidak lebih dari 90% layar (laptop 1366×768).
                #[cfg(not(target_arch = "wasm32"))]
                let attrs = {
                    let (mut w, mut h) = (1100.0f64, 720.0f64);
                    // Debug: `MARKDOWN_SIZE=1280x800` (dipakai bersama MARKDOWN_SHOT).
                    let fixed = std::env::var("MARKDOWN_SIZE").ok().and_then(|v| {
                        let (a, b) = v.split_once('x')?;
                        Some((a.parse().ok()?, b.parse().ok()?))
                    });
                    let attrs = attrs.with_active(fixed.is_none());
                    if let Some(sz) = fixed {
                        (w, h) = sz;
                    } else if let Some(m) = el.primary_monitor() {
                        let sz = m.size().to_logical::<f64>(m.scale_factor());
                        (w, h) = (w.min(sz.width * 0.9), h.min(sz.height * 0.85));
                    }
                    attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h))
                };
                // Web: kanvas ditempel ke <body>, ukurannya diatur CSS index.html.
                #[cfg(target_arch = "wasm32")]
                let attrs =
                    winit::platform::web::WindowAttributesExtWebSys::with_append(attrs, true);
                match el.create_window(attrs) {
                    Ok(w) => Arc::new(w),
                    Err(e) => return self.fail(el, format!("jendela: {e}")),
                }
            }
        };
        window.set_ime_allowed(true);
        self.s = window.scale_factor() as f32;
        let sz = window.inner_size();
        self.size = (sz.width.max(1) as f32, sz.height.max(1) as f32);
        self.sys_dark = window.theme() == Some(winit::window::Theme::Dark);
        self.apply_theme();
        self.window = Some(window.clone());
        #[cfg(not(target_arch = "wasm32"))]
        match Gpu::new(window) {
            Ok(g) => self.user_event(el, Ev::Gpu(g)),
            Err(e) => self.fail(el, e),
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(proxy) = self.proxy.clone() {
            wasm_bindgen_futures::spawn_local(async move {
                match Gpu::new_async(window).await {
                    Ok(g) => _ = proxy.send_event(Ev::Gpu(g)),
                    Err(e) => crate::web::log_error(&format!("TilDown: GPU {e}")),
                }
            });
        }
    }

    /// GPU siap (langsung di desktop, async di web), atau jawaban sync.
    fn user_event(&mut self, _el: &ActiveEventLoop, ev: Ev) {
        let mut g = match ev {
            Ev::Gpu(g) => g,
            Ev::Sync(r) => return self.sync_done(r),
            Ev::Share(id, on, r) => {
                self.share_done(id, on, r);
                return self.redraw();
            }
            Ev::Update(r) => {
                (self.upd_busy, self.upd) = (false, Some(r));
                return self.redraw();
            }
            Ev::Updated(r) => {
                self.upd_busy = false;
                match r {
                    Ok(()) => self.upd_ready = true,
                    Err(e) => self.upd = Some(Err(format!("update failed: {e}"))),
                }
                return self.redraw();
            }
            Ev::Kb(5, s) => return self.import_note(&s),
            Ev::Kb(6, s) => return self.paste_text(s),
            Ev::Kb(7, s) => return self.paste_code(s),
            Ev::Kb(8, _) => {
                self.back();
                return self.redraw();
            }
            Ev::Decoded(src, img) => {
                if img.is_some() {
                    self.imgs.insert(src, img);
                    self.rd = RdView::default();
                    self.redraw();
                }
                return;
            }
            Ev::Asset(name) => {
                // Gambar tadi dilewati (belum terbaca): decode ulang dan susun lagi.
                self.imgs.retain(|k, v| v.is_some() || k.trim_start_matches("./") != name);
                if name == "font" {
                    self.ed = EdView::default(); // font Cina datang: susun ulang teks
                }
                self.rd = RdView::default();
                return self.redraw();
            }
            Ev::Img(n, b) => {
                self.add_image(&n, &b);
                return self.redraw();
            }
            Ev::Kb(k, s) => return self.soft_key(k, s),
        };
        // `Resized` yang datang selama GPU disiapkan (web) tidak sampai ke surface.
        if let Some(w) = &self.window {
            let sz = w.inner_size();
            g.resize(sz.width, sz.height);
            self.size = (sz.width.max(1) as f32, sz.height.max(1) as f32);
        }
        self.gpu = Some(g);
        // Tekstur lama ikut hilang bersama device lama.
        if let Some(f) = &mut self.fonts {
            f.reset();
        }
        self.imgs.clear();
        self.rd = RdView::default();
        self.set_title();
        if let Ok(ui) = std::env::var("MARKDOWN_SHOT_UI") {
            self.shot_ui(&ui);
        }
        self.redraw();
    }

    // Android menghancurkan surface saat aplikasi ke background.
    fn suspended(&mut self, _el: &ActiveEventLoop) {
        self.save_now();
        self.save_state();
        self.gpu = None;
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.save_now();
        self.save_state();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        if self.nav_depth() != self.nav {
            self.nav = self.nav_depth();
            crate::web::nav_depth(self.nav);
        }
        let kb = self.kb_wanted();
        if kb != self.kb_on {
            self.kb_on = kb;
            #[cfg(target_arch = "wasm32")]
            crate::web::kb_editing(kb);
        }
        if self.save_due.is_some_and(|t| Instant::now() >= t) {
            self.save_now();
            self.redraw();
        }
        let long = |a: &Self| a.tap.as_ref().filter(|t| !t.moved && !t.held).map(|t| t.t0 + LONG_PRESS);
        if long(self).is_some_and(|t| Instant::now() >= t) {
            self.long_press();
        }
        if !self.sync_busy && self.sync_due.is_some_and(|t| Instant::now() >= t) {
            self.sync_start();
        }
        // Selama sync jalan, jawabannya yang membangunkan event loop.
        let sync_due = self.sync_due.filter(|_| !self.sync_busy);
        el.set_control_flow(match [self.save_due, sync_due, long(self)].into_iter().flatten().min() {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                self.size = (sz.width.max(1) as f32, sz.height.max(1) as f32);
                if let Some(g) = &mut self.gpu {
                    g.resize(sz.width, sz.height);
                }
                self.clamp_scroll();
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.s = scale_factor as f32;
                self.redraw();
            }
            WindowEvent::ThemeChanged(th) => {
                self.sys_dark = th == winit::window::Theme::Dark;
                self.apply_theme();
                self.redraw();
            }
            WindowEvent::Focused(true) => {
                self.focused = true;
                self.rescan();
                if self.sync.on() {
                    self.sync_due = Some(Instant::now());
                }
                self.redraw();
            }
            // Web tidak pernah memanggil `exiting`: state disimpan di sini juga.
            // Perubahan dikirim sekarang; tarik berkala berhenti sampai aktif lagi.
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.save_now();
                self.save_state();
                if self.sync.on() {
                    self.sync_due = Some(Instant::now());
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } => self.key(event),
            WindowEvent::Ime(ime) => self.ime(ime),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_move(position.x as f32, position.y as f32)
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.press(),
                ElementState::Released => self.release(),
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => self.right_click(),
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
            WindowEvent::Touch(t) => self.touch(t),
            WindowEvent::PinchGesture { delta, .. } => {
                let (x, y) = self.mouse;
                self.zoom_by((1.0 + delta as f32).max(0.1), x, y);
            }
            WindowEvent::RedrawRequested => self.render(),
            // Seret berkas gambar ke jendela: salin ke `assets/` dan tautkan.
            WindowEvent::DroppedFile(path) => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
                    match std::fs::read(&path) {
                        Ok(b) => {
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
                            self.add_image(name, &b);
                        }
                        Err(e) => self.status = format!("Couldn't add image: {e}"),
                    }
                    self.redraw();
                } else if ["md", "markdown", "txt"].contains(&ext.as_str()) {
                    match std::fs::read_to_string(&path) {
                        Ok(t) => self.import_note(&t),
                        Err(e) => {
                            self.status = format!("Couldn't import note: {e}");
                            self.redraw();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Download diagram: PlantUML dan Mermaid jadi PNG yang bisa dibaca lagi.
    #[test]
    fn update_versions() {
        assert!(newer("v99.0.0"));
        assert!(!newer(concat!("v", env!("CARGO_PKG_VERSION"))));
        assert!(!newer("v0.0.9"));
        assert!(newer(&format!("v{}.10", env!("CARGO_PKG_VERSION").rsplit_once('.').unwrap().0))); // .10 > .2, bukan urut huruf
    }

    #[test]
    fn diagram_download_png() {
        let cases = [
            ("@startuml\nAlice -> Bob: hello\n@enduml\n", DiaKind::Puml(0)),
            ("flowchart TD\n  A[Type a note] --> B{Changed?}\n  B -- yes --> C[Save]\n", DiaKind::Mermaid),
        ];
        for (i, (src, kind)) in cases.into_iter().enumerate() {
            let png = diagram_png(src, kind).unwrap();
            let img = image::load_from_memory(&png).unwrap();
            assert!(img.width() > 100 && img.height() > 100, "{}x{}", img.width(), img.height());
            if let Some(dir) = std::env::var_os("TILDOWN_TEST_OUT") {
                std::fs::write(PathBuf::from(dir).join(format!("dl{i}.png")), &png).unwrap();
            }
        }
    }
}
