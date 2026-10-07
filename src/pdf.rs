//! Minimal vector PDF writer for a list of scene primitives (Helvetica text, alpha via ExtGState).
use crate::scene::Prim;

/*
Gaurav Sablok
gsablok@proton.me
 */

const HW: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

pub fn text_width(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (32..=126).contains(&u) {
                HW[(u - 32) as usize] as f32
            } else {
                556.0
            }
        })
        .sum::<f32>()
        * size
        / 1000.0
}

fn pdf_str(s: &str) -> Vec<u8> {
    let mut o = vec![b'('];
    for ch in s.chars() {
        let b = if (ch as u32) < 256 {
            ch as u32 as u8
        } else {
            b'?'
        };
        match b {
            b'(' | b')' | b'\\' => {
                o.push(b'\\');
                o.push(b);
            }
            32..=126 => o.push(b),
            _ => o.extend(format!("\\{:03o}", b).bytes()),
        }
    }
    o.push(b')');
    o
}

pub fn to_pdf(prims: &[Prim], w: f32, h: f32) -> Vec<u8> {
    let mut alphas: Vec<u8> = vec![];
    let mut c: Vec<u8> = vec![];
    let mut push = |c: &mut Vec<u8>, s: String| c.extend(s.bytes());
    push(&mut c, format!("1 0 0 -1 0 {:.2} cm\n1 J 1 j\n", h));
    let mut gs = |alphas: &mut Vec<u8>, a: u8| -> String {
        if a == 255 {
            return String::new();
        }
        let i = alphas.iter().position(|&x| x == a).unwrap_or_else(|| {
            alphas.push(a);
            alphas.len() - 1
        });
        format!("/GS{} gs ", i)
    };
    for p in prims {
        match p {
            Prim::Line(a, b, col, lw) => {
                let [r, g, bl, al] = col.to_srgba_unmultiplied();
                push(
                    &mut c,
                    format!(
                        "q {}{:.3} {:.3} {:.3} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S Q\n",
                        gs(&mut alphas, al),
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        bl as f32 / 255.0,
                        lw,
                        a.x,
                        a.y,
                        b.x,
                        b.y
                    ),
                );
            }
            Prim::Poly(pts, col, lw) => {
                if pts.len() < 2 {
                    continue;
                }
                let [r, g, bl, al] = col.to_srgba_unmultiplied();
                let mut s = format!(
                    "q {}{:.3} {:.3} {:.3} RG {:.2} w {:.2} {:.2} m ",
                    gs(&mut alphas, al),
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    bl as f32 / 255.0,
                    lw,
                    pts[0].x,
                    pts[0].y
                );
                for q in &pts[1..] {
                    s.push_str(&format!("{:.2} {:.2} l ", q.x, q.y));
                }
                s.push_str("S Q\n");
                push(&mut c, s);
            }
            Prim::Fill(pts, col) => {
                if pts.len() < 3 {
                    continue;
                }
                let [r, g, bl, al] = col.to_srgba_unmultiplied();
                let mut s = format!(
                    "q {}{:.3} {:.3} {:.3} rg {:.2} {:.2} m ",
                    gs(&mut alphas, al),
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    bl as f32 / 255.0,
                    pts[0].x,
                    pts[0].y
                );
                for q in &pts[1..] {
                    s.push_str(&format!("{:.2} {:.2} l ", q.x, q.y));
                }
                s.push_str("f Q\n");
                push(&mut c, s);
            }
            Prim::Dot(q, r, col) => {
                let [cr, cg, cb, al] = col.to_srgba_unmultiplied();
                let k = 0.5523 * r;
                let (x, y) = (q.x, q.y);
                push(&mut c, format!(
                    "q {}{:.3} {:.3} {:.3} rg {:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c f Q\n",
                    gs(&mut alphas, al), cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0, x + r, y,
                    x + r, y + k, x + k, y + r, x, y + r,
                    x - k, y + r, x - r, y + k, x - r, y,
                    x - r, y - k, x - k, y - r, x, y - r,
                    x + k, y - r, x + r, y - k, x + r, y));
            }
            Prim::Text {
                p,
                s,
                size,
                c: col,
                ang,
                end,
            } => {
                let [r, g, bl, al] = col.to_srgba_unmultiplied();
                let (ca, sa) = (ang.cos(), ang.sin());
                let w = text_width(s, *size);
                // centre the text vertically on the anchor (cap-height ≈ 0.72 em)
                let base_off = *size * 0.36;
                let (nx, ny) = (-sa, ca);
                let mut ox = p.x + nx * base_off;
                let mut oy = p.y + ny * base_off;
                if *end {
                    ox -= ca * w;
                    oy -= sa * w;
                }
                push(&mut c, format!("q {}{:.3} {:.3} {:.3} rg BT /F1 {:.2} Tf {:.4} {:.4} {:.4} {:.4} {:.2} {:.2} Tm ", gs(&mut alphas, al), r as f32 / 255.0, g as f32 / 255.0, bl as f32 / 255.0, size, ca, sa, sa, -ca, ox, oy));
                c.extend(pdf_str(s));
                c.extend(b" Tj ET Q\n");
            }
        }
    }
    // assemble file
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs: Vec<usize> = vec![];
    let mut obj = |out: &mut Vec<u8>, offs: &mut Vec<usize>, body: &[u8]| {
        offs.push(out.len());
        out.extend(format!("{} 0 obj\n", offs.len()).bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    };
    let gs_dict: String = (0..alphas.len())
        .map(|i| format!("/GS{} {} 0 R ", i, 6 + i))
        .collect();
    obj(&mut out, &mut offs, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(
        &mut out,
        &mut offs,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    obj(&mut out, &mut offs, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents 5 0 R /Resources << /Font << /F1 4 0 R >> /ExtGState << {} >> >> >>", w, h, gs_dict).as_bytes());
    obj(
        &mut out,
        &mut offs,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    let mut stream = format!("<< /Length {} >>\nstream\n", c.len()).into_bytes();
    stream.extend(&c);
    stream.extend(b"\nendstream");
    obj(&mut out, &mut offs, &stream);
    for a in &alphas {
        let v = *a as f32 / 255.0;
        obj(
            &mut out,
            &mut offs,
            format!("<< /Type /ExtGState /ca {:.3} /CA {:.3} >>", v, v).as_bytes(),
        );
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offs.len() + 1).bytes());
    for o in &offs {
        out.extend(format!("{:010} 00000 n \n", o).bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offs.len() + 1,
            xref
        )
        .bytes(),
    );
    out
}
