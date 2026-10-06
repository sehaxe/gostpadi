//! Слой-обёртка: шапка SVG, единый масштаб через <g>, порядок слоёв.

use std::fmt::Write as _;

use super::{
    edges::edge_svg,
    shapes::shape_svg,
    text::{label_text, shape_text},
};
use crate::layout::Layout;
use crate::style::Style;

/// ГОСТ: длина усика 0.2..0.25 высоты модуля (a = 2*grid); берём верх диапазона.
const WHISKER_FRAC: f64 = 0.25;

/// Число без хвостовых нулей прямо в буфер: 14.170 -> "14.17", 1.000 -> "1",
/// -0.000 -> "0". Ни одной промежуточной String — только запись в `out`.
pub(crate) fn put(out: &mut String, v: f64) {
    let start = out.len();
    let _ = write!(out, "{v:.3}");
    let mut end = out.len();
    while end > start + 2 && out.as_bytes()[end - 1] == b'0' {
        end -= 1;
    }
    if end > start && out.as_bytes()[end - 1] == b'.' {
        end -= 1;
    }
    out.truncate(end);
    if end - start == 2 && out[start..].starts_with("-0") {
        out.remove(start); // "-0" -> "0"
    }
}

/// Масштаб вписывания раскладки в А4 (не больше 1: мелкие схемы
/// рисуются 1:1, крупные сжимаются).
pub fn fit_scale(bounds: (f64, f64, f64, f64), st: &Style) -> f64 {
    let (_, _, w, h) = bounds;
    1.0_f64
        .min((st.a4_w - 2.0 * st.page_pad) / w)
        .min((st.a4_h - 2.0 * st.page_pad) / h)
}

/// Раскладка + стиль -> SVG с автоподбором масштаба.
pub fn render_svg(l: &Layout, st: &Style) -> String {
    render_svg_at(l, st, fit_scale(l.bounds, st))
}

/// То же с внешним масштабом: пачка схем рисуется одним общим s,
/// чтобы фигуры во всех файлах пачки были одного визуального размера.
/// Всё рисуется в координатах раскладки,
/// единый масштаб применяется одним `<g transform>`: линии под
/// фигурами, фигуры под подписями. Весь документ собирается в двух
/// буферах (тело и документ) — без промежуточных строк на каждый элемент.
pub fn render_svg_at(l: &Layout, st: &Style, s: f64) -> String {
    let (minx, miny, w, h) = l.bounds;
    // усики стрелок в локальных единицах: масштаб применит g-обёртка.
    let whisker = WHISKER_FRAC * 2.0 * st.grid;

    let mut body = String::new();
    for e in &l.edges {
        edge_svg(&mut body, e, st.edge_lw, whisker);
    }
    for sh in &l.shapes {
        shape_svg(&mut body, sh, st.edge_lw);
        shape_text(&mut body, &sh.lines, sh.cx, sh.cy, st);
    }
    for lb in &l.labels {
        label_text(&mut body, lb, st);
    }

    let mut out = String::with_capacity(body.len() + 256);
    out.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"",
    );
    put(&mut out, w * s);
    out.push_str("pt\" height=\"");
    put(&mut out, h * s);
    out.push_str("pt\" viewBox=\"0 0 ");
    put(&mut out, w * s);
    out.push(' ');
    put(&mut out, h * s);
    out.push_str("\">\n<rect x=\"0\" y=\"0\" width=\"");
    put(&mut out, w * s);
    out.push_str("\" height=\"");
    put(&mut out, h * s);
    out.push_str("\" fill=\"#ffffff\"/>\n<g transform=\"translate(");
    put(&mut out, -minx * s);
    out.push(' ');
    put(&mut out, -miny * s);
    out.push_str(") scale(");
    put(&mut out, s);
    out.push_str(")\">");
    out.push_str(&body);
    out.push_str("</g>\n</svg>\n");
    out
}
