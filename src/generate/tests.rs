//! Тесты генератора: вид строк SVG фиксируем, идём от тестов.

use super::svg::render_svg;
use crate::frontend::c::c_to_gvn;
use crate::frontend::gvn::parse;
use crate::layout::{layout, normalize, Edge, Label, Layout, Shape};
use crate::style::Style;

fn sh(kind: &str, cx: f64, cy: f64, w: f64, h: f64, skew: f64) -> Shape {
    Shape {
        kind: kind.into(),
        cx,
        cy,
        w,
        h,
        skew,
        lines: vec!["x = 1".into()],
    }
}

fn hand(shapes: Vec<Shape>, edges: Vec<Edge>, labels: Vec<Label>) -> Layout {
    Layout {
        shapes,
        edges,
        labels,
        bounds: (0.0, 0.0, 200.0, 300.0),
        anchors: Vec::new(),
    }
}

/// Схема через gvn-парсер + раскладку, как в бою.
fn render_gvn(gvn: &str) -> String {
    let st = Style::default();
    let nodes = parse(gvn, &st, "").expect("parse gvn");
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    render_svg(&l, &st)
}

fn all_kinds() -> Layout {
    hand(
        vec![
            sh("term", 100.0, 30.0, 120.0, 34.0, 0.0),
            sh("io", 100.0, 90.0, 100.0, 40.0, 0.0),
            sh("if", 100.0, 160.0, 120.0, 60.0, 0.0),
            sh("loop_begin", 100.0, 230.0, 110.0, 40.0, 12.0),
            sh("loop_end", 100.0, 280.0, 110.0, 40.0, 12.0),
            sh("act", 100.0, 330.0, 90.0, 30.0, 0.0),
            sh("conn", 100.0, 380.0, 28.0, 28.0, 0.0),
        ],
        vec![Edge {
            points: vec![(100.0, 47.0), (100.0, 70.0)],
            arrow: true,
        }],
        vec![Label {
            x: 160.0,
            y: 140.0,
            text: "yes".into(),
            ha: "left".into(),
        }],
    )
}

fn polygons(svg: &str) -> Vec<Vec<(f64, f64)>> {
    svg.match_indices("<polygon")
        .map(|(i, _)| {
            let rest = &svg[i..];
            let start = rest.find("points=\"").expect("points attr") + "points=\"".len();
            let end = rest[start..].find('"').expect("closing quote") + start;
            rest[start..end]
                .split_whitespace()
                .map(|p| {
                    let (x, y) = p.split_once(',').expect("x,y pair");
                    (x.parse().expect("x"), y.parse().expect("y"))
                })
                .collect()
        })
        .collect()
}

#[test]
fn markers_all_kinds() {
    let svg = render_svg(&all_kinds(), &Style::default());
    for m in ["<?xml", "<svg", "<polygon", "<circle", "stroke-width"] {
        assert!(svg.contains(m), "нет {m}");
    }
    assert!(svg.contains("font-weight=\"normal\""));
    assert!(!svg.contains("bold"));
}

/// Белая подложка сразу после <svg>: в тёмных просмотрщиках чёрные
/// линии на прозрачном фоне не видны.
#[test]
fn white_background_rect_before_g() {
    let svg = render_svg(&all_kinds(), &Style::default());
    let head = &svg[..svg.find("<g").expect("нет <g")];
    let vb = svg.split("viewBox=\"").nth(1).expect("нет viewBox");
    let dims: Vec<&str> = vb.split('"').next().unwrap().split_whitespace().collect();
    let expect = format!(
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>",
        dims[2], dims[3]
    );
    assert!(head.contains(&expect), "нет подложки {expect} в {head}");
}

#[test]
fn markers_from_gvn_parse_and_layout() {
    let svg = render_gvn(include_str!("../../examples/hello.gvn"));
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<polygon"));
    assert!(!svg.contains("bold"));
}

#[test]
fn arrow_paths_equal_arrow_edges() {
    let l = hand(
        vec![sh("term", 100.0, 30.0, 120.0, 34.0, 0.0)],
        vec![
            Edge {
                points: vec![(100.0, 47.0), (100.0, 70.0)],
                arrow: true,
            },
            Edge {
                points: vec![(100.0, 70.0), (100.0, 100.0)],
                arrow: true,
            },
            Edge {
                points: vec![(100.0, 100.0), (100.0, 130.0)],
                arrow: true,
            },
            Edge {
                points: vec![(50.0, 47.0), (50.0, 70.0)],
                arrow: false,
            },
        ],
        vec![],
    );
    let svg = render_svg(&l, &Style::default());
    assert_eq!(svg.matches("<path").count(), 3);
    assert_eq!(svg.matches("<polyline").count(), 4);
}

