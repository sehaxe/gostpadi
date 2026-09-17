//! Открытый наконечник стрелки: развал 45°, два уса от острия.

use super::svg::n;

/// Путь усиков: острие `tip`, ось по `tip - prev`, длина уса `len`.
/// Чистая функция, без зависимостей от Layout.
pub(crate) fn arrow_path(tip: (f64, f64), prev: (f64, f64), len: f64) -> String {
    let (dx, dy) = (tip.0 - prev.0, tip.1 - prev.1);
    let d = dx.hypot(dy);
    if d < 1e-9 {
        return String::new();
    }
    // развал наконечника 45°, половина на сторону
    const HALF_SPREAD_DEG: f64 = 22.5;
    let (ux, uy) = (dx / d, dy / d);
    let (c, s) = (
        HALF_SPREAD_DEG.to_radians().cos(),
        HALF_SPREAD_DEG.to_radians().sin(),
    );
    // ось, повёрнутая на +22.5° и на -22.5°
    let (ax, ay) = (ux * c - uy * s, ux * s + uy * c);
    let (bx, by) = (ux * c + uy * s, uy * c - ux * s);
    format!(
        "M {x1} {y1} L {tx} {ty} M {x2} {y2} L {tx} {ty}",
        x1 = n(tip.0 - ax * len),
        y1 = n(tip.1 - ay * len),
        x2 = n(tip.0 - bx * len),
        y2 = n(tip.1 - by * len),
        tx = n(tip.0),
        ty = n(tip.1),
    )
}
