//! Открытый наконечник стрелки: развал 45°, два уса от острия.

use super::svg::put;

/// Путь усиков прямо в буфер: острие `tip`, ось по `tip - prev`, длина
/// уса `len`. false — вырожденный отрезок (писать нечего).
/// Чистая функция, без зависимостей от Layout.
pub(crate) fn arrow_path(out: &mut String, tip: (f64, f64), prev: (f64, f64), len: f64) -> bool {
    let (dx, dy) = (tip.0 - prev.0, tip.1 - prev.1);
    let d = dx.hypot(dy);
    if d < 1e-9 {
        return false;
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
    out.push_str("M ");
    put(out, tip.0 - ax * len);
    out.push(' ');
    put(out, tip.1 - ay * len);
    out.push_str(" L ");
    put(out, tip.0);
    out.push(' ');
    put(out, tip.1);
    out.push_str(" M ");
    put(out, tip.0 - bx * len);
    out.push(' ');
    put(out, tip.1 - by * len);
    out.push_str(" L ");
    put(out, tip.0);
    out.push(' ');
    put(out, tip.1);
    true
}
