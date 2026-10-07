//! Tree data model, parsers (Newick / NEXUS / PhyloXML), writers, editing
//! operations (reroot, ladderize, prune, ...) and tree construction (NJ / UPGMA).
use std::collections::{HashMap, HashSet};

/*
Gaurav Sablok
gsablok@proton.me
 */

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,  // tip name
    pub label: String, // internal node label (support / clade name)
    pub len: f64,      // length of branch leading to this node
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub color: Option<[u8; 3]>,
    pub collapsed: bool,
}

#[derive(Clone, Debug)]
pub struct Tree {
    pub name: String,
    pub nodes: Vec<Node>,
    pub root: usize,
    pub has_len: bool,
}

pub struct Stats {
    pub tips: usize,
    pub internal: usize,
    pub polytomies: usize,
    pub total_len: f64,
    pub max_depth: f64,
    pub rooted: bool,
}

impl Tree {
    pub fn empty(name: &str) -> Tree {
        Tree {
            name: name.into(),
            nodes: vec![],
            root: 0,
            has_len: false,
        }
    }
    pub fn add(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.nodes.len() - 1
    }
    fn link(&mut self, child: usize, parent: usize, len: f64) {
        self.nodes[child].parent = Some(parent);
        self.nodes[child].len = len;
        self.nodes[parent].children.push(child);
    }
    pub fn eff_len(&self, n: usize) -> f64 {
        if self.has_len {
            self.nodes[n].len.max(0.0)
        } else {
            1.0
        }
    }
    pub fn preorder(&self, start: usize) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut st = vec![start];
        while let Some(n) = st.pop() {
            out.push(n);
            for &c in self.nodes[n].children.iter().rev() {
                st.push(c);
            }
        }
        out
    }
    pub fn tips(&self, n: usize) -> Vec<usize> {
        self.preorder(n)
            .into_iter()
            .filter(|&u| self.nodes[u].children.is_empty())
            .collect()
    }
    pub fn tip_counts(&self) -> Vec<usize> {
        let mut c = vec![0usize; self.nodes.len()];
        for &u in self.preorder(self.root).iter().rev() {
            c[u] = if self.nodes[u].children.is_empty() {
                1
            } else {
                self.nodes[u].children.iter().map(|&k| c[k]).sum()
            };
        }
        c
    }

    /// Drop unreachable nodes and renumber in pre-order (root = 0).
    pub fn compact(&mut self) {
        self.nodes[self.root].parent = None;
        let order = self.preorder(self.root);
        let mut map = vec![usize::MAX; self.nodes.len()];
        for (i, &o) in order.iter().enumerate() {
            map[o] = i;
        }
        let mut nn = Vec::with_capacity(order.len());
        for &o in &order {
            let mut n = self.nodes[o].clone();
            n.parent = n.parent.map(|p| map[p]);
            n.children = n.children.iter().map(|&c| map[c]).collect();
            nn.push(n);
        }
        self.nodes = nn;
        self.root = 0;
    }

    pub fn stats(&self) -> Stats {
        let mut s = Stats {
            tips: 0,
            internal: 0,
            polytomies: 0,
            total_len: 0.0,
            max_depth: 0.0,
            rooted: self.nodes[self.root].children.len() == 2,
        };
        let mut depth = vec![0.0; self.nodes.len()];
        for &u in &self.preorder(self.root) {
            let nd = &self.nodes[u];
            if let Some(p) = nd.parent {
                depth[u] = depth[p] + self.eff_len(u);
                s.total_len += self.eff_len(u);
            }
            if nd.children.is_empty() {
                s.tips += 1;
                if depth[u] > s.max_depth {
                    s.max_depth = depth[u];
                }
            } else {
                s.internal += 1;
                if nd.children.len() > 2 && u != self.root {
                    s.polytomies += 1;
                }
            }
        }
        s
    }

    // ---------------------------------------------------------------- editing
    /// Re-root on the branch above `c`; `frac` is the fraction of that branch kept on the `c` side.
    pub fn reroot_at(&mut self, c: usize, frac: f64) {
        let Some(p1) = self.nodes[c].parent else {
            return;
        };
        let mut path = vec![];
        let mut x = p1;
        loop {
            path.push(x);
            match self.nodes[x].parent {
                Some(q) => x = q,
                None => break,
            }
        }
        let l = self.nodes[c].len;
        let lab_c = self.nodes[c].label.clone();
        let old_len: Vec<f64> = path.iter().map(|&x| self.nodes[x].len).collect();
        let old_lab: Vec<String> = path.iter().map(|&x| self.nodes[x].label.clone()).collect();
        let old_children: Vec<Vec<usize>> = path
            .iter()
            .map(|&x| self.nodes[x].children.clone())
            .collect();
        let r = self.add(Node::default());
        let k = path.len();
        for i in 0..k {
            let pi = path[i];
            let removed = if i == 0 { c } else { path[i - 1] };
            let mut ch: Vec<usize> = old_children[i]
                .iter()
                .copied()
                .filter(|&z| z != removed)
                .collect();
            if i + 1 < k {
                ch.push(path[i + 1]);
            }
            let np = if i == 0 { r } else { path[i - 1] };
            let nd = &mut self.nodes[pi];
            nd.children = ch;
            nd.parent = Some(np);
            nd.len = if i == 0 {
                l * (1.0 - frac)
            } else {
                old_len[i - 1]
            };
            nd.label = if i == 0 {
                lab_c.clone()
            } else {
                old_lab[i - 1].clone()
            };
        }
        self.nodes[c].parent = Some(r);
        self.nodes[c].len = l * frac;
        self.nodes[r].children = vec![c, p1];
        self.nodes[r].parent = None;
        self.root = r;
        // old root may now be a degree-2 node: splice it out
        let o = path[k - 1];
        if self.nodes[o].children.len() == 1 {
            let ch = self.nodes[o].children[0];
            let g = self.nodes[o].parent.unwrap();
            let add = self.nodes[o].len;
            let lab = self.nodes[o].label.clone();
            self.nodes[ch].len += add;
            if self.nodes[ch].label.is_empty() && !self.nodes[ch].children.is_empty() {
                self.nodes[ch].label = lab;
            }
            self.nodes[ch].parent = Some(g);
            for z in self.nodes[g].children.iter_mut() {
                if *z == o {
                    *z = ch;
                }
            }
        }
        self.compact();
    }

    fn dists_from(&self, s: usize) -> (Vec<f64>, Vec<usize>) {
        let n = self.nodes.len();
        let mut d = vec![f64::NAN; n];
        let mut prev = vec![usize::MAX; n];
        d[s] = 0.0;
        let mut st = vec![s];
        while let Some(u) = st.pop() {
            let mut nb: Vec<(usize, f64)> = vec![];
            if let Some(p) = self.nodes[u].parent {
                nb.push((p, self.eff_len(u)));
            }
            for &c in &self.nodes[u].children {
                nb.push((c, self.eff_len(c)));
            }
            for (v, w) in nb {
                if d[v].is_nan() {
                    d[v] = d[u] + w;
                    prev[v] = u;
                    st.push(v);
                }
            }
        }
        (d, prev)
    }

    pub fn midpoint_root(&mut self) {
        let tips = self.tips(self.root);
        if tips.len() < 3 {
            return;
        }
        let far = |d: &Vec<f64>, tips: &Vec<usize>| {
            *tips
                .iter()
                .max_by(|&&x, &&y| d[x].partial_cmp(&d[y]).unwrap())
                .unwrap()
        };
        let (d0, _) = self.dists_from(tips[0]);
        let a = far(&d0, &tips);
        let (da, prev) = self.dists_from(a);
        let b = far(&da, &tips);
        let half = da[b] / 2.0;
        let mut path = vec![b];
        let mut x = b;
        while x != a {
            x = prev[x];
            path.push(x);
        }
        path.reverse();
        for w in path.windows(2) {
            let (x, y) = (w[0], w[1]);
            let wl = da[y] - da[x];
            if da[x] + wl >= half - 1e-12 {
                let f = if wl > 0.0 { (half - da[x]) / wl } else { 0.5 };
                if self.nodes[x].parent == Some(y) {
                    self.reroot_at(x, f)
                } else {
                    self.reroot_at(y, 1.0 - f)
                }
                return;
            }
        }
    }

    /// Convert a bifurcating root into a trifurcation.
    pub fn unroot(&mut self) {
        let r = self.root;
        if self.nodes[r].children.len() != 2 {
            return;
        }
        let ch = self.nodes[r].children.clone();
        let counts = self.tip_counts();
        let internal = |i: usize| !self.nodes[ch[i]].children.is_empty();
        let (keep, other) = if internal(0) && (!internal(1) || counts[ch[0]] >= counts[ch[1]]) {
            (ch[0], ch[1])
        } else if internal(1) {
            (ch[1], ch[0])
        } else {
            return;
        };
        let kl = self.nodes[keep].len;
        self.nodes[other].len += kl;
        let kids = std::mem::take(&mut self.nodes[keep].children);
        for &k in &kids {
            self.nodes[k].parent = Some(r);
        }
        let pos = self.nodes[r]
            .children
            .iter()
            .position(|&x| x == keep)
            .unwrap();
        self.nodes[r].children.splice(pos..pos + 1, kids);
        self.compact();
    }

    pub fn ladderize(&mut self, start: usize, asc: bool) {
        let counts = self.tip_counts();
        for u in self.preorder(start) {
            let mut ch = std::mem::take(&mut self.nodes[u].children);
            ch.sort_by_key(|&c| counts[c]);
            if !asc {
                ch.reverse();
            }
            self.nodes[u].children = ch;
        }
    }

    pub fn mrca(&self, ids: &[usize]) -> Option<usize> {
        let mut cur = *ids.first()?;
        for &b in &ids[1..] {
            let mut anc = HashSet::new();
            let mut x = Some(cur);
            while let Some(u) = x {
                anc.insert(u);
                x = self.nodes[u].parent;
            }
            let mut y = b;
            while !anc.contains(&y) {
                y = self.nodes[y].parent?;
            }
            cur = y;
        }
        Some(cur)
    }

    fn detach(&mut self, n: usize) {
        let Some(p) = self.nodes[n].parent else {
            return;
        };
        if !self.nodes[p].children.contains(&n) {
            return;
        }
        self.nodes[p].children.retain(|&k| k != n);
        self.nodes[n].parent = None;
        if self.nodes[p].children.len() == 1 {
            let ch = self.nodes[p].children[0];
            if p == self.root {
                self.nodes[ch].parent = None;
                self.nodes[ch].len = 0.0;
                self.root = ch;
            } else {
                let g = self.nodes[p].parent.unwrap();
                let add = self.nodes[p].len;
                self.nodes[ch].len += add;
                self.nodes[ch].parent = Some(g);
                for k in self.nodes[g].children.iter_mut() {
                    if *k == p {
                        *k = ch;
                    }
                }
            }
        }
    }
    pub fn remove_many(&mut self, ids: &[usize]) {
        for &i in ids {
            if i != self.root {
                self.detach(i);
            }
        }
        self.compact();
    }

    pub fn subtree(&self, n: usize) -> Tree {
        let order = self.preorder(n);
        let mut map = HashMap::new();
        for (i, &o) in order.iter().enumerate() {
            map.insert(o, i);
        }
        let mut t = Tree::empty(&format!("{} (subtree)", self.name));
        t.has_len = self.has_len;
        for &o in &order {
            let mut nd = self.nodes[o].clone();
            nd.parent = nd.parent.and_then(|p| map.get(&p).copied());
            nd.children = nd.children.iter().map(|c| map[c]).collect();
            t.nodes.push(nd);
        }
        t.nodes[0].len = 0.0;
        t.nodes[0].collapsed = false;
        t
    }

    pub fn set_color_clade(&mut self, n: usize, c: Option<[u8; 3]>) {
        for u in self.preorder(n) {
            self.nodes[u].color = c;
        }
    }

    // ---------------------------------------------------------------- writers
    pub fn to_newick(&self) -> String {
        let mut s = String::new();
        self.write_nw(self.root, &mut s);
        s.push(';');
        s
    }
    fn write_nw(&self, n: usize, s: &mut String) {
        let nd = &self.nodes[n];
        if !nd.children.is_empty() {
            s.push('(');
            for (i, &c) in nd.children.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                self.write_nw(c, s);
            }
            s.push(')');
            s.push_str(&quote(&nd.label));
        } else {
            s.push_str(&quote(&nd.name));
        }
        if self.has_len && n != self.root {
            s.push_str(&format!(":{}", nd.len));
        }
    }
    pub fn to_nexus(&self) -> String {
        let tips: Vec<String> = self
            .tips(self.root)
            .iter()
            .map(|&t| quote(&self.nodes[t].name))
            .collect();
        format!(
            "#NEXUS\n\nbegin taxa;\n\tdimensions ntax={};\n\ttaxlabels {};\nend;\n\nbegin trees;\n\ttree {} = [&{}] {}\nend;\n",
            tips.len(), tips.join(" "), quote(&self.name.replace(' ', "_")),
            if self.nodes[self.root].children.len() == 2 { "R" } else { "U" }, self.to_newick()
        )
    }
}

