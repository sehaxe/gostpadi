//! Диагностика: размеры листов корпуса тем же путём, что CLI-пачка
//! (split + uniform sizes). Запуск:
//! cargo test --quiet dbg_pages -- --ignored --nocapture

use crate::frontend::{input_kind, InputKind};
use crate::layout::{layout, normalize, split_scheme, uniform_sizes};
use crate::pipeline::{parse_batch, Options, Scheme};
use crate::style::Style;

fn opts() -> Options {
    Options {
        labels: "en".into(),
        font: None,
        lw: None,
        no_split: false,
    }
}

fn parse(paths: &[&str]) -> Vec<Scheme> {
    let inputs: Vec<(String, String, InputKind)> = paths
        .iter()
        .map(|p| {
            (
                p.to_string(),
                std::fs::read_to_string(p).unwrap(),
                input_kind(p),
            )
        })
        .collect();
    parse_batch(&inputs, &opts()).unwrap()
}

fn pages_at(paths: &[&str], scale: f64) {
    let st = Style {
        split_scale: scale,
        ..opts().style()
    };
    let schemes = parse(paths);
    let normed: Vec<_> = schemes.iter().map(|s| normalize(&s.nodes, &st)).collect();
    let sizes = uniform_sizes(&normed);
    for s in schemes {
        let parts = split_scheme(s.nodes, &sizes, &st);
        println!("== {}: {} листов", s.path, parts.len());
        for (i, p) in parts.iter().enumerate() {
            let l = layout(p, &sizes, &st);
            let (_, _, w, h) = l.bounds;
            let fw = (st.a4_w - 2.0 * st.page_pad) / w;
            let fh = (st.a4_h - 2.0 * st.page_pad) / h;
            println!(
                "  лист {i}: {w:.0}x{h:.0} fit {:.3}/{:.3} узлов {}",
                fw,
                fh,
                p.len()
            );
            if let Err(e) = crate::layout::crossings_ok(&l.shapes, &l.edges) {
                println!("     ПЕРЕСЕЧЕНИЕ: {e}");
            }
            if fw.min(fh) < 0.35 {
                let mut lo = f64::MAX;
                let mut hi = f64::MIN;
                for sh in &l.shapes {
                    lo = lo.min(sh.cx - sh.w / 2.0);
                    hi = hi.max(sh.cx + sh.w / 2.0);
                }
                println!("     фигуры x {lo:.0}..{hi:.0}");
                for sh in &l.shapes {
                    println!(
                        "     {} ({:.0},{:.0}) {:.0}x{:.0} {:?}",
                        sh.kind, sh.cx, sh.cy, sh.w, sh.h, sh.lines
                    );
                }
            }
        }
    }
}

#[test]
#[ignore]
fn dbg_pages_labs() {
    pages_at(
        &[
            "corpus/oap-lab1.c",
            "corpus/oap-lab2.c",
            "corpus/oap-lab3.c",
            "corpus/oap-lab4.c",
        ],
        1.0,
    );
}

#[test]
#[ignore]
fn dbg_pages_at_cut() {
    pages_at(
        &[
            "corpus/oap-lab1.c",
            "corpus/oap-lab2.c",
            "corpus/oap-lab3.c",
            "corpus/oap-lab4.c",
        ],
        0.22,
    );
}

