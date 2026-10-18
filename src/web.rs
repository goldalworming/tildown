//! Web (wasm32): pengganti `store::disk` dan fungsi platform kecil.
//!
//! OPFS hanya punya API async, sedangkan `Store` sinkron. Jadi semua berkas
//! dimuat sekali ke cermin di memori (`preload`) sebelum aplikasi jalan;
//! sesudah itu baca dari cermin, tulis ke cermin lalu ke OPFS di latar.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::Duration;

use wasm_bindgen::prelude::*;
use web_time::{SystemTime, UNIX_EPOCH};

#[wasm_bindgen(inline_js = r#"
let dir, q = Promise.resolve();
export async function opfs_load() {
  navigator.storage.persist?.().catch(() => {});
  dir = await navigator.storage.getDirectory();
  const out = [];
  for await (const [name, h] of dir.entries())
    if (h.kind === 'file') { const f = await h.getFile(); out.push(name, await f.text(), f.lastModified); }
  const t = await dir.getDirectoryHandle('.trash', { create: true });
  for await (const [name, h] of t.entries())
    if (h.kind === 'file') { const f = await h.getFile(); out.push('.trash/' + name, await f.text(), f.lastModified); }
  // Gambar: hanya nama; isinya dibaca saat pertama tampil (`opfs_read_asset`).
  const a = await dir.getDirectoryHandle('assets', { create: true });
  for await (const [name, h] of a.entries())
    if (h.kind === 'file') out.push('assets/' + name, null, 0);
  return out;
}
export async function opfs_read_asset(name) {
  const f = await (await (await dir.getDirectoryHandle('assets')).getFileHandle(name)).getFile();
  return new Uint8Array(await f.arrayBuffer());
}
const put = async (d, name, text) => {
  const w = await (await d.getFileHandle(name, { create: true })).createWritable();
  await w.write(text);
  await w.close(); // createWritable menulis ke salinan; berkas lama diganti saat close
};
// Antrean: dua simpanan ke berkas yang sama tidak boleh saling salip.
const run = f => { q = q.then(f).catch(e => console.error('TilDown: OPFS', e)); };
export function opfs_write(name, text) { run(() => put(dir, name, text)); }
// `bytes` adalah tampilan memori wasm: salin sebelum menunggu.
export function opfs_asset(name, bytes) {
  const b = bytes.slice();
  run(async () => put(await dir.getDirectoryHandle('assets', { create: true }), name, b));
}
const trashDir = () => dir.getDirectoryHandle('.trash', { create: true });
export function opfs_trash(name, dst, text) {
  run(async () => { await put(await trashDir(), dst, text); await dir.removeEntry(name); });
}
export function opfs_untrash(name, to, text) {
  run(async () => { await put(dir, to, text); await (await trashDir()).removeEntry(name); });
}
export function opfs_purge(name) { run(async () => (await trashDir()).removeEntry(name)); }
// "Update": ambil build terbaru dari server, bukan dari cache service worker.
export async function update_app() {
  try {
    await (await navigator.serviceWorker?.getRegistration())?.update();
    for (const k of await caches.keys()) await caches.delete(k);
  } catch (e) { console.error('TilDown: update', e); }
  location.reload();
}
export function open_url(u) { window.open(u, '_blank', 'noopener'); }
export function clip_write(t) { navigator.clipboard?.writeText(t).catch(() => {}); }
export function log_error(m) { console.error(m); document.title = 'TilDown — galat'; }
// Keyboard layar: kanvas tidak memunculkan keyboard HP, jadi ketukan di
// kanvas (saat ada isian aktif) memfokuskan textarea tersembunyi ini dan
// isinya diteruskan ke aplikasi. Dua spasi lebar-nol sebagai isi supaya
// Backspace tetap terkirim walau "kosong".
let kb, kbOn = false, kbCb;
// Tombol "Paste code": baca clipboard di dalam gesture ketukan (jenis 7).
export function clip_read() {
  navigator.clipboard?.readText().then(t => kbCb(7, t), () => kbCb(7, ''));
}
export function kb_init(cb) {
  kbCb = cb;
  kb = document.createElement('textarea');
  kb.setAttribute('autocapitalize', 'sentences');
  kb.setAttribute('aria-hidden', 'true');
  Object.assign(kb.style, { position: 'fixed', left: '0', top: '0', width: '1px', height: '1px',
    opacity: '0', border: '0', padding: '0', resize: 'none', fontSize: '16px' });
  document.body.appendChild(kb);
  const Z = '\u200b\u200b';
  const reset = () => { kb.value = Z; kb.setSelectionRange(2, 2); };
  reset();
  let comp = false;
  kb.addEventListener('compositionstart', () => { comp = true; });
  kb.addEventListener('compositionupdate', (e) => cb(1, e.data || ''));
  kb.addEventListener('compositionend', (e) => { comp = false; cb(2, e.data || ''); reset(); });
  kb.addEventListener('input', (e) => {
    if (comp || e.isComposing) return;
    const t = e.inputType;
    if (t === 'insertText' && e.data) cb(2, e.data);
    else if (t === 'insertLineBreak' || t === 'insertParagraph') cb(3, '');
    else if (t === 'deleteContentBackward') cb(4, '');
    else if (t === 'insertFromPaste' || t === 'insertReplacementText') cb(2, kb.value.replaceAll('\u200b', ''));
    reset();
  });
  // Tombol fisik (keyboard Bluetooth) yang tidak lewat `input`.
  kb.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.isComposing) { e.preventDefault(); cb(3, ''); }
    else if (e.key === 'Backspace' && kb.value === Z && !e.isComposing) { e.preventDefault(); cb(4, ''); }
  });
  // Fokus di dalam gesture ketukan, supaya browser mau membuka keyboard.
  document.addEventListener('pointerup', (e) => {
    if (e.pointerType !== 'mouse' && kbOn) kb.focus({ preventScroll: true });
  }, true);
  // Drop berkas .md/.txt → catatan baru (jenis 5); gambar → assets/.
  const isImg = f => /^image\/(png|jpeg|gif|webp)$/.test(f.type);
  const img = async f => imgCb?.(f.name || 'pasted.png', new Uint8Array(await f.arrayBuffer()));
  document.addEventListener('dragover', e => e.preventDefault());
  document.addEventListener('drop', async e => {
    e.preventDefault();
    for (const f of e.dataTransfer.files)
      if (/\.(md|markdown|txt)$/i.test(f.name)) cb(5, await f.text());
      else if (isImg(f)) await img(f);
  });
  // Ctrl+V tidak diteruskan ke winit (yang mencegah default keydown), supaya
  // browser mengirim `paste` berisi clipboard sistem: teks (jenis 6) atau gambar.
  addEventListener('keydown', e => {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'v' && e.target !== kb)
      e.stopImmediatePropagation();
  }, true);
  document.addEventListener('paste', e => {
    const f = [...e.clipboardData.files].find(isImg);
    if (f) { e.preventDefault(); img(f); return; }
    if (e.target === kb) return; // teks lewat `input` textarea
    e.preventDefault();
    const t = e.clipboardData.getData('text/plain');
    if (t) cb(6, t);
  });
}
let imgCb;
export function img_init(cb) { imgCb = cb; }
export function kb_editing(on) {
  kbOn = on;
  if (!on && document.activeElement === kb) kb.blur();
}
export function download(name, bytes) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10000);
}
export async function http_post(url, token, body) {
  const r = await fetch(url, { method: 'POST', body,
    headers: { 'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json' } });
  return [r.status, await r.text()];
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn opfs_load() -> Result<JsValue, JsValue>;
    fn opfs_write(name: &str, text: &str);
    fn opfs_asset(name: &str, bytes: &[u8]);
    #[wasm_bindgen(catch)]
    async fn opfs_read_asset(name: &str) -> Result<JsValue, JsValue>;
    fn opfs_trash(name: &str, dst: &str, text: &str);
    fn opfs_untrash(name: &str, to: &str, text: &str);
    fn opfs_purge(name: &str);
    pub fn open_url(u: &str);
    pub fn update_app();
    pub fn clip_write(t: &str);
    pub fn log_error(m: &str);
    pub fn download(name: &str, bytes: &[u8]);
    pub fn kb_editing(on: bool);
    pub fn clip_read();
    #[wasm_bindgen(js_name = kb_init)]
    fn kb_init_js(cb: &Closure<dyn FnMut(u8, String)>);
    #[wasm_bindgen(js_name = img_init)]
    fn img_init_js(cb: &Closure<dyn FnMut(String, Vec<u8>)>);
    #[wasm_bindgen(catch, js_name = http_post)]
    async fn http_post_js(url: &str, token: &str, body: &str) -> Result<JsValue, JsValue>;
}