#[test]
fn io_parallelogram_slope() {
    let l = hand(
        vec![sh("io", 100.0, 100.0, 100.0, 40.0, 0.0)],
        vec![],
        vec![],
    );
    let svg = render_svg(&l, &Style::default());
    let pts = &polygons(&svg)[0];
    assert_eq!(pts.len(), 4);
    let dx = pts[0].0 - pts[3].0; // верх-лево минус низ-лево
    let want = 40.0 / 3.732_050_807_568_877_6; // h / tan(75°) ≈ 0.268*h
    assert!((dx - want).abs() < 0.05, "dx={dx}, want={want}");
    assert!((pts[0].1 - pts[1].1).abs() < 1e-9); // верхняя грань горизонтальна
}

#[test]
fn loop_begin_hexagon_top_cut() {
    let l = hand(
        vec![sh("loop_begin", 100.0, 100.0, 110.0, 40.0, 12.0)],
        vec![],
        vec![],
    );
    let svg = render_svg(&l, &Style::default());
    let pts = &polygons(&svg)[0];
    assert_eq!(pts.len(), 6);
    let x0 = 100.0 - 110.0 / 2.0;
    let y0 = 100.0 - 40.0 / 2.0;
    // срез сверху: обе верхние точки правее левого края
    let top: Vec<&(f64, f64)> = pts.iter().filter(|p| (p.1 - y0).abs() < 1e-9).collect();
    assert_eq!(top.len(), 2);
    assert!(top[0].0 > x0 && top[1].0 > x0, "верх не срезан: {pts:?}");
    assert!(top[0].0 < top[1].0);
}

#[test]
fn examples_render_clean() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).expect("examples dir") {
        let path = entry.expect("dir entry").path();
        let gvn = match path.extension().and_then(|e| e.to_str()) {
            Some("gvn") => std::fs::read_to_string(&path).expect("read gvn"),
            Some("c") => {
                c_to_gvn(&std::fs::read_to_string(&path).expect("read c"), "").expect("c_to_gvn")
            }
            _ => continue,
        };
        let svg = render_gvn(&gvn);
        assert!(!svg.is_empty(), "{path:?}: пустой");
        assert!(svg.starts_with("<?xml"), "{path:?}: нет шапки");
        assert!(!svg.contains("NaN"), "{path:?}: NaN");
        assert!(!svg.contains("inf"), "{path:?}: inf");
        n += 1;
    }
    assert!(n >= 6, "ожидались gvn-файлы + main.c, найдено {n}");
}

#[test]
fn single_group_wrapper() {
    let svg = render_svg(&all_kinds(), &Style::default());
    assert_eq!(svg.matches("<g transform=").count(), 1);
}

#[test]
fn arrow_whiskers_geometry() {
    // горизонтальная линия вправо: усы симметричны относительно оси
    let p = super::arrow::arrow_path((100.0, 50.0), (0.0, 50.0), 10.0);
    let (c, s) = (22.5f64.to_radians().cos(), 22.5f64.to_radians().sin());
    let f = |v: f64| super::svg::n(v);
    let want = format!(
        "M {} {} L 100 50 M {} {} L 100 50",
        f(100.0 - 10.0 * c),
        f(50.0 - 10.0 * s),
        f(100.0 - 10.0 * c),
        f(50.0 + 10.0 * s)
    );
    assert_eq!(p, want);
    assert_eq!(super::arrow::arrow_path((5.0, 5.0), (5.0, 5.0), 10.0), "");
}

#[test]
fn whisker_len_is_quarter_module_height() {
    // ГОСТ: усик 0.2..0.25 высоты модуля (a = 2*grid); рендер берёт верх диапазона.
    let st = Style::default();
    let l = hand(
        vec![],
        vec![Edge {
            points: vec![(100.0, 47.0), (100.0, 70.0)],
            arrow: true,
        }],
        vec![],
    );
    let svg = render_svg(&l, &st);
    let len = 0.25 * 2.0 * st.grid;
    let (c, s) = (22.5f64.to_radians().cos(), 22.5f64.to_radians().sin());
    let f = |v: f64| super::svg::n(v);
    // линия вниз: ось (0,1), усы в точках tip ± (s, -c)*len
    let want = format!(
        "<path d=\"M {} {} L 100 70 M {} {} L 100 70\"",
        f(100.0 + len * s),
        f(70.0 - len * c),
        f(100.0 - len * s),
        f(70.0 - len * c)
    );
    assert!(svg.contains(&want), "whisker path mismatch:\n{svg}");
}

#[test]
fn text_escaped_and_centered() {
    let l = hand(
        vec![Shape {
            kind: "act".into(),
            cx: 100.0,
            cy: 100.0,
            w: 90.0,
            h: 30.0,
            skew: 0.0,
            lines: vec!["if (a < b && c > d)".into()],
        }],
        vec![],
        vec![],
    );
    let svg = render_svg(&l, &Style::default());
    assert!(svg.contains("if (a &lt; b &amp;&amp; c &gt; d)"));
    assert!(svg.contains("dominant-baseline=\"central\""));
    assert!(svg.contains("font-family=\"DejaVu Sans Mono, monospace\""));
    // двухстрочный текст: первый tspan поднят на полшага
    assert!(svg.contains("<tspan"), "нет tspan");
}