#[test]
#[ignore]
fn dbg_oaiplab3_1() {
    let st = Style::default();
    let src = std::fs::read_to_string("/home/sehaxe/oaiplab3/1.c").unwrap();
    let schemes = crate::frontend::c::c_to_gvn_functions_full(&src, "ru").unwrap();
    for (fname, _sig, gvn) in schemes {
        if fname == "main" {
            println!("GVN:\n{gvn}");
        }
        let nodes = crate::frontend::gvn::parse(&gvn, &st, "ru").unwrap();
        if fname == "main" {
            fn dump_stmts(ns: &[crate::ir::Stmt], d: usize) {
                for s in ns {
                    match s {
                        crate::ir::Stmt::Node(n) => {
                            println!("{}{:?} {:?}", "  ".repeat(d), n.kind, n.text);
                            for b in &n.branches {
                                println!("{}br {}:", "  ".repeat(d), b.label);
                                dump_stmts(&b.stmts, d + 1);
                            }
                            if let Some(body) = &n.body {
                                println!("{}body:", "  ".repeat(d));
                                dump_stmts(body, d + 1);
                            }
                        }
                        x => println!("{}{:?}", "  ".repeat(d), x),
                    }
                }
            }
            for n in &nodes {
                println!("TOP {:?} {:?}", n.kind, n.text);
                for b in &n.branches {
                    println!("  br {}:", b.label);
                    dump_stmts(&b.stmts, 2);
                }
                if let Some(body) = &n.body {
                    println!("  body:");
                    dump_stmts(body, 2);
                }
            }
        }
        let sizes = crate::layout::normalize(&nodes, &st);
        let l = crate::layout::layout(&nodes, &sizes, &st);
        println!(
            "== {fname}: shapes={} edges={}",
            l.shapes.len(),
            l.edges.len()
        );
        for (i, sh) in l.shapes.iter().enumerate() {
            if sh.cx < 500.0 && (550.0..750.0).contains(&sh.cy) {
                println!(
                    "shape {i}: {} ({:.1},{:.1}) {:.1}x{:.1} {:?}",
                    sh.kind, sh.cx, sh.cy, sh.w, sh.h, sh.lines
                );
            }
        }
        for (i, lb) in l.labels.iter().enumerate() {
            println!(
                "label {i}: {:?} at ({:.1},{:.1}) {}",
                lb.text, lb.x, lb.y, lb.ha
            );
        }
        for (i, e) in l.edges.iter().enumerate() {
            if let Err(msg) = crate::layout::crossings_ok(
                std::slice::from_ref(&l.shapes[0]),
                std::slice::from_ref(e),
            ) {
                let _ = msg;
            }
            if e.points
                .windows(2)
                .any(|w| (w[0].1 - w[1].1).abs() < 1e-9 && w[0].1 > 560.0 && w[0].1 < 820.0)
            {
                println!("edge {i}: {:?} arrow={}", e.points, e.arrow);
            }
        }
    }
}

#[test]
#[ignore]
fn dbg_split_oaiplab3_1() {
    let st = Style::default();
    let src = std::fs::read_to_string("/home/sehaxe/oaiplab3/1.c").unwrap();
    let schemes = crate::frontend::c::c_to_gvn_functions_full(&src, "ru").unwrap();
    for (fname, _sig, gvn) in schemes {
        if fname != "main" {
            continue;
        }
        let nodes = crate::frontend::gvn::parse(&gvn, &st, "ru").unwrap();
        let sizes = crate::layout::normalize(&nodes, &st);
        let mut stp = st.clone();
        stp.split_scale = 0.16;
        let parts = crate::layout::split_scheme(nodes, &sizes, &stp);
        println!("parts={}", parts.len());
        for (pi, part) in parts.iter().enumerate() {
            println!("part {pi}: {} nodes", part.len());
            for n in part {
                println!("  {:?} {:?} branches={}", n.kind, n.text, n.branches.len());
            }
            let l = crate::layout::layout(part, &sizes, &st);
            for lb in &l.labels {
                if lb.text.contains("break") || lb.text.contains("continue") {
                    println!("    LABEL {:?} ({:.1},{:.1})", lb.text, lb.x, lb.y);
                }
            }
            for sh in &l.shapes {
                if (550.0..700.0).contains(&sh.cy) {
                    println!(
                        "    SHAPE {} ({:.1},{:.1}) {:.1}x{:.1} {:?}",
                        sh.kind, sh.cx, sh.cy, sh.w, sh.h, sh.lines
                    );
                }
            }
            for e in &l.edges {
                if e.points.iter().any(|p| (550.0..700.0).contains(&p.1)) {
                    println!("    EDGE {:?} arrow={}", e.points, e.arrow);
                }
            }
        }
    }
}
