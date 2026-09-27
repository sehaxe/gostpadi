use super::types::{Edge, Layout, Shape};

/// Округление вверх до модульной сетки 5 мм (b = 2a, ГОСТ 19.701-90).
/// Поправка 1e-9 отсекает float-мусор вида 3.0000000001.
pub(super) fn up(v: f64, g: f64) -> f64 {
    (v / g - 1e-9).ceil() * g
}

const EPS: f64 = 1e-6;

/// Ни один сегмент ребра не проходит через внутренность фигуры
/// (касание границы в конечных точках разрешено). Прямоугольная
/// аппроксимация консервативна: трапеции/ромбы уже своего bbox.
pub fn crossings_ok(shapes: &[Shape], edges: &[Edge]) -> Result<(), String> {
    for (ei, e) in edges.iter().enumerate() {
        for seg in e.points.windows(2) {
            let (p1, p2) = (seg[0], seg[1]);
            let horiz = (p1.1 - p2.1).abs() < 1e-9;
            let vert = (p1.0 - p2.0).abs() < 1e-9;
            if !horiz && !vert {
                continue; // ортогональность проверяется отдельно (layout_orthogonal)
            }
            for (si, sh) in shapes.iter().enumerate() {
                let rx1 = sh.cx - sh.w / 2.0 + EPS;
                let rx2 = sh.cx + sh.w / 2.0 - EPS;
                let ry1 = sh.cy - sh.h / 2.0 + EPS;
                let ry2 = sh.cy + sh.h / 2.0 - EPS;
                let hit = if horiz {
                    p1.1 > ry1 && p1.1 < ry2 && p1.0.min(p2.0) < rx2 && p1.0.max(p2.0) > rx1
                } else {
                    p1.0 > rx1 && p1.0 < rx2 && p1.1.min(p2.1) < ry2 && p1.1.max(p2.1) > ry1
                };
                if hit {
                    return Err(format!(
                        "edge #{ei} seg {p1:?}-{p2:?} crosses shape #{si} ({})",
                        sh.kind
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(debug_assertions)]
pub(super) fn assert_no_crossing(shapes: &[Shape], edges: &[Edge]) {
    if let Err(e) = crossings_ok(shapes, edges) {
        panic!("layout: {e}");
    }
}

/// Попарно внутренности фигур не пересекаются (допуск 1.0 pt).
pub fn overlaps_ok(shapes: &[Shape]) -> bool {
    const TOL: f64 = 1.0;
    for (i, a) in shapes.iter().enumerate() {
        for b in &shapes[i + 1..] {
            let ox =
                (a.cx + a.w / 2.0).min(b.cx + b.w / 2.0) - (a.cx - a.w / 2.0).max(b.cx - b.w / 2.0);
            let oy =
                (a.cy + a.h / 2.0).min(b.cy + b.h / 2.0) - (a.cy - a.h / 2.0).max(b.cy - b.h / 2.0);
            if ox > TOL && oy > TOL {
                return false;
            }
        }
    }
    true
}

/// Single-Entry: в терминатор «конец» входит ровно одна стрелка потока.
/// Все ветки «-> конец» сливаются T-узлом над ним (ствол с arrow=true).
/// Легальные дополнительные стрелки — от входящих кружков-соединителей
/// (горизонталь слева на высоте «конца», рисует layout для conn-узлов).
pub fn single_entry_ok(l: &Layout) -> bool {
    // «конец» — самый нижний терминатор; если его нет (схема оборвалась
    // return'ом или лист промежуточный) — проверять нечего
    let Some(end) = l
        .shapes
        .iter()
        .filter(|s| s.kind == "term")
        .max_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return true;
    };
    if std::ptr::eq(end, &l.shapes[0]) {
        return true; // нарисован только «начало»
    }
    const TOL: f64 = 0.01;
    let into_end = |e: &Edge| match e.points.last() {
        Some(&(x, y)) => {
            e.arrow
                && (x - end.cx).abs() <= end.w / 2.0 + TOL
                && (y - end.cy).abs() <= end.h / 2.0 + TOL
        }
        None => false,
    };
    let inbound_from_conn = |e: &Edge| {
        let n = e.points.len();
        n >= 2 && {
            let (x1, y1) = e.points[n - 2];
            let (x2, y2) = e.points[n - 1];
            // горизонталь справа налево по тексту — кружок левее «конца»
            (y1 - y2).abs() < 1e-9 && x2 > x1
        }
    };
    l.edges
        .iter()
        .filter(|e| into_end(e) && !inbound_from_conn(e))
        .count()
        == 1
}
