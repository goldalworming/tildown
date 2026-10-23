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
    /// Bagian dari langkah undo yang sama dengan entri sebelumnya (multi-cursor).
    joined: bool,
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
    /// Caret tambahan (anchor, cursor) untuk multi-cursor ala Sublime.
    pub extra: Vec<(usize, usize)>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    joining: bool,
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
        self.extra.clear();
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
            joined: self.joining,
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
        loop {
            self.text.replace_range(e.at..e.at + e.new.len(), &e.old);
            (self.anchor, self.cursor) = e.sel_before;
            e.sealed = true;
            let joined = e.joined;
            self.redo.push(e);
            match self.undo.pop() {
                Some(n) if joined => e = n,
                Some(n) => break self.undo.push(n),
                None => break,
            }
        }
        self.version += 1;
        self.extra.clear();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(mut e) = self.redo.pop() else {
            return false;
        };
        loop {
            self.text.replace_range(e.at..e.at + e.old.len(), &e.new);
            (self.anchor, self.cursor) = e.sel_after;
            self.undo.push(e);
            match self.redo.pop() {
                Some(n) if n.joined => e = n,
                Some(n) => break self.redo.push(n),
                None => break,
            }
        }
        self.version += 1;
        self.extra.clear();
        true
    }

    /// Semua seleksi (caret utama dan tambahan), urut dari atas.
    pub fn sels(&self) -> Vec<(usize, usize)> {
        let mut v: Vec<_> = std::iter::once((self.anchor, self.cursor))
            .chain(self.extra.iter().copied())
            .map(|(a, c)| (a.min(c), a.max(c)))
            .collect();
        v.sort();
        v
    }

    /// Jalankan `op` di setiap caret, mulai dari yang paling bawah supaya
    /// perubahan teks tidak menggeser caret yang belum diproses; caret yang
    /// sudah diproses digeser sebesar selisih panjangnya. Satu langkah undo.
    pub fn apply_all(&mut self, mut op: impl FnMut(&mut Doc)) {
        if self.extra.is_empty() {
            return op(self);
        }
        let mut cs: Vec<(usize, usize, bool)> = std::iter::once((self.anchor, self.cursor, true))
            .chain(self.extra.drain(..).map(|(a, c)| (a, c, false)))
            .collect();
        cs.sort_by_key(|&(a, c, _)| std::cmp::Reverse(a.min(c)));
        let base = self.undo.len();
        let fresh = self.undo.last().is_some_and(|e| !e.sealed);
        for k in 0..cs.len() {
            (self.anchor, self.cursor) = (cs[k].0, cs[k].1);
            self.seal();
            self.joining = self.undo.len() > base;
            let len = self.text.len();
            op(self);
            let d = self.text.len() as isize - len as isize;
            (cs[k].0, cs[k].1) = (self.anchor, self.cursor);
            for c in &mut cs[..k] {
                c.0 = c.0.saturating_add_signed(d);
                c.1 = c.1.saturating_add_signed(d);
            }
        }
        self.joining = false;
        // Ketikan beruntun di banyak caret digabung jadi satu langkah undo, seperti di satu caret.
        if base > 0 && self.undo.len() > base + 1 {
            let (p, f) = (&self.undo[base - 1], &self.undo[base]);
            if fresh && p.joined && p.kind == Kind::Type && f.kind == Kind::Type && f.time - p.time < MERGE {
                self.undo[base].joined = true;
            }
        }
        self.restore(cs);
    }

    /// Gerakkan setiap caret dengan `mv` (tanpa mengubah teks).
    pub fn move_all(&mut self, mut mv: impl FnMut(&mut Doc)) {
        let p = (self.anchor, self.cursor);
        let mut cs = Vec::new();
        for (a, c) in std::mem::take(&mut self.extra) {
            (self.anchor, self.cursor) = (a, c);
            mv(self);
            cs.push((self.anchor, self.cursor, false));
        }
        (self.anchor, self.cursor) = p;
        mv(self);
        cs.push((self.anchor, self.cursor, true));
        self.restore(cs);
    }

    /// Pasang kembali caret; caret tambahan yang menumpuk dibuang.
    fn restore(&mut self, cs: Vec<(usize, usize, bool)>) {
        let mut seen = Vec::new();
        for &(a, c, primary) in &cs {
            if primary {
                (self.anchor, self.cursor) = (a, c);
                seen.push(c);
            }
        }
        for (a, c, primary) in cs {
            if !primary && !seen.contains(&c) {
                seen.push(c);
                self.extra.push((a, c));
            }
        }
    }

    /// Alt+klik: caret baru di `pos` menjadi caret utama.
    pub fn add_caret(&mut self, pos: usize) {
        let pos = self.snap(pos);
        if let Some(i) = self.extra.iter().position(|e| e.1 == pos) {
            self.extra.remove(i); // Alt+klik pada caret yang ada = buang
            return;
        }
        if pos != self.cursor {
            self.extra.push((self.anchor, self.cursor));
        }
        self.set_cursor(pos, false);
    }

    /// Ctrl+D: tanpa seleksi pilih kata di cursor; dengan seleksi tambahkan
    /// kemunculan berikutnya (memutar ke awal) sebagai caret baru.
    pub fn select_next(&mut self) {
        let (a, b) = self.selection();
        if a == b {
            let (a, b) = self.word_range(a);
            return self.set_selection(a, b);
        }
        let tok = self.text[a..b].to_owned();
        let taken = self.sels();
        let from = taken.last().map_or(b, |s| s.1);
        let hit = self.text[from..]
            .match_indices(&tok)
            .map(|(i, _)| from + i)
            .chain(self.text[..from].match_indices(&tok).map(|(i, _)| i))
            .find(|&i| !taken.contains(&(i, i + tok.len())));
        if let Some(i) = hit {
            self.extra.push((self.anchor, self.cursor));
            self.set_selection(i, i + tok.len());
        }
    }

    /// Ctrl+Shift+L: caret di setiap kemunculan teks terpilih (atau kata di cursor).
    pub fn select_all_matches(&mut self) {
        let (a, b) = self.selection();
        let (a, b) = if a == b { self.word_range(a) } else { (a, b) };
        if a == b {
            return;
        }
        let tok = self.text[a..b].to_owned();
        self.extra = self
            .text
            .match_indices(&tok)
            .map(|(i, _)| (i, i + tok.len()))
            .filter(|&(i, _)| i != a)
            .collect();
        self.set_selection(a, b);
    }

    /// Shift+Alt+I: seleksi lintas baris dipecah jadi satu caret di akhir tiap baris.
    pub fn split_lines(&mut self) {
        let (a, b) = self.selection();
        let mut ends: Vec<usize> = self.text[a..b].match_indices('\n').map(|(i, _)| a + i).collect();
        if ends.is_empty() {
            return;
        }
        ends.push(b);
        let last = ends.pop().unwrap();
        self.extra = ends.into_iter().map(|e| (e, e)).collect();
        self.set_cursor(last, false);
    }

    /// Tab: seleksi lintas baris digeser masuk satu tab; `out` (Shift+Tab)
    /// menggeser keluar baris-baris itu, atau baris cursor bila tanpa seleksi.
    pub fn indent(&mut self, out: bool) {
        let (a, b) = self.selection();
        if !out && !self.text[a..b].contains('\n') {
            return self.insert("\t");
        }
        // Seleksi yang berakhir tepat setelah newline tidak ikut baris berikutnya.
        let b2 = if b > a && self.text[..b].ends_with('\n') { b - 1 } else { b };
        let (ls, le) = (self.line_start(a), self.line_end(b2));
        let new: Vec<String> = self.text[ls..le]
            .split('\n')
            .map(|l| {
                if !out {
                    if l.is_empty() { String::new() } else { format!("\t{l}") }
                } else if let Some(r) = l.strip_prefix('\t') {
                    r.to_owned()
                } else {
                    let sp = l.len() - l.trim_start_matches(' ').len();
                    l[sp.min(4)..].to_owned()
                }
            })
            .collect();
        let new = new.join("\n");
        let first_old = self.line_end(ls) - ls;
        let first_new = new.find('\n').unwrap_or(new.len());
        self.replace_range(ls, le, &new);
        if a == b {
            let p = (a + first_new).saturating_sub(first_old).max(ls);
            self.set_cursor(p, false);
        } else {
            self.set_selection(ls, ls + new.len());
        }
    }

    /// Ctrl+Shift+D: gandakan baris cursor ke bawahnya (cursor ikut ke salinan),
    /// atau gandakan teks terpilih tepat setelahnya.
    pub fn duplicate(&mut self) {
        let (a, b) = self.selection();
        if a == b {
            let (ls, le) = (self.line_start(a), self.line_end(a));
            let line = self.text[ls..le].to_owned();
            self.replace_range(le, le, &format!("\n{line}"));
            self.set_cursor(a + line.len() + 1, false);
        } else {
            let s = self.text[a..b].to_owned();
            self.replace_range(b, b, &s);
            self.set_selection(b, b + s.len());
        }
    }

    /// Ctrl+B / Ctrl+I: bungkus seleksi dengan penanda `m`, atau lepas bila
    /// sudah terbungkus. Tanpa seleksi cursor diletakkan di antara penanda.
    pub fn toggle_wrap(&mut self, m: &str) {
        let (a, b) = self.selection();
        let n = m.len();
        let before = &self.text[..a];
        // "*" di sebelah "**tebal**" bukan penanda miring.
        let doubled = n == 1 && before.ends_with(&m.repeat(2)) && !before.ends_with(&m.repeat(3));
        if before.ends_with(m) && self.text[b..].starts_with(m) && !doubled {
            let inner = self.text[a..b].to_owned();
            self.replace_range(a - n, b + n, &inner);
            return self.set_selection(a - n, b - n);
        }
        self.wrap(m, m);
    }

    fn wrap(&mut self, open: &str, close: &str) {
        let (a, b) = self.selection();
        let s = format!("{open}{}{close}", &self.text[a..b]);
        self.replace_range(a, b, &s);
        self.set_selection(a + open.len(), b + open.len());
    }

    /// Ketik pembuka pasangan saat ada seleksi: seleksi dibungkus, bukan diganti.
    pub fn wrap_typed(&mut self, t: &str) -> bool {
        let close = match t {
            "(" => ")",
            "[" => "]",
            "{" => "}",
            "\"" | "'" | "`" | "*" | "_" | "~" => t,
            _ => return false,
        };
        let (a, b) = self.selection();
        if a == b {
            return false;
        }
        self.wrap(t, close);
        true
    }

    /// Ctrl+K: `[seleksi](url)`. Tanpa url cursor menunggu di dalam kurung.
    pub fn link(&mut self, url: Option<&str>) {
        let (a, b) = self.selection();
        let inner = self.text[a..b].to_owned();
        let url = url.unwrap_or("");
        self.replace_range(a, b, &format!("[{inner}]({url})"));
        let p = if a == b { a + 1 } else if url.is_empty() { a + inner.len() + 3 } else { a + inner.len() + url.len() + 4 };
        self.set_cursor(p, false);
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
        let (a, b) = self.word_range(pos);
        self.set_selection(a, b);
    }

    /// Rentang deretan karakter sekelas di sekitar `pos`.
    pub fn word_range(&self, pos: usize) -> (usize, usize) {
        let pos = self.snap(pos);
        let Some(c) = self.text[pos..].chars().next() else {
            return (pos, pos);
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
        (a, b)
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

    #[test]
    fn multi_cursor_edits_and_one_undo() {
        let mut d = Doc::new("a x b x c x");
        d.set_cursor(2, false);
        d.select_next(); // kata "x"
        d.select_next();
        d.select_next();
        d.select_next(); // tidak ada lagi yang baru
        assert_eq!(d.sels(), vec![(2, 3), (6, 7), (10, 11)]);
        d.apply_all(|d| d.insert("yy"));
        assert_eq!(d.text, "a yy b yy c yy");
        d.apply_all(|d| d.backspace());
        assert_eq!(d.text, "a y b y c y");
        assert_eq!(d.extra.len(), 2);
        assert!(d.undo());
        assert_eq!(d.text, "a yy b yy c yy");
        assert!(d.undo());
        assert_eq!(d.text, "a x b x c x");
        assert!(d.redo());
        assert_eq!(d.text, "a yy b yy c yy");

        let mut d = Doc::new("satu\ndua\ntiga");
        d.select_all();
        d.split_lines();
        assert_eq!(d.sels(), vec![(4, 4), (8, 8), (13, 13)]);
        d.move_all(|d| d.move_left(false));
        d.apply_all(|d| d.insert("!"));
        assert_eq!(d.text, "sat!u\ndu!a\ntig!a");
        d.move_all(|d| d.move_right(false));
        d.move_all(|d| d.move_left(false)); // navigasi memutus grup undo
        d.apply_all(|d| d.insert("z"));
        d.apply_all(|d| d.insert("z"));
        d.undo(); // dua ketikan beruntun = satu langkah
        assert_eq!(d.text, "sat!u\ndu!a\ntig!a");
        d.select_all_matches(); // tanpa seleksi: kata di cursor ("a")
        assert_eq!(d.sels(), vec![(1, 2), (9, 10), (15, 16)]);
        d.add_caret(0);
        assert_eq!(d.sels().len(), 4);
    }

    #[test]
    fn indent_wrap_and_link() {
        let mut d = Doc::new("- a\n- b\nc");
        d.set_selection(1, 6);
        d.indent(false);
        assert_eq!(d.text, "\t- a\n\t- b\nc");
        d.indent(true);
        assert_eq!(d.text, "- a\n- b\nc");
        d.set_cursor(8, false);
        d.indent(true); // tanpa indentasi: tidak berubah
        assert_eq!(d.text, "- a\n- b\nc");
        d.set_cursor(8, false);
        d.indent(false); // tanpa seleksi: sisipkan tab
        assert_eq!(d.text, "- a\n- b\n\tc");

        let mut d = Doc::new("a\nbc\nd");
        d.set_cursor(3, false);
        d.duplicate();
        assert_eq!((d.text.as_str(), d.cursor), ("a\nbc\nbc\nd", 6));
        d.set_selection(0, 1);
        d.duplicate();
        assert_eq!((d.text.as_str(), d.selected_text()), ("aa\nbc\nbc\nd", "a"));

        let mut d = Doc::new("halo dunia");
        d.set_selection(5, 10);
        d.toggle_wrap("**");
        assert_eq!((d.text.as_str(), d.selected_text()), ("halo **dunia**", "dunia"));
        d.toggle_wrap("*"); // miring di dalam tebal
        assert_eq!(d.text, "halo ***dunia***");
        d.toggle_wrap("*");
        d.toggle_wrap("**");
        assert_eq!(d.text, "halo dunia");
        assert!(d.wrap_typed("("));
        assert_eq!((d.text.as_str(), d.selected_text()), ("halo (dunia)", "dunia"));
        d.link(Some("https://x.id"));
        assert_eq!(d.text, "halo ([dunia](https://x.id))");
        d.set_cursor(0, false);
        assert!(!d.wrap_typed("("));
    }
}
