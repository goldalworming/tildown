//! Gantt (`@startgantt`) — mengikuti `project` PlantUML: tabel Start/End/Duration
//! di kiri, kolom hari 16 px, baris 17,84, batang 13,84, hari libur diarsir.
// ponytail: skala harian saja (printscale weekly/monthly ditata harian), tanpa
// resource `on {..}`, `pause`, `link`, `today` diabaikan selain warna.

use super::*;

const DAY: f32 = 16.0;
const ROW: f32 = 17.836;

#[derive(Clone, Debug)]
struct Task {
    name: String,
    alias: Option<String>,
    label: usize,
    dur: Option<i64>,
    start: Option<Ref>,
    end: Option<Ref>,
    milestone: bool,
    done: Option<f32>,
    color: Option<(Col, Option<Col>)>,
    /// Kalender (hari libur mingguan, jumlah `closed_days`) saat tugas ini pertama
    /// dirujuk tugas lain: PlantUML meng-cache start/end-nya saat itu (TaskImpl).
    cal: Option<Cal>,
}

type Cal = ([bool; 7], usize);

#[derive(Clone, Debug)]
enum Ref {
    Day(i64),
    /// (tugas, akhir?, geser hari)
    Task(String, bool, i64),
}

enum Row {
    Task(usize),
    Sep(usize),
}

pub struct Gantt {
    texts: Texts,
    tasks: Vec<Task>,
    rows: Vec<Row>,
    /// Hari ke-0 = tanggal mulai proyek (jumlah hari sejak 1970-01-01), None = tanpa kalender.
    start: Option<i64>,
    closed_wd: [bool; 7],
    closed_days: Vec<(i64, i64)>,
    /// Indeks teks kalender (dibuat saat place butuh): disiapkan di parse.
    cal: Vec<usize>,
    links: Vec<(usize, usize)>,
}

// --- tanggal (algoritma civil Howard Hinnant) ---

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 0 = Senin.
fn weekday(z: i64) -> usize {
    ((z + 3).rem_euclid(7)) as usize
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WD: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let p: Vec<&str> = s.split(['-', '/']).collect();
    if p.len() == 3 && p[0].len() == 4 {
        return Some(days_from_civil(
            p[0].parse().ok()?,
            p[1].parse().ok()?,
            p[2].parse().ok()?,
        ));
    }
    // "the 20th of september 2020"
    let low = s.to_lowercase();
    let w: Vec<&str> = low
        .split_whitespace()
        .filter(|w| *w != "the" && *w != "of")
        .collect();
    if w.len() == 3 {
        let d: i64 = w[0]
            .trim_end_matches(|c: char| c.is_alphabetic())
            .parse()
            .ok()?;
        let m = MONTHS
            .iter()
            .position(|m| m.to_lowercase().starts_with(&w[1][..3.min(w[1].len())]))?
            as i64
            + 1;
        let y: i64 = w[2].parse().ok()?;
        return Some(days_from_civil(y, m, d));
    }
    None
}

fn bracket(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let r = s.strip_prefix('[')?;
    let k = r.find(']')?;
    Some((&r[..k], &r[k + 1..]))
}

