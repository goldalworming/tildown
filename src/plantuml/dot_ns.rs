//! Network simplex Graphviz 2.44.1 (`lib/common/ns.c`) apa adanya: pohon awal lewat
//! heap subtree, pencarian sisi keluar bergilir (`searchsize`), sisi masuk lewat DFS,
//! dan `LR_balance`. Di antara solusi optimal yang setara, urutan pivot inilah yang
//! menentukan pilihan dot — jadi graf bantu harus disusun dengan urutan node/sisi
//! yang sama dengan dot (lihat pemanggil di `graph.rs`).

pub struct Ns {
    /// Urutan `GD_nlist`.
    pub nlist: Vec<usize>,
    out: Vec<Vec<usize>>,
    inn: Vec<Vec<usize>>,
    et: Vec<usize>,
    eh: Vec<usize>,
    minlen: Vec<i64>,
    weight: Vec<i64>,
    /// Rank awal (dipakai bila layak), lalu hasil.
    pub rank: Vec<i64>,
}

struct Sub {
    rep: usize,
    size: usize,
    heap_index: i64,
    par: usize,
}

struct St<'a> {
    g: &'a mut Ns,
    tree_index: Vec<i64>,
    tree_out: Vec<Vec<usize>>,
    tree_in: Vec<Vec<usize>>,
    tree_edges: Vec<usize>,
    cut: Vec<i64>,
    low: Vec<usize>,
    lim: Vec<usize>,
    par: Vec<Option<usize>>,
    subtree: Vec<Option<usize>>,
    subs: Vec<Sub>,
    s_i: usize,
    search_size: usize,
    enter: Option<usize>,
    slack_best: i64,
    lo: usize,
    li: usize,
}

impl Ns {
    pub fn new(n: usize) -> Ns {
        Ns {
            nlist: Vec::new(),
            out: vec![Vec::new(); n],
            inn: vec![Vec::new(); n],
            et: Vec::new(),
            eh: Vec::new(),
            minlen: Vec::new(),
            weight: Vec::new(),
            rank: vec![0; n],
        }
    }

    /// Tambah node (mis. slack node); kembalikan indeksnya.
    pub fn node(&mut self) -> usize {
        self.out.push(Vec::new());
        self.inn.push(Vec::new());
        self.rank.push(0);
        self.out.len() - 1
    }

    /// make_aux_edge: sisi t→h, ditambahkan di akhir daftar keluar/masuk.
    pub fn edge(&mut self, t: usize, h: usize, minlen: i64, w: i64) -> usize {
        let e = self.et.len();
        self.et.push(t);
        self.eh.push(h);
        self.minlen.push(minlen);
        self.weight.push(w);
        self.out[t].push(e);
        self.inn[h].push(e);
        e
    }

    /// Ada sisi t→h? (find_fast_edge)
    pub fn find(&self, t: usize, h: usize) -> Option<usize> {
        self.out[t].iter().copied().find(|&e| self.eh[e] == h)
    }

    pub fn raise(&mut self, e: usize, minlen: i64, w: i64) {
        self.minlen[e] = self.minlen[e].max(minlen);
        self.weight[e] = self.weight[e].max(w);
    }

    /// rank2(g, balance = 2 (LR), maxiter, search_size). None bila graf tak terhubung.
    pub fn solve(&mut self, search_size: usize, maxiter: usize) -> Option<()> {
        self.rank2(2, search_size, maxiter)
    }

    /// rank2 dengan balance 1 (TB_balance), untuk fase rank.
    pub fn solve_tb(&mut self, search_size: usize, maxiter: usize) -> Option<()> {
        self.rank2(1, search_size, maxiter)
    }

    fn rank2(&mut self, balance: u8, search_size: usize, maxiter: usize) -> Option<()> {
        let n = self.out.len();
        let ne = self.et.len();
        let mut st = St {
            g: self,
            tree_index: vec![-1; ne],
            tree_out: vec![Vec::new(); n],
            tree_in: vec![Vec::new(); n],
            tree_edges: Vec::new(),
            cut: vec![0; ne],
            low: vec![0; n],
            lim: vec![0; n],
            par: vec![None; n],
            subtree: vec![None; n],
            subs: Vec::new(),
            s_i: 0,
            search_size,
            enter: None,
            slack_best: i64::MAX,
            lo: 0,
            li: 0,
        };
        // init_graph: rank yang ada layak?
        let feasible = (0..ne).all(|e| st.length(e) >= st.g.minlen[e]);
        if !feasible {
            st.init_rank();
        }
        st.feasible_tree()?;
        let mut iter = 0;
        while let Some(e) = st.leave_edge() {
            let f = st.enter_edge(e)?;
            st.update(e, f);
            iter += 1;
            if iter >= maxiter {
                break;
            }
        }
        if balance == 2 {
            st.lr_balance();
        } else {
            st.tb_balance();
        }
        Some(())
    }
}

