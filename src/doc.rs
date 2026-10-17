//! Inti editor. Tidak tahu apa-apa soal winit, Win32 atau Android: shell
//! menerjemahkan inputnya ke perintah di bawah. IME Android nanti memakai
//! perintah yang sama (sisipkan, hapus, set seleksi).

use std::time::Duration;

use web_time::Instant;

/// Ketikan beruntun dalam jendela waktu ini digabung jadi satu langkah undo.
const MERGE: Duration = Duration::from_millis(1000);
const UNDO_MAX: usize = 500;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Type,
    Back,
    Del,
    Other,
}

/// Satu perubahan: di `at`, `old` diganti `new`. Disimpan sebagai diff, bukan
/// salinan dokumen, supaya undo dokumen 1 MB tidak memakan ratusan MB.
#[derive(Debug)]
struct Edit {
    at: usize,
    old: String,
    new: String,
    sel_before: (usize, usize),
    sel_after: (usize, usize),
    kind: Kind,
    time: Instant,
    sealed: bool,
}

/// `cursor` dan `anchor` adalah offset byte yang selalu berada di batas
/// karakter UTF-8; seleksi = rentang di antaranya. `version` naik setiap kali
/// teks berubah (dipakai cache layout dan penanda belum disimpan).
#[derive(Default, Debug)]
pub struct Doc {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub version: u64,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

/// 0 = spasi, 1 = huruf/angka/_, 2 = tanda baca lain.
fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

impl Doc {
    pub fn new(text: &str) -> Doc {
        Doc {
            text: text.to_owned(),
            cursor: text.len(),
            anchor: text.len(),
            ..Default::default()
        }
    }

    /// Ganti seluruh isi tanpa riwayat undo (memuat ulang dari disk).
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_owned();
        self.cursor = self.cursor.min(text.len());
        while !self.text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
        self.anchor = self.cursor;
        self.version += 1;
        self.undo.clear();
        self.redo.clear();
    }

    /// Rentang seleksi terurut (awal, akhir). Kosong bila cursor == anchor.
    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    pub fn selected_text(&self) -> &str {
        let (a, b) = self.selection();
        &self.text[a..b]
    }

