//! Timing diagram (`robust`, `concise`, `binary`, `clock`, `analog`) — mengikuti
//! `timingdiagram` PlantUML: pita per peserta, sinyal hijau tua, sumbu waktu bawah.
// ponytail: tanpa `highlight`, batasan waktu `<->`, analog digambar seperti robust.

use super::*;

const GREEN: Col = Col::Rgb(0x006400);
const GRAY: Col = Col::Rgb(0x333333);

#[derive(Clone, Copy, PartialEq)]
enum PKind {
    Robust,
    Concise,
    Binary,
    Clock,
}

struct Player {
    id: String,
    title: usize,
    kind: PKind,
    /// Urutan state (robust: pertama di bawah).
    states: Vec<String>,
    state_txt: Vec<usize>,
    /// (waktu, indeks state)
    changes: Vec<(f32, usize)>,
    period: f32,
    pulse: f32,
    offset: f32,
}

struct Msg {
    a: usize,
    ta: f32,
    b: usize,
    tb: f32,
    t: usize,
}

pub struct Timing {
    texts: Texts,
    players: Vec<Player>,
    msgs: Vec<Msg>,
    times: Vec<f32>,
    time_txt: Vec<usize>,
    scale: f32,
    hide_axis: bool,
}

fn num(s: &str) -> Option<f32> {
    s.trim().trim_start_matches('+').parse().ok()
}

