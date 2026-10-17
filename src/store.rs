//! Folder catatan: satu berkas `.md` per catatan, semuanya dimuat ke memori.
//! 10.000 catatan × ~2 KB ≈ 20 MB teks — pencarian linear masih jauh di bawah
//! 30 ms, jadi tidak ada database (PRD §6).

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use web_time::{SystemTime, UNIX_EPOCH};

#[cfg(target_arch = "wasm32")]
use crate::web as disk;

pub struct Note {
    /// Nama berkas tanpa `.md`. Stabil: mengganti judul tidak me-rename berkas.
    pub id: String,
    pub text: String,
    /// `text` huruf kecil, untuk pencarian.
    pub lower: String,
    pub mtime: SystemTime,
    /// Catatan baru yang belum pernah diisi tidak ditulis ke disk.
    pub on_disk: bool,
}

impl Note {
    fn new(id: String, text: String, mtime: SystemTime, on_disk: bool) -> Note {
        let lower = text.to_lowercase();
        Note {
            id,
            text,
            lower,
            mtime,
            on_disk,
        }
    }

    pub fn title(&self) -> &str {
        title_of(&self.text)
    }

    pub fn snippet(&self) -> String {
        snippet_of(&self.text)
    }
}

/// Beberapa baris setelah judul, digabung, untuk daftar catatan.
pub fn snippet_of(text: &str) -> String {
    let mut out = String::new();
    for l in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .skip(1)
        .take(3)
    {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(l);
        if out.len() > 200 {
            break;
        }
    }
    out.chars().take(160).collect()
}

/// Baris pertama yang tidak kosong, tanpa `#`, maksimal 200 karakter.
pub fn title_of(text: &str) -> &str {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let t = line.trim_start_matches('#').trim();
    let t = t.char_indices().nth(200).map_or(t, |(k, _)| &t[..k]);
    if t.is_empty() {
        "New note"
    } else {
        t
    }
}

pub struct Trashed {
    /// Nama berkas di `.trash/`.
    pub name: String,
    pub title: String,
    pub when: SystemTime,
}

pub struct Store {
    pub dir: PathBuf,
    pub notes: Vec<Note>,
}

/// `MARKDOWN_DIR`, atau `Documents/TilDown` di folder pengguna (folder lama
/// `Documents/markdown` dipindah sekali).
pub fn default_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("MARKDOWN_DIR") {
        return d.into();
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let docs = home.join("Documents");
    let dir = docs.join("TilDown");
    if !dir.exists() {
        let _ = std::fs::rename(docs.join("markdown"), &dir);
    }
    dir
}

/// Berkas di folder catatan, dengan nama (`<id>.md`, `.state`). Di web
/// diganti `web`: cermin di memori yang ditulis ke OPFS.
#[cfg(not(target_arch = "wasm32"))]
mod disk {
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn init(dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)
    }

    /// Id dan mtime semua berkas `.md`.
    pub fn list(dir: &Path) -> io::Result<Vec<(String, SystemTime)>> {
        let mut v = Vec::new();
        for e in fs::read_dir(dir)? {
            let Ok(e) = e else { continue };
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("md") {
                continue;
            }
            let (Some(id), Ok(meta)) = (p.file_stem().and_then(|x| x.to_str()), e.metadata())
            else {
                continue;
            };
            if meta.is_file() {
                v.push((id.to_owned(), meta.modified().unwrap_or(UNIX_EPOCH)));
            }
        }
        Ok(v)
    }

    pub fn read(dir: &Path, name: &str) -> Option<String> {
        fs::read_to_string(dir.join(name)).ok()
    }

    pub fn exists(dir: &Path, name: &str) -> bool {
        dir.join(name).exists()
    }

    /// Tulis atomik: berkas `.tmp` di-fsync lalu di-rename, jadi berkas lama
    /// tidak pernah setengah tertulis kalau listrik mati.
    pub fn write(dir: &Path, name: &str, text: &str) -> io::Result<SystemTime> {
        let path = dir.join(name);
        let tmp = dir.join(format!("{name}.tmp"));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        fs::metadata(&path)?.modified()
    }

    /// Pindahkan ke `.trash/` (bisa dipulihkan dengan tangan).
    pub fn trash(dir: &Path, name: &str) -> io::Result<()> {
        let t = dir.join(".trash");
        fs::create_dir_all(&t)?;
        let mut dst = t.join(name);
        let mut k = 2;
        while dst.exists() {
            dst = t.join(format!("{}-{k}.md", name.trim_end_matches(".md")));
            k += 1;
        }
        fs::rename(dir.join(name), &dst)?;
        // mtime = waktu dihapus: dasar "Deleted 2 days ago" dan purge 30 hari.
        fs::File::options()
            .write(true)
            .open(&dst)?
            .set_modified(SystemTime::now())
    }

    /// Nama dan waktu hapus berkas di `.trash/`.
    pub fn trash_list(dir: &Path) -> Vec<(String, SystemTime)> {
        let Ok(rd) = fs::read_dir(dir.join(".trash")) else {
            return Vec::new();
        };
        rd.flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let m = e.metadata().ok()?;
                (name.ends_with(".md") && m.is_file())
                    .then(|| (name, m.modified().unwrap_or(UNIX_EPOCH)))
            })
            .collect()
    }

    pub fn trash_read(dir: &Path, name: &str) -> Option<String> {
        fs::read_to_string(dir.join(".trash").join(name)).ok()
    }

    /// Kembalikan `.trash/name` sebagai `to`.
    pub fn untrash(dir: &Path, name: &str, to: &str) -> io::Result<()> {
        fs::rename(dir.join(".trash").join(name), dir.join(to))
    }

    pub fn purge(dir: &Path, name: &str) -> io::Result<()> {
        fs::remove_file(dir.join(".trash").join(name))
    }
}

