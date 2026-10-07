//! Layout algorithms and a renderer-independent scene description
//! (used both for on-screen painting and SVG export).
use crate::annot::*;
use crate::tree::*;
use eframe::egui::{pos2, vec2, Color32, Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::f32::consts::{PI, TAU};

/*
Gaurav Sablok
gsablok@proton.me
 */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutKind {
    Rectangular,
    Slanted,
    Circular,
    Unrooted,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchMode {
    Proportional,
    Cladogram,
    Equal,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub layout: LayoutKind,
    pub mode: BranchMode,
    pub arc_deg: f32,
    pub rot_deg: f32,
    pub align_tips: bool,
    pub show_tip_labels: bool,
    pub tip_font: f32,
    pub auto_hide: bool,
    pub label_gap: f32,
    pub show_tip_dots: bool,
    pub show_node_dots: bool,
    pub dot: f32,
    pub show_node_labels: bool,
    pub node_font: f32,
    pub show_branch_len: bool,
    pub branch_font: f32,
    pub decimals: usize,
    pub line_w: f32,
    pub show_scale: bool,
    pub wheel_pans: bool,
    pub show_legend: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            layout: LayoutKind::Rectangular,
            mode: BranchMode::Proportional,
            arc_deg: 360.0,
            rot_deg: 0.0,
            align_tips: false,
            show_tip_labels: true,
            tip_font: 13.0,
            auto_hide: true,
            label_gap: 6.0,
            show_tip_dots: false,
            show_node_dots: false,
            dot: 3.0,
            show_node_labels: false,
            node_font: 10.0,
            show_branch_len: false,
            branch_font: 10.0,
            decimals: 4,
            line_w: 1.5,
            show_scale: true,
            wheel_pans: false,
            show_legend: true,
        }
    }
}

pub struct Layout {
    pub pos: Vec<Vec2>,  // world position of every node
    pub r: Vec<f32>,     // polar radius (circular)
    pub theta: Vec<f32>, // polar angle (circular)
    pub ang: Vec<f32>,   // outward direction (unrooted)
    pub far: Vec<f32>,   // extent of collapsed clades
    pub vis: Vec<bool>,
    pub tipish: Vec<bool>, // leaf or collapsed clade
    pub order: Vec<usize>, // visible nodes, pre-order
    pub ntip: usize,
    pub world_len: f64, // branch-length units per world unit
    pub dtheta: f32,
}

fn pol(r: f32, t: f32) -> Vec2 {
    vec2(r * t.cos(), r * t.sin())
}

