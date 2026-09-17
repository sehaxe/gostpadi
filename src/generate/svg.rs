//! Слой-обёртка: шапка SVG, единый масштаб через <g>, порядок слоёв.

use super::{
    edges::edge_svg,
    shapes::shape_svg,
    text::{label_text, shape_text},
};
use crate::layout::Layout;
use crate::style::Style;

/// ГОСТ: длина усика 0.2..0.25 высоты модуля (a = 2*grid); берём верх диапазона.
const WHISKER_FRAC: f64 = 0.25;

/// Число без хвостовых нулей: 14.170 -> "14.17", 1.000 -> "1".
pub(crate) fn n(v: f64) -> String {
    let mut s = format!("{v:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();
    if s == "-0" {
        s = "0".into();
    }
    s
}

/// Масштаб вписывания раскладки в А4 (не больше 1: мелкие схемы
/// рисуются 1:1, крупные сжимаются).
pub fn fit_scale(bounds: (f64, f64, f64, f64), st: &Style) -> f64 {
    let (_, _, w, h) = bounds;
    1.0_f64
        .min((st.a4_w - 2.0 * st.page_pad) / w)
        .min((st.a4_h - 2.0 * st.page_pad) / h)
}

/// Раскладка + стиль -> SVG. Всё рисуется в координатах раскладки,
/// единый масштаб применяется одним `<g transform>`: линии под
/// фигурами, фигуры под подписями.
pub fn render_svg(l: &Layout, st: &Style) -> String {
    render_svg_at(l, st, fit_scale(l.bounds, st))
}

/// То же с внешним масштабом: пачка схем рисуется одним общим s,
/// чтобы фигуры во всех файлах пачки были одного визуального размера.
pub fn render_svg_at(l: &Layout, st: &Style, s: f64) -> String {
    let (minx, miny, w, h) = l.bounds;
    // усики стрелок в локальных единицах: масштаб применит g-обёртка.
    let whisker = WHISKER_FRAC * 2.0 * st.grid;

    let mut edges = String::new();
    for e in &l.edges {
        edges.push_str(&edge_svg(e, st.edge_lw, whisker));
    }
    let mut shapes = String::new();
    for sh in &l.shapes {
        shapes.push_str(&shape_svg(sh, st.edge_lw));
        shapes.push_str(&shape_text(&sh.lines, sh.cx, sh.cy, st));
    }
    let mut labels = String::new();
    for lb in &l.labels {
        labels.push_str(&label_text(lb, st));
    }

    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}pt\" height=\"{h}pt\" ",
            "viewBox=\"0 0 {w} {h}\">\n",
            "<g transform=\"translate({tx} {ty}) scale({s})\">{body}</g>\n</svg>\n"
        ),
        w = n(w * s),
        h = n(h * s),
        tx = n(-minx * s),
        ty = n(-miny * s),
        s = n(s),
        body = format!("{edges}{shapes}{labels}"),
    )
}
