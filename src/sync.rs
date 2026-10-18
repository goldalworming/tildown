//! Sync dengan server TilDown milik pengguna sendiri (`server/`, Cloudflare
//! Worker + Durable Object). Satu request `POST /sync` mengirim catatan yang
//! berubah sejak sync terakhir dan menerima perubahan dari perangkat lain.
//!
//! Tiap catatan punya `base`: versi server (seq) terakhir yang diketahui dan
//! hash teks saat itu. Teks lokal yang hash-nya beda = perlu dikirim. Server
//! menolak kiriman yang `base`-nya kedaluwarsa; kedua versi lalu disimpan
//! (versi lain jadi catatan baru berjudul "… (conflict)").
//! ponytail: konflik = simpan dua-duanya, bukan merge 3 arah; tambahkan merge
//! bila konflik ternyata sering.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::store::Store;

/// Batas satu request: jumlah catatan dan ukuran teks.
const MAX_CHANGES: usize = 100;
const MAX_BYTES: usize = 4 << 20;
/// ponytail: nilai SQLite Durable Object maks. 2 MB; catatan lebih besar
/// tidak di-sync. Pecah ke R2 bila ada yang butuh.
const MAX_NOTE: usize = 1 << 20;

#[derive(Default)]
pub struct Sync {
    pub url: String,
    pub token: String,
    cursor: i64,
    /// id → (seq server terakhir, hash teksnya; 0 = terhapus di server).
    base: HashMap<String, (i64, u64)>,
    /// Kiriman yang sedang jalan: id → hash teks yang dikirim (0 = hapus).
    sent: HashMap<String, u64>,
    /// Catatan yang dibagikan: id → id link (`/s/<sid>`), dari server tiap sync.
    pub shares: HashMap<String, String>,
}

/// Hasil `apply`.
#[derive(Default)]
pub struct Applied {
    /// Catatan yang berubah/terhapus dari luar.
    pub changed: Vec<String>,
    /// Masih ada perubahan di server; sync lagi segera.
    pub more: bool,
    pub conflicts: usize,
}

/// FNV-1a: stabil antarversi Rust (disimpan di `.sync`), tidak pernah 0.
pub fn hash(t: &str) -> u64 {
    let h = t.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    });
    h.max(1)
}

/// Token acak 256 bit (hex) untuk server sync.
pub fn new_token() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("RNG sistem");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Id dari server dipakai sebagai nama berkas: tolak yang bisa keluar folder.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

impl Sync {
    pub fn on(&self) -> bool {
        !self.url.is_empty() && !self.token.is_empty()
    }

    /// Isi berkas `.sync`: `url …`, `token …`, `cursor n`, lalu `id seq hash`.
    pub fn load(s: &str) -> Sync {
        let mut y = Sync::default();
        for l in s.lines() {
            let (k, v) = l.split_once(' ').unwrap_or((l, ""));
            match k {
                "url" => y.url = v.to_owned(),
                "token" => y.token = v.to_owned(),
                "cursor" => y.cursor = v.parse().unwrap_or(0),
                _ => {
                    let mut it = v.split(' ');
                    if let (Some(Ok(seq)), Some(Ok(h))) = (it.next().map(str::parse), it.next().map(str::parse)) {
                        y.base.insert(k.to_owned(), (seq, h));
                    }
                }
            }
        }
        y
    }

    pub fn dump(&self) -> String {
        let mut s = format!("url {}\ntoken {}\ncursor {}\n", self.url, self.token, self.cursor);
        for (id, (seq, h)) in &self.base {
            s += &format!("{id} {seq} {h}\n");
        }
        s
    }

    /// Ganti server: lupakan semua versi (sync pertama mencocokkan ulang).
    /// `url`/`token` boleh berupa kode koneksi (`<url>#<token>`, lihat `code`)
    /// yang ditempel di salah satu isian.
    pub fn connect(&mut self, url: &str, token: &str) {
        let (url, token) = [url, token]
            .into_iter()
            .find_map(|s| s.trim().split_once('#'))
            .unwrap_or((url, token));
        let mut url = url.trim().trim_end_matches('/').to_owned();
        if !url.is_empty() && !url.contains("://") {
            url = format!("https://{url}");
        }
        *self = Sync {
            url,
            token: token.trim().to_owned(),
            ..Sync::default()
        };
    }

    /// Satu kode untuk perangkat lain: URL dan token sekaligus.
    pub fn code(&self) -> String {
        format!("{}#{}", self.url, self.token)
    }

    pub fn endpoint(&self) -> String {
        format!("{}/sync", self.url)
    }