pub fn compute_layout(t: &Tree, st: &Settings) -> Layout {
    let n = t.nodes.len();
    let mut order = vec![];
    let mut vis = vec![false; n];
    let mut tipish = vec![false; n];
    let mut stk = vec![t.root];
    while let Some(u) = stk.pop() {
        vis[u] = true;
        order.push(u);
        let nd = &t.nodes[u];
        if nd.children.is_empty() || (nd.collapsed && u != t.root) {
            tipish[u] = true;
        } else {
            for &c in nd.children.iter().rev() {
                stk.push(c);
            }
        }
    }
    // depths over the full tree
    let full = t.preorder(t.root);
    let mut depth = vec![0f64; n];
    match st.mode {
        BranchMode::Cladogram => {
            let mut h = vec![0f64; n];
            for &u in full.iter().rev() {
                h[u] = t.nodes[u]
                    .children
                    .iter()
                    .map(|&c| h[c] + 1.0)
                    .fold(0.0, f64::max);
            }
            let rh = h[t.root];
            for &u in &full {
                depth[u] = rh - h[u];
            }
        }
        _ => {
            for &u in &full {
                if let Some(p) = t.nodes[u].parent {
                    let l = if st.mode == BranchMode::Equal {
                        1.0
                    } else {
                        t.eff_len(u)
                    };
                    depth[u] = depth[p] + l;
                }
            }
        }
    }
    let maxd = depth.iter().cloned().fold(0.0, f64::max).max(1e-12);
    let mut fard = depth.clone();
    for &u in full.iter().rev() {
        for &c in &t.nodes[u].children {
            if fard[c] > fard[u] {
                fard[u] = fard[c];
            }
        }
    }
    let nt = order.iter().filter(|&&u| tipish[u]).count().max(1);
    let mut lay = Layout {
        pos: vec![Vec2::ZERO; n],
        r: vec![0.0; n],
        theta: vec![0.0; n],
        ang: vec![0.0; n],
        far: vec![0.0; n],
        vis,
        tipish,
        order,
        ntip: nt,
        world_len: maxd,
        dtheta: 0.0,
    };
    // leaf rank / y
    let mut yy = vec![0f32; n];
    let mut li = 0;
    for &u in &lay.order {
        if lay.tipish[u] {
            yy[u] = (li as f32 + 0.5) / nt as f32;
            li += 1;
        }
    }
    for &u in lay.order.clone().iter().rev() {
        if !lay.tipish[u] {
            let ch = &t.nodes[u].children;
            yy[u] = (yy[ch[0]] + yy[*ch.last().unwrap()]) * 0.5;
        }
    }
    match st.layout {
        LayoutKind::Rectangular | LayoutKind::Slanted => {
            for &u in &lay.order {
                lay.pos[u] = vec2((depth[u] / maxd) as f32, yy[u]);
                lay.far[u] = (fard[u] / maxd) as f32;
            }
        }
        LayoutKind::Circular => {
            let arc = st.arc_deg.to_radians().clamp(0.2, TAU);
            let rot = st.rot_deg.to_radians();
            lay.dtheta = arc / nt as f32;
            for &u in &lay.order {
                lay.theta[u] = rot + arc * yy[u];
                lay.r[u] = (depth[u] / maxd) as f32;
                lay.pos[u] = pol(lay.r[u], lay.theta[u]);
                lay.ang[u] = lay.theta[u];
                lay.far[u] = (fard[u] / maxd) as f32;
            }
        }
        LayoutKind::Unrooted => {
            let mut cnt = vec![0f32; n];
            for &u in lay.order.iter().rev() {
                cnt[u] = if lay.tipish[u] {
                    1.0
                } else {
                    t.nodes[u].children.iter().map(|&c| cnt[c]).sum()
                };
            }
            let (mut a0, mut a1) = (vec![0f32; n], vec![0f32; n]);
            let rot = st.rot_deg.to_radians();
            a0[t.root] = rot;
            a1[t.root] = rot + TAU;
            let mut raw = vec![Vec2::ZERO; n];
            for &u in &lay.order {
                if lay.tipish[u] {
                    continue;
                }
                let span = a1[u] - a0[u];
                let mut a = a0[u];
                for &c in &t.nodes[u].children {
                    let s = span * cnt[c] / cnt[u];
                    a0[c] = a;
                    a1[c] = a + s;
                    a += s;
                    lay.ang[c] = (a0[c] + a1[c]) / 2.0;
                    let l = if st.mode == BranchMode::Proportional {
                        t.eff_len(c)
                    } else {
                        1.0
                    } as f32;
                    raw[c] = raw[u] + pol(l, lay.ang[c]);
                }
            }
            let ext = lay
                .order
                .iter()
                .map(|&u| raw[u].length())
                .fold(0.0, f32::max)
                .max(1e-9);
            for &u in &lay.order {
                lay.pos[u] = raw[u] / ext;
                lay.far[u] = ((fard[u] - depth[u]) as f32) / ext;
            }
            lay.world_len = ext as f64;
        }
    }
    lay
}

// ------------------------------------------------------------------- scene
#[derive(Clone, Copy)]
pub struct Xf {
    pub origin: Pos2,
    pub pan: Vec2,
    pub sx: f32,
    pub sy: f32,
}
impl Xf {
    pub fn p(&self, w: Vec2) -> Pos2 {
        self.origin + self.pan + vec2(w.x * self.sx, w.y * self.sy)
    }
}

pub enum Prim {
    Line(Pos2, Pos2, Color32, f32),
    Poly(Vec<Pos2>, Color32, f32),
    Fill(Vec<Pos2>, Color32),
    Dot(Pos2, f32, Color32),
    /// `end` = anchor is at the end of the text (used for flipped labels)
    Text {
        p: Pos2,
        s: String,
        size: f32,
        c: Color32,
        ang: f32,
        end: bool,
    },
}

