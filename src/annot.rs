//! iTOL-style annotation tracks: a CSV/TSV table (first column = tip name) whose
//! columns can be drawn as colour strips, heatmaps, bar charts or text next to the tips.
use crate::scene::Prim;
use crate::tree::Tree;
use eframe::egui::{pos2, Color32, Pos2};
use std::collections::HashMap;

/*
Gaurav Sablok
gsablok@proton.me
 */

pub const PALETTE: [[u8; 3]; 10] = [
    [31, 119, 180],
    [255, 127, 14],
    [44, 160, 44],
    [214, 39, 40],
    [148, 103, 189],
    [140, 86, 75],
    [227, 119, 194],
    [127, 127, 127],
    [188, 189, 34],
    [23, 190, 207],
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackKind {
    ColorStrip,
    Heatmap,
    Bar,
    Text,
}
impl TrackKind {
    pub fn name(&self) -> &'static str {
        match self {
            TrackKind::ColorStrip => "Colour strip",
            TrackKind::Heatmap => "Heatmap",
            TrackKind::Bar => "Bar chart",
            TrackKind::Text => "Text",
        }
    }
}

#[derive(Clone)]
pub struct Column {
    pub name: String,
    pub numeric: bool,
    pub min: f64,
    pub max: f64,
    pub enabled: bool,
    pub kind: TrackKind,
    pub width: f32,
    pub cats: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Annotations {
    pub cols: Vec<Column>,
    pub rows: HashMap<String, Vec<String>>,
    pub file: String,
}

pub fn norm_key(s: &str) -> String {
    s.trim().replace(' ', "_").to_lowercase()
}
pub fn missing(v: &str) -> bool {
    matches!(
        v.trim(),
        "" | "NA" | "N/A" | "na" | "n/a" | "-" | "nan" | "NaN"
    )
}

fn ramp(t: f64) -> [u8; 3] {
    // viridis-like 3 stop ramp
    let stops = [
        [68.0, 1.0, 84.0],
        [33.0, 145.0, 140.0],
        [253.0, 231.0, 37.0],
    ];
    let t = t.clamp(0.0, 1.0) * 2.0;
    let (i, f) = if t >= 2.0 {
        (1, 1.0)
    } else {
        (t as usize, t.fract())
    };
    let mut o = [0u8; 3];
    for k in 0..3 {
        o[k] = (stops[i][k] + (stops[i + 1][k] - stops[i][k]) * f) as u8;
    }
    o
}

impl Annotations {
    pub fn parse(text: &str, file: &str) -> Result<Annotations, String> {
        let mut lines = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
        let header = lines.next().ok_or("empty file")?;
        let delim = if header.contains('\t') {
            '\t'
        } else if header.contains(';') && !header.contains(',') {
            ';'
        } else {
            ','
        };
        let split = |l: &str| -> Vec<String> {
            l.split(delim)
                .map(|x| x.trim().trim_matches('"').to_string())
                .collect()
        };
        let h = split(header);
        if h.len() < 2 {
            return Err("need at least 2 columns (tip name + one value column)".into());
        }
        let nc = h.len() - 1;
        let mut rows = HashMap::new();
        for l in lines {
            let mut f = split(l);
            let key = norm_key(&f.remove(0));
            f.resize(nc, String::new());
            rows.insert(key, f);
        }
        if rows.is_empty() {
            return Err("no data rows".into());
        }
        let mut cols = vec![];
        for ci in 0..nc {
            let vals: Vec<&str> = rows
                .values()
                .map(|r| r[ci].as_str())
                .filter(|v| !missing(v))
                .collect();
            let nums: Vec<f64> = vals.iter().filter_map(|v| v.parse::<f64>().ok()).collect();
            let numeric = !vals.is_empty() && nums.len() == vals.len();
            let (min, max) = nums
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |a, &x| {
                    (a.0.min(x), a.1.max(x))
                });
            let mut cats: Vec<String> = vec![];
            if !numeric {
                cats = vals.iter().map(|s| s.to_string()).collect();
                cats.sort();
                cats.dedup();
            }
            let kind = if numeric {
                TrackKind::Heatmap
            } else {
                TrackKind::ColorStrip
            };
            cols.push(Column {
                name: h[ci + 1].clone(),
                numeric,
                min,
                max,
                enabled: ci < 3,
                kind,
                width: 14.0,
                cats,
            });
        }
        Ok(Annotations {
            cols,
            rows,
            file: file.to_string(),
        })
    }
    pub fn row(&self, tip: &str) -> Option<&Vec<String>> {
        self.rows.get(&norm_key(tip))
    }
    pub fn matched(&self, t: &Tree) -> (usize, usize) {
        let tips = t.tips(t.root);
        (
            tips.iter()
                .filter(|&&i| self.row(&t.nodes[i].name).is_some())
                .count(),
            tips.len(),
        )
    }
    pub fn active(&self) -> Vec<usize> {
        (0..self.cols.len())
            .filter(|&i| self.cols[i].enabled)
            .collect()
    }
    /// (column, offset-from-tip, width) in pixels for enabled tracks
    pub fn layout_tracks(&self) -> Vec<(usize, f32, f32)> {
        let mut off = 6.0;
        let mut v = vec![];
        for i in self.active() {
            let w = self.cols[i].width;
            v.push((i, off, w));
            off += w + 2.0;
        }
        v
    }
    pub fn total_width(&self) -> f32 {
        self.layout_tracks()
            .last()
            .map(|&(_, o, w)| o + w + 4.0)
            .unwrap_or(0.0)
    }
    pub fn norm(&self, ci: usize, v: &str) -> Option<f64> {
        let c = &self.cols[ci];
        let x: f64 = v.trim().parse().ok()?;
        let n = if c.min >= 0.0 {
            if c.max > 0.0 {
                x / c.max
            } else {
                0.0
            }
        } else if c.max > c.min {
            (x - c.min) / (c.max - c.min)
        } else {
            0.5
        };
        Some(n.clamp(0.0, 1.0))
    }
    pub fn color_rgb(&self, ci: usize, v: &str) -> Option<[u8; 3]> {
        if missing(v) {
            return None;
        }
        let c = &self.cols[ci];
        if c.numeric {
            let x: f64 = v.trim().parse().ok()?;
            let t = if c.max > c.min {
                (x - c.min) / (c.max - c.min)
            } else {
                0.5
            };
            Some(ramp(t))
        } else {
            let i = c.cats.iter().position(|k| k == v.trim())?;
            Some(PALETTE[i % 10])
        }
    }
    pub fn color(&self, ci: usize, v: &str, mono: bool) -> Option<Color32> {
        if !mono {
            return self
                .color_rgb(ci, v)
                .map(|c| Color32::from_rgb(c[0], c[1], c[2]));
        }
        if missing(v) {
            return None;
        }
        let c = &self.cols[ci];
        let g = if c.numeric {
            let x: f64 = v.trim().parse().ok()?;
            let t = if c.max > c.min {
                (x - c.min) / (c.max - c.min)
            } else {
                0.5
            };
            235.0 - t.clamp(0.0, 1.0) * 205.0
        } else {
            let i = c.cats.iter().position(|k| k == v.trim())? as f64;
            let n = c.cats.len().max(2) as f64 - 1.0;
            30.0 + i * 170.0 / n
        };
        Some(Color32::from_gray(g as u8))
    }
    pub fn bar_color(&self, ci: usize, mono: bool) -> Color32 {
        if mono {
            Color32::from_gray(70)
        } else {
            let c = PALETTE[ci % 10];
            Color32::from_rgb(c[0], c[1], c[2])
        }
    }
    /// Colour the tree's branches by a column (internal nodes only if all their tips agree).
    pub fn color_by(&self, t: &mut Tree, ci: usize) {
        let n = t.nodes.len();
        let mut col: Vec<Option<[u8; 3]>> = vec![None; n];
        for &u in t.preorder(t.root).iter().rev() {
            if t.nodes[u].children.is_empty() {
                col[u] = self
                    .row(&t.nodes[u].name)
                    .and_then(|r| self.color_rgb(ci, &r[ci]));
            } else {
                let first = col[t.nodes[u].children[0]];
                if first.is_some() && t.nodes[u].children.iter().all(|&c| col[c] == first) {
                    col[u] = first;
                }
            }
        }
        for u in 0..n {
            t.nodes[u].color = col[u];
        }
    }
    pub fn legend_prims(&self, origin: Pos2, mono: bool, text: Color32) -> Vec<Prim> {
        let mut out = vec![];
        let (x, mut y) = (origin.x, origin.y);
        let sw = |x: f32, y: f32, c: Color32| {
            Prim::Fill(
                vec![
                    pos2(x, y),
                    pos2(x + 10.0, y),
                    pos2(x + 10.0, y + 10.0),
                    pos2(x, y + 10.0),
                ],
                c,
            )
        };
        let txt = |x: f32, y: f32, s: String, size: f32| Prim::Text {
            p: pos2(x, y),
            s,
            size,
            c: text,
            ang: 0.0,
            end: false,
        };
        for ci in self.active() {
            let c = &self.cols[ci];
            out.push(txt(x, y + 6.0, c.name.clone(), 12.0));
            y += 17.0;
            if c.kind == TrackKind::Bar || c.kind == TrackKind::Text {
                if c.kind == TrackKind::Bar && c.numeric {
                    out.push(txt(x, y + 5.0, format!("0 - {}", c.max), 10.0));
                    y += 14.0;
                }
                y += 6.0;
                continue;
            }
            if c.numeric {
                for k in 0..5 {
                    let t = k as f64 / 4.0;
                    let col = if mono {
                        Color32::from_gray((235.0 - t * 205.0) as u8)
                    } else {
                        let r = ramp(t);
                        Color32::from_rgb(r[0], r[1], r[2])
                    };
                    out.push(sw(x + k as f32 * 11.0, y, col));
                }
                out.push(txt(
                    x + 60.0,
                    y + 5.0,
                    format!("{} - {}", c.min, c.max),
                    10.0,
                ));
                y += 14.0;
            } else {
                for cat in c.cats.iter().take(14) {
                    if let Some(col) = self.color(ci, cat, mono) {
                        out.push(sw(x, y, col));
                    }
                    out.push(txt(x + 15.0, y + 5.0, cat.clone(), 10.0));
                    y += 14.0;
                }
                if c.cats.len() > 14 {
                    out.push(txt(
                        x,
                        y + 5.0,
                        format!("... +{} more", c.cats.len() - 14),
                        10.0,
                    ));
                    y += 14.0;
                }
            }
            y += 6.0;
        }
        out
    }
}
