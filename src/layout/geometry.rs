use super::types::{Edge, Layout, Shape};

/// Округление вверх до модульной сетки 5 мм (b = 2a, ГОСТ 19.701-90).
/// Поправка 1e-9 отсекает float-мусор вида 3.0000000001.
pub(super) fn up(v: f64, g: f64) -> f64 {
    (v / g - 1e-9).ceil() * g
}

const EPS: f64 = 1e-6;

/// Абсцисса вертикальной рельсы, отведённой от фигур на её участке.
/// Рельса не должна идти по кромке блока или сквозь него: если желаемый
/// x попадает в bbox фигуры, пересекающей [y0, y1] по высоте, рельса
/// уходит за её кромку на clearance. right — сторона обхода: правые
/// рельсы уходят вправо, левые влево, иначе рельса залезла бы в чужую
/// колонку. Обход повторяется, пока новая абсцисса не очистит все
/// фигуры: сдвиг за одну может попасть в соседнюю.
pub(super) fn clear_rail_x(
    shapes: &[Shape],
    x: f64,
    y0: f64,
    y1: f64,
    right: bool,
    clearance: f64,
) -> f64 {
    const TOL: f64 = 0.01;
    if clearance <= TOL {
        return x;
    }
    let (lo, hi) = (y0.min(y1), y0.max(y1));
    let mut out = x;
    loop {
        let mut moved = false;
        for sh in shapes {
            let (sy0, sy1) = (sh.cy - sh.h / 2.0, sh.cy + sh.h / 2.0);
            if sy1 <= lo + TOL || sy0 >= hi - TOL {
                continue;
            }
            let (l, r) = (sh.cx - sh.w / 2.0, sh.cx + sh.w / 2.0);
            if out > l - TOL && out < r + TOL {
                out = if right { r + clearance } else { l - clearance };
                moved = true;
            }
        }
        if !moved {
            return out;
        }
    }
}

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