/// Pasang keyboard layar; `f(jenis, teks)` dipanggil untuk tiap ketikan
/// (1 = preedit, 2 = teks, 3 = Enter, 4 = Backspace), juga isi berkas
/// yang di-drop (5), teks yang ditempel (6) dan isi clipboard untuk
/// "Paste code" (7).
pub fn kb_init(f: impl FnMut(u8, String) + 'static) {
    let c = Closure::<dyn FnMut(u8, String)>::new(f);
    kb_init_js(&c);
    c.forget(); // hidup selama halaman
}

/// Gambar yang ditempel atau di-drop: `f(nama, isi)`.
pub fn img_init(f: impl FnMut(String, Vec<u8>) + 'static) {
    let c = Closure::<dyn FnMut(String, Vec<u8>)>::new(f);
    img_init_js(&c);
    c.forget();
}

/// `POST` untuk sync → (status, isi).
pub async fn http_post(url: &str, token: &str, body: &str) -> Result<(u16, String), String> {
    let v = http_post_js(url, token, body).await.map_err(|e| {
        e.dyn_ref::<js_sys::Error>()
            .map_or_else(|| format!("{e:?}"), |e| String::from(e.message()))
    })?;
    let a = js_sys::Array::from(&v);
    Ok((a.get(0).as_f64().unwrap_or(0.0) as u16, a.get(1).as_string().unwrap_or_default()))
}

