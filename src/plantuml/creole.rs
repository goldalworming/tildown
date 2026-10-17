//! Creole PlantUML (subset): `**tebal**`, `//miring//`, `""mono""`, `__garis__`,
//! `--coret--`, `~~gelombang~~`, tag HTML `<b> <i> <u> <s> <strike> <color:X>
//! <size:N> <back:X> <font color=X size=N>`, `~` sebagai escape, butir `* `.
// ponytail: tanpa tabel creole, ikon `<&x>`, sprite `<$x>`, `<img>`; ikon dibuang.

use super::Col;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub under: bool,
    pub strike: bool,
    pub color: Option<Col>,
    pub back: Option<Col>,
    pub size: Option<f32>,
}

/// Teks polos + daftar (akhir byte, format) berurutan.
pub fn parse(src: &str, base: Fmt) -> (String, Vec<(usize, Fmt)>) {
    let mut out = String::new();
    let mut runs: Vec<(usize, Fmt)> = Vec::new();
    let mut f = base;
    let mut stack: Vec<Fmt> = Vec::new();
    let lines: Vec<&str> = src.split('\n').collect();
    for (li, line) in lines.iter().enumerate() {
        let mut l: &str = line;
        let t = l.trim_start();
        if let Some(r) = t.strip_prefix("** ").or(t.strip_prefix("* ")) {
            out.push_str(if t.starts_with("** ") {
                "    • "
            } else {
                "• "
            });
            close_run(&out, &mut runs, f);
            l = r;
        } else if ["----", "====", "....", "____"]
            .iter()
            .any(|p| t.starts_with(p))
        {
            l = "";
        }
        let mut i = 0;
        while i < l.len() {
            let r = &l[i..];
            let two = |p: &str| r.starts_with(p);
            i += if r.starts_with('~') && r.len() > 1 {
                let c = r[1..].chars().next().unwrap();
                out.push(c);
                close_run(&out, &mut runs, f);
                1 + c.len_utf8()
            } else if two("**") {
                f.bold = !f.bold;
                2
            } else if two("//") && !l[..i].ends_with(':') {
                f.italic = !f.italic;
                2
            } else if two("\"\"") {
                f.mono = !f.mono;
                2
            } else if two("__") || two("~~") {
                f.under = !f.under;
                2
            } else if two("--") && !two("-->") && (f.strike || l[i + 2..].contains("--")) {
                f.strike = !f.strike;
                2
            } else if let Some((len, t)) = r.starts_with('<').then(|| tag(r)).flatten() {
                apply(&t, &mut f, &mut stack, base);
                len
            } else {
                let c = r.chars().next().unwrap();
                out.push(c);
                close_run(&out, &mut runs, f);
                c.len_utf8()
            };
        }
        if li + 1 < lines.len() {
            out.push('\n');
            close_run(&out, &mut runs, f);
        }
    }
    if runs.is_empty() {
        runs.push((out.len(), base));
    }
    (out, runs)
}

fn close_run(out: &str, runs: &mut Vec<(usize, Fmt)>, f: Fmt) {
    match runs.last_mut() {
        Some(r) if r.1 == f => r.0 = out.len(),
        _ => runs.push((out.len(), f)),
    }
}

/// `<tag>` atau `<tag:arg>` atau `</tag>`; None bila bukan tag dikenal.
fn tag(r: &str) -> Option<(usize, String)> {
    let end = r.find('>')?;
    let t = r[1..end].trim().to_lowercase();
    let name = t
        .trim_start_matches('/')
        .split([':', ' ', '='])
        .next()
        .unwrap_or("");
    let ok = [
        "b", "i", "u", "s", "strike", "del", "color", "size", "back", "font", "w", "plain", "sub",
        "sup", "code", "img", "&", "$", "strong", "em",
    ];
    if ok.contains(&name) || t.starts_with('&') || t.starts_with('$') || t.starts_with("img") {
        Some((end + 1, r[1..end].trim().to_string()))
    } else {
        None
    }
}

fn apply(t: &str, f: &mut Fmt, stack: &mut Vec<Fmt>, base: Fmt) {
    let low = t.to_lowercase();
    if low.starts_with('/') {
        *f = stack.pop().unwrap_or(base);
        return;
    }
    if low.starts_with('&') || low.starts_with('$') || low.starts_with("img") {
        return; // ikon/sprite/gambar: dibuang
    }
    stack.push(*f);
    let arg = |k: &str| -> Option<String> {
        // `color:red`, `color red`, `font color=red size=12`
        let i = low.find(k)?;
        let r = &t[i + k.len()..];
        let r = r.trim_start_matches([':', '=', ' ']).trim_matches('"');
        Some(
            r.split([' ', '>'])
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .to_string(),
        )
    };
    match low.split([':', ' ', '=']).next().unwrap_or("") {
        "b" | "strong" => f.bold = true,
        "i" | "em" => f.italic = true,
        "u" | "w" => f.under = true,
        "s" | "strike" | "del" => f.strike = true,
        "code" => f.mono = true,
        "plain" => {
            *f = Fmt {
                color: f.color,
                size: f.size,
                ..base
            }
        }
        "color" => f.color = arg("color").and_then(|c| Col::parse(&c)),
        "back" => f.back = arg("back").and_then(|c| Col::parse(&c)),
        "size" => f.size = arg("size").and_then(|n| n.parse().ok()),
        "font" => {
            if let Some(c) = arg("color").and_then(|c| Col::parse(&c)) {
                f.color = Some(c);
            }
            if let Some(n) = arg("size").and_then(|n| n.parse().ok()) {
                f.size = Some(n);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creole_runs() {
        let (s, r) = parse("a **b** <color:red>c</color> ~**", Fmt::default());
        assert_eq!(s, "a b c **");
        assert!(r.iter().any(|x| x.1.bold));
        assert!(r.iter().any(|x| x.1.color == Some(Col::Rgb(0xFF0000))));
        let (s, _) = parse("* satu\nx --> y", Fmt::default());
        assert_eq!(s, "• satu\nx --> y");
    }
}
