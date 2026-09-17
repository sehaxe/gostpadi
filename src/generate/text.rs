//! Текст: в фигурах (центрированный, многострочный) и подписи ветвей.

use super::svg::n;
use crate::layout::Label;
use crate::style::Style;

/// XML-экранирование текста (& первым).
pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn family(st: &Style) -> String {
    format!("{}, monospace", st.font_stack[0])
}

fn open(x: f64, y: f64, anchor: &str, st: &Style) -> String {
    format!(
        concat!(
            "<text x=\"{x}\" y=\"{y}\" font-family=\"{f}\" font-size=\"{fs}\" ",
            "font-weight=\"normal\" text-anchor=\"{a}\" dominant-baseline=\"central\">"
        ),
        x = n(x),
        y = n(y),
        f = family(st),
        fs = n(st.font),
        a = anchor,
    )
}

/// Текст фигуры: строки по вертикали с шагом pitch, блок центрирован
/// на (cx, cy); tspan держит x, dy ведёт каретку.
pub(crate) fn shape_text(lines: &[String], cx: f64, cy: f64, st: &Style) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let k = (lines.len() as f64 - 1.0) / 2.0;
    let tspans: String = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let dy = if i == 0 { -k * st.pitch } else { st.pitch };
            format!(
                "<tspan x=\"{0}\" dy=\"{1}\">{2}</tspan>",
                n(cx),
                n(dy),
                esc(line)
            )
        })
        .collect();
    format!("{}{tspans}</text>", open(cx, cy, "middle", st))
}

/// Подпись ветви/линии: якорь по ha.
pub(crate) fn label_text(lb: &Label, st: &Style) -> String {
    let anchor = match lb.ha.as_str() {
        "left" => "start",
        "right" => "end",
        _ => "middle",
    };
    format!("{}{}</text>", open(lb.x, lb.y, anchor, st), esc(&lb.text))
}