thread_local! {
    static FILES: RefCell<HashMap<String, (String, SystemTime)>> = RefCell::new(HashMap::new());
    /// Gambar di `assets/`, kunci `assets/<nama>`; `None` = belum dibaca.
    static ASSETS: RefCell<HashMap<String, Option<Vec<u8>>>> = RefCell::new(HashMap::new());
    static LOADING: RefCell<std::collections::HashSet<String>> = RefCell::default();
    static ON_ASSET: RefCell<Option<Box<dyn Fn(String)>>> = RefCell::new(None);
}

/// `f(nama)` dipanggil saat isi gambar selesai dibaca dari OPFS.
pub fn on_asset(f: impl Fn(String) + 'static) {
    ON_ASSET.set(Some(Box::new(f)));
}

/// Muat semua berkas OPFS ke cermin. Dipanggil sekali sebelum `Store::open`.
pub async fn preload() -> Result<(), String> {
    let v = opfs_load().await.map_err(|e| format!("OPFS: {e:?}"))?;
    let a = js_sys::Array::from(&v);
    FILES.with_borrow_mut(|m| {
        for k in (0..a.length()).step_by(3) {
            if let Some(name) = a.get(k).as_string().filter(|n| n.starts_with("assets/")) {
                ASSETS.with_borrow_mut(|m| m.insert(name, None));
                continue;
            }
            let (Some(name), Some(text), Some(ms)) = (
                a.get(k).as_string(),
                a.get(k + 1).as_string(),
                a.get(k + 2).as_f64(),
            ) else {
                continue;
            };
            m.insert(name, (text, UNIX_EPOCH + Duration::from_millis(ms as u64)));
        }
    });
    Ok(())
}

// --- pengganti store::disk ------------------------------------------------

pub fn init(_: &Path) -> io::Result<()> {
    Ok(())
}

pub fn list(_: &Path) -> io::Result<Vec<(String, SystemTime)>> {
    Ok(FILES.with_borrow(|m| {
        m.iter()
            .filter(|(n, _)| !n.starts_with(".trash/"))
            .filter_map(|(n, (_, t))| Some((n.strip_suffix(".md")?.to_owned(), *t)))
            .collect()
    }))
}