#[derive(Clone, Copy)]
pub struct Style {
    pub line: Color32,
    pub text: Color32,
    pub hl: Color32,
    pub find: Color32,
    pub mono: bool,
}

#[derive(Default)]
pub struct Scene {
    pub prims: Vec<Prim>,
    pub pts: Vec<(usize, Pos2)>,
    pub rects: Vec<(usize, Rect)>,
}

pub fn est_w(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.55
}

fn cell_poly(
    kind: LayoutKind,
    ax: Pos2,
    th: f32,
    c0: Pos2,
    rad: f32,
    spacing: f32,
    dth: f32,
    r0: f32,
    r1: f32,
    band: f32,
) -> Vec<Pos2> {
    if kind == LayoutKind::Circular {
        let half = dth * 0.5 * band * 0.96;
        let segs = ((2.0 * half * (rad + r1)) / 5.0).ceil().clamp(1.0, 10.0) as usize;
        let at = |a: f32, r: f32| c0 + vec2(a.cos(), a.sin()) * r;
        let mut v: Vec<Pos2> = (0..=segs)
            .map(|i| at(th - half + 2.0 * half * i as f32 / segs as f32, rad + r0))
            .collect();
        v.extend(
            (0..=segs)
                .rev()
                .map(|i| at(th - half + 2.0 * half * i as f32 / segs as f32, rad + r1)),
        );
        v
    } else {
        let hh = (spacing * 0.5 * band - 0.25).max(0.4);
        vec![
            pos2(ax.x + r0, ax.y - hh),
            pos2(ax.x + r1, ax.y - hh),
            pos2(ax.x + r1, ax.y + hh),
            pos2(ax.x + r0, ax.y + hh),
        ]
    }
}