    fn snap(&self, mut i: usize) -> usize {
        i = i.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    /// Navigasi memutus penggabungan undo: ketikan setelah cursor dipindah
    /// adalah langkah baru.
    fn seal(&mut self) {
        if let Some(e) = self.undo.last_mut() {
            e.sealed = true;
        }
    }

    pub fn set_cursor(&mut self, pos: usize, extend: bool) {
        self.cursor = self.snap(pos);
        if !extend {
            self.anchor = self.cursor;
        }
        self.seal();
    }

    pub fn set_selection(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.snap(anchor);
        self.cursor = self.snap(cursor);
        self.seal();
    }

    fn edit(&mut self, a: usize, b: usize, s: &str, kind: Kind) {
        if a == b && s.is_empty() {
            return;
        }
        let sel_before = (self.anchor, self.cursor);
        let old = self.text[a..b].to_owned();
        self.text.replace_range(a..b, s);
        self.cursor = a + s.len();
        self.anchor = self.cursor;
        self.version += 1;
        self.redo.clear();
        let sel_after = (self.anchor, self.cursor);
        let now = Instant::now();
        if let Some(p) = self.undo.last_mut() {
            if !p.sealed && p.kind == kind && now - p.time < MERGE {
                let merged = match kind {
                    Kind::Type if old.is_empty() && a == p.at + p.new.len() => {
                        p.new.push_str(s);
                        true
                    }
                    Kind::Back if b == p.at && p.new.is_empty() => {
                        p.at = a;
                        p.old.insert_str(0, &old);
                        true
                    }
                    Kind::Del if a == p.at && p.new.is_empty() => {
                        p.old.push_str(&old);
                        true
                    }
                    _ => false,
                };
                if merged {
                    p.sel_after = sel_after;
                    p.time = now;
                    return;
                }
            }
        }
        self.undo.push(Edit {
            at: a,
            old,
            new: s.to_owned(),
            sel_before,
            sel_after,
            kind,
            time: now,
            sealed: false,
        });
        if self.undo.len() > UNDO_MAX {
            self.undo.remove(0);
        }
    }

    /// Ganti seleksi dengan `s` (atau sisipkan di cursor bila tidak ada seleksi).
    pub fn insert(&mut self, s: &str) {
        let (a, b) = self.selection();
        let typing = a == b && s.chars().count() == 1 && !s.contains(char::is_whitespace);
        self.edit(a, b, s, if typing { Kind::Type } else { Kind::Other });
    }

    /// Ganti rentang tertentu tanpa melihat seleksi (mis. centang task list).
    pub fn replace_range(&mut self, a: usize, b: usize, s: &str) {
        let (a, b) = (self.snap(a), self.snap(b));
        self.edit(a, b, s, Kind::Other);
    }

    pub fn backspace(&mut self) {
        let (a, b) = self.selection();
        if a != b {
            self.edit(a, b, "", Kind::Other);
        } else if a > 0 {
            self.edit(self.prev(a), a, "", Kind::Back);
        }
    }

    pub fn delete(&mut self) {
        let (a, b) = self.selection();
        if a != b {
            self.edit(a, b, "", Kind::Other);
        } else if a < self.text.len() {
            self.edit(a, self.next(a), "", Kind::Del);
        }
    }

    pub fn delete_word_back(&mut self) {
        let (a, b) = self.selection();
        let a = if a == b { self.word_left_of(a) } else { a };
        self.edit(a, b, "", Kind::Other);
    }

    pub fn delete_word_fwd(&mut self) {
        let (a, b) = self.selection();
        let b = if a == b { self.word_right_of(b) } else { b };
        self.edit(a, b, "", Kind::Other);
    }

    pub fn undo(&mut self) -> bool {
        let Some(mut e) = self.undo.pop() else {
            return false;
        };
        self.text.replace_range(e.at..e.at + e.new.len(), &e.old);
        (self.anchor, self.cursor) = e.sel_before;
        self.version += 1;
        e.sealed = true;
        self.redo.push(e);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(e) = self.redo.pop() else {
            return false;
        };
        self.text.replace_range(e.at..e.at + e.old.len(), &e.new);
        (self.anchor, self.cursor) = e.sel_after;
        self.version += 1;
        self.undo.push(e);
        true
    }

    /// Geser cursor satu karakter. `extend` = Shift ditahan (seleksi melebar).
    /// Tanpa `extend`, panah pada seleksi aktif melompat ke tepinya.
    pub fn move_left(&mut self, extend: bool) {
        let (a, b) = self.selection();
        let p = if !extend && a != b {
            a
        } else {
            self.prev(self.cursor)
        };
        self.set_cursor(p, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        let (a, b) = self.selection();
        let p = if !extend && a != b {
            b
        } else {
            self.next(self.cursor)
        };
        self.set_cursor(p, extend);
    }

    pub fn word_left(&mut self, extend: bool) {
        let p = self.word_left_of(self.cursor);
        self.set_cursor(p, extend);
    }

    pub fn word_right(&mut self, extend: bool) {
        let p = self.word_right_of(self.cursor);
        self.set_cursor(p, extend);
    }

    pub fn select_all(&mut self) {
        self.set_selection(0, self.text.len());
    }

    /// Klik ganda: pilih deretan karakter sekelas di sekitar `pos`.
    pub fn select_word(&mut self, pos: usize) {
        let pos = self.snap(pos);
        let Some(c) = self.text[pos..].chars().next() else {
            return self.set_cursor(pos, false);
        };
        let cl = class(c);
        let a = self.text[..pos]
            .char_indices()
            .rev()
            .take_while(|&(_, d)| class(d) == cl)
            .last()
            .map_or(pos, |(k, _)| k);
        let b = self.text[pos..]
            .char_indices()
            .find(|&(_, d)| class(d) != cl)
            .map_or(self.text.len(), |(k, _)| pos + k);
        self.set_selection(a, b);
    }

    /// Klik tiga kali: pilih satu baris logis beserta newline-nya.
    pub fn select_line(&mut self, pos: usize) {
        let a = self.line_start(pos);
        let b = (self.line_end(pos) + 1).min(self.text.len());
        self.set_selection(a, b);
    }

    pub fn line_start(&self, pos: usize) -> usize {
        self.text[..pos].rfind('\n').map_or(0, |k| k + 1)
    }

    pub fn line_end(&self, pos: usize) -> usize {
        self.text[pos..]
            .find('\n')
            .map_or(self.text.len(), |k| pos + k)
    }

    fn prev(&self, i: usize) -> usize {
        self.text[..i]
            .char_indices()
            .next_back()
            .map_or(0, |(j, _)| j)
    }

    fn next(&self, i: usize) -> usize {
        self.text[i..]
            .chars()
            .next()
            .map_or(i, |c| i + c.len_utf8())
    }

    /// Lewati spasi ke kiri, lalu satu deretan karakter sekelas.
    fn word_left_of(&self, i: usize) -> usize {
        let mut j = i;
        let mut cls = None;
        for (k, c) in self.text[..i].char_indices().rev() {
            let cl = class(c);
            match cls {
                None if cl == 0 => {}
                None => cls = Some(cl),
                Some(x) if x != cl => break,
                _ => {}
            }
            j = k;
        }
        j
    }

    /// Lewati deretan karakter sekelas, lalu spasi: berhenti di awal kata berikutnya.
    fn word_right_of(&self, i: usize) -> usize {
        let s = &self.text[i..];
        let Some(first) = s.chars().next().map(class) else {
            return i;
        };
        let mut in_ws = first == 0;
        for (k, c) in s.char_indices() {
            let cl = class(c);
            if !in_ws {
                if cl == first {
                    continue;
                }
                in_ws = true;
            }
            if cl != 0 {
                return i + k;
            }
        }
        self.text.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_ops_stay_on_char_boundaries() {
        let mut d = Doc::new("héllo");
        d.backspace();
        assert_eq!(d.text, "héll");
        d.move_left(false);
        d.move_left(false);
        d.move_left(false);
        d.backspace(); // menghapus 'h', bukan setengah byte 'é'
        assert_eq!(d.text, "éll");
        d.delete();
        assert_eq!(d.text, "ll");
        d.insert("ö");
        assert_eq!((d.text.as_str(), d.cursor), ("öll", 2));

        d.move_right(true);
        assert_eq!(d.selected_text(), "l");
        d.insert("X"); // seleksi diganti
        assert_eq!(d.text, "öXl");

        d.select_all();
        d.move_left(false); // panah pada seleksi → lompat ke tepi kiri
        assert_eq!((d.cursor, d.anchor), (0, 0));
        d.backspace(); // di awal teks: tidak terjadi apa-apa
        assert_eq!(d.text, "öXl");
        d.select_all();
        d.backspace();
        assert_eq!((d.text.as_str(), d.cursor), ("", 0));
        d.delete(); // di akhir teks kosong: tidak panik
    }

    #[test]
    fn undo_groups_typing_and_restores_selection() {
        let mut d = Doc::new("");
        for c in ["h", "a", "i"] {
            d.insert(c);
        }
        d.insert(" "); // spasi memutus grup
        d.insert("x");
        d.backspace();
        d.backspace();
        assert_eq!(d.text, "hai");
        assert!(d.undo()); // dua backspace = satu langkah
        assert_eq!(d.text, "hai x");
        assert!(d.undo());
        assert!(d.undo());
        assert!(d.undo());
        assert_eq!((d.text.as_str(), d.cursor), ("", 0));
        assert!(!d.undo());
        assert!(d.redo());
        assert_eq!(d.text, "hai");
        d.set_cursor(0, false);
        d.insert("Z"); // ketikan baru membuang redo
        assert!(!d.redo());

        d.select_all();
        d.insert("baru");
        d.undo();
        assert_eq!((d.text.as_str(), d.selection()), ("Zhai", (0, 4)));
    }

    #[test]
    fn word_and_line_navigation() {
        let mut d = Doc::new("satu dua,  tiga\nempat");
        d.set_cursor(0, false);
        d.word_right(false);
        assert_eq!(d.cursor, 5); // awal "dua"
        d.word_right(false);
        assert_eq!(d.cursor, 8); // ","
        d.word_left(false);
        assert_eq!(d.cursor, 5);
        d.set_cursor(15, false);
        d.delete_word_back();
        assert_eq!(d.text, "satu dua,  \nempat");
        d.select_word(1);
        assert_eq!(d.selected_text(), "satu");
        d.select_line(3);
        assert_eq!(d.selected_text(), "satu dua,  \n");
        assert_eq!((d.line_start(14), d.line_end(2)), (12, 11));
    }
}