pub fn parse_lines(lines: &[String]) -> Option<Timing> {
    let mut t = Timing {
        texts: Texts::default(),
        players: Vec::new(),
        msgs: Vec::new(),
        times: Vec::new(),
        time_txt: Vec::new(),
        scale: 1.0,
        hide_axis: false,
    };
    let mut now = 0.0f32;
    let mut cur_player: Option<usize> = None; // blok `@ID`
    for raw in lines {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        let w0 = low.split_whitespace().next().unwrap_or("");
        match w0 {
            "robust" | "concise" | "binary" | "clock" | "analog" => {
                let kind = match w0 {
                    "concise" => PKind::Concise,
                    "binary" => PKind::Binary,
                    "clock" => PKind::Clock,
                    _ => PKind::Robust,
                };
                let rest = l[w0.len()..].trim();
                let (decl, opts) = match rest.to_lowercase().find(" with ") {
                    Some(k) => (&rest[..k], rest[k + 6..].to_lowercase()),
                    None => (rest, String::new()),
                };
                let (label, id) = match decl.split_once(" as ") {
                    Some((a, b)) => (unescape(a), b.trim().to_string()),
                    None => (unescape(decl), decl.trim().trim_matches('"').to_string()),
                };
                let mut it = opts.split_whitespace();
                let (mut period, mut pulse, mut offset) = (0.0, 0.0, 0.0);
                while let Some(k) = it.next() {
                    let v = it.next().and_then(num).unwrap_or(0.0);
                    match k {
                        "period" => period = v,
                        "pulse" => pulse = v,
                        "offset" => offset = v,
                        _ => {}
                    }
                }
                let title = t.texts.add(Txt::new(&label, 14.0).bold().color(GRAY));
                t.players.push(Player {
                    id,
                    title,
                    kind,
                    states: Vec::new(),
                    state_txt: Vec::new(),
                    changes: Vec::new(),
                    period,
                    pulse,
                    offset,
                });
                continue;
            }
            "scale" => {
                // scale 100 as 50 pixels
                let v: Vec<f32> = low.split_whitespace().filter_map(num).collect();
                if v.len() >= 2 && v[0] > 0.0 {
                    t.scale = v[1] / v[0];
                }
                continue;
            }
            "hide" => {
                t.hide_axis |= low.contains("time-axis") || low.contains("time axis");
                continue;
            }
            "highlight" | "mode" | "manual" | "compact" => continue,
            _ => {}
        }
        if let Some(r) = l.strip_prefix('@') {
            // @100, @+50, @ID
            let r = r.trim();
            if let Some(v) = num(r) {
                now = if r.starts_with('+') { now + v } else { v };
                cur_player = None;
            } else if let Some(k) = t.players.iter().position(|p| p.id == r) {
                cur_player = Some(k);
            }
            continue;
        }
        if let Some(k) = cur_player {
            // Di dalam blok @ID: `100 is state`
            if let Some((tm, st)) = l.split_once(" is ") {
                let tm = num(tm.trim_start_matches('@'))?;
                set_state(&mut t, k, tm, st);
                continue;
            }
        }
        // Pesan: A -> B : x   |  A@10 -> B@20 : x
        if let Some(k) = l.find("->") {
            let (lhs, rhs) = (&l[..k], &l[k + 2..]);
            let (rhs, label) = rhs.split_once(':').map_or((rhs, ""), |(a, b)| (a, b));
            let end = |s: &str| -> Option<(usize, f32)> {
                let (id, tm) = s
                    .trim()
                    .split_once('@')
                    .map_or((s.trim(), None), |(a, b)| (a, num(b)));
                let p = t.players.iter().position(|p| p.id == id.trim())?;
                Some((p, tm.unwrap_or(now)))
            };
            let ((a, ta), (b, tb)) = (end(lhs)?, end(rhs)?);
            let tx = t
                .texts
                .add(Txt::new(&unescape(label.trim()), 14.0).color(Col::Rgb(0x00008B)));
            t.msgs.push(Msg {
                a,
                ta,
                b,
                tb,
                t: tx,
            });
            continue;
        }
        // `ID is state`, `ID@100 is state`, `ID has a,b,c`
        if let Some((who, st)) = l.split_once(" is ") {
            let (id, tm) = who.split_once('@').map_or((who.trim(), now), |(a, b)| {
                (a.trim(), num(b).unwrap_or(now))
            });
            let k = t.players.iter().position(|p| p.id == id)?;
            set_state(&mut t, k, tm, st);
            continue;
        }
        if let Some((who, st)) = l.split_once(" has ") {
            let k = t.players.iter().position(|p| p.id == who.trim())?;
            for s in st.split(',') {
                state_index(&mut t, k, s.trim());
            }
            continue;
        }
        return None;
    }
    if t.players.is_empty() {
        return None;
    }
    let mut times: Vec<f32> = t
        .players
        .iter()
        .flat_map(|p| p.changes.iter().map(|c| c.0))
        .collect();
    if let Some(p) = t
        .players
        .iter()
        .find(|p| p.kind == PKind::Clock && p.period > 0.0)
    {
        times.push(p.period);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times.dedup();
    if times.is_empty() {
        times.push(0.0);
    }
    for &tm in &times {
        let s = if tm.fract() == 0.0 {
            format!("{}", tm as i64)
        } else {
            format!("{tm}")
        };
        let tx = t.texts.add(Txt::new(&s, 11.0).color(GRAY));
        t.time_txt.push(tx);
    }
    t.times = times;
    Some(t)
}

fn state_index(t: &mut Timing, k: usize, s: &str) -> usize {
    let s = s.trim().trim_matches('"').to_string();
    if let Some(i) = t.players[k].states.iter().position(|x| *x == s) {
        return i;
    }
    let kind = t.players[k].kind;
    let txt = if kind == PKind::Concise {
        Txt::new(&s, 12.0).bold().color(GRAY)
    } else {
        Txt::new(&s, 12.0).color(GRAY)
    };
    let tx = t.texts.add(txt);
    let p = &mut t.players[k];
    p.states.push(s);
    p.state_txt.push(tx);
    p.states.len() - 1
}

fn set_state(t: &mut Timing, k: usize, tm: f32, st: &str) {
    let i = state_index(t, k, st);
    t.players[k].changes.push((tm, i));
}

impl Diagram for Timing {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let mut p = Vec::new();
        // Tinggi teks versi PlantUML (SansSerif ≈ 1,1654 em per baris).
        let plh = |i: usize| {
            self.texts.0[i].size * 1.1654 * self.texts.0[i].s.split('\n').count() as f32
        };
        let label_w = self
            .players
            .iter()
            .filter(|pl| pl.kind == PKind::Robust)
            .flat_map(|pl| pl.state_txt.iter().map(|&s| size[s].0))
            .fold(0.0f32, f32::max);
        let left = 20.0;
        let x0 = left + 5.0 + label_w.max(0.0);
        let tmin = self.times.first().copied().unwrap_or(0.0).min(0.0);
        let tmax = self.times.last().copied().unwrap_or(0.0);
        let extra = self
            .players
            .iter()
            .find(|p| p.kind == PKind::Clock)
            .map_or(50.0, |p| p.period.max(50.0));
        let tend = tmax + extra;
        let tx = |tm: f32| x0 + (tm - tmin) * self.scale;
        let right = tx(tend) + 5.0;
        let mut y = 20.0;
        let mut mids: Vec<Vec<f32>> = Vec::new(); // y tiap state per peserta
        let mut bands = Vec::new();
        for pl in &self.players {
            let top = y;
            let (tw, th) = (size[pl.title].0, plh(pl.title));
            p.push(line(vec![(left, top), (right, top)], GRAY, 0.5));
            p.push(Prim::Text(pl.title, left + 5.0, top + 2.0));
            let tab_y = top + th + 2.3;
            p.push(line(
                vec![
                    (left, tab_y),
                    (left + tw + 10.0, tab_y),
                    (left + tw + 20.0, top),
                ],
                GRAY,
                0.5,
            ));
            let base = tab_y + 10.0;
            let mut ys = Vec::new();
            let h = match pl.kind {
                PKind::Robust => {
                    let n = pl.states.len().max(1);
                    for i in 0..n {
                        let sy = base + (n - 1 - i) as f32 * 20.0;
                        ys.push(sy);
                        let sh = plh(pl.state_txt[i]);
                        p.push(Prim::Text(pl.state_txt[i], left + 5.0, sy - sh / 2.0 + 1.0));
                    }
                    let segs = segments(&pl.changes, tend);
                    let mut prev: Option<f32> = None;
                    for (a, b, s) in segs {
                        let yy = ys[s];
                        if let Some(py) = prev {
                            p.push(line(vec![(tx(a), py), (tx(a), yy)], GREEN, 2.0));
                        }
                        p.push(line(vec![(tx(a), yy), (tx(b), yy)], GREEN, 2.0));
                        prev = Some(yy);
                    }
                    // PanelsRobust.getFullHeight: 10 + langkah·(n−1) + 12 + 6.
                    (n - 1) as f32 * 20.0 + 28.0 + th + 2.3
                }
                PKind::Concise => {
                    let cy = base + 7.0;
                    ys.push(cy);
                    for (a, b, s) in segments(&pl.changes, tend) {
                        let (x1, x2) = (tx(a), tx(b));
                        let k = 12.0f32.min((x2 - x1) / 2.0);
                        let open_end = (b - tend).abs() < 1e-3;
                        if open_end {
                            // Segmen terakhir terbuka: sisi kanan tanpa garis.
                            let pts = vec![
                                (x2, cy - 12.0),
                                (x1 + k, cy - 12.0),
                                (x1, cy),
                                (x1 + k, cy + 12.0),
                                (x2, cy + 12.0),
                            ];
                            p.push(poly(pts.clone(), Col::Part, Col::None, 1.5));
                            p.push(line(pts, GREEN, 1.5));
                        } else {
                            let pts = vec![
                                (x1 + k, cy - 12.0),
                                (x2 - k, cy - 12.0),
                                (x2, cy),
                                (x2 - k, cy + 12.0),
                                (x1 + k, cy + 12.0),
                                (x1, cy),
                            ];
                            p.push(poly(pts, Col::Part, GREEN, 1.5));
                        }
                        let (sw, sh) = (size[pl.state_txt[s]].0, plh(pl.state_txt[s]));
                        let cxm = if open_end {
                            x1 + k
                        } else {
                            (x1 + x2 - sw) / 2.0
                        };
                        p.push(Prim::Text(pl.state_txt[s], cxm, cy - sh / 2.0 + 1.0));
                    }
                    th + 2.3 + 39.0
                }
                PKind::Binary => {
                    let (hi_y, lo_y) = (base - 2.0, base + 12.0);
                    ys.push(lo_y);
                    ys.push(hi_y);
                    let mut prev: Option<f32> = None;
                    for (a, b, s) in segments(&pl.changes, tend) {
                        let high = pl.states.get(s).is_some_and(|x| {
                            matches!(x.to_lowercase().as_str(), "high" | "1" | "true" | "on")
                        });
                        let yy = if high { hi_y } else { lo_y };
                        if let Some(py) = prev {
                            p.push(line(vec![(tx(a), py), (tx(a), yy)], GREEN, 2.0));
                        }
                        p.push(line(vec![(tx(a), yy), (tx(b), yy)], GREEN, 2.0));
                        prev = Some(yy);
                    }
                    th + 2.3 + 30.0
                }
                PKind::Clock => {
                    let (hi_y, lo_y) = (base - 2.0, base + 12.0);
                    ys.push(lo_y);
                    let per = pl.period.max(1.0);
                    let pulse = if pl.pulse > 0.0 { pl.pulse } else { per / 2.0 };
                    let mut c = tmin + pl.offset;
                    let mut pts = vec![(tx(tmin), lo_y)];
                    while c < tend {
                        let up = c;
                        let dn = (c + pulse).min(tend);
                        pts.extend_from_slice(&[
                            (tx(up), lo_y),
                            (tx(up), hi_y),
                            (tx(dn), hi_y),
                            (tx(dn), lo_y),
                        ]);
                        c += per;
                    }
                    pts.push((tx(tend), lo_y));
                    if (c - tend).abs() < 1e-3 {
                        pts.push((tx(tend), hi_y));
                    }
                    p.push(line(pts, GREEN, 1.5));
                    th + 2.3 + 30.0
                }
            };
            mids.push(ys);
            bands.push((top, top + h));
            y = top + h;
        }
        let bottom = y;
        // Kisi waktu.
        p.insert(0, line(vec![(left, 20.0), (left, bottom)], GRAY, 0.5));
        p.insert(0, line(vec![(right, 20.0), (right, bottom)], GRAY, 0.5));
        // Kisi: tiap periode clock (bila ada) dan tiap waktu kejadian.
        let mut grid: Vec<f32> = self.times.clone();
        if let Some(per) = self
            .players
            .iter()
            .find(|p| p.kind == PKind::Clock)
            .map(|p| p.period)
            .filter(|&v| v > 0.0)
        {
            let mut c = tmin;
            while c <= tend + 1e-3 {
                grid.push(c);
                c += per;
            }
        }
        grid.sort_by(|a, b| a.partial_cmp(b).unwrap());
        grid.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        for &tm in &grid {
            p.insert(
                0,
                dashed(
                    vec![(tx(tm), 20.0), (tx(tm), bottom)],
                    GRAY,
                    0.5,
                    (3.0, 5.0),
                ),
            );
        }
        if !self.hide_axis {
            p.push(line(
                vec![(tx(tmin), bottom), (tx(tend), bottom)],
                GRAY,
                2.0,
            ));
            for &tm in &grid {
                p.push(line(
                    vec![(tx(tm), bottom), (tx(tm), bottom + 5.0)],
                    GRAY,
                    2.0,
                ));
            }
            for (i, &tm) in self.times.iter().enumerate() {
                p.push(line(
                    vec![(tx(tm), bottom), (tx(tm), bottom + 5.0)],
                    GRAY,
                    2.0,
                ));
                let (w, _) = size[self.time_txt[i]];
                p.push(Prim::Text(self.time_txt[i], tx(tm) - w / 2.0, bottom + 7.5));
            }
        }
        // Pesan antarpeserta.
        for m in &self.msgs {
            let ya = signal_y(&self.players[m.a], &mids[m.a], m.ta, bands[m.a]);
            let yb = signal_y(&self.players[m.b], &mids[m.b], m.tb, bands[m.b]);
            let (xa, xb) = (tx(m.ta), tx(m.tb));
            let c = Col::Rgb(0x00008B);
            p.push(line(vec![(xa, ya), (xb, yb)], c, 1.5));
            let dir = (xb - xa, yb - ya);
            let l = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-6);
            let (ux, uy) = (dir.0 / l, dir.1 / l);
            // TimeArrow: ujung panah di lingkaran r 8, ±20°; label di tengah alasnya.
            let (ca, sa) = (8.0 * 20f32.to_radians().cos(), 8.0 * 20f32.to_radians().sin());
            let (p1, p2) = (
                (xb - ux * ca - uy * sa, yb - uy * ca + ux * sa),
                (xb - ux * ca + uy * sa, yb - uy * ca - ux * sa),
            );
            p.push(poly(vec![p1, p2, (xb, yb)], c, c, 1.5));
            let (lx, mut ly) = ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0);
            if ya < yb {
                ly -= plh(m.t);
            }
            p.push(Prim::Text(m.t, lx, ly));
        }
        // Margin kanvas PlantUML: kiri/atas 20, kanan ≈ 20,65, bawah sumbu ≈ 34,56.
        Placed {
            prims: p,
            w: right + 20.65,
            h: bottom + if self.hide_axis { 20.0 } else { 34.56 },
        }
    }
}