fn quote(s: &str) -> String {
    if s.chars().any(|c| "(),:;[]' \t".contains(c)) {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

// ------------------------------------------------------------------- parsers
fn skip_ws(c: &[char], i: &mut usize) {
    loop {
        while *i < c.len() && c[*i].is_whitespace() {
            *i += 1;
        }
        if *i < c.len() && c[*i] == '[' {
            while *i < c.len() && c[*i] != ']' {
                *i += 1;
            }
            *i += 1;
        } else {
            break;
        }
    }
}
fn read_label(c: &[char], i: &mut usize) -> String {
    skip_ws(c, i);
    let mut s = String::new();
    if *i < c.len() && c[*i] == '\'' {
        *i += 1;
        while *i < c.len() {
            if c[*i] == '\'' {
                if *i + 1 < c.len() && c[*i + 1] == '\'' {
                    s.push('\'');
                    *i += 2;
                    continue;
                }
                *i += 1;
                break;
            }
            s.push(c[*i]);
            *i += 1;
        }
        s
    } else {
        while *i < c.len() && !"(),:;[".contains(c[*i]) {
            s.push(c[*i]);
            *i += 1;
        }
        s.trim().to_string()
    }
}
fn read_len(c: &[char], i: &mut usize) -> Option<f64> {
    skip_ws(c, i);
    if *i < c.len() && c[*i] == ':' {
        *i += 1;
        skip_ws(c, i);
        let mut s = String::new();
        while *i < c.len() && !"(),:;[".contains(c[*i]) {
            s.push(c[*i]);
            *i += 1;
        }
        return s.trim().parse::<f64>().ok();
    }
    None
}

pub fn parse_newick(s: &str) -> Result<Tree, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut t = Tree::empty("tree");
    let mut stack: Vec<usize> = vec![];
    let mut root = None;
    fn attach(
        t: &mut Tree,
        stack: &[usize],
        root: &mut Option<usize>,
        n: usize,
    ) -> Result<(), String> {
        if let Some(&p) = stack.last() {
            t.nodes[n].parent = Some(p);
            t.nodes[p].children.push(n);
        } else if root.is_none() {
            *root = Some(n);
        } else {
            return Err("multiple roots".into());
        }
        Ok(())
    }
    loop {
        skip_ws(&c, &mut i);
        if i >= c.len() {
            break;
        }
        match c[i] {
            '(' => {
                let n = t.add(Node::default());
                attach(&mut t, &stack, &mut root, n)?;
                stack.push(n);
                i += 1;
            }
            ',' => i += 1,
            ')' => {
                let n = stack.pop().ok_or("unbalanced parentheses")?;
                i += 1;
                t.nodes[n].label = read_label(&c, &mut i);
                if let Some(v) = read_len(&c, &mut i) {
                    t.nodes[n].len = v;
                    t.has_len = true;
                }
            }
            ';' => break,
            _ => {
                let name = if c[i] == ':' {
                    String::new()
                } else {
                    read_label(&c, &mut i)
                };
                let n = t.add(Node {
                    name,
                    ..Default::default()
                });
                attach(&mut t, &stack, &mut root, n)?;
                if let Some(v) = read_len(&c, &mut i) {
                    t.nodes[n].len = v;
                    t.has_len = true;
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err("unbalanced parentheses".into());
    }
    t.root = root.ok_or("empty tree")?;
    t.compact();
    Ok(t)
}

/// Split into ';'-terminated statements, honouring quotes and dropping [comments].
fn split_statements(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let (mut q, mut depth) = (false, 0);
    for ch in s.chars() {
        if q {
            cur.push(ch);
            if ch == '\'' {
                q = false;
            }
            continue;
        }
        match ch {
            '\'' if depth == 0 => {
                q = true;
                cur.push(ch);
            }
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            ';' => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

pub fn parse_nexus(s: &str) -> Result<Vec<Tree>, String> {
    let body = s.trim_start();
    let body = if body.len() >= 6 && body[..6].eq_ignore_ascii_case("#nexus") {
        &body[6..]
    } else {
        body
    };
    let mut trans: HashMap<String, String> = HashMap::new();
    let mut trees = vec![];
    let mut in_trees = false;
    for st in split_statements(body) {
        let t = st.trim();
        let lower = t.to_lowercase();
        if lower.starts_with("begin") {
            in_trees = lower.contains("trees");
            continue;
        }
        if lower.starts_with("end") {
            in_trees = false;
            continue;
        }
        if !in_trees {
            continue;
        }
        if lower.starts_with("translate") {
            for pair in t[9..].split(',') {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                let mut it = pair.splitn(2, char::is_whitespace);
                let k = it.next().unwrap();
                let v = it.next().unwrap_or("").trim().trim_matches('\'');
                trans.insert(k.to_string(), v.to_string());
            }
        } else if lower.starts_with("tree ") || lower.starts_with("utree ") {
            if let Some(eq) = t.find('=') {
                let name = t[..eq]
                    .split_whitespace()
                    .last()
                    .unwrap_or("tree")
                    .trim_matches('\'')
                    .to_string();
                let mut tr = parse_newick(&t[eq + 1..])?;
                tr.name = name;
                for n in tr.nodes.iter_mut() {
                    if n.children.is_empty() {
                        if let Some(v) = trans.get(&n.name) {
                            n.name = v.clone();
                        }
                    }
                }
                trees.push(tr);
            }
        }
    }
    if trees.is_empty() {
        Err("no trees found in NEXUS file".into())
    } else {
        Ok(trees)
    }
}

pub fn parse_newick_multi(s: &str) -> Result<Vec<Tree>, String> {
    let mut v = vec![];
    for (i, st) in split_statements(s).iter().enumerate() {
        if st.trim().is_empty() {
            continue;
        }
        let mut t = parse_newick(st)?;
        t.name = format!("tree {}", i + 1);
        v.push(t);
    }
    if v.is_empty() {
        Err("no trees found".into())
    } else {
        Ok(v)
    }
}

pub fn parse_phyloxml(s: &str) -> Result<Vec<Tree>, String> {
    let mut trees = vec![];
    let mut cur: Option<Tree> = None;
    let mut stack: Vec<usize> = vec![];
    let mut tags: Vec<String> = vec![];
    let mut pos = 0;
    let unesc = |x: &str| {
        x.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
    };
    while let Some(lt) = s[pos..].find('<') {
        let start = pos + lt;
        let Some(gt) = s[start..].find('>') else {
            break;
        };
        let end = start + gt;
        let tag = &s[start + 1..end];
        let next = s[end + 1..]
            .find('<')
            .map(|x| end + 1 + x)
            .unwrap_or(s.len());
        let text = unesc(s[end + 1..next].trim());
        pos = end + 1;
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(close) = tag.strip_prefix('/') {
            match close.trim() {
                "clade" => {
                    stack.pop();
                }
                "phylogeny" => {
                    if let Some(mut t) = cur.take() {
                        t.compact();
                        trees.push(t);
                    }
                }
                _ => {}
            }
            tags.pop();
            continue;
        }
        let selfclose = tag.ends_with('/');
        let body = tag.trim_end_matches('/');
        let name = body.split_whitespace().next().unwrap_or("");
        let parent_is_clade = tags.last().map(|x| x == "clade").unwrap_or(false);
        match name {
            "phylogeny" => {
                cur = Some(Tree::empty(&format!("tree {}", trees.len() + 1)));
                stack.clear();
            }
            "clade" => {
                if let Some(t) = cur.as_mut() {
                    let n = t.add(Node::default());
                    if let Some(&p) = stack.last() {
                        t.nodes[n].parent = Some(p);
                        t.nodes[p].children.push(n);
                    } else {
                        t.root = n;
                    }
                    if let Some(a) = body.find("branch_length=\"") {
                        let rest = &body[a + 15..];
                        if let Some(e) = rest.find('"') {
                            if let Ok(v) = rest[..e].parse::<f64>() {
                                t.nodes[n].len = v;
                                t.has_len = true;
                            }
                        }
                    }
                    stack.push(n);
                }
            }
            "name" if parent_is_clade => {
                if let (Some(t), Some(&n)) = (cur.as_mut(), stack.last()) {
                    t.nodes[n].name = text.clone();
                }
            }
            "branch_length" if parent_is_clade => {
                if let (Some(t), Some(&n)) = (cur.as_mut(), stack.last()) {
                    if let Ok(v) = text.parse::<f64>() {
                        t.nodes[n].len = v;
                        t.has_len = true;
                    }
                }
            }
            "confidence" if parent_is_clade => {
                if let (Some(t), Some(&n)) = (cur.as_mut(), stack.last()) {
                    t.nodes[n].label = text.clone();
                }
            }
            _ => {}
        }
        if selfclose {
            if name == "clade" {
                stack.pop();
            }
        } else {
            tags.push(name.to_string());
        }
    }
    for t in trees.iter_mut() {
        for n in t.nodes.iter_mut() {
            if !n.children.is_empty() && n.label.is_empty() {
                n.label = std::mem::take(&mut n.name);
            }
        }
    }
    if trees.is_empty() {
        Err("no <phylogeny> found".into())
    } else {
        Ok(trees)
    }
}

pub fn parse_any(text: &str) -> Result<Vec<Tree>, String> {
    let t = text.trim_start();
    if t.len() >= 6 && t[..6].eq_ignore_ascii_case("#nexus") {
        parse_nexus(text)
    } else if t.starts_with('<') {
        parse_phyloxml(text)
    } else {
        parse_newick_multi(text)
    }
}

// -------------------------------------------------------------- construction
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    NeighborJoining,
    Upgma,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DistModel {
    PDistance,
    Jc69,
    Poisson,
}

pub struct DistMatrix {
    pub names: Vec<String>,
    pub d: Vec<Vec<f64>>,
}

pub fn parse_phylip_matrix(text: &str) -> Result<DistMatrix, String> {
    let mut lines = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty());
    let n: usize = lines
        .next()
        .ok_or("empty input")?
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .map_err(|_| "first line must be the number of taxa")?;
    let mut names = vec![];
    let mut d = vec![vec![0.0; n]; n];
    for i in 0..n {
        let line = lines.next().ok_or("too few rows in matrix")?;
        let mut it = line.split_whitespace();
        names.push(it.next().unwrap().to_string());
        let vals: Vec<f64> = it
            .map(|x| x.parse::<f64>().map_err(|_| format!("bad number '{}'", x)))
            .collect::<Result<_, _>>()?;
        if vals.len() == n {
            for j in 0..n {
                d[i][j] = vals[j];
            }
        } else if vals.len() == i {
            for j in 0..i {
                d[i][j] = vals[j];
                d[j][i] = vals[j];
            }
        } else {
            return Err(format!(
                "row {} has {} values (expected {} or {})",
                i + 1,
                vals.len(),
                n,
                i
            ));
        }
    }
    for i in 0..n {
        for j in 0..i {
            let a = (d[i][j] + d[j][i]) / 2.0;
            let a = if d[i][j] == 0.0 {
                d[j][i]
            } else if d[j][i] == 0.0 {
                d[i][j]
            } else {
                a
            };
            d[i][j] = a;
            d[j][i] = a;
        }
        d[i][i] = 0.0;
    }
    Ok(DistMatrix { names, d })
}

pub fn parse_fasta(text: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = vec![];
    for l in text.lines() {
        let l = l.trim();
        if let Some(h) = l.strip_prefix('>') {
            v.push((h.trim().to_string(), String::new()));
        } else if let Some(last) = v.last_mut() {
            last.1.push_str(&l.replace(' ', "").to_uppercase());
        }
    }
    v
}

pub fn distances_from_alignment(
    seqs: &[(String, String)],
    model: DistModel,
) -> Result<DistMatrix, String> {
    let n = seqs.len();
    if n < 2 {
        return Err("need at least 2 sequences".into());
    }
    let len = seqs[0].1.len();
    if seqs.iter().any(|s| s.1.len() != len) {
        return Err("sequences are not aligned (different lengths)".into());
    }
    let bad = |c: u8| matches!(c, b'-' | b'?' | b'N' | b'X' | b'.');
    let mut d = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (seqs[i].1.as_bytes(), seqs[j].1.as_bytes());
            let (mut valid, mut diff) = (0usize, 0usize);
            for k in 0..len {
                if bad(a[k]) || bad(b[k]) {
                    continue;
                }
                valid += 1;
                if a[k] != b[k] {
                    diff += 1;
                }
            }
            let p = if valid == 0 {
                0.75
            } else {
                diff as f64 / valid as f64
            };
            let v = match model {
                DistModel::PDistance => p,
                DistModel::Jc69 => {
                    if p >= 0.749 {
                        5.0
                    } else {
                        -0.75 * (1.0 - 4.0 * p / 3.0).ln()
                    }
                }
                DistModel::Poisson => {
                    if p >= 0.99 {
                        5.0
                    } else {
                        -(1.0 - p).ln()
                    }
                }
            };
            d[i][j] = v;
            d[j][i] = v;
        }
    }
    Ok(DistMatrix {
        names: seqs.iter().map(|s| s.0.clone()).collect(),
        d,
    })
}

fn shrink(d: &[Vec<f64>], i: usize, j: usize, newrow: &[f64]) -> Vec<Vec<f64>> {
    let k = d.len();
    let keep: Vec<usize> = (0..k).filter(|&x| x != i && x != j).collect();
    let mut nd = vec![vec![0.0; k - 1]; k - 1];
    for (a, &xa) in keep.iter().enumerate() {
        for (b, &xb) in keep.iter().enumerate() {
            nd[a][b] = d[xa][xb];
        }
        nd[a][k - 2] = newrow[a];
        nd[k - 2][a] = newrow[a];
    }
    nd
}

pub fn build_tree(m: &DistMatrix, method: Method) -> Result<Tree, String> {
    let n = m.names.len();
    if n < 2 {
        return Err("need at least 2 taxa".into());
    }
    let mut t = Tree::empty(if method == Method::NeighborJoining {
        "NJ tree"
    } else {
        "UPGMA tree"
    });
    t.has_len = true;
    let mut ids: Vec<usize> = (0..n)
        .map(|i| {
            t.add(Node {
                name: m.names[i].clone(),
                ..Default::default()
            })
        })
        .collect();
    let mut d = m.d.clone();
    match method {
        Method::NeighborJoining => {
            while ids.len() > 3 {
                let k = ids.len();
                let kf = k as f64;
                let r: Vec<f64> = (0..k).map(|i| d[i].iter().sum()).collect();
                let mut best = (0, 1, f64::INFINITY);
                for i in 0..k {
                    for j in i + 1..k {
                        let q = (kf - 2.0) * d[i][j] - r[i] - r[j];
                        if q < best.2 {
                            best = (i, j, q);
                        }
                    }
                }
                let (i, j, _) = best;
                let dij = d[i][j];
                let li = (0.5 * dij + (r[i] - r[j]) / (2.0 * (kf - 2.0))).max(0.0);
                let lj = (dij - li).max(0.0);
                let u = t.add(Node::default());
                t.link(ids[i], u, li);
                t.link(ids[j], u, lj);
                let newrow: Vec<f64> = (0..k)
                    .filter(|&x| x != i && x != j)
                    .map(|x| 0.5 * (d[i][x] + d[j][x] - dij))
                    .collect();
                d = shrink(&d, i, j, &newrow);
                let mut nids: Vec<usize> = (0..k)
                    .filter(|&x| x != i && x != j)
                    .map(|x| ids[x])
                    .collect();
                nids.push(u);
                ids = nids;
            }
            let root = t.add(Node::default());
            if ids.len() == 3 {
                let (d01, d02, d12) = (d[0][1], d[0][2], d[1][2]);
                t.link(ids[0], root, ((d01 + d02 - d12) / 2.0).max(0.0));
                t.link(ids[1], root, ((d01 + d12 - d02) / 2.0).max(0.0));
                t.link(ids[2], root, ((d02 + d12 - d01) / 2.0).max(0.0));
            } else {
                t.link(ids[0], root, d[0][1] / 2.0);
                t.link(ids[1], root, d[0][1] / 2.0);
            }
            t.root = root;
        }
        Method::Upgma => {
            let mut h = vec![0.0; n];
            let mut sz = vec![1.0; n];
            while ids.len() > 1 {
                let k = ids.len();
                let mut best = (0, 1, f64::INFINITY);
                for i in 0..k {
                    for j in i + 1..k {
                        if d[i][j] < best.2 {
                            best = (i, j, d[i][j]);
                        }
                    }
                }
                let (i, j, dij) = best;
                let u = t.add(Node::default());
                let hu = dij / 2.0;
                t.link(ids[i], u, (hu - h[i]).max(0.0));
                t.link(ids[j], u, (hu - h[j]).max(0.0));
                let newrow: Vec<f64> = (0..k)
                    .filter(|&x| x != i && x != j)
                    .map(|x| (d[i][x] * sz[i] + d[j][x] * sz[j]) / (sz[i] + sz[j]))
                    .collect();
                let su = sz[i] + sz[j];
                d = shrink(&d, i, j, &newrow);
                let keep: Vec<usize> = (0..k).filter(|&x| x != i && x != j).collect();
                let mut nids: Vec<usize> = keep.iter().map(|&x| ids[x]).collect();
                let mut nh: Vec<f64> = keep.iter().map(|&x| h[x]).collect();
                let mut ns: Vec<f64> = keep.iter().map(|&x| sz[x]).collect();
                nids.push(u);
                nh.push(hu);
                ns.push(su);
                ids = nids;
                h = nh;
                sz = ns;
            }
            t.root = ids[0];
        }
    }
    t.compact();
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tipset(t: &Tree) -> Vec<String> {
        let mut v: Vec<String> = t
            .tips(t.root)
            .iter()
            .map(|&i| t.nodes[i].name.clone())
            .collect();
        v.sort();
        v
    }
    fn total(t: &Tree) -> f64 {
        t.nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != t.root)
            .map(|(_, n)| n.len)
            .sum()
    }

    #[test]
    fn newick_roundtrip_and_quotes() {
        let t = parse_newick("((A:1,'B c':2)90:3,(C:1,D:1):2,E:5);").unwrap();
        assert_eq!(t.tips(t.root).len(), 5);
        let s = t.to_newick();
        let t2 = parse_newick(&s).unwrap();
        assert_eq!(tipset(&t), tipset(&t2));
        assert!(s.contains("'B c'"));
    }
    #[test]
    fn reroot_preserves_topology_and_length() {
        let t0 = parse_newick("((A:1,B:2)90:3,(C:1,D:1)80:2,E:5);").unwrap();
        for n in 0..t0.nodes.len() {
            if n == t0.root {
                continue;
            }
            let mut t = t0.clone();
            t.reroot_at(n, 0.5);
            assert_eq!(tipset(&t), tipset(&t0));
            assert!((total(&t) - total(&t0)).abs() < 1e-9, "node {n}");
            assert_eq!(t.nodes[t.root].children.len(), 2);
            let t2 = parse_newick(&t.to_newick()).unwrap();
            assert_eq!(tipset(&t2), tipset(&t0));
        }
    }
    #[test]
    fn midpoint() {
        let mut t = parse_newick("((A:1,B:1):1,C:10);").unwrap();
        t.midpoint_root();
        // longest path A..C = 12 → root 6 from each end → C branch becomes 6
        let c = t
            .tips(t.root)
            .into_iter()
            .find(|&i| t.nodes[i].name == "C")
            .unwrap();
        assert!((t.nodes[c].len - 6.0).abs() < 1e-9, "{}", t.to_newick());
    }
    #[test]
    fn nexus_translate() {
        let s = "#NEXUS\nbegin trees;\n translate 1 Alpha, 2 Beta, 3 Gamma;\n tree t1 = [&R] ((1:1,2:1):1,3:2);\nend;";
        let v = parse_nexus(s).unwrap();
        assert_eq!(tipset(&v[0]), vec!["Alpha", "Beta", "Gamma"]);
    }
    #[test]
    fn phyloxml() {
        let s = r#"<phyloxml><phylogeny rooted="true"><clade><clade branch_length="0.1"><name>A</name></clade><clade branch_length="0.2"><name>B</name></clade></clade></phylogeny></phyloxml>"#;
        let v = parse_phyloxml(s).unwrap();
        assert_eq!(tipset(&v[0]), vec!["A", "B"]);
    }
    #[test]
    fn nj_recovers_tree() {
        // additive distances for ((A:1,B:2):1,(C:3,D:1):1)
        let names = vec!["A", "B", "C", "D"]
            .into_iter()
            .map(String::from)
            .collect();
        let d = vec![
            vec![0., 3., 6., 4.],
            vec![3., 0., 7., 5.],
            vec![6., 7., 0., 4.],
            vec![4., 5., 4., 0.],
        ];
        let t = build_tree(&DistMatrix { names, d }, Method::NeighborJoining).unwrap();
        assert!((total(&t) - 9.0).abs() < 1e-9, "{}", t.to_newick());
        let u = build_tree(
            &DistMatrix {
                names: vec!["A".into(), "B".into(), "C".into()],
                d: vec![vec![0., 2., 4.], vec![2., 0., 4.], vec![4., 4., 0.]],
            },
            Method::Upgma,
        )
        .unwrap();
        assert!((total(&u) - 5.0).abs() < 1e-9, "{}", u.to_newick());
    }
    #[test]
    fn prune_unroot_ladderize() {
        let mut t = parse_newick("((A:1,B:1):1,(C:1,(D:1,E:1):1):1);").unwrap();
        let a = t
            .tips(t.root)
            .into_iter()
            .find(|&i| t.nodes[i].name == "A")
            .unwrap();
        t.remove_many(&[a]);
        assert_eq!(tipset(&t), vec!["B", "C", "D", "E"]);
        t.unroot();
        assert_eq!(tipset(&t), vec!["B", "C", "D", "E"]);
        let r = t.root;
        t.ladderize(r, true);
        assert!(parse_newick(&t.to_newick()).is_ok());
    }
    #[test]
    fn fasta_nj() {
        let seqs = parse_fasta(">a\nACGTACGT\n>b\nACGTACGA\n>c\nTCGTTCGA\n>d\nTCGTTCCA");
        let m = distances_from_alignment(&seqs, DistModel::Jc69).unwrap();
        let t = build_tree(&m, Method::NeighborJoining).unwrap();
        assert_eq!(t.tips(t.root).len(), 4);
    }
}