pub fn build_scene(
    t: &Tree,
    lay: &Layout,
    xf: &Xf,
    st: &Settings,
    sty: &Style,
    view: Rect,
    sel: &[bool],
    hits: &HashSet<usize>,
    counts: &[usize],
    ann: Option<&Annotations>,
) -> Scene {
    let mut sc = Scene::default();
    let mut texts: Vec<Prim> = vec![];
    let vb = view.expand(80.0);
    let vis = |a: Pos2, b: Pos2| vb.intersects(Rect::from_two_pos(a, b));
    let nt = lay.ntip as f32;
    let kind = st.layout;
    let mono = sty.mono;
    let rgb = |c: [u8; 3]| -> Color32 {
        if mono {
            Color32::from_gray(
                ((0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32) as u8).min(150),
            )
        } else {
            Color32::from_rgb(c[0], c[1], c[2])
        }
    };
    let ann = if kind == LayoutKind::Unrooted {
        None
    } else {
        ann.filter(|a| !a.active().is_empty())
    };
    let tracks: Vec<(usize, f32, f32)> = ann.map(|a| a.layout_tracks()).unwrap_or_default();
    let track_end = ann.map(|a| a.total_width()).unwrap_or(0.0);
    let c0 = xf.p(Vec2::ZERO);
    let spacing = match kind {
        LayoutKind::Rectangular | LayoutKind::Slanted => xf.sy / nt,
        LayoutKind::Circular => xf.sx * lay.dtheta,
        LayoutKind::Unrooted => xf.sx * TAU * 0.5 / nt,
    };
    let show_tips = st.show_tip_labels && (!st.auto_hide || spacing >= st.tip_font * 0.85);
    let gap = st.label_gap;

    let label = |sc: &mut Scene,
                 texts: &mut Vec<Prim>,
                 u: usize,
                 s: String,
                 anchor: Pos2,
                 ang: f32,
                 end: bool,
                 c: Color32| {
        let w = est_w(&s, st.tip_font);
        let d = vec2(ang.cos(), ang.sin());
        let s0 = if end { anchor - d * w } else { anchor };
        let r = Rect::from_two_pos(s0, s0 + d * w).expand(st.tip_font * 0.6);
        if vb.intersects(r) {
            sc.rects.push((u, r));
            texts.push(Prim::Text {
                p: anchor,
                s,
                size: st.tip_font,
                c,
                ang,
                end,
            });
        }
    };
    let radial = |a: f32, base: Pos2| -> (Pos2, f32, bool) {
        let anchor = base + vec2(a.cos(), a.sin()) * gap;
        if a.cos() < 0.0 {
            (anchor, a + PI, true)
        } else {
            (anchor, a, false)
        }
    };

    for &u in &lay.order {
        let nd = &t.nodes[u];
        let p = xf.p(lay.pos[u]);
        sc.pts.push((u, p));
        let col = if sel[u] {
            sty.hl
        } else if let Some(c) = nd.color {
            rgb(c)
        } else {
            sty.line
        };
        let w = st.line_w + if sel[u] { 1.0 } else { 0.0 };
        // ---- branch to parent
        let mut mid: Option<Pos2> = None;
        if let Some(par) = nd.parent {
            let c = match kind {
                LayoutKind::Rectangular => xf.p(vec2(lay.pos[par].x, lay.pos[u].y)),
                LayoutKind::Slanted | LayoutKind::Unrooted => xf.p(lay.pos[par]),
                LayoutKind::Circular => xf.p(pol(lay.r[par], lay.theta[u])),
            };
            if vis(c, p) {
                sc.prims.push(Prim::Line(c, p, col, w));
            }
            mid = Some(c + (p - c) * 0.5);
        } else if kind == LayoutKind::Rectangular {
            sc.prims
                .push(Prim::Line(xf.p(vec2(-0.012, lay.pos[u].y)), p, col, w));
        }
        // ---- connectors of internal nodes
        if !lay.tipish[u] {
            let (f, l) = (nd.children[0], *nd.children.last().unwrap());
            match kind {
                LayoutKind::Rectangular => {
                    let a = xf.p(vec2(lay.pos[u].x, lay.pos[f].y));
                    let b = xf.p(vec2(lay.pos[u].x, lay.pos[l].y));
                    if vis(a, b) {
                        sc.prims.push(Prim::Line(a, b, col, w));
                    }
                }
                LayoutKind::Circular if lay.r[u] > 0.0 => {
                    let (t0, t1) = (lay.theta[f], lay.theta[l]);
                    let segs =
                        (((t1 - t0).abs() * lay.r[u] * xf.sx) / 4.0).clamp(2.0, 300.0) as usize;
                    let pts: Vec<Pos2> = (0..=segs)
                        .map(|i| xf.p(pol(lay.r[u], t0 + (t1 - t0) * i as f32 / segs as f32)))
                        .collect();
                    sc.prims.push(Prim::Poly(pts, col, w));
                }
                _ => {}
            }
        }
        // ---- markers
        if vb.contains(p) {
            let tipdot = lay.tipish[u] && !nd.collapsed;
            if (tipdot && st.show_tip_dots) || (!tipdot && !lay.tipish[u] && st.show_node_dots) {
                sc.prims.push(Prim::Dot(p, st.dot, col));
            }
            if hits.contains(&u) {
                sc.prims.push(Prim::Dot(p, st.dot + 3.0, sty.find));
            }
        }
        // ---- branch length / node label
        if st.show_branch_len
            && t.has_len
            && nd.parent.is_some()
            && st.mode == BranchMode::Proportional
        {
            if let Some(m) = mid {
                let s = format!("{:.*}", st.decimals, nd.len);
                let wd = est_w(&s, st.branch_font);
                let q = m + vec2(-wd / 2.0, -st.branch_font * 0.8);
                if vb.contains(q) {
                    texts.push(Prim::Text {
                        p: q,
                        s,
                        size: st.branch_font,
                        c: sty.text.gamma_multiply(0.7),
                        ang: 0.0,
                        end: false,
                    });
                }
            }
        }
        if st.show_node_labels && !nd.label.is_empty() && !lay.tipish[u] && vb.contains(p) {
            texts.push(Prim::Text {
                p: p + vec2(-4.0, -st.node_font * 0.8),
                s: nd.label.clone(),
                size: st.node_font,
                c: sty.text.gamma_multiply(0.8),
                ang: 0.0,
                end: true,
            });
        }
        // ---- tips and collapsed clades
        if !lay.tipish[u] {
            continue;
        }
        let is_clade = nd.collapsed && !nd.children.is_empty();
        let tcol = if hits.contains(&u) {
            sty.find
        } else if sel[u] {
            sty.hl
        } else if let Some(c) = nd.color {
            rgb(c)
        } else {
            sty.text
        };
        let text = if is_clade {
            if nd.label.is_empty() {
                format!("{} tips", counts[u])
            } else {
                format!("{} ({} tips)", nd.label, counts[u])
            }
        } else {
            nd.name.clone()
        };
        let mut tip_pt = p;
        let mut tip_ang = 0.0f32;
        let aligned_pt;
        match kind {
            LayoutKind::Rectangular | LayoutKind::Slanted => {
                aligned_pt = xf.p(vec2(1.0, lay.pos[u].y))
            }
            LayoutKind::Circular => {
                tip_ang = lay.theta[u];
                aligned_pt = xf.p(pol(1.0, lay.theta[u]));
            }
            LayoutKind::Unrooted => {
                tip_ang = lay.ang[u];
                aligned_pt = p;
            }
        }
        if is_clade {
            let fill = col.gamma_multiply(0.35);
            let tri: Vec<Pos2> = match kind {
                LayoutKind::Rectangular | LayoutKind::Slanted => {
                    let (fx, y) = (xf.p(vec2(lay.far[u], lay.pos[u].y)).x, p.y);
                    let h = spacing * 0.4;
                    tip_pt = pos2(fx, y);
                    vec![p, pos2(fx, y - h), pos2(fx, y + h)]
                }
                LayoutKind::Circular => {
                    let hd = lay.dtheta * 0.4;
                    tip_pt = xf.p(pol(lay.far[u], lay.theta[u]));
                    vec![
                        p,
                        xf.p(pol(lay.far[u], lay.theta[u] - hd)),
                        tip_pt,
                        xf.p(pol(lay.far[u], lay.theta[u] + hd)),
                    ]
                }
                LayoutKind::Unrooted => {
                    let d = vec2(lay.ang[u].cos(), lay.ang[u].sin());
                    let e = xf.p(lay.pos[u] + d * lay.far[u]);
                    let perp = vec2(-d.y, d.x) * (e - p).length() * 0.25;
                    tip_pt = e;
                    vec![p, e + perp, e - perp]
                }
            };
            let mut closed = tri.clone();
            closed.push(tri[0]);
            sc.prims.push(Prim::Fill(tri, fill));
            sc.prims.push(Prim::Poly(closed, col, 1.0));
        }
        let dirv = if matches!(kind, LayoutKind::Rectangular | LayoutKind::Slanted) {
            vec2(1.0, 0.0)
        } else {
            vec2(tip_ang.cos(), tip_ang.sin())
        };
        let outer = aligned_pt + dirv * track_end;
        let onscreen = vb.contains(tip_pt) || vb.contains(aligned_pt) || vb.contains(outer);
        if !onscreen {
            continue;
        }
        let use_aligned = st.align_tips || !tracks.is_empty();
        // ---- annotation tracks
        if let (Some(a), false) = (ann, is_clade) {
            if let Some(row) = a.row(&nd.name) {
                let rad = xf.sx;
                for &(ci, off, wd) in &tracks {
                    let v = &row[ci];
                    if missing(v) {
                        continue;
                    }
                    let colm = &a.cols[ci];
                    match colm.kind {
                        TrackKind::ColorStrip | TrackKind::Heatmap => {
                            if let Some(c) = a.color(ci, v, mono) {
                                sc.prims.push(Prim::Fill(
                                    cell_poly(
                                        kind,
                                        aligned_pt,
                                        lay.theta[u],
                                        c0,
                                        rad,
                                        spacing,
                                        lay.dtheta,
                                        off,
                                        off + wd,
                                        1.0,
                                    ),
                                    c,
                                ));
                            }
                        }
                        TrackKind::Bar => {
                            if let Some(nv) = a.norm(ci, v) {
                                let l = (nv as f32 * wd).max(0.5);
                                sc.prims.push(Prim::Fill(
                                    cell_poly(
                                        kind,
                                        aligned_pt,
                                        lay.theta[u],
                                        c0,
                                        rad,
                                        spacing,
                                        lay.dtheta,
                                        off,
                                        off + l,
                                        0.7,
                                    ),
                                    a.bar_color(ci, mono),
                                ));
                            }
                        }
                        TrackKind::Text => {
                            let size = (spacing * 0.8).min(11.0);
                            if size >= 5.0 {
                                let base = aligned_pt + dirv * (off + 2.0);
                                let (an, ang, end) = if kind == LayoutKind::Circular {
                                    radial(tip_ang, base)
                                } else {
                                    (base, 0.0, false)
                                };
                                let an = if kind == LayoutKind::Circular {
                                    an - dirv * gap + dirv * 0.0
                                } else {
                                    an
                                };
                                texts.push(Prim::Text {
                                    p: an,
                                    s: v.clone(),
                                    size,
                                    c: sty.text,
                                    ang,
                                    end,
                                });
                            }
                        }
                    }
                }
            }
        }
        if !show_tips {
            continue;
        }
        let base = if use_aligned {
            if vis(tip_pt, aligned_pt) && !is_clade {
                sc.prims.push(Prim::Line(
                    tip_pt,
                    aligned_pt,
                    sty.text.gamma_multiply(0.2),
                    1.0,
                ));
            }
            outer
        } else {
            tip_pt
        };
        match kind {
            LayoutKind::Rectangular | LayoutKind::Slanted => label(
                &mut sc,
                &mut texts,
                u,
                text,
                base + vec2(gap, 0.0),
                0.0,
                false,
                tcol,
            ),
            _ => {
                let (a, ang, end) = radial(tip_ang, base);
                label(&mut sc, &mut texts, u, text, a, ang, end, tcol);
            }
        }
    }
    sc.prims.extend(texts);
    sc
}