/// Segmen (mulai, akhir, state) dari daftar perubahan.
fn segments(changes: &[(f32, usize)], tend: f32) -> Vec<(f32, f32, usize)> {
    let mut c = changes.to_vec();
    c.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut out = Vec::new();
    for i in 0..c.len() {
        let end = c.get(i + 1).map_or(tend, |n| n.0);
        out.push((c[i].0, end, c[i].1));
    }
    out
}

fn signal_y(p: &Player, ys: &[f32], tm: f32, band: (f32, f32)) -> f32 {
    match p.kind {
        PKind::Robust => {
            let mut c = p.changes.clone();
            c.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            // State yang berlaku pada `tm` (saat berubah: state baru).
            let s = c
                .iter()
                .filter(|x| x.0 <= tm)
                .last()
                .or(c.first())
                .map_or(0, |x| x.1);
            ys.get(s).copied().unwrap_or((band.0 + band.1) / 2.0)
        }
        _ => ys.first().copied().unwrap_or((band.0 + band.1) / 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_parse() {
        let l: Vec<String> = "robust \"Web\" as W\nconcise \"U\" as U\n@0\nW is Idle\nU is A\n@100\nW is Busy\nW -> U : x".lines().map(String::from).collect();
        let t = parse_lines(&l).unwrap();
        assert_eq!(t.players[0].states, vec!["Idle", "Busy"]);
        assert_eq!(t.msgs.len(), 1);
        assert_eq!(t.times, vec![0.0, 100.0]);
        let pl = t.place(&vec![(30.0, 14.0); t.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