pub fn parse_lines(lines: &[String]) -> Option<Gantt> {
    let mut g = Gantt {
        texts: Texts::default(),
        tasks: Vec::new(),
        rows: Vec::new(),
        start: None,
        closed_wd: [false; 7],
        closed_days: Vec::new(),
        cal: Vec::new(),
        links: Vec::new(),
    };
    let mut last: Option<usize> = None;
    for raw in lines {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        if low.starts_with("project starts") || low.starts_with("project start") {
            let d = low
                .trim_start_matches("project starts")
                .trim_start_matches("project start")
                .trim();
            g.start = parse_date(d.trim_start_matches("on").trim());
            continue;
        }
        if l.starts_with("--") && l.ends_with("--") {
            let t = g
                .texts
                .add(Txt::new(&unescape(l.trim_matches('-').trim()), 11.0));
            g.rows.push(Row::Sep(t));
            continue;
        }
        if low.ends_with("are closed") || low.ends_with("is closed") {
            let head = low
                .trim_end_matches("are closed")
                .trim_end_matches("is closed")
                .trim();
            let names = [
                "monday",
                "tuesday",
                "wednesday",
                "thursday",
                "friday",
                "saturday",
                "sunday",
            ];
            if let Some(k) = names.iter().position(|n| head.starts_with(n)) {
                g.closed_wd[k] = true;
            } else if let Some((a, b)) = head.split_once(" to ") {
                if let (Some(a), Some(b)) = (parse_date(a), parse_date(b)) {
                    g.closed_days.push((a, b));
                }
            } else if let Some(d) = parse_date(head) {
                g.closed_days.push((d, d));
            }
            continue;
        }
        if low.starts_with("printscale")
            || low.starts_with("projectscale")
            || low.starts_with("language")
            || low.starts_with("today")
            || low.starts_with("hide")
            || low.starts_with("<style")
            || low.starts_with("skinparam")
        {
            continue;
        }
        // Pernyataan tugas: `[T] ...` atau `then [T] ...` atau `[T] as [A] ...`.
        let (body, chained) = match low.strip_prefix("then ") {
            Some(_) => (&l[5..], true),
            None => (l, false),
        };
        let Some((name, rest)) = bracket(body) else {
            return None;
        };
        let mut rest = rest.trim().to_string();
        let mut alias = None;
        if rest.to_lowercase().starts_with("as ") {
            if let Some((a, r2)) = bracket(&rest[3..]) {
                alias = Some(a.to_string());
                rest = r2.trim().to_string();
            }
        }
        let k = match g
            .tasks
            .iter()
            .position(|t| t.name == name || t.alias.as_deref() == Some(name))
        {
            Some(k) => k,
            None => {
                let label = g.texts.add(Txt::new(&unescape(name), 11.0));
                g.tasks.push(Task {
                    name: name.to_string(),
                    alias: None,
                    label,
                    dur: None,
                    start: None,
                    end: None,
                    milestone: false,
                    done: None,
                    color: None,
                    cal: None,
                });
                g.rows.push(Row::Task(g.tasks.len() - 1));
                g.tasks.len() - 1
            }
        };
        if alias.is_some() {
            g.tasks[k].alias = alias;
        }
        if chained {
            if let Some(p) = last {
                let pn = g.tasks[p].name.clone();
                g.tasks[p].cal.get_or_insert((g.closed_wd, g.closed_days.len()));
                g.tasks[k].start.get_or_insert(Ref::Task(pn, true, 0));
                g.links.push((p, k));
            }
        }
        last = Some(k);
        for part in rest.split(" and ") {
            let p = part.trim();
            let pl = p.to_lowercase();
            if let Some(r) = pl.strip_prefix("requires").or(pl.strip_prefix("lasts")) {
                let mut it = r.split_whitespace();
                let n: i64 = it.next()?.parse().ok()?;
                let unit = it.next().unwrap_or("days");
                g.tasks[k].dur = Some(if unit.starts_with("week") { n * 7 } else { n });
                g.tasks[k].cal = None;
            } else if pl.starts_with("starts")
                || pl.starts_with("ends")
                || pl.starts_with("happens")
            {
                let is_end = pl.starts_with("ends");
                let is_ms = pl.starts_with("happens");
                let r = p.splitn(2, char::is_whitespace).nth(1).unwrap_or("").trim();
                let rl = r.to_lowercase();
                let rf = if let Some(pos) = r.find('[') {
                    // `at [U]'s end`, `5 days after [U]'s end`
                    let (u, after) = bracket(&r[pos..])?;
                    let after = after.to_lowercase();
                    let at_end = after.contains("end");
                    let mut shift: i64 = rl
                        .split_whitespace()
                        .next()
                        .and_then(|x| x.parse().ok())
                        .unwrap_or(0);
                    if rl.contains("before") {
                        shift = -shift;
                    }
                    if let Some(j) = g
                        .tasks
                        .iter()
                        .position(|t| t.name == u || t.alias.as_deref() == Some(u))
                    {
                        if !is_ms {
                            g.links.push((j, k));
                        }
                        g.tasks[j].cal.get_or_insert((g.closed_wd, g.closed_days.len()));
                    }
                    Ref::Task(u.to_string(), at_end, shift)
                } else {
                    let d = rl
                        .trim_start_matches("at ")
                        .trim_start_matches("on ")
                        .trim();
                    let day = match (parse_date(d), g.start) {
                        (Some(x), Some(s)) => x - s,
                        _ => d.trim_start_matches('d').parse::<i64>().ok()?,
                    };
                    Ref::Day(day)
                };
                g.tasks[k].cal = None;
                if is_ms {
                    // `happens at [X]'s end`: hari yang sama dengan akhir X.
                    let rf = match rf {
                        Ref::Task(u, true, sh) => Ref::Task(u, true, sh - 1),
                        r => r,
                    };
                    g.tasks[k].milestone = true;
                    g.tasks[k].start = Some(rf);
                    g.tasks[k].dur = Some(0);
                } else if is_end {
                    g.tasks[k].end = Some(rf);
                } else {
                    g.tasks[k].start = Some(rf);
                }
            } else if pl.starts_with("is ") && pl.contains("complete") {
                let n: f32 = pl[3..]
                    .trim()
                    .trim_end_matches(|c: char| c.is_alphabetic())
                    .trim()
                    .trim_end_matches('%')
                    .trim()
                    .parse()
                    .ok()?;
                g.tasks[k].done = Some(n / 100.0);
            } else if pl.starts_with("is colored in") {
                let c = p[13..].trim();
                let (a, b) = c.split_once('/').map_or((c, None), |(a, b)| (a, Some(b)));
                g.tasks[k].color = Some((Col::parse(a)?, b.and_then(Col::parse)));
            } else if pl.starts_with("is deleted")
                || pl.starts_with("is displayed")
                || pl.starts_with("on {")
                || pl.is_empty()
            {
            } else {
                return None;
            }
        }
    }
    if g.tasks.is_empty() {
        return None;
    }
    Some(g)
}

