//! Линии потока: ломаная + открытый наконечник отдельным путём.

use super::arrow::arrow_path;
use super::svg::put;
use crate::layout::Edge;

/// Ломаная линия; при arrow — усики 45° в конце, тем же пером.
pub(crate) fn edge_svg(out: &mut String, e: &Edge, lw: f64, whisker: f64) {
    if e.points.len() < 2 {
        return;
    }
    out.push_str("<polyline points=\"");
    for (i, (x, y)) in e.points.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        put(out, *x);
        out.push(',');
        put(out, *y);
    }
    out.push_str("\" fill=\"none\" stroke=\"black\" stroke-width=\"");
    put(out, lw);
    out.push_str("\" stroke-linejoin=\"miter\"/>");
    if e.arrow {
        let tip = e.points[e.points.len() - 1];
        let prev = e.points[e.points.len() - 2];
        let mark = out.len();
        out.push_str("<path d=\"");
        if arrow_path(out, tip, prev, whisker) {
            out.push_str("\" fill=\"none\" stroke=\"black\" stroke-width=\"");
            put(out, lw);
            out.push_str("\"/>");
        } else {
            out.truncate(mark); // вырожденный отрезок — усики не нужны
        }
    }
}