fn read_note(dir: &std::path::Path, id: &str) -> Option<String> {
    disk::read(dir, &format!("{id}.md")).map(|t| {
        if t.contains('\r') {
            t.replace("\r\n", "\n")
        } else {
            t
        }
    })
}

impl Store {
    pub fn open(dir: PathBuf) -> io::Result<Store> {
        disk::init(&dir)?;
        let mut s = Store {
            dir,
            notes: Vec::new(),
        };
        s.rescan(None)?;
        // Sampah lebih dari 30 hari dihapus permanen.
        let old = SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
        for (name, when) in disk::trash_list(&s.dir) {
            if when < old {
                let _ = disk::purge(&s.dir, &name);
            }
        }
        Ok(s)
    }

    /// Sinkronkan dengan disk: muat berkas baru atau yang berubah dari luar
    /// (Syncthing, editor lain), buang yang hilang. `keep` = id catatan yang
    /// sedang diedit dan belum disimpan; versi di memori menang.
    /// Mengembalikan id catatan yang dimuat ulang.
    pub fn rescan(&mut self, keep: Option<&str>) -> io::Result<Vec<String>> {
        let seen: HashMap<String, SystemTime> = disk::list(&self.dir)?.into_iter().collect();
        let idx: HashMap<String, usize> = self
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut changed = Vec::new();
        for (id, &mtime) in &seen {
            if Some(id.as_str()) == keep {
                continue;
            }
            match idx.get(id) {
                Some(&i) if self.notes[i].on_disk && self.notes[i].mtime == mtime => {}
                Some(&i) => {
                    if let Some(t) = read_note(&self.dir, id) {
                        self.notes[i] = Note::new(id.clone(), t, mtime, true);
                        changed.push(id.clone());
                    }
                }
                None => {
                    if let Some(t) = read_note(&self.dir, id) {
                        self.notes.push(Note::new(id.clone(), t, mtime, true));
                    }
                }
            }
        }
        self.notes
            .retain(|n| !n.on_disk || seen.contains_key(&n.id) || Some(n.id.as_str()) == keep);
        Ok(changed)
    }

    pub fn set_text(&mut self, i: usize, text: &str) {
        let n = &mut self.notes[i];
        n.text = text.to_owned();
        n.lower = text.to_lowercase();
        n.mtime = SystemTime::now();
    }

    pub fn save(&mut self, i: usize) -> io::Result<()> {
        let n = &self.notes[i];
        if !n.on_disk && n.text.trim().is_empty() {
            return Ok(());
        }
        let mtime = disk::write(&self.dir, &format!("{}.md", n.id), &n.text)?;
        let n = &mut self.notes[i];
        n.mtime = mtime;
        n.on_disk = true;
        Ok(())
    }

    pub fn create(&mut self, text: &str) -> usize {
        let base = stamp(SystemTime::now());
        let mut id = base.clone();
        let mut k = 2;
        while self.notes.iter().any(|n| n.id == id) || disk::exists(&self.dir, &format!("{id}.md"))
        {
            id = format!("{base}-{k}");
            k += 1;
        }
        self.notes
            .push(Note::new(id, text.to_owned(), SystemTime::now(), false));
        self.notes.len() - 1
    }

    /// Pindahkan ke `.trash/`, lalu lupakan.
    pub fn trash(&mut self, i: usize) -> io::Result<()> {
        let n = &self.notes[i];
        if n.on_disk {
            disk::trash(&self.dir, &format!("{}.md", n.id))?;
        }
        self.notes.remove(i);
        Ok(())
    }

