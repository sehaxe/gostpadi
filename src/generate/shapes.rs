//! Контуры фигур по ГОСТ 19.701-90 (вид A, все линии чёрные, заливка белая).

use super::svg::n;
use crate::layout::Shape;

/// tan(75°): наклон боковых сторон параллелограмма «ввод-вывод».
const TAN75: f64 = 3.732_050_807_568_877_6;

fn poly(pts: &[(f64, f64)]) -> String {
    let pts: Vec<String> = pts
        .iter()
        .map(|(x, y)| format!("{},{}", n(*x), n(*y)))
        .collect();
    format!("<polygon points=\"{}\"", pts.join(" "))
}

fn rect(x0: f64, y0: f64, w: f64, h: f64, rx: f64) -> String {
    let r = if rx > 0.0 {
        format!(" rx=\"{}\" ry=\"{}\"", n(rx), n(rx))
    } else {
        String::new()
    };
    format!(
        "<rect x=\"{0}\" y=\"{1}\" width=\"{2}\" height=\"{3}\"{4}",
        n(x0),
        n(y0),
        n(w),
        n(h),
        r
    )
}

/// Контур фигуры: polygon/rect/circle без заливки в атрибутах —
/// заливка и обводка добавляются общим стилем.
pub(crate) fn shape_svg(sh: &Shape, lw: f64) -> String {
    let (x0, y0) = (sh.cx - sh.w / 2.0, sh.cy - sh.h / 2.0);
    let (x1, y1) = (sh.cx + sh.w / 2.0, sh.cy + sh.h / 2.0);
    let body = match sh.kind.as_str() {
        // терминатор: капсула, rx = ry = h/2 (полуокружные торцы)
        "term" | "ret" => rect(x0, y0, sh.w, sh.h, sh.h / 2.0),
        "io" => {
            let dx = sh.h / TAN75;
            poly(&[(x0 + dx, y0), (x1, y0), (x1 - dx, y1), (x0, y1)])
        }
        "if" => poly(&[(x0, sh.cy), (sh.cx, y0), (x1, sh.cy), (sh.cx, y1)]),
        // подготовка: срезаны верхние углы
        "loop_begin" => poly(&[
            (x0 + sh.skew, y0),
            (x1 - sh.skew, y0),
            (x1, y0 + sh.skew),
            (x1, y1),
            (x0, y1),
            (x0, y0 + sh.skew),
        ]),
        // слияние/соединение цикла: срезаны нижние углы
        "loop_end" => poly(&[
            (x0, y0),
            (x1, y0),
            (x1, y1 - sh.skew),
            (x1 - sh.skew, y1),
            (x0 + sh.skew, y1),
            (x0, y1 - sh.skew),
        ]),
        "conn" => {
            return format!(
                "<circle cx=\"{0}\" cy=\"{1}\" r=\"{2}\"{s}",
                n(sh.cx),
                n(sh.cy),
                n(sh.w / 2.0),
                s = style(lw),
            )
        }
        // процесс и всё неизвестное — прямоугольник
        _ => rect(x0, y0, sh.w, sh.h, 0.0),
    };
    format!("{body}{s}", s = style(lw))
}

fn style(lw: f64) -> String {
    format!(
        " fill=\"white\" stroke=\"black\" stroke-width=\"{}\" stroke-linejoin=\"miter\"/>",
        n(lw)
    )
}