impl Gantt {
    fn closed(&self, d: i64) -> bool {
        self.closed_in((self.closed_wd, self.closed_days.len()), d)
    }

    /// Libur menurut kalender `cal` (bisa kalender lama, lihat `Task::cal`).
    fn closed_in(&self, cal: Cal, d: i64) -> bool {
        let Some(s) = self.start else { return false };
        let z = s + d;
        cal.0[weekday(z)] || self.closed_days[..cal.1].iter().any(|&(a, b)| z >= a && z <= b)
    }

    /// Tambah `n` hari kerja mulai hari `d` (inklusif) → hari terakhir.
    fn add_work(&self, cal: Cal, d: i64, n: i64) -> i64 {
        if n <= 0 {
            return d - 1;
        }
        let mut k = d;
        let mut left = n;
        loop {
            if !self.closed_in(cal, k) {
                left -= 1;
                if left == 0 {
                    return k;
                }
            }
            k += 1;
            if k > d + 10_000 {
                return k;
            }
        }
    }

    /// (mulai, akhir inklusif) tiap tugas.
    fn schedule(&self) -> Vec<(i64, i64)> {
        let n = self.tasks.len();
        let mut se: Vec<Option<(i64, i64)>> = vec![None; n];
        let find = |nm: &str| {
            self.tasks
                .iter()
                .position(|t| t.name == nm || t.alias.as_deref() == Some(nm))
        };
        for _ in 0..n + 2 {
            for k in 0..n {
                if se[k].is_some() {
                    continue;
                }
                let t = &self.tasks[k];
                let resolve = |r: &Ref, se: &Vec<Option<(i64, i64)>>| -> Option<i64> {
                    match r {
                        Ref::Day(d) => Some(*d),
                        Ref::Task(u, end, sh) => {
                            let (s, e) = se[find(u)?]?;
                            Some(if *end { e + 1 } else { s } + sh)
                        }
                    }
                };
                let dur = t.dur.unwrap_or(1);
                let cal = t.cal.unwrap_or((self.closed_wd, self.closed_days.len()));
                let start = match (&t.start, &t.end) {
                    (Some(s), _) => resolve(s, &se),
                    (None, Some(e)) => resolve(e, &se).map(|e| {
                        let mut d = e - 1;
                        let mut left = dur;
                        while left > 1 {
                            d -= 1;
                            if !self.closed_in(cal, d) {
                                left -= 1;
                            }
                        }
                        d
                    }),
                    (None, None) => Some(0),
                };
                let Some(mut s) = start else { continue };
                while !t.milestone && self.closed_in(cal, s) {
                    s += 1;
                }
                let e = if t.milestone {
                    s
                } else if let (Some(_), Some(er)) = (&t.start, &t.end) {
                    match resolve(er, &se) {
                        Some(x) => x - 1,
                        None => continue,
                    }
                } else {
                    self.add_work(cal, s, dur)
                };
                se[k] = Some((s, e));
            }
        }
        se.into_iter().map(|x| x.unwrap_or((0, 0))).collect()
    }
}