/// X-пересечение двух сегментов разных рёбер. T-стык (конец одного
/// сегмента лежит на другом) — это слияние потока, оно легально;
/// коллинеарные наложения — отдельный класс, здесь не проверяются.
/// Пары «горизонталь × вертикаль»: диагоналей раскладка не рисует.
pub fn edges_cross_ok(l: &Layout) -> Result<(), String> {
    const TOL: f64 = 0.01;
    let is_h = |a: (f64, f64), b: (f64, f64)| (a.1 - b.1).abs() < 1e-9;
    let is_v = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9;
    for (i, ea) in l.edges.iter().enumerate() {
        for eb in l.edges.iter().skip(i + 1) {
            for pa in ea.points.windows(2) {
                for pb in eb.points.windows(2) {
                    let (a1, a2, b1, b2) = (pa[0], pa[1], pb[0], pb[1]);
                    // упорядочиваем: h — горизонталь, v — вертикаль
                    let (h, v) = if is_h(a1, a2) && is_v(b1, b2) {
                        ((a1, a2), (b1, b2))
                    } else if is_v(a1, a2) && is_h(b1, b2) {
                        ((b1, b2), (a1, a2))
                    } else {
                        continue;
                    };
                    let (hx1, hx2) = (h.0 .0.min(h.1 .0), h.0 .0.max(h.1 .0));
                    let (vy1, vy2) = (v.0 .1.min(v.1 .1), v.0 .1.max(v.1 .1));
                    let (x, y) = (v.0 .0, h.0 .1);
                    let inside =
                        x >= hx1 - TOL && x <= hx2 + TOL && y >= vy1 - TOL && y <= vy2 + TOL;
                    if !inside {
                        continue;
                    }
                    // конец любого из сегментов в точке встречи — стык
                    let at_end = [h.0, h.1, v.0, v.1]
                        .iter()
                        .any(|p| (p.0 - x).abs() <= TOL && (p.1 - y).abs() <= TOL);
                    if at_end {
                        continue;
                    }
                    return Err(format!(
                        "ребро #{i} {:?}-{:?} пересекает ребро {:?}-{:?} в ({x:.2}, {y:.2})",
                        a1, a2, b1, b2
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Двойная линия: два ребра идут по одной прямой и перекрываются.
/// Допустимо только T-слияние — короткий отрезок лежит внутри длинного и
/// делит с ним ровно один конец (хвост рельсы вливается в общий отрезок).
/// Копия, частичное перекрытие и внутренний отрезок без общего конца —
/// дефект: на рендере это жирная линия или «двойной» маршрут.
pub fn collinear_overlap_ok(l: &Layout) -> Result<(), String> {
    const TOL: f64 = 0.01;
    // (ребро, горизонталь?, координата прямой, начало, конец)
    let mut segs: Vec<(usize, bool, f64, f64, f64)> = Vec::new();
    for (i, e) in l.edges.iter().enumerate() {
        for w in e.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let h = (a.1 - b.1).abs() < 1e-9;
            let v = (a.0 - b.0).abs() < 1e-9;
            if h && v {
                continue;
            }
            if h {
                segs.push((i, true, a.1, a.0.min(b.0), a.0.max(b.0)));
            } else if v {
                segs.push((i, false, a.0, a.1.min(b.1), a.1.max(b.1)));
            }
        }
    }
    for (k, &(ei, h, line, lo, hi)) in segs.iter().enumerate() {
        for &(ej, hj, lj, a, b) in segs.iter().skip(k + 1) {
            if ei == ej || hj != h || (lj - line).abs() > TOL {
                continue;
            }
            let ov = hi.min(b) - lo.max(a);
            if ov <= TOL {
                continue; // касание концом: T-стык или общий угол
            }
            let a_in_b = a <= lo + TOL && b >= hi - TOL;
            let b_in_a = lo <= a + TOL && hi >= b - TOL;
            let share_lo = (lo - a).abs() <= TOL;
            let share_hi = (hi - b).abs() <= TOL;
            // T-слияние: меньший целиком внутри, общий ровно один конец
            let t_merge = (a_in_b || b_in_a) && (share_lo ^ share_hi);
            if t_merge {
                continue;
            }
            return Err(format!(
                "ребро #{ei} {lo:.2}..{hi:.2} наложено на ребро #{ej} {a:.2}..{b:.2} (общая координата {line:.2})"
            ));
        }
        // шина, собранная из кусков: отрезок целиком покрыт объединением
        // более коротких коллинеарных отрезков других рёбер
        let mut cover: Vec<(f64, f64)> = Vec::new();
        for &(ej, hj, lj, a, b) in &segs {
            if ej == ei || hj != h || (lj - line).abs() > TOL {
                continue;
            }
            if b - a < hi - lo - TOL && a >= lo - TOL && b <= hi + TOL {
                cover.push((a, b));
            }
        }
        cover.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let empty = cover.is_empty();
        let mut cur = lo;
        for (a, b) in cover {
            if a > cur + TOL {
                break;
            }
            cur = cur.max(b);
        }
        if cur >= hi - TOL && !empty {
            return Err(format!(
                "ребро #{ei} {lo:.2}..{hi:.2} собрано из кусков (общая координата {line:.2})"
            ));
        }
    }
    Ok(())
}

/// Плоские участки контура по сторонам bbox: (верх, низ, лево, право).
/// None — сторона не прямая (ромб, круг) либо её нет (скос
/// параллелограмма), и линия «на кромке» там не читается гранью.
fn flat_sides(sh: &Shape) -> [Option<(f64, f64)>; 4] {
    let (x0, x1) = (sh.cx - sh.w / 2.0, sh.cx + sh.w / 2.0);
    let (y0, y1) = (sh.cy - sh.h / 2.0, sh.cy + sh.h / 2.0);
    match sh.kind.as_str() {
        // капсула: прямые участки между полуокружностями
        "term" | "ret" => [
            Some((x0 + sh.h / 2.0, x1 - sh.h / 2.0)),
            Some((x0 + sh.h / 2.0, x1 - sh.h / 2.0)),
            Some((y0 + sh.h / 2.0, y1 - sh.h / 2.0)),
            Some((y0 + sh.h / 2.0, y1 - sh.h / 2.0)),
        ],
        // параллелограмм: боковые стороны наклонные, считаем только
        // горизонтальные кромки (наклон не совпадает с осью сегмента)
        "io" => {
            let dx = sh.h / 3.732_050_807_568_877_6; // tan(75°)
            [Some((x0 + dx, x1)), Some((x0, x1 - dx)), None, None]
        }
        "if" | "conn" => [None, None, None, None],
        "loop_begin" | "exit" => [
            Some((x0 + sh.skew, x1 - sh.skew)),
            Some((x0, x1)),
            Some((y0 + sh.skew, y1)),
            Some((y0 + sh.skew, y1)),
        ],
        "loop_end" => [
            Some((x0, x1)),
            Some((x0 + sh.skew, x1 - sh.skew)),
            Some((y0, y1 - sh.skew)),
            Some((y0, y1 - sh.skew)),
        ],
        _ => [
            Some((x0, x1)),
            Some((x0, x1)),
            Some((y0, y1)),
            Some((y0, y1)),
        ],
    }
}

/// Сегмент лежит на кромке фигуры (EPS 0.01) — линия сливается с
/// границей блока и читается как его грань. Вход в фигуру по нормали
/// (перпендикулярно кромке, пересечение нулевой длины) — не дефект.
pub fn edge_on_border_ok(l: &Layout) -> Result<(), String> {
    const TOL: f64 = 0.01;
    for (ei, e) in l.edges.iter().enumerate() {
        for seg in e.points.windows(2) {
            let (p1, p2) = (seg[0], seg[1]);
            let horiz = (p1.1 - p2.1).abs() < 1e-9;
            let vert = (p1.0 - p2.0).abs() < 1e-9;
            if !horiz && !vert {
                continue;
            }
            for (si, sh) in l.shapes.iter().enumerate() {
                let x0 = sh.cx - sh.w / 2.0;
                let x1 = sh.cx + sh.w / 2.0;
                let y0 = sh.cy - sh.h / 2.0;
                let y1 = sh.cy + sh.h / 2.0;
                let sides = flat_sides(sh);
                let (span, at) = if horiz {
                    ((p1.0.min(p2.0), p1.0.max(p2.0)), p1.1)
                } else {
                    ((p1.1.min(p2.1), p1.1.max(p2.1)), p1.0)
                };
                let candidates: [(f64, usize); 2] = if horiz {
                    [(y0, 0), (y1, 1)]
                } else {
                    [(x0, 2), (x1, 3)]
                };
                for &(coord, side) in &candidates {
                    if (at - coord).abs() > TOL {
                        continue;
                    }
                    let Some((a, b)) = sides[side] else {
                        continue;
                    };
                    let overlap = span.1.min(b) - span.0.max(a);
                    if overlap > TOL {
                        return Err(format!(
                            "ребро #{ei} сегмент {p1:?}-{p2:?} идёт по кромке фигуры #{si} ({})",
                            sh.kind
                        ));
                    }
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

/// Конец ребра не висит в пустоте: он касается фигуры, либо лежит на
/// другом ребре — в его точке или на внутренности (T-стык).
///
/// Обрыв линии читается на схеме как ошибка рисунка, поэтому это тот же
/// класс дефекта, что и пересечение, и проверять его нужно тем же
/// местом. Ловит разные по причине, но одинаковые по виду обрывы:
/// коридор каскада, свисающий над первой ветвью; спуск колонки,
/// кончающийся в пустоте над блоком соседней колонки; конец полосы,
/// оставшийся без кружка-соединителя.
pub fn dangling_ok(l: &Layout) -> Result<(), String> {
    const TOL: f64 = 0.75;
    let on = |p: (f64, f64), a: (f64, f64), b: (f64, f64)| -> bool {
        if (a.0 - b.0).abs() < 1e-9 {
            (p.0 - a.0).abs() <= TOL && p.1 >= a.1.min(b.1) - TOL && p.1 <= a.1.max(b.1) + TOL
        } else if (a.1 - b.1).abs() < 1e-9 {
            (p.1 - a.1).abs() <= TOL && p.0 >= a.0.min(b.0) - TOL && p.0 <= a.0.max(b.0) + TOL
        } else {
            // диагональ в раскладке быть не должно, касание только концами
            (p.0 - a.0).hypot(p.1 - a.1) <= TOL || (p.0 - b.0).hypot(p.1 - b.1) <= TOL
        }
    };
    for (ei, e) in l.edges.iter().enumerate() {
        let n = e.points.len();
        if n < 2 {
            continue;
        }
        for (k, &p) in [e.points[0], e.points[n - 1]].iter().enumerate() {
            if l.shapes.iter().any(|s| {
                (p.0 - s.cx).abs() <= s.w / 2.0 + TOL && (p.1 - s.cy).abs() <= s.h / 2.0 + TOL
            }) {
                continue;
            }
            if l.edges
                .iter()
                .enumerate()
                .any(|(j, o)| j != ei && o.points.windows(2).any(|w| on(p, w[0], w[1])))
            {
                continue;
            }
            return Err(format!("edge #{ei} конец #{k} {p:?} висит в пустоте"));
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(points: &[(f64, f64)]) -> Edge {
        Edge {
            points: points.to_vec(),
            arrow: false,
        }
    }

    fn layout(edges: Vec<Edge>) -> Layout {
        Layout {
            edges,
            ..Layout::default()
        }
    }

    fn shape(kind: &str, cx: f64, cy: f64, w: f64, h: f64) -> Shape {
        Shape {
            kind: kind.into(),
            cx,
            cy,
            w,
            h,
            skew: 0.0,
            lines: Vec::new(),
        }
    }

    #[test]
    fn x_crossing_is_defect() {
        let l = layout(vec![
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
            edge(&[(100.0, 0.0), (100.0, 100.0)]),
        ]);
        assert!(edges_cross_ok(&l).is_err(), "X в (100,50)");
    }

    #[test]
    fn t_junction_and_shared_corner_are_ok() {
        // T: конец вертикали на внутренности горизонтали
        let t = layout(vec![
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
            edge(&[(100.0, 50.0), (100.0, 100.0)]),
        ]);
        assert!(edges_cross_ok(&t).is_ok(), "T-стык — слияние потока");
        // общий угол двух ломаных
        let corner = layout(vec![
            edge(&[(0.0, 50.0), (100.0, 50.0)]),
            edge(&[(100.0, 50.0), (100.0, 100.0)]),
        ]);
        assert!(edges_cross_ok(&corner).is_ok());
    }

    #[test]
    fn double_line_is_defect() {
        // копия ребра
        let dup = layout(vec![
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
        ]);
        assert!(edges_cross_ok(&dup).is_ok(), "наложение — отдельный класс");
        assert!(
            collinear_overlap_ok(&dup).is_err(),
            "линия нарисована дважды"
        );
        // шина и повторно собранные из половин те же 170–708
        let bus = layout(vec![
            edge(&[(170.0, 50.0), (708.0, 50.0)]),
            edge(&[(170.0, 50.0), (439.0, 50.0)]),
            edge(&[(439.0, 50.0), (708.0, 50.0)]),
        ]);
        assert!(collinear_overlap_ok(&bus).is_err(), "шина из кусков");
        // вертикальная копия
        let vdup = layout(vec![
            edge(&[(10.0, 0.0), (10.0, 100.0)]),
            edge(&[(10.0, 0.0), (10.0, 100.0)]),
        ]);
        assert!(collinear_overlap_ok(&vdup).is_err());
    }

    #[test]
    fn collinear_shared_end_and_apart_are_ok() {
        // общий конец: продолжение линии, не наложение
        let joint = layout(vec![
            edge(&[(0.0, 50.0), (100.0, 50.0)]),
            edge(&[(100.0, 50.0), (200.0, 50.0)]),
        ]);
        assert!(collinear_overlap_ok(&joint).is_ok());
        // разные прямые
        let apart = layout(vec![
            edge(&[(0.0, 50.0), (100.0, 50.0)]),
            edge(&[(0.0, 60.0), (100.0, 60.0)]),
            edge(&[(0.0, 50.0), (0.0, 100.0)]),
        ]);
        assert!(collinear_overlap_ok(&apart).is_ok());
        // сходящиеся хвосты (рельсы в одну точку): короткий не копия
        // длинного и не шина из кусков — T-слияние, а не двойная линия
        let fan = layout(vec![
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
            edge(&[(100.0, 50.0), (200.0, 50.0)]),
        ]);
        assert!(collinear_overlap_ok(&fan).is_ok());
    }

    #[test]
    fn collinear_overlap_is_its_own_defect() {
        let l = layout(vec![
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
            edge(&[(100.0, 50.0), (300.0, 50.0)]),
        ]);
        assert!(collinear_overlap_ok(&l).is_err(), "общий отрезок дважды");
        // касание концом и T-стык — не наложение
        let touch = layout(vec![
            edge(&[(0.0, 50.0), (100.0, 50.0)]),
            edge(&[(100.0, 50.0), (200.0, 50.0)]),
        ]);
        assert!(collinear_overlap_ok(&touch).is_ok());
        let t = layout(vec![
            edge(&[(0.0, 50.0), (200.0, 50.0)]),
            edge(&[(100.0, 50.0), (100.0, 100.0)]),
        ]);
        assert!(collinear_overlap_ok(&t).is_ok());
    }

    #[test]
    fn segment_on_rect_border_is_defect() {
        let l = Layout {
            shapes: vec![shape("act", 100.0, 100.0, 80.0, 40.0)],
            edges: vec![edge(&[(60.0, 120.0), (120.0, 120.0)])],
            ..Layout::default()
        };
        assert!(edge_on_border_ok(&l).is_err(), "линия по нижней грани");
    }

    #[test]
    fn normal_entry_and_corner_touch_are_ok() {
        // вход сверху по нормали: пересечение с кромкой нулевой длины
        let entry = Layout {
            shapes: vec![shape("act", 100.0, 100.0, 80.0, 40.0)],
            edges: vec![edge(&[(100.0, 40.0), (100.0, 80.0)])],
            ..Layout::default()
        };
        assert!(edge_on_border_ok(&entry).is_ok());
        // касание угла: отрезок уходит влево от левого нижнего угла
        let corner = Layout {
            shapes: vec![shape("act", 100.0, 100.0, 80.0, 40.0)],
            edges: vec![edge(&[(60.0, 120.0), (0.0, 120.0)])],
            ..Layout::default()
        };
        assert!(edge_on_border_ok(&corner).is_ok());
    }

    #[test]
    fn diamond_sides_are_not_flat_borders() {
        // вертикаль из правой вершины ромба вниз: bbox-сторона не контур
        let l = Layout {
            shapes: vec![shape("if", 0.0, 100.0, 200.0, 100.0)],
            edges: vec![edge(&[(100.0, 100.0), (100.0, 160.0)])],
            ..Layout::default()
        };
        assert!(edge_on_border_ok(&l).is_ok());
    }
}
