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
  return out;
}
const put = async (d, name, text) => {
  const w = await (await d.getFileHandle(name, { create: true })).createWritable();
  await w.write(text);
  await w.close(); // createWritable menulis ke salinan; berkas lama diganti saat close
};
// Antrean: dua simpanan ke berkas yang sama tidak boleh saling salip.
const run = f => { q = q.then(f).catch(e => console.error('TilDown: OPFS', e)); };
export function opfs_write(name, text) { run(() => put(dir, name, text)); }
const trashDir = () => dir.getDirectoryHandle('.trash', { create: true });
export function opfs_trash(name, dst, text) {
  run(async () => { await put(await trashDir(), dst, text); await dir.removeEntry(name); });
}
export function opfs_untrash(name, to, text) {
  run(async () => { await put(dir, to, text); await (await trashDir()).removeEntry(name); });
}
export function opfs_purge(name) { run(async () => (await trashDir()).removeEntry(name)); }
export function open_url(u) { window.open(u, '_blank', 'noopener'); }
export function clip_write(t) { navigator.clipboard?.writeText(t).catch(() => {}); }
export function log_error(m) { console.error(m); document.title = 'TilDown — galat'; }
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
    fn opfs_trash(name: &str, dst: &str, text: &str);
    fn opfs_untrash(name: &str, to: &str, text: &str);
    fn opfs_purge(name: &str);
    pub fn open_url(u: &str);
    pub fn clip_write(t: &str);
    pub fn log_error(m: &str);
    pub fn download(name: &str, bytes: &[u8]);
    #[wasm_bindgen(catch, js_name = http_post)]
    async fn http_post_js(url: &str, token: &str, body: &str) -> Result<JsValue, JsValue>;
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
}

/// Muat semua berkas OPFS ke cermin. Dipanggil sekali sebelum `Store::open`.
pub async fn preload() -> Result<(), String> {
    let v = opfs_load().await.map_err(|e| format!("OPFS: {e:?}"))?;
    let a = js_sys::Array::from(&v);
    FILES.with_borrow_mut(|m| {
        for k in (0..a.length()).step_by(3) {
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
    FILES.with_borrow(|m| m.contains_key(name))
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
