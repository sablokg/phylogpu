//! PhyloView — a FigTree / iTOL style phylogenetic tree viewer & editor built on egui.
mod annot;
mod pdf;
mod scene;
mod tree;

use eframe::egui::{self, Align2, Color32, FontId, Key, PointerButton, RichText, Sense, Shape, Stroke, Vec2};
use annot::*;
use scene::*;
use scene::Style as SceneStyle;
use std::collections::HashSet;
use tree::*;

/*
Gaurav Sablok
gsablok@proton.me
 */

fn example_newick() -> String {
    let apes = "(((Homo_sapiens:0.0067,Pan_troglodytes:0.0072)95:0.0224,Gorilla_gorilla:0.0088)100:0.0095,Pongo_abelii:0.0183)100:0.0361";
    let monkeys = "((Macaca_mulatta:0.0226,Papio_anubis:0.0215)100:0.0170,Chlorocebus_sabaeus:0.0297)100:0.0261";
    format!("(({apes},{monkeys})100:0.0517,(Mus_musculus:0.0903,Rattus_norvegicus:0.0852)100:0.0963,Bos_taurus:0.1700);")
}

enum Act {
    Reroot(usize), RerootSel, Midpoint, Unroot,
    Rotate(usize), Collapse(usize), ExpandAll,
    OpenRename(usize), DoRename(usize, String),
    Color(usize, [u8; 3]), ClearColor(usize), ClearAllColors, ColorHits([u8; 3]),
    Extract(usize), RemoveSel, Remove(Vec<usize>), PruneHits,
    Ladderize(usize, bool), CopyTips(usize), SelectHits,
    Replace(String, String), UnderscoreToSpace, BatchRename(Vec<(String, String)>),
    AddTree(Tree),
    ColorBy(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Theme { Dark, Light, BlackWhite }
fn apply_theme(ctx: &egui::Context, t: Theme) {
    match t {
        Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
        Theme::Light => ctx.set_visuals(egui::Visuals::light()),
        Theme::BlackWhite => {
            let mut v = egui::Visuals::light();
            v.override_text_color = Some(Color32::BLACK);
            v.extreme_bg_color = Color32::WHITE;
            v.panel_fill = Color32::from_gray(245);
            v.window_fill = Color32::WHITE;
            ctx.set_visuals(v);
        }
    }
}

struct App {
    trees: Vec<Tree>, cur: usize,
    undo: Vec<(usize, Tree)>, redo: Vec<(usize, Tree)>,
    st: Settings,
    zx: f32, zy: f32, pan: Vec2,
    selected: HashSet<usize>,
    find: String, find_hits: HashSet<usize>,
    rep_from: String, rep_to: String, map_text: String,
    rename: Option<(usize, String)>, rename_first: bool,
    ctx_node: Option<usize>, pick_color: [u8; 3],
    status: String, hover_text: String,
    build_open: bool, build_text: String, build_method: Method, build_model: DistModel, build_midpoint: bool, build_err: String,
    paste_open: bool, paste_text: String, help_open: bool,
    ann: Option<Annotations>, theme: Theme, applied_theme: Option<Theme>, color_by_col: usize,
}

impl App {
    fn blank() -> Self {
        App {
            trees: vec![], cur: 0, undo: vec![], redo: vec![], st: Settings::default(),
            zx: 1.0, zy: 1.0, pan: Vec2::ZERO, selected: HashSet::new(),
            find: String::new(), find_hits: HashSet::new(), rep_from: String::new(), rep_to: String::new(), map_text: String::new(),
            rename: None, rename_first: false, ctx_node: None, pick_color: [220, 60, 60],
            status: "Ready".into(), hover_text: String::new(),
            build_open: false, build_text: String::new(), build_method: Method::NeighborJoining, build_model: DistModel::Jc69, build_midpoint: true, build_err: String::new(),
            paste_open: false, paste_text: String::new(), help_open: false,
            ann: None, theme: Theme::Dark, applied_theme: None, color_by_col: 0,
        }
    }
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut a = App::blank();
        apply_theme(&cc.egui_ctx, a.theme);
        a.applied_theme = Some(a.theme);
        if let Some(p) = std::env::args().skip(1).find(|x| !x.starts_with("--")) { a.open_path(std::path::Path::new(&p)); }
        if a.trees.is_empty() { a.load_text(&example_newick()); }
        a
    }
    fn load_annotations(&mut self, p: &std::path::Path) {
        match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|s| Annotations::parse(&s, &p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())) {
            Ok(a) => {
                let (m, n) = self.trees.get(self.cur).map(|t| a.matched(t)).unwrap_or((0, 0));
                self.status = format!("Annotations: {} columns, {}/{} tips matched", a.cols.len(), m, n);
                self.ann = Some(a);
            }
            Err(e) => self.status = format!("Annotation error: {e}"),
        }
    }
    fn fit(&mut self) { self.zx = 1.0; self.zy = 1.0; self.pan = Vec2::ZERO; }
    fn load_text(&mut self, text: &str) {
        match parse_any(text) {
            Ok(v) => {
                self.status = format!("Loaded {} tree(s)", v.len());
                self.trees = v; self.cur = 0; self.undo.clear(); self.redo.clear(); self.selected.clear(); self.fit();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }
    fn open_path(&mut self, p: &std::path::Path) {
        match std::fs::read_to_string(p) {
            Ok(s) => { self.load_text(&s); if !self.status.starts_with("Error") { self.status = format!("{} — {}", p.display(), self.status); } }
            Err(e) => self.status = format!("Cannot read {}: {e}", p.display()),
        }
    }
    fn save_as(&mut self, name: &str, ext: &str, content: String) {
        if let Some(p) = rfd::FileDialog::new().set_file_name(name).add_filter(ext, &[ext]).save_file() {
            self.status = match std::fs::write(&p, content) { Ok(_) => format!("Saved {}", p.display()), Err(e) => format!("Save failed: {e}") };
        }
    }
    fn push_undo(&mut self) {
        self.undo.push((self.cur, self.trees[self.cur].clone()));
        if self.undo.len() > 100 { self.undo.remove(0); }
        self.redo.clear();
    }
    fn edit(&mut self, keep_sel: bool, f: impl FnOnce(&mut Tree)) {
        if self.trees.is_empty() { return; }
        self.push_undo();
        f(&mut self.trees[self.cur]);
        if !keep_sel { self.selected.clear(); }
    }
    fn do_undo(&mut self) {
        if let Some((i, t)) = self.undo.pop() {
            if i < self.trees.len() { let old = std::mem::replace(&mut self.trees[i], t); self.redo.push((i, old)); self.cur = i; self.selected.clear(); }
        }
    }
    fn do_redo(&mut self) {
        if let Some((i, t)) = self.redo.pop() {
            if i < self.trees.len() { let old = std::mem::replace(&mut self.trees[i], t); self.undo.push((i, old)); self.cur = i; self.selected.clear(); }
        }
    }

    fn apply(&mut self, a: Act, ctx: &egui::Context) {
        if self.trees.is_empty() && !matches!(a, Act::AddTree(_)) { return; }
        match a {
            Act::Reroot(n) => self.edit(false, |t| t.reroot_at(n, 0.5)),
            Act::RerootSel => {
                let ids: Vec<usize> = self.selected.iter().copied().collect();
                let t = &self.trees[self.cur];
                match t.mrca(&ids) {
                    Some(m) if m != t.root => self.edit(false, |t| t.reroot_at(m, 0.5)),
                    _ => self.status = "Select an outgroup (a tip or clade that is not the whole tree) first".into(),
                }
            }
            Act::Midpoint => self.edit(false, |t| t.midpoint_root()),
            Act::Unroot => self.edit(false, |t| t.unroot()),
            Act::Rotate(n) => self.edit(true, |t| t.nodes[n].children.reverse()),
            Act::Collapse(n) => self.edit(true, |t| t.nodes[n].collapsed = !t.nodes[n].collapsed),
            Act::ExpandAll => self.edit(true, |t| t.nodes.iter_mut().for_each(|n| n.collapsed = false)),
            Act::OpenRename(n) => {
                let nd = &self.trees[self.cur].nodes[n];
                let s = if nd.children.is_empty() { nd.name.clone() } else { nd.label.clone() };
                self.rename = Some((n, s));
                self.rename_first = true;
            }
            Act::DoRename(n, s) => self.edit(true, |t| { if t.nodes[n].children.is_empty() { t.nodes[n].name = s } else { t.nodes[n].label = s } }),
            Act::Color(n, c) => self.edit(true, |t| t.set_color_clade(n, Some(c))),
            Act::ClearColor(n) => self.edit(true, |t| t.set_color_clade(n, None)),
            Act::ClearAllColors => self.edit(true, |t| { let r = t.root; t.set_color_clade(r, None) }),
            Act::ColorHits(c) => {
                let hits = self.find_hits.clone();
                self.edit(true, |t| for h in hits { if h < t.nodes.len() { t.nodes[h].color = Some(c); } });
            }
            Act::Extract(n) => {
                let sub = self.trees[self.cur].subtree(n);
                self.apply(Act::AddTree(sub), ctx);
            }
            Act::RemoveSel => { let v: Vec<usize> = self.selected.iter().copied().collect(); self.apply(Act::Remove(v), ctx); }
            Act::Remove(v) => {
                let t = &self.trees[self.cur];
                if v.iter().any(|&i| i == t.root) { self.status = "Cannot remove the root".into(); return; }
                self.edit(false, |t| t.remove_many(&v));
            }
            Act::PruneHits => { let v: Vec<usize> = self.find_hits.iter().copied().collect(); self.apply(Act::Remove(v), ctx); }
            Act::Ladderize(n, asc) => self.edit(true, |t| t.ladderize(n, asc)),
            Act::CopyTips(n) => {
                let t = &self.trees[self.cur];
                let names: Vec<&str> = t.tips(n).iter().map(|&i| t.nodes[i].name.as_str()).collect();
                self.status = format!("Copied {} tip names", names.len());
                ctx.copy_text(names.join("\n"));
            }
            Act::SelectHits => self.selected = self.find_hits.clone(),
            Act::Replace(from, to) => {
                if from.is_empty() { return; }
                self.edit(true, |t| for n in t.nodes.iter_mut().filter(|n| n.children.is_empty()) { n.name = n.name.replace(&from, &to); });
            }
            Act::UnderscoreToSpace => self.edit(true, |t| for n in t.nodes.iter_mut().filter(|n| n.children.is_empty()) { n.name = n.name.replace('_', " "); }),
            Act::BatchRename(map) => {
                let m: std::collections::HashMap<String, String> = map.into_iter().collect();
                let mut count = 0;
                self.edit(true, |t| for n in t.nodes.iter_mut().filter(|n| n.children.is_empty()) {
                    if let Some(v) = m.get(&n.name) { n.name = v.clone(); count += 1; }
                });
                self.status = format!("Renamed {count} tips");
            }
            Act::ColorBy(ci) => {
                if let Some(a) = self.ann.clone() { if ci < a.cols.len() { self.edit(true, |t| a.color_by(t, ci)); } }
            }
            Act::AddTree(t) => {
                self.trees.push(t); self.cur = self.trees.len() - 1;
                self.undo.clear(); self.redo.clear(); self.selected.clear(); self.fit();
            }
        }
    }

    fn export_scene(&self) -> (Vec<Prim>, f32, f32) {
        let t = &self.trees[self.cur];
        let mut st = self.st.clone();
        st.auto_hide = false;
        let lay = compute_layout(t, &st);
        let ann = self.ann.as_ref().filter(|a| st.layout != LayoutKind::Unrooted && !a.active().is_empty());
        let tw = ann.map(|a| a.total_width()).unwrap_or(0.0);
        let legw = if st.show_legend && ann.is_some() { 200.0 } else { 0.0 };
        let size = st.tip_font;
        let maxlen = t.nodes.iter().filter(|n| n.children.is_empty()).map(|n| n.name.chars().count()).max().unwrap_or(0);
        let labw = maxlen as f32 * size * 0.55 + 30.0;
        let (w, h, xf) = match st.layout {
            LayoutKind::Rectangular | LayoutKind::Slanted => {
                let h = (lay.ntip as f32 * size * 1.35).max(300.0) + 60.0;
                (1000.0 + legw, h, Xf { origin: egui::pos2(40.0, 30.0), pan: Vec2::ZERO, sx: 1000.0 - 40.0 - labw - tw, sy: h - 60.0 })
            }
            _ => { let r = 450.0; let side = 2.0 * (r + labw + tw) + 60.0; (side + legw, side, Xf { origin: egui::pos2(side / 2.0, side / 2.0), pan: Vec2::ZERO, sx: r, sy: r }) }
        };
        let mono = self.theme == Theme::BlackWhite;
        let sty = SceneStyle { line: Color32::BLACK, text: Color32::BLACK, hl: Color32::from_rgb(255, 140, 0), find: Color32::from_rgb(230, 50, 50), mono };
        let view = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
        let sel = vec![false; t.nodes.len()];
        let sc = build_scene(t, &lay, &xf, &st, &sty, view, &sel, &self.find_hits, &t.tip_counts(), ann);
        let mut prims = sc.prims;
        if legw > 0.0 { prims.extend(ann.unwrap().legend_prims(egui::pos2(w - legw + 10.0, 20.0), mono, Color32::BLACK)); }
        (prims, w, h)
    }
    fn export_svg(&self) -> String { let (p, w, h) = self.export_scene(); to_svg(&p, w, h) }
    fn save_bytes(&mut self, name: &str, ext: &str, data: Vec<u8>) {
        if let Some(p) = rfd::FileDialog::new().set_file_name(name).add_filter(ext, &[ext]).save_file() {
            self.status = match std::fs::write(&p, data) { Ok(_) => format!("Saved {}", p.display()), Err(e) => format!("Save failed: {e}") };
        }
    }

    // ------------------------------------------------------------- canvas
    fn canvas(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        if self.trees.is_empty() {
            painter.text(rect.center(), Align2::CENTER_CENTER, "Drop a tree file here, or File ▸ Open…", FontId::proportional(18.0), ui.visuals().weak_text_color());
            return;
        }
        let tree = &self.trees[self.cur];
        let st = &self.st;
        let mono = self.theme == Theme::BlackWhite;
        let ann = self.ann.as_ref().filter(|_| st.layout != LayoutKind::Unrooted);
        let lay = compute_layout(tree, st);
        let rectlike = matches!(st.layout, LayoutKind::Rectangular | LayoutKind::Slanted);
        let maxlen = tree.nodes.iter().filter(|n| n.children.is_empty()).map(|n| n.name.chars().count()).max().unwrap_or(0);
        let tw = ann.map(|a| a.total_width()).unwrap_or(0.0);
        let labw = (if st.show_tip_labels { maxlen as f32 * st.tip_font * 0.55 + st.label_gap + 10.0 } else { 10.0 }) + tw;
        let (origin, bsx, bsy) = if rectlike {
            (egui::pos2(rect.min.x + 30.0, rect.min.y + 20.0), (rect.width() - 30.0 - labw).max(50.0), (rect.height() - 70.0).max(50.0))
        } else {
            let m = rect.width().min(rect.height());
            let r = (m / 2.0 - labw.min(m * 0.3) - 10.0).max(60.0);
            (rect.center(), r, r)
        };
        if !rectlike { self.zy = self.zx; }
        if resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Middle) { self.pan += resp.drag_delta(); }
        if resp.hovered() {
            let (scroll, zd, mods, ptr) = ui.input(|i| (i.raw_scroll_delta, i.zoom_delta(), i.modifiers, i.pointer.hover_pos()));
            if st.wheel_pans && !mods.ctrl {
                self.pan += scroll;
            } else {
                let s = scroll.y + if mods.shift { scroll.x } else { 0.0 };
                let f = zd * (s * 0.0015).exp();
                if (f - 1.0).abs() > 1e-4 {
                    if let Some(pp) = ptr {
                        let rel = pp - origin;
                        let (fx, fy) = if rectlike { if mods.shift { (f, 1.0) } else if mods.alt { (1.0, f) } else { (f, f) } } else { (f, f) };
                        let nzx = (self.zx * fx).clamp(0.05, 3000.0);
                        let nzy = (self.zy * fy).clamp(0.05, 3000.0);
                        let (rx, ry) = (nzx / self.zx, nzy / self.zy);
                        self.pan.x = rel.x - (rel.x - self.pan.x) * rx;
                        self.pan.y = rel.y - (rel.y - self.pan.y) * ry;
                        self.zx = nzx; self.zy = nzy;
                    }
                }
            }
        }
        let xf = Xf { origin, pan: self.pan, sx: bsx * self.zx, sy: if rectlike { bsy * self.zy } else { bsx * self.zx } };

        let n = tree.nodes.len();
        let mut sel = vec![false; n];
        for &u in &tree.preorder(tree.root) {
            sel[u] = self.selected.contains(&u) || tree.nodes[u].parent.map(|p| sel[p]).unwrap_or(false);
        }
        let counts = tree.tip_counts();
        let sty = SceneStyle { line: ui.visuals().text_color(), text: ui.visuals().text_color(), hl: Color32::from_rgb(255, 150, 0), find: Color32::from_rgb(235, 60, 60), mono };
        let sty = if mono { SceneStyle { hl: Color32::from_gray(110), find: Color32::BLACK, ..sty } } else { sty };
        let sc = build_scene(tree, &lay, &xf, st, &sty, rect, &sel, &self.find_hits, &counts, ann);

        // hit testing
        let ptr = resp.hover_pos();
        let hover = ptr.and_then(|pp| {
            sc.pts.iter().filter(|(_, q)| q.distance(pp) < 8.0)
                .min_by(|a, b| a.1.distance(pp).partial_cmp(&b.1.distance(pp)).unwrap()).map(|x| x.0)
                .or_else(|| sc.rects.iter().find(|(_, r)| r.contains(pp)).map(|x| x.0))
        });
        let mods = ui.input(|i| i.modifiers);
        let multi = mods.command || mods.shift;
        if resp.clicked() {
            match hover {
                Some(h) => {
                    if multi { if !self.selected.remove(&h) { self.selected.insert(h); } }
                    else { self.selected.clear(); self.selected.insert(h); }
                }
                None => if !multi { self.selected.clear(); },
            }
        }
        if resp.double_clicked() {
            if let Some(h) = hover {
                if tree.nodes[h].children.is_empty() { acts.push(Act::OpenRename(h)); } else { acts.push(Act::Collapse(h)); }
            }
        }
        if resp.secondary_clicked() { self.ctx_node = hover; }
        let ctxn = self.ctx_node;
        let pick = &mut self.pick_color;
        resp.context_menu(|ui| match ctxn {
            Some(n) if n < tree.nodes.len() => node_menu(ui, tree, n, pick, acts),
            _ => {
                ui.label(RichText::new("Tree").strong());
                if ui.button("Midpoint root").clicked() { acts.push(Act::Midpoint); ui.close_menu(); }
                if ui.button("Ladderize (small first)").clicked() { acts.push(Act::Ladderize(tree.root, true)); ui.close_menu(); }
                if ui.button("Ladderize (large first)").clicked() { acts.push(Act::Ladderize(tree.root, false)); ui.close_menu(); }
                if ui.button("Expand all clades").clicked() { acts.push(Act::ExpandAll); ui.close_menu(); }
            }
        });

        // paint
        paint_scene(&painter, &sc);
        if st.show_legend { if let Some(a) = ann { paint_scene(&painter, &Scene { prims: a.legend_prims(egui::pos2(rect.max.x - 190.0, rect.min.y + 10.0), mono, sty.text), ..Default::default() }); } }
        if let Some(h) = hover {
            let nd = &tree.nodes[h];
            let txt = if nd.children.is_empty() {
                let mut s = format!("{}\nbranch length: {}", nd.name, nd.len);
                if let Some(a) = ann { if let Some(r) = a.row(&nd.name) { for (c, v) in a.cols.iter().zip(r) { s.push_str(&format!("\n{}: {}", c.name, v)); } } }
                s
            } else {
                format!("{}{} tips\nbranch length: {}", if nd.label.is_empty() { String::new() } else { format!("label: {}\n", nd.label) }, counts[h], nd.len)
            };
            self.hover_text = txt.replace('\n', "  |  ");
            if !resp.dragged() { resp.clone().on_hover_text_at_pointer(txt); }
        } else { self.hover_text.clear(); }

        // scale bar
        if st.show_scale && tree.has_len && st.mode == BranchMode::Proportional {
            let unit = xf.sx / lay.world_len as f32;
            if unit.is_finite() && unit > 0.0 {
                let raw = 100.0 / unit;
                let p10 = 10f32.powf(raw.log10().floor());
                let m = raw / p10;
                let nice = p10 * if m < 1.5 { 1.0 } else if m < 3.5 { 2.0 } else if m < 7.5 { 5.0 } else { 10.0 };
                let len = nice * unit;
                let a = egui::pos2(rect.min.x + 20.0, rect.max.y - 22.0);
                let b = a + egui::vec2(len, 0.0);
                let c = ui.visuals().text_color();
                painter.line_segment([a, b], Stroke::new(1.5, c));
                painter.line_segment([a - egui::vec2(0.0, 4.0), a + egui::vec2(0.0, 4.0)], Stroke::new(1.5, c));
                painter.line_segment([b - egui::vec2(0.0, 4.0), b + egui::vec2(0.0, 4.0)], Stroke::new(1.5, c));
                let label = format!("{:.6}", nice); let label = label.trim_end_matches('0').trim_end_matches('.').to_string();
                painter.text(egui::pos2(a.x, a.y - 6.0), Align2::LEFT_BOTTOM, label, FontId::proportional(12.0), c);
            }
        }
    }
}

fn paint_scene(painter: &egui::Painter, sc: &Scene) {
    for p in &sc.prims {
        match p {
            Prim::Line(a, b, c, w) => { painter.line_segment([*a, *b], Stroke::new(*w, *c)); }
            Prim::Poly(pts, c, w) => { painter.add(Shape::line(pts.clone(), Stroke::new(*w, *c))); }
            Prim::Fill(pts, c) => { painter.add(Shape::convex_polygon(pts.clone(), *c, Stroke::NONE)); }
            Prim::Dot(q, r, c) => { painter.circle_filled(*q, *r, *c); }
            Prim::Text { p, s, size, c, ang, end } => {
                let g = painter.layout_no_wrap(s.clone(), FontId::proportional(*size), *c);
                let (w, h) = (g.size().x, g.size().y);
                let d = egui::vec2(ang.cos(), ang.sin());
                let nrm = egui::vec2(-ang.sin(), ang.cos());
                let mut o = *p - nrm * (h / 2.0);
                if *end { o -= d * w; }
                if *ang == 0.0 { painter.galley(o, g, *c); }
                else { painter.add(Shape::Text(egui::epaint::TextShape::new(o, g, *c).with_angle(*ang))); }
            }
        }
    }
}

fn node_menu(ui: &mut egui::Ui, t: &Tree, n: usize, col: &mut [u8; 3], acts: &mut Vec<Act>) {
    let nd = &t.nodes[n];
    let leaf = nd.children.is_empty();
    let title = if leaf { format!("Tip: {}", nd.name) } else { format!("Clade — {} tips", t.tips(n).len()) };
    ui.label(RichText::new(title).strong());
    ui.separator();
    if n != t.root && ui.button("⤴ Reroot here").clicked() { acts.push(Act::Reroot(n)); ui.close_menu(); }
    if !leaf {
        if ui.button("🔄 Rotate (flip children)").clicked() { acts.push(Act::Rotate(n)); ui.close_menu(); }
        if ui.button(if nd.collapsed { "Expand clade" } else { "Collapse clade" }).clicked() { acts.push(Act::Collapse(n)); ui.close_menu(); }
        if ui.button("Ladderize clade ↑").clicked() { acts.push(Act::Ladderize(n, true)); ui.close_menu(); }
        if ui.button("Ladderize clade ↓").clicked() { acts.push(Act::Ladderize(n, false)); ui.close_menu(); }
        if ui.button("Extract as new tree").clicked() { acts.push(Act::Extract(n)); ui.close_menu(); }
        if ui.button("Copy tip names").clicked() { acts.push(Act::CopyTips(n)); ui.close_menu(); }
    }
    if ui.button("✏ Rename…").clicked() { acts.push(Act::OpenRename(n)); ui.close_menu(); }
    ui.horizontal(|ui| {
        ui.label("Colour clade:");
        if ui.color_edit_button_srgb(col).changed() { acts.push(Act::Color(n, *col)); }
    });
    if ui.button("Clear colour").clicked() { acts.push(Act::ClearColor(n)); ui.close_menu(); }
    if n != t.root && ui.button("🗑 Remove (prune)").clicked() { acts.push(Act::Remove(vec![n])); ui.close_menu(); }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        let mut acts: Vec<Act> = vec![];
        if self.applied_theme != Some(self.theme) { apply_theme(ctx, self.theme); self.applied_theme = Some(self.theme); }
        // dropped files
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(p) = f.path {
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if ext == "csv" || ext == "tsv" { self.load_annotations(&p); } else { self.open_path(&p); }
            }
            else if let Some(b) = f.bytes { let s = String::from_utf8_lossy(&b).to_string(); self.load_text(&s); }
        }
        // shortcuts
        use egui::{KeyboardShortcut as KS, Modifiers as M};
        if ctx.input_mut(|i| i.consume_shortcut(&KS::new(M::COMMAND | M::SHIFT, Key::Z))) { self.do_redo(); }
        if ctx.input_mut(|i| i.consume_shortcut(&KS::new(M::COMMAND, Key::Z))) { self.do_undo(); }
        if ctx.input_mut(|i| i.consume_shortcut(&KS::new(M::COMMAND, Key::Y))) { self.do_redo(); }
        if ctx.input_mut(|i| i.consume_shortcut(&KS::new(M::COMMAND, Key::O))) {
            if let Some(p) = rfd::FileDialog::new().pick_file() { self.open_path(&p); }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KS::new(M::COMMAND, Key::S))) && !self.trees.is_empty() {
            let s = self.trees[self.cur].to_newick(); self.save_as("tree.nwk", "nwk", s);
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(Key::F)) { self.fit(); }
            if ctx.input(|i| i.key_pressed(Key::Delete)) && !self.selected.is_empty() { acts.push(Act::RemoveSel); }
        }
        // search hits
        self.find_hits.clear();
        if !self.find.is_empty() && !self.trees.is_empty() {
            let q = self.find.to_lowercase();
            for (i, n) in self.trees[self.cur].nodes.iter().enumerate() {
                let s = if n.children.is_empty() { &n.name } else { &n.label };
                if !s.is_empty() && s.to_lowercase().contains(&q) { self.find_hits.insert(i); }
            }
        }

        // ------------------------------------------------------------ menu
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…   Ctrl+O").clicked() { ui.close_menu(); if let Some(p) = rfd::FileDialog::new().pick_file() { self.open_path(&p); } }
                    if ui.button("Paste Newick / NEXUS…").clicked() { self.paste_open = true; ui.close_menu(); }
                    if ui.button("Load example").clicked() { let e = example_newick(); self.load_text(&e); ui.close_menu(); }
                    if ui.button("Load annotation table (CSV/TSV)…").clicked() { ui.close_menu(); if let Some(p) = rfd::FileDialog::new().add_filter("Table", &["csv", "tsv", "txt"]).pick_file() { self.load_annotations(&p); } }
                    ui.separator();
                    let has = !self.trees.is_empty();
                    ui.add_enabled_ui(has, |ui| {
                        if ui.button("Save Newick…   Ctrl+S").clicked() { let s = self.trees[self.cur].to_newick(); self.save_as("tree.nwk", "nwk", s); ui.close_menu(); }
                        if ui.button("Save NEXUS…").clicked() { let s = self.trees[self.cur].to_nexus(); self.save_as("tree.nex", "nex", s); ui.close_menu(); }
                        if ui.button("Export PDF…").clicked() { let (p, w, h) = self.export_scene(); self.save_bytes("tree.pdf", "pdf", pdf::to_pdf(&p, w, h)); ui.close_menu(); }
                        if ui.button("Export SVG…").clicked() { let s = self.export_svg(); self.save_as("tree.svg", "svg", s); ui.close_menu(); }
                        if ui.button("Copy Newick to clipboard").clicked() { ctx.copy_text(self.trees[self.cur].to_newick()); self.status = "Newick copied".into(); ui.close_menu(); }
                    });
                    ui.separator();
                    if ui.button("Quit").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo   Ctrl+Z")).clicked() { self.do_undo(); ui.close_menu(); }
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo   Ctrl+Y")).clicked() { self.do_redo(); ui.close_menu(); }
                    ui.separator();
                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Remove selected   Del")).clicked() { acts.push(Act::RemoveSel); ui.close_menu(); }
                });
                ui.menu_button("Tree", |ui| {
                    if ui.button("Build tree from alignment / distances…").clicked() { self.build_open = true; ui.close_menu(); }
                    ui.separator();
                    if ui.button("Midpoint root").clicked() { acts.push(Act::Midpoint); ui.close_menu(); }
                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Root on selection (outgroup)")).clicked() { acts.push(Act::RerootSel); ui.close_menu(); }
                    if ui.button("Unroot (trifurcate root)").clicked() { acts.push(Act::Unroot); ui.close_menu(); }
                    ui.separator();
                    if self.trees.len() > 0 {
                        let r = self.trees[self.cur].root;
                        if ui.button("Ladderize — small clades first").clicked() { acts.push(Act::Ladderize(r, true)); ui.close_menu(); }
                        if ui.button("Ladderize — large clades first").clicked() { acts.push(Act::Ladderize(r, false)); ui.close_menu(); }
                    }
                    if ui.button("Expand all clades").clicked() { acts.push(Act::ExpandAll); ui.close_menu(); }
                    if ui.button("Clear all colours").clicked() { acts.push(Act::ClearAllColors); ui.close_menu(); }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Fit to window   F").clicked() { self.fit(); ui.close_menu(); }
                    ui.separator();
                    for (k, l) in [(LayoutKind::Rectangular, "Rectangular"), (LayoutKind::Slanted, "Slanted"), (LayoutKind::Circular, "Circular"), (LayoutKind::Unrooted, "Unrooted (equal angle)")] {
                        if ui.radio_value(&mut self.st.layout, k, l).clicked() { self.fit(); }
                    }
                    ui.separator();
                    ui.label("Theme");
                    ui.radio_value(&mut self.theme, Theme::Dark, "Dark");
                    ui.radio_value(&mut self.theme, Theme::Light, "Light");
                    ui.radio_value(&mut self.theme, Theme::BlackWhite, "Black & white");
                });
                ui.menu_button("Help", |ui| { if ui.button("Controls & shortcuts").clicked() { self.help_open = true; ui.close_menu(); } });
                ui.separator();
                for (k, l) in [(LayoutKind::Rectangular, "Rectangular"), (LayoutKind::Circular, "Circular"), (LayoutKind::Unrooted, "Unrooted")] {
                    if ui.selectable_label(self.st.layout == k, l).clicked() { self.st.layout = k; self.fit(); }
                }
                ui.separator();
                if self.trees.len() > 1 {
                    if ui.button("◀").clicked() && self.cur > 0 { self.cur -= 1; self.selected.clear(); self.undo.clear(); self.redo.clear(); self.fit(); }
                    let name = self.trees[self.cur].name.clone();
                    egui::ComboBox::from_id_salt("treesel").selected_text(format!("{} ({}/{})", name, self.cur + 1, self.trees.len())).show_ui(ui, |ui| {
                        for i in 0..self.trees.len() {
                            if ui.selectable_label(i == self.cur, self.trees[i].name.clone()).clicked() { self.cur = i; self.selected.clear(); self.undo.clear(); self.redo.clear(); self.fit(); }
                        }
                    });
                    if ui.button("▶").clicked() && self.cur + 1 < self.trees.len() { self.cur += 1; self.selected.clear(); self.undo.clear(); self.redo.clear(); self.fit(); }
                }
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if !self.hover_text.is_empty() { ui.separator(); ui.label(&self.hover_text); }
                if !self.selected.is_empty() { ui.separator(); ui.label(format!("{} selected", self.selected.len())); }
            });
        });
        egui::SidePanel::left("side").default_width(270.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.side(ui, &mut acts));
        });
        egui::CentralPanel::default().frame(egui::Frame::none().fill(ctx.style().visuals.extreme_bg_color)).show(ctx, |ui| self.canvas(ui, &mut acts));

        // ----------------------------------------------------------- windows
        if self.rename.is_some() {
            let mut apply = None;
            let mut close = false;
            let first = std::mem::take(&mut self.rename_first);
            if let Some((n, text)) = self.rename.as_mut() {
                egui::Window::new("Rename").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    let r = ui.text_edit_singleline(text);
                    if first { r.request_focus(); }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || enter { apply = Some((*n, text.clone())); }
                        if ui.button("Cancel").clicked() { close = true; }
                    });
                });
            }
            if let Some((n, s)) = apply { acts.push(Act::DoRename(n, s)); self.rename = None; } else if close { self.rename = None; }
        }
        if self.paste_open {
            let mut open = true;
            let mut load = false;
            egui::Window::new("Paste tree").open(&mut open).default_width(480.0).show(ctx, |ui| {
                ui.label("Newick, NEXUS or PhyloXML text:");
                ui.add(egui::TextEdit::multiline(&mut self.paste_text).font(egui::TextStyle::Monospace).desired_rows(10).desired_width(f32::INFINITY));
                if ui.button("Load").clicked() { load = true; }
            });
            if load { let t = self.paste_text.clone(); self.load_text(&t); open = false; }
            self.paste_open = open;
        }
        if self.build_open {
            let mut open = true;
            let mut build = false;
            egui::Window::new("Build tree").open(&mut open).default_width(520.0).show(ctx, |ui| {
                ui.label("Paste a FASTA alignment (aligned, equal length) or a PHYLIP distance matrix (square or lower-triangular).");
                ui.add(egui::TextEdit::multiline(&mut self.build_text).font(egui::TextStyle::Monospace).desired_rows(12).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("Method:");
                    ui.radio_value(&mut self.build_method, Method::NeighborJoining, "Neighbor-Joining");
                    ui.radio_value(&mut self.build_method, Method::Upgma, "UPGMA");
                });
                ui.horizontal(|ui| {
                    ui.label("Distance (alignments):");
                    ui.radio_value(&mut self.build_model, DistModel::PDistance, "p-distance");
                    ui.radio_value(&mut self.build_model, DistModel::Jc69, "Jukes–Cantor");
                    ui.radio_value(&mut self.build_model, DistModel::Poisson, "Poisson (protein)");
                });
                ui.checkbox(&mut self.build_midpoint, "Midpoint-root result");
                ui.horizontal(|ui| {
                    if ui.button("Load file…").clicked() { if let Some(p) = rfd::FileDialog::new().pick_file() { if let Ok(s) = std::fs::read_to_string(p) { self.build_text = s; } } }
                    if ui.button("Build").clicked() { build = true; }
                });
                if !self.build_err.is_empty() { ui.colored_label(Color32::LIGHT_RED, &self.build_err); }
            });
            if build {
                let txt = self.build_text.trim().to_string();
                let res: Result<Tree, String> = (|| {
                    let m = if txt.starts_with('>') { distances_from_alignment(&parse_fasta(&txt), self.build_model)? } else { parse_phylip_matrix(&txt)? };
                    let mut t = build_tree(&m, self.build_method)?;
                    if self.build_midpoint { t.midpoint_root(); }
                    Ok(t)
                })();
                match res {
                    Ok(t) => { acts.push(Act::AddTree(t)); self.build_err.clear(); open = false; }
                    Err(e) => self.build_err = e,
                }
            }
            self.build_open = open;
        }
        if self.help_open {
            egui::Window::new("Controls").open(&mut self.help_open).show(ctx, |ui| {
                ui.label("Mouse wheel — zoom at cursor (Shift: horizontal only, Alt: vertical only)\nDrag — pan\nClick — select clade   Ctrl/Shift+click — multi-select\nDouble-click tip — rename   Double-click node — collapse / expand\nRight-click node — reroot, rotate, collapse, colour, extract, prune…\n\nCtrl+O open · Ctrl+S save Newick · Ctrl+Z / Ctrl+Y undo / redo\nF fit to window · Del remove selection\nDrop files onto the window to open them.");
            });
        }

        for a in acts { self.apply(a, ctx); }
    }
}

