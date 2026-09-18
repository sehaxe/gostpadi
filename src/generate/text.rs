//! Текст: в фигурах (центрированный, многострочный) и подписи ветвей.

use super::svg::put;
use crate::layout::Label;
use crate::style::Style;

const FONT_FAMILY: &str = "DejaVu Sans Mono, monospace";

/// XML-экранирование прямо в буфер, посимвольно: без трёх проходов
/// `replace` и без промежуточных String.
pub(crate) fn esc(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn open(out: &mut String, x: f64, y: f64, anchor: &str, st: &Style) {
    out.push_str("<text x=\"");
    put(out, x);
    out.push_str("\" y=\"");
    put(out, y);
    out.push_str("\" font-family=\"");
    out.push_str(FONT_FAMILY);
    out.push_str("\" font-size=\"");
    put(out, st.font);
    out.push_str("\" font-weight=\"normal\" text-anchor=\"");
    out.push_str(anchor);
    out.push_str("\" dominant-baseline=\"central\">");
}

/// Текст фигуры: строки по вертикали с шагом pitch, блок центрирован
/// на (cx, cy); tspan держит x, dy ведёт каретку.
pub(crate) fn shape_text(out: &mut String, lines: &[String], cx: f64, cy: f64, st: &Style) {
    if lines.is_empty() {
        return;
    }
    let k = (lines.len() as f64 - 1.0) / 2.0;
    open(out, cx, cy, "middle", st);
    for (i, line) in lines.iter().enumerate() {
        let dy = if i == 0 { -k * st.pitch } else { st.pitch };
        out.push_str("<tspan x=\"");
        put(out, cx);
        out.push_str("\" dy=\"");
        put(out, dy);
        out.push_str("\">");
        esc(out, line);
        out.push_str("</tspan>");
    }
    out.push_str("</text>");
}

/// Подпись ветви/линии: якорь по ha.
pub(crate) fn label_text(out: &mut String, lb: &Label, st: &Style) {
    let anchor = match lb.ha.as_str() {
        "left" => "start",
        "right" => "end",
        _ => "middle",
    };
    open(out, lb.x, lb.y, anchor, st);
    esc(out, &lb.text);
    out.push_str("</text>");
}
