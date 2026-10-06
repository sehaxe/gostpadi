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
    put_prec(out, v, 3);
}

/// То же с заданной точностью: масштаб пишется шестью знаками — при
/// трёх округление сдвигало бы фазу пикселей (13/19 = 0.684210...).
pub(crate) fn put_prec(out: &mut String, v: f64, prec: usize) {
    let start = out.len();
    let _ = write!(out, "{v:.prec$}");
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

/// Масштаб вписывания раскладки в лист (не больше 1: мелкие схемы
/// рисуются 1:1, крупные сжимаются). Решение принимает лист.
pub fn fit_scale(bounds: (f64, f64, f64, f64), st: &Style) -> f64 {
    let (_, _, w, h) = bounds;
    st.sheet.scale_for(w, h)
}

/// Модуль сетки в пикселях при 96 dpi: 1 pt = 4/3 px.
fn grid_px(st: &Style) -> f64 {
    st.grid * 4.0 / 3.0
}

/// Масштаб, при котором модуль сетки остаётся ЦЕЛЫМ числом пикселей.
///
/// Без этого вписывание ломает фазу: при масштабе 0.714 линия, стоящая
/// на целом модуле, попадает то в пиксель, то между двумя, и одна и та
/// же линия в разных местах схемы выглядит по-разному. Округляем ВНИЗ —
/// схема остаётся вписанной; цена — до 5 % незанятого места листа.
pub fn snap_scale(s: f64, st: &Style) -> f64 {
    let u = grid_px(st);
    (s * u).floor().max(1.0) / u
}

/// Ближайшая координата с фазой полпикселя (не больше `v`): попав в
/// центр пикселя, штрих в один пиксель рисуется ровно в один пиксель.
fn half_px(v: f64) -> f64 {
    let q = v * 4.0 / 3.0;
    ((q - 0.5).floor() + 0.5) * 3.0 / 4.0
}

/// Раскладка + стиль -> SVG с автоподбором масштаба.
pub fn render_svg(l: &Layout, st: &Style) -> String {
    render_svg_at(l, st, fit_scale(l.bounds, st))
}

/// Пункт в миллиметре.
const PT_PER_MM: f64 = 72.0 / 25.4;

/// SVG для вставки в отчёт: ровно по содержимому, без пустого листа.
///
/// Лист А4 удобен для печати, но в отчёт его вставлять нельзя —
/// схема занимает треть страницы, а вокруг пустое поле. Здесь
/// `viewBox` равен габариту содержимого, а `width`/`height` заданы
/// в миллиметрах из тех же пунктов, поэтому в Word и LaTeX схема
/// встаёт физического размера «миллиметр в миллиметр».
///
/// Никакого вписывания и `<g transform>`: содержимое рисуется в своих
/// же координатах 1:1. Из-за этого и у всех файлов пачки размер
/// совпадает — вписывания, которое ломало бы единообразие, тут нет.
pub fn render_svg_tight(l: &Layout, st: &Style) -> String {
    let (x, y, w, h) = l.bounds;
    // Окно сдвигается на доли пикселя (содержимое остаётся на месте):
    // координаты, кратные модулю, попадают в центр пикселя — линии
    // выходят одной толщины и без `shape-rendering`.
    let (ox, oy) = (half_px(x), half_px(y));
    let (w, h) = (x + w - ox + 0.375, y + h - oy + 0.375);
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
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
    put(&mut out, w / PT_PER_MM);
    out.push_str("mm\" height=\"");
    put(&mut out, h / PT_PER_MM);
    out.push_str("mm\" viewBox=\"");
    put(&mut out, ox);
    out.push(' ');
    put(&mut out, oy);
    out.push(' ');
    put(&mut out, w);
    out.push(' ');
    put(&mut out, h);
    out.push_str("\">\n");
    out.push_str(&body);
    out.push_str("</svg>\n");
    out
}

/// То же с внешним масштабом: пачка схем рисуется одним общим s,
/// чтобы фигуры во всех файлах пачки были одного визуального размера.
/// Всё рисуется в координатах раскладки,
/// единый масштаб применяется одним `<g transform>`: линии под
/// фигурами, фигуры под подписями. Весь документ собирается в двух
/// буферах (тело и документ) — без промежуточных строк на каждый элемент.
pub fn render_svg_at(l: &Layout, st: &Style, s: f64) -> String {
    let sheet = st.sheet;
    let s = snap_scale(s, st);
    // усики стрелок в локальных единицах: масштаб применит g-обёртка.
    let whisker = WHISKER_FRAC * 2.0 * st.grid;
    // лист центрирует содержимое в текстовой зоне; страница всегда
    // одного размера, а не обрезается по содержимому
    let (tx, ty) = sheet.origin(l.bounds, s);
    // Полпиксельная фаза. Модуль сетки — 19 px, координаты линий кратны
    // модулю, а масштаб подобран так, чтобы 19·s было целым; остаётся
    // поставить начало координат на полпикселя — и штрих в 1 px ляжет в
    // центр пикселя. Отсюда и отказ от `shape-rendering` crispEdges: он
    // лечил ту же болезнь, но ценой ступенек на дугах и пропадающих
    // линий при масштабе.
    let (tx, ty) = (half_px(tx), half_px(ty));

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
    put(&mut out, sheet.w);
    out.push_str("pt\" height=\"");
    put(&mut out, sheet.h);
    out.push_str("pt\" viewBox=\"0 0 ");
    put(&mut out, sheet.w);
    out.push(' ');
    put(&mut out, sheet.h);
    out.push_str("\">\n<rect x=\"0\" y=\"0\" width=\"");
    put(&mut out, sheet.w);
    out.push_str("\" height=\"");
    put(&mut out, sheet.h);
    out.push_str("\" fill=\"#ffffff\"/>\n<g transform=\"translate(");
    put(&mut out, tx);
    out.push(' ');
    put(&mut out, ty);
    out.push_str(") scale(");
    put_prec(&mut out, s, 6);
    out.push_str(")\" data-box=\"");
    // Габарит содержимого в координатах страницы: коллаж на сайте
    // показывает схему, а пустое поле листа. Без него плитка А4 с
    // крошечной схемой посреди листа — это пустая страница.
    put(&mut out, l.bounds.0 * s + tx);
    out.push(' ');
    put(&mut out, l.bounds.1 * s + ty);
    out.push(' ');
    put(&mut out, l.bounds.2 * s);
    out.push(' ');
    put(&mut out, l.bounds.3 * s);
    out.push_str("\">");
    out.push_str(&body);
    out.push_str("</g>\n</svg>\n");
    out
}

/// Порезка листа по содержимому: тот же `data-box`, но в координатах
/// страницы без обёртки масштаба — для тестов и для проверок.
#[cfg(test)]
mod box_tests {
    use super::*;
    use crate::frontend::cts::CParser;
    use crate::layout::{layout, normalize};

    fn st() -> Style {
        Style::with_metrics(14.0, 1.0)
    }

    /// `data-box` обязан лечь внутрь листа и совпасть с габаритом
    /// содержимого: плитка коллажа обрезает лист по нему, и ошибка
    /// здесь — обрезанный блок на главной странице сайта.
    #[test]
    fn data_box_is_inside_the_sheet_and_matches_bounds() {
        let nodes = CParser::new().parse("int main(void){ x = 1; }", "en");
        let s = st();
        let l = layout(&nodes, &normalize(&nodes, &s), &s);
        let svg = render_svg_at(&l, &s, 1.0);
        let boxv: Vec<f64> = svg
            .split("data-box=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        let sheet = s.sheet;
        assert_eq!(boxv.len(), 4, "в data-box должно быть четыре числа");
        assert!(
            boxv[0] >= 0.0 && boxv[1] >= 0.0,
            "рамка уходит в поле: {boxv:?}"
        );
        assert!(
            boxv[0] + boxv[2] <= sheet.w + 0.5,
            "рамка шире листа: {boxv:?} при ширине {}",
            sheet.w
        );
        assert!(
            boxv[1] + boxv[3] <= sheet.h + 0.5,
            "рамка выше листа: {boxv:?} при высоте {}",
            sheet.h
        );
        assert!(boxv[2] > 0.0 && boxv[3] > 0.0, "пустая рамка: {boxv:?}");
    }
}