// --------------------------------------------------------------------- SVG
fn css(c: Color32) -> String {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    format!("rgb({},{},{})", r, g, b)
}
fn alpha(c: Color32) -> f32 {
    c.a() as f32 / 255.0
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn to_svg(prims: &[Prim], w: f32, h: f32) -> String {
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
    for p in prims {
        match p {
            Prim::Line(a, b, c, lw) => s.push_str(&format!("<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-opacity=\"{:.2}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>\n", a.x, a.y, b.x, b.y, css(*c), alpha(*c), lw)),
            Prim::Poly(pts, c, lw) => {
                let pp: Vec<String> = pts.iter().map(|q| format!("{:.2},{:.2}", q.x, q.y)).collect();
                s.push_str(&format!("<polyline fill=\"none\" points=\"{}\" stroke=\"{}\" stroke-opacity=\"{:.2}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>\n", pp.join(" "), css(*c), alpha(*c), lw));
            }
            Prim::Fill(pts, c) => {
                let pp: Vec<String> = pts.iter().map(|q| format!("{:.2},{:.2}", q.x, q.y)).collect();
                s.push_str(&format!("<polygon points=\"{}\" fill=\"{}\" fill-opacity=\"{:.2}\"/>\n", pp.join(" "), css(*c), alpha(*c)));
            }
            Prim::Dot(q, r, c) => s.push_str(&format!("<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{}\" fill=\"{}\"/>\n", q.x, q.y, r, css(*c))),
            Prim::Text { p, s: txt, size, c, ang, end } => s.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"Helvetica,Arial,sans-serif\" font-size=\"{}\" fill=\"{}\" fill-opacity=\"{:.2}\" text-anchor=\"{}\" dominant-baseline=\"central\" transform=\"rotate({:.2} {:.2} {:.2})\">{}</text>\n",
                p.x, p.y, size, css(*c), alpha(*c), if *end { "end" } else { "start" }, ang.to_degrees(), p.x, p.y, esc(txt))),
        }
    }
    s.push_str("</svg>\n");
    s
}