    /// Isi `.trash/`, terbaru dihapus di atas.
    pub fn trashed(&self) -> Vec<Trashed> {
        let mut v: Vec<Trashed> = disk::trash_list(&self.dir)
            .into_iter()
            .map(|(name, when)| {
                let text = disk::trash_read(&self.dir, &name).unwrap_or_default();
                let title = title_of(&text).to_owned();
                Trashed { name, title, when }
            })
            .collect();
        v.sort_by(|a, b| b.when.cmp(&a.when));
        v
    }

    /// Pulihkan dari `.trash/`; mengembalikan indeks catatan.
    pub fn restore(&mut self, name: &str) -> io::Result<usize> {
        let text = disk::trash_read(&self.dir, name).ok_or(io::ErrorKind::NotFound)?;
        let base = name.trim_end_matches(".md").to_owned();
        let mut id = base.clone();
        let mut k = 2;
        while self.notes.iter().any(|n| n.id == id) || disk::exists(&self.dir, &format!("{id}.md"))
        {
            id = format!("{base}-{k}");
            k += 1;
        }
        disk::untrash(&self.dir, name, &format!("{id}.md"))?;
        let text = text.replace("\r\n", "\n");
        self.notes.push(Note::new(id, text, SystemTime::now(), true));
        Ok(self.notes.len() - 1)
    }

    /// Hapus permanen dari `.trash/`.
    pub fn purge(&self, name: &str) -> io::Result<()> {
        disk::purge(&self.dir, name)
    }

    /// Hasil pencarian (tanpa memandang huruf besar) dan jumlah hasil persis.
    /// Persis = semua kata di `q` ada di teks; ini di depan, terbaru di atas.
    /// Sesudahnya hasil fuzzy: kata yang tidak ada di teks cukup muncul
    /// berurutan di judul (`blj` → "Belanja"). Hanya judul: subsequence di
    /// isi panjang cocok dengan hampir semua query pendek.
    pub fn filter(&self, q: &str) -> (Vec<usize>, usize) {
        let q = q.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let (mut exact, mut fuzzy) = (Vec::new(), Vec::new());
        for (i, n) in self.notes.iter().enumerate() {
            if terms.iter().all(|t| n.lower.contains(t)) {
                exact.push(i);
            } else {
                let title = title_of(&n.lower);
                if terms
                    .iter()
                    .all(|t| n.lower.contains(t) || subseq(t, title))
                {
                    fuzzy.push(i);
                }
            }
        }
        for v in [&mut exact, &mut fuzzy] {
            v.sort_by(|&a, &b| self.notes[b].mtime.cmp(&self.notes[a].mtime));
        }
        let k = exact.len();
        exact.append(&mut fuzzy);
        (exact, k)
    }

    /// Tulis catatan dari luar (sync) dengan id tertentu; buat bila belum ada.
    pub fn put(&mut self, id: &str, text: &str) -> io::Result<usize> {
        let mtime = disk::write(&self.dir, &format!("{id}.md"), text)?;
        let n = Note::new(id.to_owned(), text.to_owned(), mtime, true);
        Ok(match self.notes.iter().position(|n| n.id == id) {
            Some(i) => {
                self.notes[i] = n;
                i
            }
            None => {
                self.notes.push(n);
                self.notes.len() - 1
            }
        })
    }

    /// Berkas bantu di folder catatan (mis. `.sync`).
    pub fn read_file(&self, name: &str) -> Option<String> {
        disk::read(&self.dir, name)
    }

    pub fn write_file(&self, name: &str, text: &str) -> io::Result<()> {
        disk::write(&self.dir, name, text).map(|_| ())
    }

