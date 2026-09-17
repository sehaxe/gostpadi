//! Перенесённые тесты measure/normalize (без изменений по смыслу).

use super::*;
use crate::layout::{measure, uniform_sizes};

#[test]
fn measure_on_grid() {
    let st = Style::default();
    for kind in ["term", "ret", "io", "act", "if", "loop"] {
        for text in ["x", "a = 1", "printf(hello world)\nfoo(a, b)"] {
            let (w, h) = measure(&st, kind, text);
            let q = |v: f64| (v / st.grid).round() - v / st.grid;
            assert!(q(w).abs() < 1e-9, "{kind} w={w} off-grid");
            if kind != "if" && kind != "conn" {
                assert!(q(h).abs() < 1e-9, "{kind} h={h} off-grid");
            }
        }
    }
}

#[test]
fn term_min_height_two_grids() {
    let st = Style::default();
    let (_, h) = measure(&st, "term", "x");
    assert!(h >= 2.0 * st.grid);
}

#[test]
fn if_height_tracks_aspect() {
    let st = Style::default();
    let (w, h) = measure(&st, "if", "long condition a < b and c > d ?");
    assert!((h - w * st.aspect).abs() < 1e-9);
}

/// Кегль 24 удваивает геометрию: measure растёт от font согласованно
/// (шрифт — база всех метрик фигур).
#[test]
fn font_doubles_measure() {
    let s12 = Style::with_metrics(12.0, 1.0);
    let s24 = Style::with_metrics(24.0, 1.0);
    let (w12, h12) = measure(&s12, "act", "x = 1");
    let (w24, h24) = measure(&s24, "act", "x = 1");
    let ratio = w24 / w12;
    assert!(
        (ratio - 2.0).abs() < 0.5,
        "ширина при font=24 должна быть ~2x от font=12: {ratio}"
    );
    assert!(w24 > w12 && h24 > h12);
}

#[test]
fn normalize_all_seven_kinds() {
    let st = Style::default();
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        node(NodeKind::Act, "a = 1\nprintf(x)\nif a>0\n    да: b = 2"),
        node(NodeKind::Return, "конец"),
    ];
    let sizes = normalize(&nodes, &st);
    for k in ["term", "ret", "io", "act", "if", "loop", "conn"] {
        let sz = sizes.get(k).unwrap_or_else(|| panic!("missing {k}"));
        assert!(sz.0 > 0.0 && sz.1 > 0.0, "{k} zero size");
    }
    assert!(sizes["io"].0 < sizes["act"].0);
}

#[test]
fn normalize_recurses_branches_and_body() {
    let st = Style::default();
    let mut nd = node(NodeKind::Decision, "c?");
    let mut inner = node(NodeKind::Act, "d = 1");
    inner.body = Some(vec![Stmt::Text("a very long body line indeed xx".into())]);
    nd.branches.push(br(
        "да",
        vec![Stmt::Text("printf(long io statement here ok)".into())],
        false,
    ));
    let sizes = normalize(&[nd, inner], &st);
    let (w, _) = measure(&st, "io", "printf(long io statement here ok)");
    assert_eq!(sizes["io"].0, w);
    let (w2, _) = measure(&st, "act", "a very long body line indeed xx");
    assert_eq!(sizes["act"].0, w2);
}

#[test]
fn text_stmt_classified_io_vs_act() {
    let st = Style::default();
    let mut nd = node(NodeKind::Decision, "c?");
    nd.branches
        .push(br("да", vec![s("printf(x)"), s("a = 1")], false));
    let sizes = normalize(&[nd], &st);
    let (w, _) = measure(&st, "act", "a = 1");
    assert_eq!(sizes["act"].0, w);
    let (wi, _) = measure(&st, "io", "printf(x)");
    assert_eq!(sizes["io"].0, wi);
}

#[test]
fn uniform_takes_max() {
    let st = Style::default();
    let mut a = normalize(&[node(NodeKind::Term, "начало")], &st);
    let mut b = normalize(&[node(NodeKind::Term, "начало")], &st);
    a.insert("if", (100.0, 100.0));
    b.insert("if", (200.0, 70.0));
    let u = uniform_sizes(&[a, b]);
    assert_eq!(u["if"], (200.0, 100.0));
}
