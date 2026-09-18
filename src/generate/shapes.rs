//! Контуры фигур по ГОСТ 19.701-90 (вид A, все линии чёрные, заливка белая).

use super::svg::put;
use crate::layout::Shape;

/// tan(75°): наклон боковых сторон параллелограмма «ввод-вывод».
const TAN75: f64 = 3.732_050_807_568_877_6;

fn poly(out: &mut String, pts: &[(f64, f64)]) {
    out.push_str("<polygon points=\"");
    for (i, (x, y)) in pts.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        put(out, *x);
        out.push(',');
        put(out, *y);
    }
    out.push('"');
}

fn rect(out: &mut String, x0: f64, y0: f64, w: f64, h: f64, rx: f64) {
    out.push_str("<rect x=\"");
    put(out, x0);
    out.push_str("\" y=\"");
    put(out, y0);
    out.push_str("\" width=\"");
    put(out, w);
    out.push_str("\" height=\"");
    put(out, h);
    out.push('"');
    // терминатор: капсула, rx = ry = h/2 (полуокружные торцы)
    if rx > 0.0 {
        out.push_str(" rx=\"");
        put(out, rx);
        out.push_str("\" ry=\"");
        put(out, rx);
        out.push('"');
    }
}

/// Общая обводка фигуры; заливка белая — чёрные линии видны в тёмных темах.
fn stroke(out: &mut String, lw: f64) {
    out.push_str(" fill=\"white\" stroke=\"black\" stroke-width=\"");
    put(out, lw);
    out.push_str("\" stroke-linejoin=\"miter\"/>");
}

/// Контур фигуры: polygon/rect/circle без заливки в атрибутах —
/// заливка и обводка добавляются общим стилем.
pub(crate) fn shape_svg(out: &mut String, sh: &Shape, lw: f64) {
    let (x0, y0) = (sh.cx - sh.w / 2.0, sh.cy - sh.h / 2.0);
    let (x1, y1) = (sh.cx + sh.w / 2.0, sh.cy + sh.h / 2.0);
    match sh.kind.as_str() {
        "term" | "ret" => rect(out, x0, y0, sh.w, sh.h, sh.h / 2.0),
        "io" => {
            let dx = sh.h / TAN75;
            poly(out, &[(x0 + dx, y0), (x1, y0), (x1 - dx, y1), (x0, y1)]);
        }
        "if" => poly(out, &[(x0, sh.cy), (sh.cx, y0), (x1, sh.cy), (sh.cx, y1)]),
        // подготовка: срезаны верхние углы
        "loop_begin" => poly(
            out,
            &[
                (x0 + sh.skew, y0),
                (x1 - sh.skew, y0),
                (x1, y0 + sh.skew),
                (x1, y1),
                (x0, y1),
                (x0, y0 + sh.skew),
            ],
        ),
        // слияние/соединение цикла: срезаны нижние углы
        "loop_end" => poly(
            out,
            &[
                (x0, y0),
                (x1, y0),
                (x1, y1 - sh.skew),
                (x1 - sh.skew, y1),
                (x0 + sh.skew, y1),
                (x0, y1 - sh.skew),
            ],
        ),
        "conn" => {
            out.push_str("<circle cx=\"");
            put(out, sh.cx);
            out.push_str("\" cy=\"");
            put(out, sh.cy);
            out.push_str("\" r=\"");
            put(out, sh.w / 2.0);
            out.push('"');
            stroke(out, lw);
            return;
        }
        // процесс и всё неизвестное — прямоугольник
        _ => rect(out, x0, y0, sh.w, sh.h, 0.0),
    }
    stroke(out, lw);
}
