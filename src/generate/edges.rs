//! Линии потока: ломаная + открытый наконечник отдельным путём.

use super::arrow::arrow_path;
use super::svg::n;
use crate::layout::Edge;

/// Ломаная линия; при arrow — усики 45° в конце, тем же пером.
pub(crate) fn edge_svg(e: &Edge, lw: f64, whisker: f64) -> String {
    if e.points.len() < 2 {
        return String::new();
    }
    let pts: Vec<String> = e
        .points
        .iter()
        .map(|(x, y)| format!("{},{}", n(*x), n(*y)))
        .collect();
    let mut out = format!(
        concat!(
            "<polyline points=\"{p}\" fill=\"none\" stroke=\"black\" ",
            "stroke-width=\"{lw}\" stroke-linejoin=\"miter\"/>"
        ),
        p = pts.join(" "),
        lw = n(lw),
    );
    if e.arrow {
        let tip = e.points[e.points.len() - 1];
        let prev = e.points[e.points.len() - 2];
        let d = arrow_path(tip, prev, whisker);
        if !d.is_empty() {
            out.push_str(&format!(
                concat!(
                    "<path d=\"{d}\" fill=\"none\" stroke=\"black\" ",
                    "stroke-width=\"{lw}\"/>"
                ),
                d = d,
                lw = n(lw),
            ));
        }
    }
    out
}