impl App {
    fn side(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let rectlike = matches!(self.st.layout, LayoutKind::Rectangular | LayoutKind::Slanted);
        egui::CollapsingHeader::new("Theme").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.theme, Theme::Dark, "Dark");
                ui.selectable_value(&mut self.theme, Theme::Light, "Light");
                ui.selectable_value(&mut self.theme, Theme::BlackWhite, "Black & white");
            });
        });
        egui::CollapsingHeader::new("Layout").default_open(true).show(ui, |ui| {
            let before = self.st.layout;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.st.layout, LayoutKind::Rectangular, "Rectangular");
                ui.selectable_value(&mut self.st.layout, LayoutKind::Slanted, "Slanted");
                ui.selectable_value(&mut self.st.layout, LayoutKind::Circular, "Circular");
                ui.selectable_value(&mut self.st.layout, LayoutKind::Unrooted, "Unrooted");
            });
            if before != self.st.layout { self.fit(); }
            egui::ComboBox::from_label("Branches").selected_text(match self.st.mode { BranchMode::Proportional => "Proportional", BranchMode::Cladogram => "Cladogram (tips aligned)", BranchMode::Equal => "Equal length" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.st.mode, BranchMode::Proportional, "Proportional");
                ui.selectable_value(&mut self.st.mode, BranchMode::Cladogram, "Cladogram (tips aligned)");
                ui.selectable_value(&mut self.st.mode, BranchMode::Equal, "Equal length");
            });
            if self.st.layout == LayoutKind::Circular { ui.add(egui::Slider::new(&mut self.st.arc_deg, 60.0..=360.0).text("Arc °")); }
            if matches!(self.st.layout, LayoutKind::Circular | LayoutKind::Unrooted) { ui.add(egui::Slider::new(&mut self.st.rot_deg, -180.0..=180.0).text("Rotate °")); }
            ui.checkbox(&mut self.st.align_tips, "Align tip labels");
            ui.checkbox(&mut self.st.show_scale, "Scale bar");
        });
        egui::CollapsingHeader::new("Zoom").default_open(true).show(ui, |ui| {
            ui.add(egui::Slider::new(&mut self.zx, 0.05..=200.0).logarithmic(true).text(if rectlike { "Horizontal" } else { "Zoom" }));
            if rectlike { ui.add(egui::Slider::new(&mut self.zy, 0.05..=500.0).logarithmic(true).text("Vertical (expansion)")); }
            ui.horizontal(|ui| {
                if ui.button("Fit").clicked() { self.fit(); }
                if ui.button("Zoom in").clicked() { self.zx *= 1.5; if rectlike { self.zy *= 1.5; } }
                if ui.button("Zoom out").clicked() { self.zx /= 1.5; if rectlike { self.zy /= 1.5; } }
            });
            ui.checkbox(&mut self.st.wheel_pans, "Wheel pans (Ctrl+wheel zooms)");
        });
        egui::CollapsingHeader::new("Tip labels").default_open(true).show(ui, |ui| {
            ui.checkbox(&mut self.st.show_tip_labels, "Show");
            ui.add(egui::Slider::new(&mut self.st.tip_font, 6.0..=40.0).text("Font size"));
            ui.add(egui::Slider::new(&mut self.st.label_gap, 0.0..=40.0).text("Offset"));
            ui.checkbox(&mut self.st.auto_hide, "Hide when too dense");
            ui.checkbox(&mut self.st.show_tip_dots, "Tip dots");
        });
        egui::CollapsingHeader::new("Annotations").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Load table…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Table", &["csv", "tsv", "txt"]).pick_file() { self.load_annotations(&p); }
                }
                if self.ann.is_some() && ui.button("Clear").clicked() { self.ann = None; }
            });
            if self.ann.is_none() { ui.label("CSV/TSV: first column = tip name, then one column per track. Circular layout draws rings."); }
            let matched = self.ann.as_ref().zip(self.trees.get(self.cur)).map(|(a, t)| a.matched(t));
            if let Some(a) = self.ann.as_mut() {
                if let Some((m, n)) = matched { ui.label(format!("{} — {}/{} tips matched", a.file, m, n)); }
                for i in 0..a.cols.len() {
                    ui.push_id(i, |ui| {
                        let c = &mut a.cols[i];
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut c.enabled, c.name.clone());
                            let kinds: &[TrackKind] = if c.numeric { &[TrackKind::Heatmap, TrackKind::Bar, TrackKind::ColorStrip, TrackKind::Text] } else { &[TrackKind::ColorStrip, TrackKind::Text] };
                            egui::ComboBox::from_id_salt("kind").selected_text(c.kind.name()).width(100.0).show_ui(ui, |ui| {
                                for &k in kinds { ui.selectable_value(&mut c.kind, k, k.name()); }
                            });
                        });
                        if c.enabled { ui.add(egui::Slider::new(&mut c.width, 4.0..=200.0).text("width")); }
                    });
                }
                if !a.cols.is_empty() {
                    ui.separator();
                    self.color_by_col = self.color_by_col.min(a.cols.len() - 1);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("colorby").selected_text(a.cols[self.color_by_col].name.clone()).show_ui(ui, |ui| {
                            for i in 0..a.cols.len() { ui.selectable_value(&mut self.color_by_col, i, a.cols[i].name.clone()); }
                        });
                        if ui.button("Colour branches").clicked() { acts.push(Act::ColorBy(self.color_by_col)); }
                    });
                }
                ui.checkbox(&mut self.st.show_legend, "Show legend");
            }
        });
        egui::CollapsingHeader::new("Node & branch labels").show(ui, |ui| {
            ui.checkbox(&mut self.st.show_node_labels, "Node labels / support");
            ui.add(egui::Slider::new(&mut self.st.node_font, 6.0..=30.0).text("Node font"));
            ui.checkbox(&mut self.st.show_branch_len, "Branch lengths");
            ui.add(egui::Slider::new(&mut self.st.branch_font, 6.0..=30.0).text("Branch font"));
            ui.add(egui::Slider::new(&mut self.st.decimals, 0..=8).text("Decimals"));
            ui.checkbox(&mut self.st.show_node_dots, "Node dots");
            ui.add(egui::Slider::new(&mut self.st.dot, 1.0..=10.0).text("Dot size"));
            ui.add(egui::Slider::new(&mut self.st.line_w, 0.5..=6.0).text("Line width"));
        });
        egui::CollapsingHeader::new("Find, colour & rename").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| { ui.label("🔍"); ui.text_edit_singleline(&mut self.find); });
            if !self.find.is_empty() {
                ui.label(format!("{} match(es)", self.find_hits.len()));
                ui.horizontal(|ui| {
                    if ui.button("Select").clicked() { acts.push(Act::SelectHits); }
                    ui.color_edit_button_srgb(&mut self.pick_color);
                    if ui.button("Colour").clicked() { acts.push(Act::ColorHits(self.pick_color)); }
                    if ui.button("Prune").clicked() { acts.push(Act::PruneHits); }
                });
            }
            ui.separator();
            ui.label("Replace in tip names:");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.rep_from).desired_width(80.0));
                ui.label("→");
                ui.add(egui::TextEdit::singleline(&mut self.rep_to).desired_width(80.0));
                if ui.button("Apply").clicked() { acts.push(Act::Replace(self.rep_from.clone(), self.rep_to.clone())); }
            });
            if ui.button("Underscores → spaces").clicked() { acts.push(Act::UnderscoreToSpace); }
            ui.separator();
            ui.label("Batch rename (old,new per line):");
            ui.add(egui::TextEdit::multiline(&mut self.map_text).desired_rows(4).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if ui.button("Apply mapping").clicked() {
                let map: Vec<(String, String)> = self.map_text.lines().filter_map(|l| {
                    let mut it = l.splitn(2, |c| c == ',' || c == '\t' || c == ';');
                    let (a, b) = (it.next()?.trim(), it.next()?.trim());
                    if a.is_empty() { None } else { Some((a.to_string(), b.to_string())) }
                }).collect();
                acts.push(Act::BatchRename(map));
            }
        });
        egui::CollapsingHeader::new("Root & order").default_open(true).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Midpoint root").clicked() { acts.push(Act::Midpoint); }
                if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Root on selection")).clicked() { acts.push(Act::RerootSel); }
                if ui.button("Unroot").clicked() { acts.push(Act::Unroot); }
            });
            if !self.trees.is_empty() {
                let r = self.trees[self.cur].root;
                ui.horizontal(|ui| {
                    if ui.button("Ladderize ↑").clicked() { acts.push(Act::Ladderize(r, true)); }
                    if ui.button("Ladderize ↓").clicked() { acts.push(Act::Ladderize(r, false)); }
                });
            }
        });
        if !self.trees.is_empty() {
            egui::CollapsingHeader::new("Tree info").show(ui, |ui| {
                let s = self.trees[self.cur].stats();
                ui.label(format!("Tips: {}\nInternal nodes: {}\nPolytomies: {}\nRooted (bifurcating root): {}", s.tips, s.internal, s.polytomies, s.rooted));
                if self.trees[self.cur].has_len { ui.label(format!("Total branch length: {:.5}\nMax root-to-tip: {:.5}", s.total_len, s.max_depth)); } else { ui.label("No branch lengths"); }
            });
        }
    }
}

