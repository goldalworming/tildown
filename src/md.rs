//! Markdown, dua jalur:
//! - `highlight` — per baris, untuk editor. Penanda (`#`, `**`, `[`) tetap
//!   terlihat tapi diredupkan; offset byte persis sama dengan teks sumber.
//! - `parse` — pulldown-cmark → blok bergaya, untuk mode baca.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Body,
    Heading(u8),
    Quote,
    Item,
    Fence,
    Code,
    Rule,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tok {
    Text,
    Mark,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Link,
    Url,
    Strike,
}

/// Hasil highlight satu baris. `toks` = (akhir byte, token), berurutan.
#[derive(Debug)]
pub struct Line {
    pub kind: Kind,
    pub toks: Vec<(usize, Tok)>,
    /// Apakah baris berikutnya masih di dalam blok ``` .
    pub fence_after: bool,
}

fn push(out: &mut Vec<(usize, Tok)>, end: usize, t: Tok) {
    if end <= out.last().map_or(0, |l| l.0) {
        return;
    }
    match out.last_mut() {
        Some(l) if l.1 == t => l.0 = end,
        _ => out.push((end, t)),
    }
}

/// Panjang penanda list (`- `, `* `, `+ `, `12. `, `3) `) di awal `t`.
fn list_marker(t: &str) -> Option<usize> {
    let b = t.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        return Some(2);
    }
    let n = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&n) && b.len() > n + 1 && matches!(b[n], b'.' | b')') && b[n + 1] == b' ' {
        return Some(n + 2);
    }
    None
}

fn task_len(rest: &str) -> usize {
    if rest.starts_with("[ ] ") || rest.starts_with("[x] ") || rest.starts_with("[X] ") {
        4
    } else {
        0
    }
}

fn is_rule(t: &str) -> bool {
    let t = t.trim_end();
    let Some(c) = t.chars().next().filter(|c| matches!(c, '-' | '*' | '_')) else {
        return false;
    };
    t.chars().all(|d| d == c || d == ' ') && t.chars().filter(|&d| d == c).count() >= 3
}

pub fn highlight(line: &str, in_fence: bool) -> Line {
    let t = line.trim_start();
    let ind = line.len() - t.len();
    let mut toks = Vec::new();
    if t.starts_with("```") || t.starts_with("~~~") {
        push(&mut toks, line.len(), Tok::Mark);
        return Line {
            kind: Kind::Fence,
            toks,
            fence_after: !in_fence,
        };
    }
    if in_fence {
        push(&mut toks, line.len(), Tok::Code);
        return Line {
            kind: Kind::Code,
            toks,
            fence_after: true,
        };
    }
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    let kind = if ind < 4
        && (1..=6).contains(&hashes)
        && matches!(t.as_bytes().get(hashes), None | Some(b' '))
    {
        push(&mut toks, (ind + hashes + 1).min(line.len()), Tok::Mark);
        Kind::Heading(hashes as u8)
    } else if is_rule(t) {
        push(&mut toks, line.len(), Tok::Mark);
        return Line {
            kind: Kind::Rule,
            toks,
            fence_after: false,
        };
    } else if let Some(rest) = t.strip_prefix('>') {
        push(
            &mut toks,
            ind + 1 + rest.starts_with(' ') as usize,
            Tok::Mark,
        );
        Kind::Quote
    } else if let Some(m) = list_marker(t) {
        push(&mut toks, ind + m + task_len(&t[m..]), Tok::Mark);
        Kind::Item
    } else {
        Kind::Body
    };
    let from = toks.last().map_or(0, |l| l.0);
    inline(line, from, &mut toks);
    Line {
        kind,
        toks,
        fence_after: false,
    }
}