    /// `/share` (link baru; link lama mati) atau `/unshare`.
    pub fn share_endpoint(&self, on: bool) -> String {
        format!("{}/{}", self.url, if on { "share" } else { "unshare" })
    }

    /// Link publik catatan `id` bila sedang dibagikan.
    pub fn share_url(&self, id: &str) -> Option<String> {
        self.shares.get(id).map(|sid| format!("{}/s/{sid}", self.url))
    }

    /// Body request: catatan yang berubah sejak sync terakhir.
    pub fn request(&mut self, store: &Store) -> String {
        self.sent.clear();
        let mut changes = Vec::new();
        let mut bytes = 0;
        let full = |changes: &Vec<Value>, bytes: usize| changes.len() >= MAX_CHANGES || bytes >= MAX_BYTES;
        for n in store.notes.iter().filter(|n| n.on_disk) {
            if full(&changes, bytes) {
                break;
            }
            let h = hash(&n.text);
            let base = match self.base.get(&n.id) {
                Some(&(_, bh)) if bh == h => continue,
                Some(&(seq, _)) => seq,
                None => 0,
            };
            if !valid_id(&n.id) || n.text.len() > MAX_NOTE {
                continue;
            }
            bytes += n.text.len();
            changes.push(json!({ "id": n.id, "base": base, "text": n.text }));
            self.sent.insert(n.id.clone(), h);
        }
        // Ada di `base` tapi tidak ada lagi di folder = dihapus di sini.
        let live: HashSet<&str> = store.notes.iter().map(|n| n.id.as_str()).collect();
        for (id, &(seq, bh)) in &self.base {
            if full(&changes, bytes) {
                break;
            }
            if bh != 0 && !live.contains(id.as_str()) {
                changes.push(json!({ "id": id, "base": seq, "text": null }));
                self.sent.insert(id.clone(), 0);
            }
        }
        json!({ "cursor": self.cursor, "changes": changes }).to_string()
    }

    /// Terapkan jawaban server ke folder catatan.
    pub fn apply(&mut self, store: &mut Store, resp: &str) -> Result<Applied, String> {
        let v: Value = serde_json::from_str(resp).map_err(|e| format!("bad reply: {e}"))?;
        let mut out = Applied::default();
        let sent = std::mem::take(&mut self.sent);
        for r in v["results"].as_array().into_iter().flatten() {
            let (Some(id), Some(seq)) = (r["id"].as_str(), r["seq"].as_i64()) else {
                continue;
            };
            if r["conflict"].as_bool() == Some(true) {
                self.remote(store, id, seq, r["text"].as_str(), &mut out);
            } else {
                match sent.get(id) {
                    Some(0) => _ = self.base.remove(id),
                    Some(&h) => _ = self.base.insert(id.to_owned(), (seq, h)),
                    None => {}
                }
            }
        }
        for n in v["notes"].as_array().into_iter().flatten() {
            let (Some(id), Some(seq)) = (n["id"].as_str(), n["seq"].as_i64()) else {
                continue;
            };
            // Kiriman sendiri yang barusan diterima.
            if self.base.get(id).map(|b| b.0) == Some(seq) {
                continue;
            }
            self.remote(store, id, seq, n["text"].as_str(), &mut out);
        }
        if let Some(a) = v["shares"].as_array() {
            self.shares = a
                .iter()
                .filter_map(|s| Some((s["note"].as_str()?.to_owned(), s["sid"].as_str()?.to_owned())))
                .collect();
        }
        self.cursor = v["cursor"].as_i64().unwrap_or(self.cursor);
        out.more = v["more"].as_bool().unwrap_or(false);
        Ok(out)
    }

    /// Versi server `seq` dari `id` (`None` = dihapus di sana).
    fn remote(&mut self, store: &mut Store, id: &str, seq: i64, text: Option<&str>, out: &mut Applied) {
        if !valid_id(id) {
            return;
        }
        let local = store.notes.iter().position(|n| n.id == id && n.on_disk);
        // Bersih = tidak diubah di sini sejak versi server terakhir.
        let clean = match (local, self.base.get(id)) {
            (Some(i), Some(&(_, bh))) => hash(&store.notes[i].text) == bh,
            (Some(_), None) => false,
            (None, _) => true,
        };
        match (text, local) {
            (Some(t), Some(i)) if store.notes[i].text == t => {
                self.base.insert(id.to_owned(), (seq, hash(t)));
            }
            (Some(t), _) if clean => {
                if store.put(id, t).is_ok() {
                    self.base.insert(id.to_owned(), (seq, hash(t)));
                    out.changed.push(id.to_owned());
                }
            }
            (Some(t), _) => {
                // Dua-duanya diubah: versi di sini tetap (dan menang di server
                // pada sync berikut), versi server jadi catatan baru.
                let (first, rest) = t.split_once('\n').unwrap_or((t, ""));
                let copy = format!("{} (conflict)\n{rest}", first.trim_end());
                let k = store.create(&copy);
                if store.save(k).is_ok() {
                    self.base.insert(id.to_owned(), (seq, hash(t)));
                    out.conflicts += 1;
                }
            }
            (None, Some(i)) if clean => {
                if store.trash(i).is_ok() {
                    self.base.remove(id);
                    out.changed.push(id.to_owned());
                }
            }
            // Dihapus di sana tapi diubah di sini: kirim ulang.
            (None, Some(_)) => _ = self.base.insert(id.to_owned(), (seq, 0)),
            (None, None) => _ = self.base.remove(id),
        }
    }
}