impl Diagram for Gantt {
    fn texts(&self) -> &[Txt] {
        &self.texts.0
    }

    fn place(&self, size: &[(f32, f32)]) -> Placed {
        let sched = self.schedule();
        let first = sched.iter().map(|s| s.0).min().unwrap_or(0).min(0);
        let last = sched.iter().map(|s| s.1).max().unwrap_or(0).max(first);
        let ndays = (last - first + 1).max(1);
        // Teks kalender ada di akhir daftar teks (lihat `with_calendar`).
        let cal = &self.cal;
        let mut p = Vec::new();
        let table = self.start.is_some();
        // Tabel kiri: Start / End / Duration.
        let col_w = [40.024f32, 40.024, 47.798];
        let tw: f32 = if table { col_w.iter().sum() } else { 0.0 };
        let head = 39.0;
        let row_h = |r: &Row| if matches!(r, Row::Sep(_)) { ROW + 16.0 } else { ROW };
        let body_h: f32 = self.rows.iter().map(row_h).sum();
        let x0 = tw;
        let grid = Col::Rgb(0xC0C0C0);
        let dx = |d: i64| x0 + (d - first) as f32 * DAY;
        // Hari libur.
        for d in first..=last {
            if self.closed(d) {
                p.push(rect(
                    [dx(d), head, DAY, body_h],
                    0.0,
                    Col::Rgb(0xF1E5E5),
                    Col::None,
                    0.0,
                ));
            }
        }
        // Garis kisi.
        for d in first..=last + 1 {
            p.push(line(vec![(dx(d), head), (dx(d), head + body_h)], grid, 1.0));
        }
        p.push(line(vec![(x0, head), (dx(last + 1), head)], grid, 1.0));
        p.push(line(
            vec![(x0, head + body_h), (dx(last + 1), head + body_h)],
            grid,
            1.0,
        ));
        // Kepala kalender (atas & bawah).
        let mut ci = 0;
        let mut next = || {
            ci += 1;
            cal[ci - 1]
        };
        for (top, mirror) in [(0.0, false), (head + body_h, true)] {
            if let Some(s) = self.start {
                // Bulan.
                let mut d = first;
                while d <= last {
                    let (y, m, _) = civil_from_days(s + d);
                    let mut e = d;
                    while e < last && civil_from_days(s + e + 1).1 == m {
                        e += 1;
                    }
                    let t = next();
                    let (mw, _) = size[t];
                    let cx = (dx(d) + dx(e + 1)) / 2.0;
                    let ty = if mirror { top + 27.0 } else { top };
                    p.push(Prim::Text(t, cx - mw / 2.0, ty));
                    let _ = y;
                    d = e + 1;
                }
                for d in first..=last {
                    let t = next();
                    let (w, _) = size[t];
                    let ty = if mirror { top } else { top + 14.0 };
                    p.push(Prim::Text(t, dx(d) + (DAY - w) / 2.0, ty));
                }
            }
            for d in first..=last {
                let t = next();
                let (w, _) = size[t];
                let ty = if mirror { top + 14.0 } else { top + 26.0 };
                p.push(Prim::Text(t, dx(d) + (DAY - w) / 2.0, ty));
            }
        }
        // Tabel.
        if table {
            for x in [0.0, col_w[0], col_w[0] + col_w[1], tw] {
                p.push(line(vec![(x, 0.0), (x, head + body_h)], grid, 1.0));
            }
            p.push(line(vec![(0.0, 0.0), (tw, 0.0)], grid, 1.0));
            p.push(line(vec![(0.0, head), (tw, head)], grid, 1.0));
            p.push(line(
                vec![(0.0, head + body_h), (tw, head + body_h)],
                grid,
                1.0,
            ));
            for c in 0..3 {
                let t = next();
                p.push(Prim::Text(
                    t,
                    [0.0, col_w[0], col_w[0] + col_w[1]][c] + 5.0,
                    13.2,
                ));
            }
        }
        // Baris.
        let mut y = head + 2.0;
        let mut ends: Vec<(f32, f32, f32)> = vec![(0.0, 0.0, 0.0); self.tasks.len()]; // (x awal, x akhir, y tengah)
        for row in &self.rows {
            match *row {
                Row::Sep(t) => {
                    let my = y - 2.0 + 16.918;
                    let (w, h) = size[t];
                    p.push(line(vec![(x0, my), (x0 + 5.0, my)], Col::Ink, 1.0));
                    p.push(line(
                        vec![(x0 + 15.0 + w, my), (dx(last + 1) - 1.0, my)],
                        Col::Ink,
                        1.0,
                    ));
                    p.push(Prim::Text(t, x0 + 10.0, my - h / 2.0));
                }
                Row::Task(k) => {
                    let task = &self.tasks[k];
                    let (s, e) = sched[k];
                    let my = y + 13.836 / 2.0;
                    let (fill, line_c) = task
                        .color
                        .map_or((Col::Part, Col::Ink), |(a, b)| (a, b.unwrap_or(Col::Ink)));
                    if task.milestone {
                        let cx = dx(s) + DAY / 2.0;
                        let my = y + 5.0;
                        p.push(poly(
                            vec![
                                (cx, my - 5.0),
                                (cx + 5.0, my),
                                (cx, my + 5.0),
                                (cx - 5.0, my),
                            ],
                            Col::Text,
                            Col::Text,
                            1.0,
                        ));
                        p.push(Prim::Text(
                            task.label,
                            cx + 8.0,
                            my - size[task.label].1 / 2.0,
                        ));
                        ends[k] = (cx, cx, my);
                    } else {
                        // Segmen kerja (dipecah oleh hari libur), garis putus di sela.
                        let mut segs: Vec<(i64, i64)> = Vec::new();
                        let mut d = s;
                        while d <= e {
                            // Hari mulai selalu digambar penuh (PlantUML), walau libur.
                            if self.closed(d) && d != s {
                                d += 1;
                                continue;
                            }
                            let a = d;
                            while d + 1 <= e && !self.closed(d + 1) {
                                d += 1;
                            }
                            segs.push((a, d));
                            d += 1;
                        }
                        let bx0 = dx(s) + 2.0;
                        let bx1 = dx(e + 1) - 2.0;
                        let done_x = task.done.map(|f| bx0 + (bx1 - bx0) * f);
                        for (i, &(a, b)) in segs.iter().enumerate() {
                            let (sx, ex) = (
                                if i == 0 { dx(a) + 2.0 } else { dx(a) },
                                if i + 1 == segs.len() {
                                    dx(b + 1) - 2.0
                                } else {
                                    dx(b + 1)
                                },
                            );
                            let r = [sx, y, ex - sx, 13.836];
                            match done_x {
                                Some(dxp) if dxp < ex => {
                                    if dxp > sx {
                                        p.push(rect(
                                            [sx, y, dxp - sx, 13.836],
                                            0.0,
                                            fill,
                                            Col::None,
                                            0.0,
                                        ));
                                    }
                                    let a0 = dxp.max(sx);
                                    p.push(rect(
                                        [a0, y, ex - a0, 13.836],
                                        0.0,
                                        Col::Paper,
                                        Col::None,
                                        0.0,
                                    ));
                                }
                                _ => p.push(rect(r, 0.0, fill, Col::None, 0.0)),
                            }
                            // Tepi: atas, bawah, kiri (segmen pertama), kanan (terakhir).
                            p.push(line(vec![(sx, y), (ex, y)], line_c, 1.0));
                            p.push(line(vec![(sx, y + 13.836), (ex, y + 13.836)], line_c, 1.0));
                            if i == 0 {
                                p.push(line(vec![(sx, y), (sx, y + 13.836)], line_c, 1.0));
                            }
                            if i + 1 == segs.len() {
                                p.push(line(vec![(ex, y), (ex, y + 13.836)], line_c, 1.0));
                            }
                            if i + 1 < segs.len() {
                                let nx = dx(segs[i + 1].0);
                                p.push(dashed(
                                    vec![(ex + 3.0, y), (nx - 3.0, y)],
                                    line_c,
                                    1.0,
                                    (2.0, 3.0),
                                ));
                                p.push(dashed(
                                    vec![(ex + 3.0, y + 13.836), (nx - 3.0, y + 13.836)],
                                    line_c,
                                    1.0,
                                    (2.0, 3.0),
                                ));
                            }
                        }
                        // Label: di dalam batang bila muat, selain itu di kanan.
                        let (lw, lh) = size[task.label];
                        let lx = if lw + 8.0 <= bx1 - bx0 {
                            bx0 + 4.0
                        } else {
                            bx1 + 4.0
                        };
                        p.push(Prim::Text(task.label, lx, my - lh / 2.0));
                        ends[k] = (bx0, bx1, my);
                    }
                    // Kolom tabel.
                    if table {
                        let st = self.start.unwrap();
                        let fmt = |d: i64| {
                            let (_, m, dd) = civil_from_days(st + d);
                            format!("{} {}", &MONTHS[(m - 1) as usize][..3], dd)
                        };
                        let _ = fmt;
                        for c in 0..3 {
                            let t = next();
                            p.push(Prim::Text(
                                t,
                                [0.0, col_w[0], col_w[0] + col_w[1]][c] + 5.0,
                                y + 0.6,
                            ));
                        }
                    }
                }
            }
            y += row_h(row);
        }
        // Panah dependensi: turun dari akhir pendahulu, lalu ke kanan ke awal tugas.
        for &(a, b) in &self.links {
            let (_, ax1, ay) = ends[a];
            let (bx0, _, by) = ends[b];
            // Dari milestone: dari pusat belah ketupat; dari batang: tepi bawah.
            let (sx, sy) = if self.tasks[a].milestone {
                (ax1, ay)
            } else {
                (ax1 - 6.0, ay + 6.918)
            };
            let ty = by;
            p.push(line(vec![(sx, sy), (sx, ty), (bx0 - 5.0, ty)], Col::Ink, 1.5));
            p.push(poly(
                vec![(bx0 - 6.0, ty - 4.0), (bx0 - 2.0, ty), (bx0 - 6.0, ty + 4.0)],
                Col::Ink,
                Col::Ink,
                1.0,
            ));
        }
        // Kanvas PlantUML: tanpa margin kiri/atas, ≈ 20 di kanan isi terjauh.
        let w = p
            .iter()
            .map(|q| q.bounds(size)[2])
            .fold(dx(last + 1), f32::max)
            + 20.0;
        let _ = ndays;
        Placed {
            prims: p,
            w,
            h: head + body_h + 38.15,
        }
    }
}