/// Penanda inline: `kode`, **tebal**, *miring*, ~~coret~~, [link](url), \escape.
/// Penanda pembuka hanya dihitung bila penutupnya ada di baris yang sama.
fn inline(s: &str, mut i: usize, out: &mut Vec<(usize, Tok)>) {
    let b = s.as_bytes();
    let (mut bold, mut ital, mut strike) = (false, false, false);
    let cur = |bold: bool, ital: bool, strike: bool| match (strike, bold, ital) {
        (true, ..) => Tok::Strike,
        (_, true, true) => Tok::BoldItalic,
        (_, true, _) => Tok::Bold,
        (_, _, true) => Tok::Italic,
        _ => Tok::Text,
    };
    while i < b.len() {
        let c = b[i];
        match c {
            b'\\' if b.get(i + 1).is_some_and(u8::is_ascii_punctuation) => {
                push(out, i, cur(bold, ital, strike));
                push(out, i + 1, Tok::Mark);
                i += 2;
                push(out, i, cur(bold, ital, strike));
            }
            b'`' => {
                let n = b[i..].iter().take_while(|&&x| x == b'`').count();
                let fence = &s[i..i + n];
                match s[i + n..].find(fence) {
                    Some(k) => {
                        let j = i + n + k;
                        push(out, i, cur(bold, ital, strike));
                        push(out, i + n, Tok::Mark);
                        push(out, j, Tok::Code);
                        push(out, j + n, Tok::Mark);
                        i = j + n;
                    }
                    None => i += n,
                }
            }
            b'*' | b'_' => {
                let n = b[i..].iter().take_while(|&&x| x == c).count();
                let len = n.min(3);
                if c == b'_' && i > 0 && b[i - 1].is_ascii_alphanumeric() {
                    i += n; // garis bawah di tengah kata (nama_berkas) bukan penanda
                    continue;
                }
                let (want_b, want_i) = (len >= 2, len != 2);
                let after_ok = b.get(i + n).is_some_and(|x| !x.is_ascii_whitespace());
                let before_ok = i > 0 && !b[i - 1].is_ascii_whitespace();
                let closing = before_ok && ((want_b && bold) || (want_i && ital));
                let delim = &s[i..i + len];
                if closing {
                    push(out, i, cur(bold, ital, strike));
                    push(out, i + len, Tok::Mark);
                    bold &= !want_b;
                    ital &= !want_i;
                } else if after_ok && s[i + len..].contains(delim) {
                    push(out, i, cur(bold, ital, strike));
                    push(out, i + len, Tok::Mark);
                    bold |= want_b;
                    ital |= want_i;
                }
                i += len;
            }
            b'~' if b.get(i + 1) == Some(&b'~') => {
                if strike || s[i + 2..].contains("~~") {
                    push(out, i, cur(bold, ital, strike));
                    push(out, i + 2, Tok::Mark);
                    strike = !strike;
                }
                i += 2;
            }
            b'[' => {
                let st = if i > 0 && b[i - 1] == b'!' { i - 1 } else { i };
                let link = s[i..]
                    .find("](")
                    .and_then(|k| s[i + k + 2..].find(')').map(|u| (i + k, i + k + 2 + u)));
                match link {
                    Some((j, e)) => {
                        push(out, st, cur(bold, ital, strike));
                        push(out, i + 1, Tok::Mark);
                        push(out, j, Tok::Link);
                        push(out, j + 2, Tok::Mark);
                        push(out, e, Tok::Url);
                        push(out, e + 1, Tok::Mark);
                        i = e + 1;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    push(out, b.len(), cur(bold, ital, strike));
}

/// Enter di item list: (panjang prefix baris ini, prefix baris berikutnya).
/// Bila baris hanya berisi prefix, pemanggil menghapusnya (keluar dari list).
pub fn continue_list(line: &str) -> Option<(usize, String)> {
    let t = line.trim_start();
    let ind = &line[..line.len() - t.len()];
    let m = list_marker(t)?;
    let marker = &t[..m];
    let mut next = if marker.as_bytes()[0].is_ascii_digit() {
        let n: u64 = marker[..m - 2].parse().ok()?;
        format!("{}{} ", n + 1, &marker[m - 2..m - 1])
    } else {
        marker.to_owned()
    };
    let task = task_len(&t[m..]);
    if task > 0 {
        next.push_str("[ ] ");
    }
    Some((ind.len() + m + task, format!("{ind}{next}")))
}

// ---------------------------------------------------------------------------
// Mode baca

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Inl {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    /// Indeks ke `Parsed::links`.
    pub link: Option<u16>,
}

#[derive(Default, Debug, Clone)]
pub struct Rich {
    pub text: String,
    pub spans: Vec<(usize, Inl)>,
}

impl Rich {
    fn push(&mut self, s: &str, st: Inl) {
        self.text.push_str(s);
        let end = self.text.len();
        match self.spans.last_mut() {
            Some(l) if l.1 == st => l.0 = end,
            _ => self.spans.push((end, st)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BKind {
    Para,
    Heading(u8),
    Code,
    /// Blok ```mermaid; jatuh ke tampilan kode bila bukan flowchart.
    Mermaid,
    /// Blok ```plantuml; jatuh ke tampilan kode bila bukan sequence diagram.
    Plantuml,
    /// `task` = (tercentang, rentang `[ ]` di sumber) untuk diklik.
    Item {
        marker: String,
        task: Option<(bool, Range<usize>)>,
    },
    Rule,
    /// `rich.text` = alt.
    Image {
        src: String,
    },
    Table {
        rows: Vec<Vec<Rich>>,
    },
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BKind,
    pub rich: Rich,
    pub indent: u8,
    pub quote: u8,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    pub links: Vec<String>,
}

fn block(kind: BKind, indent: usize, quote: u8) -> Option<Block> {
    Some(Block {
        kind,
        rich: Rich::default(),
        indent: indent.min(255) as u8,
        quote,
    })
}

fn flush(cur: &mut Option<Block>, out: &mut Vec<Block>) {
    let Some(mut b) = cur.take() else { return };
    if matches!(b.kind, BKind::Code | BKind::Mermaid | BKind::Plantuml) {
        let t = b.rich.text.trim_end_matches('\n').len();
        b.rich.text.truncate(t);
        for s in &mut b.rich.spans {
            s.0 = s.0.min(t);
        }
        b.rich.spans.dedup_by_key(|s| s.0);
    }
    if !b.rich.text.is_empty() || matches!(b.kind, BKind::Item { .. }) {
        out.push(b);
    }
}

/// Ikon setelah tautan web: klik membuka browser.
const EXT: &str = "\u{a0}↗";

/// URL `http(s)://` polos di `s`: diawali awal teks, spasi, atau `(`; tanda
/// baca penutup tidak ikut.
fn url_spans(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (a, _) in s.match_indices("http") {
        let rest = &s[a..];
        let before = s[..a].chars().next_back();
        if !(rest.starts_with("https://") || rest.starts_with("http://"))
            || before.is_some_and(|c| !c.is_whitespace() && c != '(')
            || out.last().is_some_and(|&(_, b)| a < b)
        {
            continue;
        }
        let n = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let n = rest[..n].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"']).len();
        if !rest[..n].ends_with("//") {
            out.push((a, a + n));
        }
    }
    out
}

/// Teks biasa: tag `#nama` jadi tautan `tag:nama` (klik = cari tag itu), URL
/// polos jadi tautan + ikon ↗.
fn push_text(r: &mut Rich, links: &mut Vec<String>, s: &str, st: Inl) {
    if st.code || st.link.is_some() {
        return r.push(s, st);
    }
    let mut spans: Vec<(usize, usize, bool)> = crate::store::tag_spans(s).into_iter().map(|(a, b)| (a, b, false)).collect();
    let urls = url_spans(s);
    spans.retain(|t| !urls.iter().any(|u| t.0 < u.1 && u.0 < t.1)); // `#` di dalam URL
    spans.extend(urls.into_iter().map(|(a, b)| (a, b, true)));
    spans.sort();
    let mut last = 0;
    for (a, b, url) in spans {
        r.push(&s[last..a], st);
        links.push(if url { s[a..b].to_owned() } else { format!("tag:{}", s[a..b].to_lowercase()) });
        let link = Some((links.len() - 1).min(u16::MAX as usize - 1) as u16);
        r.push(&s[a..b], Inl { link, ..st });
        if url {
            r.push(EXT, Inl { link, ..st });
        }
        last = b;
    }
    r.push(&s[last..], st);
}

pub fn parse(src: &str) -> Parsed {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut p = Parsed::default();
    let mut cur: Option<Block> = None;
    let mut st = Inl::default();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote = 0u8;
    let mut table: Option<Vec<Vec<Rich>>> = None;
    let mut cell: Option<Rich> = None;
    let mut image: Option<(String, String)> = None;
    let mut link_stack = Vec::new();

    for (ev, range) in Parser::new_ext(src, opts).into_offset_iter() {
        let text = match ev {
            Event::Start(tag) => {
                match tag {
                    Tag::Paragraph => {
                        // Item list "longgar": paragraf pertamanya adalah teks item itu sendiri.
                        let fresh_item = matches!(&cur, Some(b) if matches!(b.kind, BKind::Item { .. }) && b.rich.text.is_empty());
                        if !fresh_item {
                            flush(&mut cur, &mut p.blocks);
                            cur = block(BKind::Para, lists.len(), quote);
                        }
                    }
                    Tag::Heading { level, .. } => {
                        flush(&mut cur, &mut p.blocks);
                        cur = block(BKind::Heading(level as u8), 0, quote);
                    }
                    Tag::BlockQuote(_) => {
                        flush(&mut cur, &mut p.blocks);
                        quote += 1;
                    }
                    Tag::CodeBlock(k) => {
                        flush(&mut cur, &mut p.blocks);
                        let lang = if let CodeBlockKind::Fenced(l) = &k {
                            l.trim()
                        } else {
                            ""
                        };
                        let kind = match lang {
                            "mermaid" => BKind::Mermaid,
                            "plantuml" | "puml" | "uml" => BKind::Plantuml,
                            _ => BKind::Code,
                        };
                        cur = block(kind, lists.len(), quote);
                    }
                    Tag::List(start) => {
                        flush(&mut cur, &mut p.blocks);
                        lists.push(start);
                    }
                    Tag::Item => {
                        flush(&mut cur, &mut p.blocks);
                        let marker = match lists.last_mut() {
                            Some(Some(n)) => {
                                *n += 1;
                                format!("{}.", *n - 1)
                            }
                            _ => "•".to_owned(),
                        };
                        cur = block(
                            BKind::Item { marker, task: None },
                            lists.len().saturating_sub(1),
                            quote,
                        );
                    }
                    Tag::Table(_) => {
                        flush(&mut cur, &mut p.blocks);
                        table = Some(Vec::new());
                    }
                    Tag::TableHead | Tag::TableRow => {
                        if let Some(t) = &mut table {
                            t.push(Vec::new());
                        }
                    }
                    Tag::TableCell => cell = Some(Rich::default()),
                    Tag::Emphasis => st.italic = true,
                    Tag::Strong => st.bold = true,
                    Tag::Strikethrough => st.strike = true,
                    Tag::Link { dest_url, .. } => {
                        p.links.push(dest_url.to_string());
                        link_stack.push(st.link);
                        st.link = Some((p.links.len() - 1).min(u16::MAX as usize - 1) as u16);
                    }
                    Tag::Image { dest_url, .. } => {
                        image = Some((dest_url.to_string(), String::new()))
                    }
                    _ => {}
                }
                continue;
            }
            Event::End(tag) => {
                match tag {
                    TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::Item => {
                        flush(&mut cur, &mut p.blocks)
                    }
                    TagEnd::BlockQuote(_) => {
                        flush(&mut cur, &mut p.blocks);
                        quote = quote.saturating_sub(1);
                    }
                    TagEnd::List(_) => {
                        flush(&mut cur, &mut p.blocks);
                        lists.pop();
                    }
                    TagEnd::TableCell => {
                        if let (Some(c), Some(t)) = (cell.take(), &mut table) {
                            if let Some(r) = t.last_mut() {
                                r.push(c);
                            }
                        }
                    }
                    TagEnd::Table => {
                        if let Some(rows) = table.take() {
                            p.blocks.push(Block {
                                kind: BKind::Table { rows },
                                rich: Rich::default(),
                                indent: lists.len() as u8,
                                quote,
                            });
                        }
                    }
                    TagEnd::Emphasis => st.italic = false,
                    TagEnd::Strong => st.bold = false,
                    TagEnd::Strikethrough => st.strike = false,
                    TagEnd::Link => {
                        let web = st.link.and_then(|l| p.links.get(l as usize)).is_some_and(|u| u.starts_with("http"));
                        if web && image.is_none() {
                            match (&mut cell, &mut cur) {
                                (Some(c), _) => c.push(EXT, st),
                                (None, Some(b)) => b.rich.push(EXT, st),
                                _ => {}
                            }
                        }
                        st.link = link_stack.pop().flatten();
                    }
                    TagEnd::Image => {
                        if let Some((src, alt)) = image.take() {
                            if let Some(c) = &mut cell {
                                c.push(&alt, st); // gambar di dalam tabel: cukup alt-nya
                            } else {
                                let (indent, q) = cur
                                    .as_ref()
                                    .map_or((lists.len() as u8, quote), |b| (b.indent, b.quote));
                                let resume = cur.as_ref().map(|b| b.kind.clone());
                                flush(&mut cur, &mut p.blocks);
                                p.blocks.push(Block {
                                    kind: BKind::Image { src },
                                    rich: Rich {
                                        text: alt,
                                        spans: Vec::new(),
                                    },
                                    indent,
                                    quote: q,
                                });
                                // Teks setelah gambar di paragraf yang sama jadi paragraf baru.
                                if matches!(resume, Some(BKind::Para) | Some(BKind::Item { .. })) {
                                    cur = block(BKind::Para, indent as usize, q);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }
            Event::Rule => {
                flush(&mut cur, &mut p.blocks);
                p.blocks.push(Block {
                    kind: BKind::Rule,
                    rich: Rich::default(),
                    indent: 0,
                    quote,
                });
                continue;
            }
            Event::TaskListMarker(done) => {
                if let Some(Block {
                    kind: BKind::Item { task, .. },
                    ..
                }) = &mut cur
                {
                    *task = Some((done, range));
                }
                continue;
            }
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => (t.into_string(), st),
            Event::Code(t) => (t.into_string(), Inl { code: true, ..st }),
            // Enter = baris baru (seperti di editor), bukan spasi ala CommonMark.
            Event::SoftBreak => ("
".to_owned(), st),
            Event::HardBreak => ("\n".to_owned(), st),
            _ => continue,
        };
        let (s, inl) = text;
        if let Some((_, alt)) = &mut image {
            alt.push_str(&s);
        } else if let Some(c) = &mut cell {
            push_text(c, &mut p.links, &s, inl);
        } else {
            let b = cur.get_or_insert_with(|| block(BKind::Para, lists.len(), quote).unwrap());
            push_text(&mut b.rich, &mut p.links, &s, inl);
        }
    }
    flush(&mut cur, &mut p.blocks);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans<'a>(line: &'a str, l: &Line) -> Vec<(&'a str, Tok)> {
        let mut a = 0;
        l.toks
            .iter()
            .map(|&(e, t)| (&line[std::mem::replace(&mut a, e)..e], t))
            .collect()
    }

    #[test]
    fn highlight_marks_syntax_without_losing_bytes() {
        let s = "## Judul **tebal**";
        let l = highlight(s, false);
        assert_eq!(l.kind, Kind::Heading(2));
        assert_eq!(
            spans(s, &l),
            [
                ("## ", Tok::Mark),
                ("Judul ", Tok::Text),
                ("**", Tok::Mark),
                ("tebal", Tok::Bold),
                ("**", Tok::Mark)
            ]
        );

        let s = "- [x] beli `kopi` di [toko](https://x.id) nama_berkas *a*";
        let l = highlight(s, false);
        assert_eq!(l.kind, Kind::Item);
        let sp = spans(s, &l);
        assert_eq!(sp[0], ("- [x] ", Tok::Mark));
        assert!(sp.contains(&("kopi", Tok::Code)));
        assert!(sp.contains(&("toko", Tok::Link)));
        assert!(sp.contains(&("https://x.id", Tok::Url)));
        assert!(sp.contains(&("a", Tok::Italic)));
        assert!(sp.contains(&(" nama_berkas ", Tok::Text)));

        // Bintang tanpa penutup dan "2 * 3" bukan penanda.
        let s = "2 * 3 dan *tanpa tutup";
        assert_eq!(spans(s, &highlight(s, false)), [(s, Tok::Text)]);

        let l = highlight("```rust", false);
        assert!(l.kind == Kind::Fence && l.fence_after);
        let l = highlight("let a = *b;", true);
        assert!(l.kind == Kind::Code && l.fence_after);
        assert!(!highlight("```", true).fence_after);
        assert_eq!(highlight("---", false).kind, Kind::Rule);
        assert_eq!(highlight("#tag", false).kind, Kind::Body);
        assert!(highlight("", false).toks.is_empty());
    }

    #[test]
    fn list_continuation() {
        assert_eq!(continue_list("  - item"), Some((4, "  - ".into())));
        assert_eq!(continue_list("9. sembilan"), Some((3, "10. ".into())));
        assert_eq!(continue_list("- [x] selesai"), Some((6, "- [ ] ".into())));
        assert_eq!(continue_list("biasa"), None);
    }

    #[test]
    fn parse_blocks() {
        let src = "# Judul\n\nParagraf **tebal** dan [link](https://a.b).\n\n- satu\n- [ ] tugas\n  1. dalam\n\n> kutipan\n\n```\nkode\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n![alt](gambar.png)\n\n---\n";
        let p = parse(src);
        let kinds: Vec<String> = p
            .blocks
            .iter()
            .map(|b| {
                format!("{:?}", b.kind)
                    .split([' ', '{', '('])
                    .next()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            kinds,
            ["Heading", "Para", "Item", "Item", "Item", "Para", "Code", "Table", "Image", "Rule"]
        );
        assert_eq!(p.links, ["https://a.b"]);
        let t = parse("#Kajian hari ini `#bukan`");
        assert_eq!(t.links, ["tag:#kajian"]);
        let u = parse("Lihat https://a.b/c#x, [situs](https://d.e) dan (http://f.g).");
        assert_eq!(u.links, ["https://a.b/c#x", "https://d.e", "http://f.g"]);
        assert_eq!(u.blocks[0].rich.text, "Lihat https://a.b/c#x\u{a0}↗, situs\u{a0}↗ dan (http://f.g\u{a0}↗).");
        assert_eq!(t.blocks[0].rich.text, "#Kajian hari ini #bukan");
        let para = &p.blocks[1].rich;
        assert!(para
            .spans
            .iter()
            .any(|&(e, s)| s.bold && para.text[..e].ends_with("tebal")));
        match &p.blocks[3].kind {
            BKind::Item {
                task: Some((false, r)),
                ..
            } => assert_eq!(&src[r.clone()], "[ ]"),
            k => panic!("{k:?}"),
        }
        assert!(matches!(&p.blocks[4].kind, BKind::Item { marker, .. } if marker == "1."));
        assert_eq!(p.blocks[4].indent, 1);
        assert_eq!(p.blocks[5].quote, 1);
        assert_eq!(p.blocks[6].rich.text, "kode");
        assert!(
            matches!(&p.blocks[7].kind, BKind::Table { rows } if rows.len() == 2 && rows[1][1].text == "2")
        );
        assert!(matches!(&p.blocks[8].kind, BKind::Image { src } if src == "gambar.png"));
    }
}