    pub fn load_state(&self) -> HashMap<String, String> {
        let s = disk::read(&self.dir, ".state").unwrap_or_default();
        s.lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    pub fn save_state(&self, kv: &[(&str, &str)]) {
        let s: String = kv.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let _ = disk::write(&self.dir, ".state", &s);
    }
}

/// Semua karakter `needle` muncul di `hay` dengan urutan yang sama.
fn subseq(needle: &str, hay: &str) -> bool {
    let mut h = hay.chars();
    needle.chars().all(|c| h.any(|x| x == c))
}

/// Hari sejak 1970-01-01 → (tahun, bulan, tanggal). Algoritma Howard Hinnant.
fn civil(days: i64) -> (i64, usize, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as usize;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

fn secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// ponytail: semua waktu dalam UTC. Id berkas tidak terpengaruh; tanggal di
// daftar bisa meleset sehari di dekat tengah malam. Tambahkan zona waktu lokal
// (GetTimeZoneInformation / localtime_r) bila itu mengganggu.
pub fn stamp(t: SystemTime) -> String {
    let s = secs(t);
    let (y, m, d) = civil(s.div_euclid(86_400));
    let r = s.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        r / 3600,
        r % 3600 / 60,
        r % 60
    )
}

/// "just now", "5 min ago", "3 hours ago", "yesterday", "2 weeks ago", ...
pub fn ago(t: SystemTime, now: SystemTime) -> String {
    let d = (secs(now) - secs(t)).max(0);
    let n = |v: i64, unit: &str| format!("{v} {unit}{} ago", if v == 1 { "" } else { "s" });
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => n(d / 3600, "hour"),
        86_400..=172_799 => "yesterday".into(),
        _ if d < 7 * 86_400 => n(d / 86_400, "day"),
        _ if d < 30 * 86_400 => n(d / (7 * 86_400), "week"),
        _ if d < 365 * 86_400 => n(d / (30 * 86_400), "month"),
        _ => n(d / (365 * 86_400), "year"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn dates() {
        assert_eq!(stamp(UNIX_EPOCH), "19700101-000000");
        assert_eq!(
            stamp(UNIX_EPOCH + Duration::from_secs(1_704_067_200 + 3_661)),
            "20240101-010101"
        );
        assert_eq!(civil(-1), (1969, 12, 31));
        let now = UNIX_EPOCH + Duration::from_secs(1_704_067_200);
        assert_eq!(ago(now - Duration::from_secs(5), now), "just now");
        assert_eq!(ago(now - Duration::from_secs(300), now), "5 min ago");
        assert_eq!(ago(now - Duration::from_secs(3600), now), "1 hour ago");
        assert_eq!(ago(now - Duration::from_secs(90_000), now), "yesterday");
        assert_eq!(ago(now - Duration::from_secs(86_400 * 3), now), "3 days ago");
        assert_eq!(ago(now - Duration::from_secs(86_400 * 14), now), "2 weeks ago");
    }

    #[test]
    fn notes_roundtrip_through_disk() {
        let dir = std::env::temp_dir().join(format!("markdown-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut s = Store::open(dir.clone()).unwrap();
        assert!(s.notes.is_empty());

        let empty = s.create("   ");
        s.save(empty).unwrap();
        assert!(!s.notes[empty].on_disk, "catatan kosong tidak ditulis");

        let a = s.create("");
        s.set_text(a, "# Belanja\n\nKopi dan GULA\n");
        s.save(a).unwrap();
        let id = s.notes[a].id.clone();
        assert_ne!(id, s.notes[empty].id);
        assert_eq!(
            fs::read_to_string(dir.join(format!("{id}.md"))).unwrap(),
            "# Belanja\n\nKopi dan GULA\n"
        );
        assert_eq!(s.notes[a].title(), "Belanja");
        assert_eq!(s.notes[a].snippet(), "Kopi dan GULA");
        assert_eq!(s.filter("gula kopi"), (vec![a], 1));
        assert!(s.filter("teh").0.is_empty());
        assert_eq!(s.filter("blj"), (vec![a], 0), "fuzzy di judul");
        assert_eq!(
            s.filter("blj gula"),
            (vec![a], 0),
            "campuran fuzzy dan persis"
        );
        assert!(s.filter("jlb").0.is_empty(), "urutan huruf harus sama");
        assert!(
            s.filter("kpi").0.is_empty(),
            "fuzzy hanya di judul, bukan isi"
        );

        // Berubah dari luar, CRLF dinormalisasi.
        std::thread::sleep(Duration::from_millis(20));
        fs::write(dir.join(format!("{id}.md")), "# Belanja\r\nteh\r\n").unwrap();
        fs::write(dir.join("lain.md"), "lain").unwrap();
        assert_eq!(s.rescan(None).unwrap(), [id.clone()]);
        assert_eq!(s.notes[a].text, "# Belanja\nteh\n");
        assert_eq!(s.notes.len(), 3);

        let s2 = Store::open(dir.clone()).unwrap();
        assert_eq!(s2.notes.len(), 2, "hanya berkas di disk");

        s.trash(a).unwrap();
        assert!(dir.join(".trash").join(format!("{id}.md")).exists());
        assert!(!dir.join(format!("{id}.md")).exists());
        let t = s.trashed();
        assert_eq!((t.len(), t[0].title.as_str()), (1, "Belanja"));
        let r = s.restore(&t[0].name).unwrap();
        assert_eq!((s.notes[r].id.as_str(), s.notes[r].text.as_str()), (id.as_str(), "# Belanja\nteh\n"));
        assert!(dir.join(format!("{id}.md")).exists() && s.trashed().is_empty());
        s.trash(r).unwrap();
        s.purge(&s.trashed()[0].name).unwrap();
        assert!(s.trashed().is_empty());
        s.save_state(&[("last", "lain")]);
        assert_eq!(s.load_state()["last"], "lain");
        let _ = fs::remove_dir_all(&dir);
    }
}