// --- transport -------------------------------------------------------------

/// Kirim `body` ke server di latar; `done` dipanggil dengan isi jawaban.
#[cfg(not(target_arch = "wasm32"))]
pub fn post(url: String, token: String, body: String, done: impl FnOnce(Result<String, String>) + Send + 'static) {
    std::thread::spawn(move || {
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .http_status_as_error(false)
                .timeout_global(Some(std::time::Duration::from_secs(60)))
                .build(),
        );
        let r = agent
            .post(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .send(&body)
            .map_err(|e| e.to_string())
            .and_then(|mut r| {
                let status = r.status().as_u16();
                let text = r
                    .body_mut()
                    .with_config()
                    .limit(256 << 20)
                    .read_to_string()
                    .map_err(|e| e.to_string())?;
                Ok((status, text))
            });
        done(r.and_then(|(status, text)| reply(status, text)));
    });
}

#[cfg(target_arch = "wasm32")]
pub fn post(url: String, token: String, body: String, done: impl FnOnce(Result<String, String>) + 'static) {
    wasm_bindgen_futures::spawn_local(async move {
        let r = crate::web::http_post(&url, &token, &body).await;
        done(r.and_then(|(status, text)| reply(status, text)));
    });
}

/// Status bukan 200 → pesan galat dari server (`{"error": …}`).
fn reply(status: u16, text: String) -> Result<String, String> {
    if status == 200 {
        return Ok(text);
    }
    let msg = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_owned))
        .unwrap_or_else(|| text.chars().take(80).collect());
    Err(format!("{status} {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Server tiruan dengan aturan yang sama seperti `server/src/sync.js`.
    #[derive(Default)]
    struct Server {
        rows: HashMap<String, (i64, Option<String>)>,
        seq: i64,
    }

    impl Server {
        fn sync(&mut self, body: &str) -> String {
            let v: Value = serde_json::from_str(body).unwrap();
            let mut results = Vec::new();
            for c in v["changes"].as_array().unwrap() {
                let id = c["id"].as_str().unwrap().to_owned();
                let cur = self.rows.get(&id).cloned();
                if cur.as_ref().map_or(0, |r| r.0) != c["base"].as_i64().unwrap() {
                    let (seq, text) = cur.unwrap_or((0, None));
                    results.push(json!({"id": id, "conflict": true, "seq": seq, "text": text}));
                    continue;
                }
                self.seq += 1;
                self.rows.insert(id.clone(), (self.seq, c["text"].as_str().map(str::to_owned)));
                results.push(json!({"id": id, "seq": self.seq}));
            }
            let cursor = v["cursor"].as_i64().unwrap();
            let mut notes: Vec<_> = self.rows.iter().filter(|r| r.1 .0 > cursor).collect();
            notes.sort_by_key(|r| r.1 .0);
            let notes: Vec<Value> =
                notes.iter().map(|(id, (s, t))| json!({"id": id, "seq": s, "text": t})).collect();
            let cursor = notes.last().map_or(cursor, |n| n["seq"].as_i64().unwrap());
            json!({"cursor": cursor, "more": false, "results": results, "notes": notes}).to_string()
        }
    }

    fn device(name: &str) -> (Store, Sync) {
        let dir = std::env::temp_dir().join(format!("tildown-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::open(dir).unwrap(), Sync::default())
    }

    fn round(srv: &mut Server, st: &mut Store, y: &mut Sync) -> Applied {
        let body = y.request(st);
        y.apply(st, &srv.sync(&body)).unwrap()
    }

    fn text(st: &Store, id: &str) -> Option<String> {
        st.notes.iter().find(|n| n.id == id).map(|n| n.text.clone())
    }

    #[test]
    fn two_devices() {
        let mut srv = Server::default();
        let (mut a, mut ya) = device("a");
        let (mut b, mut yb) = device("b");
        let i = a.create("Hello\nfrom A");
        a.save(i).unwrap();
        let id = a.notes[i].id.clone();
        round(&mut srv, &mut a, &mut ya);
        // B menerima; tidak ada yang dikirim balik.
        assert_eq!(round(&mut srv, &mut b, &mut yb).changed, vec![id.clone()]);
        assert_eq!(text(&b, &id).as_deref(), Some("Hello\nfrom A"));
        assert!(serde_json::from_str::<Value>(&yb.request(&b)).unwrap()["changes"]
            .as_array()
            .unwrap()
            .is_empty());

        // Edit di B sampai ke A.
        let k = b.notes.iter().position(|n| n.id == id).unwrap();
        b.set_text(k, "Hello\nedited on B");
        b.save(k).unwrap();
        round(&mut srv, &mut b, &mut yb);
        round(&mut srv, &mut a, &mut ya);
        assert_eq!(text(&a, &id).as_deref(), Some("Hello\nedited on B"));

        // Konflik: dua-duanya mengedit; tidak ada yang hilang.
        let ia = a.notes.iter().position(|n| n.id == id).unwrap();
        a.set_text(ia, "Hello\nA wins");
        a.save(ia).unwrap();
        let ib = b.notes.iter().position(|n| n.id == id).unwrap();
        b.set_text(ib, "Hello\nB too");
        b.save(ib).unwrap();
        round(&mut srv, &mut a, &mut ya);
        assert_eq!(round(&mut srv, &mut b, &mut yb).conflicts, 1);
        assert_eq!(text(&b, &id).as_deref(), Some("Hello\nB too"));
        assert!(b.notes.iter().any(|n| n.text == "Hello (conflict)\nA wins"));
        round(&mut srv, &mut b, &mut yb); // versi B + salinan konflik naik
        round(&mut srv, &mut a, &mut ya);
        assert_eq!(text(&a, &id).as_deref(), Some("Hello\nB too"));
        assert!(a.notes.iter().any(|n| n.text == "Hello (conflict)\nA wins"));

        // Hapus di A → hilang di B (ke sampah).
        let ia = a.notes.iter().position(|n| n.id == id).unwrap();
        a.trash(ia).unwrap();
        round(&mut srv, &mut a, &mut ya);
        round(&mut srv, &mut b, &mut yb);
        assert_eq!(text(&b, &id), None);
        assert!(b.trashed().iter().any(|t| t.name == format!("{id}.md")));

        // `.sync` bolak-balik.
        let y2 = Sync::load(&yb.dump());
        assert_eq!((y2.cursor, y2.base.len()), (yb.cursor, yb.base.len()));
        let _ = std::fs::remove_dir_all(&a.dir);
        let _ = std::fs::remove_dir_all(&b.dir);
    }

    #[test]
    fn tokens() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn code() {
        let mut a = Sync::default();
        a.connect("tildown-sync.x.workers.dev/", "abc123");
        let code = a.code();
        assert_eq!(code, "https://tildown-sync.x.workers.dev#abc123");
        for (u, t) in [(code.as_str(), ""), ("", code.as_str()), (code.as_str(), "old")] {
            let mut b = Sync::default();
            b.connect(u, t);
            assert_eq!((b.url.as_str(), b.token.as_str()), (a.url.as_str(), "abc123"));
        }
    }

    #[test]
    fn shares() {
        let mut y = Sync::default();
        y.connect("https://s.dev", "t");
        let mut st = device("shares").0;
        y.apply(&mut st, r#"{"cursor":0,"notes":[],"results":[],"shares":[{"note":"a","sid":"Abc123def456"}]}"#).unwrap();
        assert_eq!(y.share_url("a").as_deref(), Some("https://s.dev/s/Abc123def456"));
        assert_eq!(y.share_endpoint(false), "https://s.dev/unshare");
        // Server lama (tanpa `shares`): daftar tetap.
        y.apply(&mut st, r#"{"cursor":0,"notes":[],"results":[]}"#).unwrap();
        assert!(y.share_url("a").is_some());
        y.apply(&mut st, r#"{"cursor":0,"notes":[],"results":[],"shares":[]}"#).unwrap();
        assert_eq!(y.share_url("a"), None);
    }

    #[test]
    fn ids() {
        assert!(valid_id("20261014-090000-2"));
        for bad in ["", "../x", ".sync", "a/b", "a\\b", "a b"] {
            assert!(!valid_id(bad), "{bad}");
        }
    }
}