pub fn read(_: &Path, name: &str) -> Option<String> {
    FILES.with_borrow(|m| m.get(name).map(|(t, _)| t.clone()))
}

pub fn exists(_: &Path, name: &str) -> bool {
    FILES.with_borrow(|m| m.contains_key(name)) || ASSETS.with_borrow(|m| m.contains_key(name))
}

/// Isi gambar kalau sudah dibaca; kalau belum, baca di latar lalu `on_asset`.
pub fn read_bytes(_: &Path, name: &str) -> Option<Vec<u8>> {
    match ASSETS.with_borrow(|m| m.get(name).cloned())? {
        Some(b) => Some(b),
        None => {
            if LOADING.with_borrow_mut(|s| s.insert(name.to_owned())) {
                let name = name.to_owned();
                wasm_bindgen_futures::spawn_local(async move {
                    // Gagal: tetap di LOADING supaya tidak dicoba terus tiap layout.
                    let Ok(v) = opfs_read_asset(&name["assets/".len()..]).await else { return };
                    let b = js_sys::Uint8Array::new(&v).to_vec();
                    ASSETS.with_borrow_mut(|m| m.insert(name.clone(), Some(b)));
                    LOADING.with_borrow_mut(|s| s.remove(&name));
                    ON_ASSET.with_borrow(|f| f.as_ref().map(|f| f(name)));
                });
            }
            None
        }
    }
}

/// Hanya untuk `assets/<nama>`.
pub fn write_bytes(_: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let file = name.strip_prefix("assets/").ok_or(io::ErrorKind::InvalidInput)?;
    opfs_asset(file, bytes);
    ASSETS.with_borrow_mut(|m| m.insert(name.to_owned(), Some(bytes.to_vec())));
    Ok(())
}

// ponytail: penulisan OPFS selesai di latar. Tab yang ditutup tepat saat
// menyimpan bisa kehilangan ketikan ≤ 1 detik terakhir. Tambahkan
// sync access handle di worker bila itu terjadi.
pub fn write(_: &Path, name: &str, text: &str) -> io::Result<SystemTime> {
    let now = SystemTime::now();
    FILES.with_borrow_mut(|m| m.insert(name.to_owned(), (text.to_owned(), now)));
    opfs_write(name, text);
    Ok(now)
}

/// Sampah di cermin: kunci `.trash/<nama>`, waktu = saat dihapus.
pub fn trash(_: &Path, name: &str) -> io::Result<()> {
    FILES.with_borrow_mut(|m| {
        let (text, _) = m.remove(name).ok_or(io::ErrorKind::NotFound)?;
        let stem = name.trim_end_matches(".md");
        let mut dst = name.to_owned();
        let mut k = 2;
        while m.contains_key(&format!(".trash/{dst}")) {
            dst = format!("{stem}-{k}.md");
            k += 1;
        }
        opfs_trash(name, &dst, &text);
        m.insert(format!(".trash/{dst}"), (text, SystemTime::now()));
        Ok(())
    })
}

pub fn trash_list(_: &Path) -> Vec<(String, SystemTime)> {
    FILES.with_borrow(|m| {
        m.iter()
            .filter_map(|(n, (_, t))| Some((n.strip_prefix(".trash/")?.to_owned(), *t)))
            .collect()
    })
}

pub fn trash_read(_: &Path, name: &str) -> Option<String> {
    FILES.with_borrow(|m| m.get(&format!(".trash/{name}")).map(|(t, _)| t.clone()))
}

pub fn untrash(_: &Path, name: &str, to: &str) -> io::Result<()> {
    FILES.with_borrow_mut(|m| {
        let (text, _) = m
            .remove(&format!(".trash/{name}"))
            .ok_or(io::ErrorKind::NotFound)?;
        opfs_untrash(name, to, &text);
        m.insert(to.to_owned(), (text, SystemTime::now()));
        Ok(())
    })
}

pub fn purge(_: &Path, name: &str) -> io::Result<()> {
    FILES.with_borrow_mut(|m| m.remove(&format!(".trash/{name}")));
    opfs_purge(name);
    Ok(())
}