fn headless(args: &[String]) {
    let val = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let mut app = App::blank();
    let Some(input) = args.iter().skip(1).find(|a| !a.starts_with("--") && Some(a.as_str()) != val("--export").as_deref() && Some(a.as_str()) != val("--annot").as_deref() && Some(a.as_str()) != val("--layout").as_deref() && Some(a.as_str()) != val("--theme").as_deref() && Some(a.as_str()) != val("--kinds").as_deref()) else { eprintln!("usage: phyloview TREE --export out.{{pdf,svg}} [--annot table.csv] [--layout rect|circular|unrooted|slanted] [--theme bw|dark|light]"); return; };
    app.open_path(std::path::Path::new(input));
    if let Some(a) = val("--annot") { app.load_annotations(std::path::Path::new(&a)); }
    app.st.layout = match val("--layout").as_deref() { Some("circular") => LayoutKind::Circular, Some("unrooted") => LayoutKind::Unrooted, Some("slanted") => LayoutKind::Slanted, _ => LayoutKind::Rectangular };
    app.theme = match val("--theme").as_deref() { Some("bw") => Theme::BlackWhite, Some("light") => Theme::Light, _ => Theme::Dark };
    if let (Some(k), Some(a)) = (val("--kinds"), app.ann.as_mut()) {
        for (c, name) in a.cols.iter_mut().zip(k.split(',')) {
            c.kind = match name { "bar" => TrackKind::Bar, "text" => TrackKind::Text, "strip" => TrackKind::ColorStrip, _ => c.kind };
            if c.kind == TrackKind::Bar { c.width = 60.0; } if c.kind == TrackKind::Text { c.width = 50.0; }
        }
    }
    if app.trees.is_empty() { eprintln!("{}", app.status); return; }
    let out = val("--export").unwrap();
    let (p, w, h) = app.export_scene();
    let data = if out.ends_with(".pdf") { pdf::to_pdf(&p, w, h) } else { to_svg(&p, w, h).into_bytes() };
    std::fs::write(&out, data).expect("write failed");
    println!("wrote {out}");
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--export") { headless(&args); return Ok(()); }
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1300.0, 820.0]).with_drag_and_drop(true).with_title("PhyloView"),
        ..Default::default()
    };
    eframe::run_native("PhyloView", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