/// Siapkan teks kalender & tabel (urutan sesuai pemakaian di `place`).
pub fn with_calendar(mut g: Gantt) -> Gantt {
    let sched = g.schedule();
    let first = sched.iter().map(|s| s.0).min().unwrap_or(0).min(0);
    let last = sched.iter().map(|s| s.1).max().unwrap_or(0).max(first);
    for _mirror in 0..2 {
        if let Some(s) = g.start {
            let mut d = first;
            while d <= last {
                let (y, m, _) = civil_from_days(s + d);
                let mut e = d;
                while e < last && civil_from_days(s + e + 1).1 == m {
                    e += 1;
                }
                let t = g
                    .texts
                    .add(Txt::new(&format!("{} {}", MONTHS[(m - 1) as usize], y), 12.0).bold());
                g.cal.push(t);
                d = e + 1;
            }
            for d in first..=last {
                let closed = g.closed(d);
                let t = g
                    .texts
                    .add(Txt::new(WD[weekday(s + d)], 10.0).color(if closed {
                        Col::Rgb(0x989898)
                    } else {
                        Col::Text
                    }));
                g.cal.push(t);
            }
        }
        for d in first..=last {
            let label = match g.start {
                Some(s) => civil_from_days(s + d).2.to_string(),
                None => (d + 1).to_string(),
            };
            let closed = g.closed(d);
            let t = g.texts.add(Txt::new(&label, 10.0).color(if closed {
                Col::Rgb(0x989898)
            } else {
                Col::Text
            }));
            g.cal.push(t);
        }
    }
    if let Some(st) = g.start {
        for h in ["Start", "End", "Duration"] {
            let t = g.texts.add(Txt::new(h, 10.0));
            g.cal.push(t);
        }
        let fmt = |d: i64| {
            let (_, m, dd) = civil_from_days(st + d);
            format!("{} {}", &MONTHS[(m - 1) as usize][..3], dd)
        };
        let rows: Vec<usize> = g
            .rows
            .iter()
            .filter_map(|r| if let Row::Task(k) = r { Some(*k) } else { None })
            .collect();
        for k in rows {
            let (s, e) = sched[k];
            let days = if g.tasks[k].milestone { 1 } else { e - s + 1 };
            for txt in [
                fmt(s),
                fmt(e),
                format!("{} day{}", days, if days == 1 { "" } else { "s" }),
            ] {
                let t = g.texts.add(Txt::new(&txt, 10.0));
                g.cal.push(t);
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_with_weekends() {
        let l: Vec<String> = "Project starts 2024-01-01\n[A] requires 5 days\n[B] requires 3 days\n[B] starts at [A]'s end\nsaturday are closed\nsunday are closed\n[M] happens at [B]'s end".lines().map(String::from).collect();
        let g = with_calendar(parse_lines(&l).unwrap());
        assert_eq!(days_from_civil(2024, 1, 1) + 3, days_from_civil(2024, 1, 4));
        assert_eq!(weekday(days_from_civil(2024, 1, 1)), 0); // Senin
        let s = g.schedule();
        assert_eq!(s[0], (0, 4)); // Sen–Jum
        assert_eq!(s[1], (7, 9)); // Sen–Rab minggu berikut
        assert_eq!(s[2].0, 9); // milestone di hari akhir B (PlantUML: Jan 10)
        let pl = g.place(&vec![(20.0, 12.0); g.texts.0.len()]);
        assert!(pl.w > 0.0);
    }
}