impl St<'_> {
    fn length(&self, e: usize) -> i64 {
        self.g.rank[self.g.eh[e]] - self.g.rank[self.g.et[e]]
    }
    fn slack(&self, e: usize) -> i64 {
        self.length(e) - self.g.minlen[e]
    }
    fn is_tree(&self, e: usize) -> bool {
        self.tree_index[e] >= 0
    }
    fn seq(a: usize, b: usize, c: usize) -> bool {
        a <= b && b <= c
    }

    fn init_rank(&mut self) {
        let n = self.g.out.len();
        let mut prio: Vec<usize> = (0..n).map(|v| self.g.inn[v].len()).collect();
        let mut q: std::collections::VecDeque<usize> = self
            .g
            .nlist
            .iter()
            .copied()
            .filter(|&v| prio[v] == 0)
            .collect();
        while let Some(v) = q.pop_front() {
            let mut r = 0;
            for &e in &self.g.inn[v] {
                r = r.max(self.g.rank[self.g.et[e]] + self.g.minlen[e]);
            }
            self.g.rank[v] = r;
            for i in 0..self.g.out[v].len() {
                let h = self.g.eh[self.g.out[v][i]];
                prio[h] -= 1;
                if prio[h] == 0 {
                    q.push_back(h);
                }
            }
        }
    }

    fn add_tree_edge(&mut self, e: usize) {
        self.tree_index[e] = self.tree_edges.len() as i64;
        self.tree_edges.push(e);
        self.tree_out[self.g.et[e]].push(e);
        self.tree_in[self.g.eh[e]].push(e);
    }

    fn tight_subtree_search(&mut self, v: usize, st: usize) -> usize {
        let mut rv = 1;
        self.subtree[v] = Some(st);
        for i in 0..self.g.inn[v].len() {
            let e = self.g.inn[v][i];
            if self.is_tree(e) {
                continue;
            }
            let t = self.g.et[e];
            if self.subtree[t].is_none() && self.slack(e) == 0 {
                self.add_tree_edge(e);
                rv += self.tight_subtree_search(t, st);
            }
        }
        for i in 0..self.g.out[v].len() {
            let e = self.g.out[v][i];
            if self.is_tree(e) {
                continue;
            }
            let h = self.g.eh[e];
            if self.subtree[h].is_none() && self.slack(e) == 0 {
                self.add_tree_edge(e);
                rv += self.tight_subtree_search(h, st);
            }
        }
        rv
    }

    fn set_find(&mut self, n: usize) -> usize {
        let mut s0 = self.subtree[n].unwrap();
        while self.subs[s0].par != s0 {
            let p = self.subs[s0].par;
            self.subs[s0].par = self.subs[p].par;
            s0 = self.subs[s0].par;
        }
        s0
    }

    fn set_union(&mut self, s0: usize, s1: usize) -> usize {
        let mut r0 = s0;
        while self.subs[r0].par != r0 {
            r0 = self.subs[r0].par;
        }
        let mut r1 = s1;
        while self.subs[r1].par != r1 {
            r1 = self.subs[r1].par;
        }
        if r0 == r1 {
            return r0;
        }
        let r = if self.subs[r1].heap_index == -1 {
            r0
        } else if self.subs[r0].heap_index == -1 || self.subs[r1].size >= self.subs[r0].size {
            r1
        } else {
            r0
        };
        self.subs[r0].par = r;
        self.subs[r1].par = r;
        self.subs[r].size = self.subs[r0].size + self.subs[r1].size;
        r
    }

    fn inter_tree_edge_search(
        &mut self,
        v: usize,
        from: Option<usize>,
        mut best: Option<usize>,
    ) -> Option<usize> {
        let ts = self.set_find(v);
        if let Some(b) = best {
            if self.slack(b) == 0 {
                return best;
            }
        }
        for i in 0..self.g.out[v].len() {
            let e = self.g.out[v][i];
            let h = self.g.eh[e];
            if self.is_tree(e) {
                if Some(h) == from {
                    continue;
                }
                best = self.inter_tree_edge_search(h, Some(v), best);
            } else if self.set_find(h) != ts && best.is_none_or(|b| self.slack(e) < self.slack(b)) {
                best = Some(e);
            }
        }
        for i in 0..self.g.inn[v].len() {
            let e = self.g.inn[v][i];
            let t = self.g.et[e];
            if self.is_tree(e) {
                if Some(t) == from {
                    continue;
                }
                best = self.inter_tree_edge_search(t, Some(v), best);
            } else if self.set_find(t) != ts && best.is_none_or(|b| self.slack(e) < self.slack(b)) {
                best = Some(e);
            }
        }
        best
    }

    // STheapify apa adanya (termasuk `else smallest = i` pada anak kanan).
    fn heapify(&mut self, heap: &mut [usize], size: usize, mut i: usize) {
        loop {
            let left = 2 * (i + 1) - 1;
            let right = 2 * (i + 1);
            let mut smallest =
                if left < size && self.subs[heap[left]].size < self.subs[heap[i]].size {
                    left
                } else {
                    i
                };
            smallest =
                if right < size && self.subs[heap[right]].size < self.subs[heap[smallest]].size {
                    right
                } else {
                    i
                };
            if smallest != i {
                heap.swap(i, smallest);
                self.subs[heap[i]].heap_index = i as i64;
                self.subs[heap[smallest]].heap_index = smallest as i64;
                i = smallest;
            } else {
                break;
            }
            if i >= size {
                break;
            }
        }
    }

    fn tree_adjust(&mut self, v: usize, from: Option<usize>, delta: i64) {
        self.g.rank[v] += delta;
        for i in 0..self.tree_in[v].len() {
            let w = self.g.et[self.tree_in[v][i]];
            if Some(w) != from {
                self.tree_adjust(w, Some(v), delta);
            }
        }
        for i in 0..self.tree_out[v].len() {
            let w = self.g.eh[self.tree_out[v][i]];
            if Some(w) != from {
                self.tree_adjust(w, Some(v), delta);
            }
        }
    }

    fn feasible_tree(&mut self) -> Option<()> {
        let nlist = self.g.nlist.clone();
        let mut heap: Vec<usize> = Vec::new();
        for &n in &nlist {
            if self.subtree[n].is_none() {
                let s = self.subs.len();
                self.subs.push(Sub {
                    rep: n,
                    size: 0,
                    heap_index: 0,
                    par: s,
                });
                self.subs[s].size = self.tight_subtree_search(n, s);
                heap.push(s);
            }
        }
        let mut size = heap.len();
        for (i, &s) in heap.iter().enumerate() {
            self.subs[s].heap_index = i as i64;
        }
        for i in (0..=size / 2).rev() {
            self.heapify(&mut heap, size, i);
        }
        while size > 1 {
            // STextractmin
            let t0 = heap[0];
            self.subs[t0].heap_index = -1;
            heap[0] = heap[size - 1];
            self.subs[heap[0]].heap_index = 0;
            heap[size - 1] = t0;
            size -= 1;
            self.heapify(&mut heap, size, 0);
            let rep = self.subs[t0].rep;
            let ee = self.inter_tree_edge_search(rep, None, None)?;
            // merge_trees
            let (a, b) = (self.set_find(self.g.et[ee]), self.set_find(self.g.eh[ee]));
            if self.subs[a].heap_index == -1 {
                let d = self.slack(ee);
                self.tree_adjust(self.subs[a].rep, None, d);
            } else {
                let d = -self.slack(ee);
                self.tree_adjust(self.subs[b].rep, None, d);
            }
            self.add_tree_edge(ee);
            let t1 = self.set_union(a, b);
            let hi = self.subs[t1].heap_index as usize;
            self.heapify(&mut heap, size, hi);
        }
        // init_cutvalues
        let root = nlist[0];
        self.dfs_range(root, None, 1);
        self.dfs_cutval(root, None);
        Some(())
    }

    fn dfs_range(&mut self, v: usize, par: Option<usize>, low: usize) -> usize {
        let mut lim = low;
        self.par[v] = par;
        self.low[v] = low;
        for i in 0..self.tree_out[v].len() {
            let e = self.tree_out[v][i];
            if Some(e) != par {
                lim = self.dfs_range(self.g.eh[e], Some(e), lim);
            }
        }
        for i in 0..self.tree_in[v].len() {
            let e = self.tree_in[v][i];
            if Some(e) != par {
                lim = self.dfs_range(self.g.et[e], Some(e), lim);
            }
        }
        self.lim[v] = lim;
        lim + 1
    }

    fn dfs_cutval(&mut self, v: usize, par: Option<usize>) {
        for i in 0..self.tree_out[v].len() {
            let e = self.tree_out[v][i];
            if Some(e) != par {
                self.dfs_cutval(self.g.eh[e], Some(e));
            }
        }
        for i in 0..self.tree_in[v].len() {
            let e = self.tree_in[v][i];
            if Some(e) != par {
                self.dfs_cutval(self.g.et[e], Some(e));
            }
        }
        if let Some(f) = par {
            self.x_cutval(f);
        }
    }

    fn x_cutval(&mut self, f: usize) {
        let (v, dir) = if self.par[self.g.et[f]] == Some(f) {
            (self.g.et[f], 1)
        } else {
            (self.g.eh[f], -1)
        };
        let mut sum = 0;
        for &e in self.g.out[v].iter().chain(self.g.inn[v].iter()) {
            sum += self.x_val(e, v, dir);
        }
        self.cut[f] = sum;
    }

    fn x_val(&self, e: usize, v: usize, dir: i32) -> i64 {
        let other = if self.g.et[e] == v {
            self.g.eh[e]
        } else {
            self.g.et[e]
        };
        let (f, mut rv) = if !Self::seq(self.low[v], self.lim[other], self.lim[v]) {
            (true, self.g.weight[e])
        } else {
            let c = if self.is_tree(e) { self.cut[e] } else { 0 };
            (false, c - self.g.weight[e])
        };
        let mut d = if dir > 0 {
            if self.g.eh[e] == v {
                1
            } else {
                -1
            }
        } else if self.g.et[e] == v {
            1
        } else {
            -1
        };
        if f {
            d = -d;
        }
        if d < 0 {
            rv = -rv;
        }
        rv
    }

    fn leave_edge(&mut self) -> Option<usize> {
        let mut rv: Option<usize> = None;
        let mut cnt = 0;
        let j = self.s_i;
        let mut scan = |st: &mut St, end: usize| -> bool {
            while st.s_i < end {
                let f = st.tree_edges[st.s_i];
                if st.cut[f] < 0 {
                    if rv.is_none_or(|r| st.cut[r] > st.cut[f]) {
                        rv = Some(f);
                    }
                    cnt += 1;
                    if cnt >= st.search_size {
                        return true;
                    }
                }
                st.s_i += 1;
            }
            false
        };
        let len = self.tree_edges.len();
        if scan(self, len) {
            return rv;
        }
        if j > 0 {
            self.s_i = 0;
            if scan(self, j) {
                return rv;
            }
        }
        rv
    }

    fn dfs_enter_outedge(&mut self, v: usize) {
        for i in 0..self.g.out[v].len() {
            let e = self.g.out[v][i];
            let h = self.g.eh[e];
            if !self.is_tree(e) {
                if !Self::seq(self.lo, self.lim[h], self.li) {
                    let s = self.slack(e);
                    if s < self.slack_best || self.enter.is_none() {
                        self.enter = Some(e);
                        self.slack_best = s;
                    }
                }
            } else if self.lim[h] < self.lim[v] {
                self.dfs_enter_outedge(h);
            }
        }
        let mut i = 0;
        while i < self.tree_in[v].len() && self.slack_best > 0 {
            let t = self.g.et[self.tree_in[v][i]];
            if self.lim[t] < self.lim[v] {
                self.dfs_enter_outedge(t);
            }
            i += 1;
        }
    }

    fn dfs_enter_inedge(&mut self, v: usize) {
        for i in 0..self.g.inn[v].len() {
            let e = self.g.inn[v][i];
            let t = self.g.et[e];
            if !self.is_tree(e) {
                if !Self::seq(self.lo, self.lim[t], self.li) {
                    let s = self.slack(e);
                    if s < self.slack_best || self.enter.is_none() {
                        self.enter = Some(e);
                        self.slack_best = s;
                    }
                }
            } else if self.lim[t] < self.lim[v] {
                self.dfs_enter_inedge(t);
            }
        }
        let mut i = 0;
        while i < self.tree_out[v].len() && self.slack_best > 0 {
            let h = self.g.eh[self.tree_out[v][i]];
            if self.lim[h] < self.lim[v] {
                self.dfs_enter_inedge(h);
            }
            i += 1;
        }
    }

    fn enter_edge(&mut self, e: usize) -> Option<usize> {
        let (t, h) = (self.g.et[e], self.g.eh[e]);
        let (v, outsearch) = if self.lim[t] < self.lim[h] {
            (t, false)
        } else {
            (h, true)
        };
        self.enter = None;
        self.slack_best = i64::MAX;
        self.lo = self.low[v];
        self.li = self.lim[v];
        if outsearch {
            self.dfs_enter_outedge(v);
        } else {
            self.dfs_enter_inedge(v);
        }
        self.enter
    }

    fn rerank(&mut self, v: usize, delta: i64) {
        self.g.rank[v] -= delta;
        for i in 0..self.tree_out[v].len() {
            let e = self.tree_out[v][i];
            if Some(e) != self.par[v] {
                self.rerank(self.g.eh[e], delta);
            }
        }
        for i in 0..self.tree_in[v].len() {
            let e = self.tree_in[v][i];
            if Some(e) != self.par[v] {
                self.rerank(self.g.et[e], delta);
            }
        }
    }

    fn treeupdate(&mut self, mut v: usize, w: usize, cutvalue: i64, dir: bool) -> usize {
        while !Self::seq(self.low[v], self.lim[w], self.lim[v]) {
            let e = self.par[v].unwrap();
            let d = if v == self.g.et[e] { dir } else { !dir };
            if d {
                self.cut[e] += cutvalue;
            } else {
                self.cut[e] -= cutvalue;
            }
            let (t, h) = (self.g.et[e], self.g.eh[e]);
            v = if self.lim[t] > self.lim[h] { t } else { h };
        }
        v
    }

    fn update(&mut self, e: usize, f: usize) {
        let delta = self.slack(f);
        let (t, h) = (self.g.et[e], self.g.eh[e]);
        if delta > 0 {
            if self.tree_in[t].len() + self.tree_out[t].len() == 1 {
                self.rerank(t, delta);
            } else if self.tree_in[h].len() + self.tree_out[h].len() == 1 {
                self.rerank(h, -delta);
            } else if self.lim[t] < self.lim[h] {
                self.rerank(t, delta);
            } else {
                self.rerank(h, -delta);
            }
        }
        let cutvalue = self.cut[e];
        let lca = self.treeupdate(self.g.et[f], self.g.eh[f], cutvalue, true);
        self.treeupdate(self.g.eh[f], self.g.et[f], cutvalue, false);
        self.cut[f] = -cutvalue;
        self.cut[e] = 0;
        // exchange_tree_edges
        let idx = self.tree_index[e];
        self.tree_index[f] = idx;
        self.tree_edges[idx as usize] = f;
        self.tree_index[e] = -1;
        let j = self.tree_out[t].iter().position(|&x| x == e).unwrap();
        self.tree_out[t].swap_remove(j);
        let j = self.tree_in[h].iter().position(|&x| x == e).unwrap();
        self.tree_in[h].swap_remove(j);
        self.tree_out[self.g.et[f]].push(f);
        self.tree_in[self.g.eh[f]].push(f);
        let (p, l) = (self.par[lca], self.low[lca]);
        self.dfs_range(lca, p, l);
    }

    // TB_balance: node dengan bobot masuk = keluar pindah ke rank tersepi.
    // ponytail: urutan seri qsort dot tak tentu; di sini stabil menurut nlist.
    fn tb_balance(&mut self) {
        let nl = self.g.nlist.clone();
        let minr = nl.iter().map(|&v| self.g.rank[v]).min().unwrap_or(0);
        for &v in &nl {
            self.g.rank[v] -= minr;
        }
        let maxr = nl.iter().map(|&v| self.g.rank[v]).max().unwrap_or(0);
        let mut nrank = vec![0i64; maxr as usize + 1];
        let mut order = nl.clone();
        order.sort_by_key(|&v| self.g.rank[v]);
        for &v in &order {
            nrank[self.g.rank[v] as usize] += 1;
        }
        for &v in &order {
            let (mut win, mut wout, mut low, mut high) = (0, 0, 0i64, maxr);
            for &e in &self.g.inn[v] {
                win += self.g.weight[e];
                low = low.max(self.g.rank[self.g.et[e]] + self.g.minlen[e]);
            }
            for &e in &self.g.out[v] {
                wout += self.g.weight[e];
                high = high.min(self.g.rank[self.g.eh[e]] - self.g.minlen[e]);
            }
            if low < 0 {
                low = 0;
            }
            if win == wout {
                let mut choice = low;
                for i in low + 1..=high {
                    if nrank[i as usize] < nrank[choice as usize] {
                        choice = i;
                    }
                }
                nrank[self.g.rank[v] as usize] -= 1;
                nrank[choice as usize] += 1;
                self.g.rank[v] = choice;
            }
        }
    }

    fn lr_balance(&mut self) {
        for i in 0..self.tree_edges.len() {
            let e = self.tree_edges[i];
            if self.cut[e] != 0 {
                continue;
            }
            let Some(f) = self.enter_edge(e) else {
                continue;
            };
            let delta = self.slack(f);
            if delta <= 1 {
                continue;
            }
            let (t, h) = (self.g.et[e], self.g.eh[e]);
            if self.lim[t] < self.lim[h] {
                self.rerank(t, delta / 2);
            } else {
                self.rerank(h, -delta / 2);
            }
        }
    }
}
