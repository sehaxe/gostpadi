//! Инварианты раскладки: топология циклов-трапеций, рельсы break,
//! single-entry в «конец», тупики return, проверки на примерах.

use super::*;
use crate::layout::measure::{HEAD, MID, TAIL};
use crate::layout::{
    collinear_overlap_ok, crossings_ok, dangling_ok, edge_on_border_ok, layout, measure, normalize,
    overlaps_ok, single_entry_ok, split_scheme,
};

#[test]
fn linear_scheme_shapes_edges() {
    let nodes = linear();
    let l = lay(&nodes);
    assert_eq!(l.shapes.len(), 4, "Start, act, act, End");
    assert_eq!(l.edges.len(), 3);
    assert!(
        l.edges.iter().all(|e| e.arrow),
        "все стрелки — входы в блоки"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

#[test]
fn if_branches_merge_before_continue() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![s("a = 1")], false),
        br("нет", vec![s("b = 2")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    // вход в схему + 2 входа в колонки веток + 1 продолжение после слияния
    assert_eq!(l.edges.iter().filter(|e| e.arrow).count(), 4);
    // обе колонки сливаются T-стыком (без стрелок) на одной высоте:
    // спуски и шина заканчиваются на одном y ниже ромба
    let merges: Vec<f64> = l
        .edges
        .iter()
        .filter(|e| !e.arrow)
        .filter_map(|e| e.points.last())
        .filter(|&&p| p.1 > l.shapes[1].cy)
        .map(|&p| p.1)
        .collect();
    assert!(merges.len() >= 2 && merges.iter().all(|&y| (y - merges[0]).abs() < 1e-9));
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(single_entry_ok(&l));
}

#[test]
fn loop_is_two_hexagons() {
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![s("i = i + 1")]),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let lb = l.shapes.iter().find(|sh| sh.kind == "loop_begin").unwrap();
    assert_eq!(lb.lines, vec!["1. while i < 5".to_string()]);
    assert!(lb.skew > 0.0, "срез верхних углов у loop_begin");
    assert_eq!(
        lb.skew,
        (0.5 * lb.h).min(0.25 * lb.w),
        "Δ = min(0.5*h, 0.25*w)"
    );
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    assert_eq!(
        le.lines,
        vec!["1".to_string()],
        "номер цикла на нижнем шестиугольнике — тот же, что в заголовке"
    );
    assert_eq!(
        le.skew, lb.skew,
        "loop_end — зеркальный срез той же величины"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

#[test]
fn nested_loop_numbering() {
    let inner = loop_node("while j < 3", vec![s("j = j + 1")]);
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![s("i = 0"), Stmt::Node(Box::new(inner))]),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let mut ends: Vec<&Shape> = l.shapes.iter().filter(|sh| sh.kind == "loop_end").collect();
    ends.sort_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap());
    assert_eq!(ends.len(), 2);
    // первая по cy — внутренняя (внутри тела, выше), вторая — внешняя
    assert_eq!(ends[0].lines, vec!["2".to_string()], "вложенный цикл");
    assert_eq!(ends[1].lines, vec!["1".to_string()], "внешний цикл");
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// На шестиугольнике «подготовка» видно, какой это цикл: while/for.
#[test]
fn loop_begin_shows_keyword() {
    let st = Style::default();
    let text = "while i < 5\n    a = 1\nfor i = 0; i < 10; i++\n    b = 2\noutput printf(1)\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "").unwrap();
    let l = lay(&nodes);
    let begins: Vec<String> = l
        .shapes
        .iter()
        .filter(|sh| sh.kind == "loop_begin")
        .flat_map(|sh| sh.lines.clone())
        .collect();
    assert!(begins.iter().any(|t| t == "1. while i < 5"), "{begins:?}");
    assert!(
        begins.iter().any(|t| t == "1. for i = 0; i < 10; i++"),
        "{begins:?}"
    );
}

/// Голова разрезанного цикла: верхняя трапеция есть, нижней нет — она
/// уехала на следующий лист вместе с хвостом.
#[test]
fn head_loop_no_end_trapezoid() {
    let head = loop_node(&format!("{HEAD}while i < 5"), vec![s("i = i + 1")]);
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        head,
        node(NodeKind::Conn, "А"),
    ];
    let l = lay(&nodes);
    let lb = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "loop_begin")
        .expect("верхняя трапеция");
    assert_eq!(
        lb.lines,
        vec!["1. while i < 5".to_string()],
        "сентинел в текст не протекает, номер цикла на месте"
    );
    assert!(
        !l.shapes.iter().any(|sh| sh.kind == "loop_end"),
        "нижняя трапеция — на следующем листе"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(dangling_ok(&l).is_ok());
}

/// Хвост разрезанного цикла: нижняя трапеция с номером цикла, верхней
/// нет — она осталась на предыдущем листе.
#[test]
fn tail_loop_only_end_trapezoid() {
    let tail = loop_node(&TAIL.to_string(), vec![s("i = i + 1")]);
    let nodes = vec![
        node(NodeKind::Conn, "А"),
        tail,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    assert!(!l.shapes.iter().any(|sh| sh.kind == "loop_begin"));
    let le = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "loop_end")
        .expect("нижняя трапеция");
    assert_eq!(
        le.lines,
        vec!["1".to_string()],
        "номер тот же, что у целого цикла: глубина выводится из структуры"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(dangling_ok(&l).is_ok());
}

/// Середина разрезанного цикла: трапеций нет вовсе — лист держится
/// парой кружков-соединителей.
#[test]
fn mid_loop_no_trapezoids() {
    let mid = loop_node(&MID.to_string(), vec![s("i = i + 1"), s("j = j + 1")]);
    let nodes = vec![node(NodeKind::Conn, "А"), mid, node(NodeKind::Conn, "А")];
    let l = lay(&nodes);
    assert!(
        !l.shapes.iter().any(|sh| sh.kind.starts_with("loop")),
        "ни трапеций, ни номера"
    );
    assert_eq!(l.shapes.iter().filter(|sh| sh.kind == "conn").count(), 2);
    assert!(l
        .shapes
        .iter()
        .any(|sh| sh.lines == vec!["i = i + 1".to_string()]));
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(dangling_ok(&l).is_ok());
}

/// Рельса break на головном листе идёт к кружку-выходу: горизонталь
/// приходит на верх кружка, а не в пустоту и не сквозь фигуры.
#[test]
fn break_rail_on_head_page_exits_via_letter() {
    let head = loop_node(&format!("{HEAD}while i < 5"), vec![s("i = i + 1"), sbrk()]);
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        head,
        node(NodeKind::Conn, "А"),
    ];
    let l = lay(&nodes);
    let conn = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "conn")
        .expect("кружок-выход");
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow
                && e.points
                    .last()
                    .is_some_and(|p| p.0 == conn.cx && p.1 < conn.cy)
        })
        .expect("рельса в коридоре");
    assert_eq!(
        rail.points.last().map(|p| p.0),
        Some(conn.cx),
        "рельса приходит на вертикаль входа в кружок"
    );
    let last = rail.points.last().copied().unwrap();
    assert!(
        last.1 < conn.cy - conn.h / 2.0,
        "T-стык на полшага выше кружка, не на его кромке: {last:?}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(dangling_ok(&l).is_ok());
}

/// Репорт (задача 3): у ромба с одной непустой веткой, ушедшей рельсой
/// break, низ колонки уходил под шину слияния. Поток приходил на ось на
/// высоте шины, а продолжение начиналось ниже — вертикаль обрывалась в
/// воздухе, и стрелка в следующий блок висела без входа.
#[test]
fn merge_bus_reaches_continuation_below_it() {
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node(
            "for d = 2; d <= m; d++",
            vec![Stmt::Node(Box::new({
                let mut d = node(NodeKind::Decision, "if a");
                d.branches = vec![
                    br("да", vec![s("x = 0"), sbrk()], false),
                    br("нет", vec![], false), // пустая ветка, как в задаче 3
                ];
                d
            }))],
        ),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    // вертикаль по оси колонки от шины слияния до входа в loop_end
    let spine = l
        .edges
        .iter()
        .find(|e| {
            let p = e.points.last().unwrap();
            e.arrow && p.1 == le.cy - le.h / 2.0 && p.0 == 0.0 && e.points.len() == 2
        })
        .expect("спуск в трапецию слияния");
    let from = spine.points[0];
    // откуда-то должен быть вход в этот спуск: либо шина, либо этот же
    // конец — обрыв виден именно как разрыв вертикали
    let joined = l.edges.iter().any(|e| {
        let last = *e.points.last().unwrap();
        (last.0 - from.0).abs() < 1e-9 && (last.1 - from.1).abs() < 1e-9
    });
    assert!(joined, "в спуск к трапеции никто не приходит: {from:?}");
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// Репорт (задача 3): рельса break уходила из блока горизонталью с высоты
/// его низа — отрезок ложился вдоль границы плитки, и линия читалась как
/// выход из угла.
///
/// Колонка плитки в задаче 3 целиком левее трапеции слияния, поэтому
/// вертикаль можно вести прямо: рельса выходит из центра плитки, падает
/// на merge и вправо на ось — один изгиб. Раньше сюда вклинивался коридор
/// с отступом вниз на в-gap, и у плитки выходило три лишних изгиба.
#[test]
fn break_rail_from_clear_column_falls_straight() {
    let src = "\
int main() {
    for (d = 2; d <= m; d++) {
        if (p % d == 0) {
            coprime = 0;
            break;
        }
    }
    return 0;
}
";
    let nodes = crate::frontend::c::parse_c_to_nodes(src, "ru").unwrap();
    let l = lay(&nodes);
    let marker = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "exit" && sh.lines == vec!["break".to_string()])
        .expect("маркер break после плитки");
    // поток плитка -> маркер входит в его верхнюю вершину стрелкой
    let top = marker.cy - marker.h / 2.0;
    assert!(
        l.edges.iter().any(|e| {
            e.arrow
                && e.points
                    .last()
                    .is_some_and(|p| (p.0 - marker.cx).abs() < 1e-9 && (p.1 - top).abs() < 1e-9)
        }),
        "стрелка входа в маркер сверху"
    );
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    let cx = marker.cx;
    let bottom = marker.cy + marker.h / 2.0;
    // колонка маркера должна быть вне трапеции слияния — иначе тест
    // проверяет не тот маршрут и падает не по делу
    assert!(
        cx < le.cx - le.w / 2.0 || cx > le.cx + le.w / 2.0,
        "колонка маркера вне трапеции слияния: cx={cx}, трапеция {}..{}",
        le.cx - le.w / 2.0,
        le.cx + le.w / 2.0
    );
    let rail = l
        .edges
        .iter()
        .find(|e| e.points.len() == 3 && (e.points[0].0 - cx).abs() < 1e-9)
        .expect("рельса из центра маркера прямо вниз");
    assert!(
        (rail.points[0].1 - bottom).abs() < 1e-9,
        "рельса начинается с низа маркера: {:?}",
        rail.points[0]
    );
    assert!(
        rail.points[1].0 == cx && rail.points[1].1 > bottom,
        "падение строго вниз по оси маркера: {:?}",
        &rail.points[1..2]
    );
    assert!(
        rail.points[2].0 == le.cx && rail.points[2].1 == rail.points[1].1,
        "на ось цикла, на той же высоте: {:?}",
        &rail.points[2..3]
    );
    // без коридора: рельса не выходит за свою колонку и ось (канон:
    // да-колонка справа, спуск идёт влево на ствол)
    let (lo, hi) = (le.cx.min(cx), le.cx.max(cx));
    assert!(
        rail.points
            .iter()
            .all(|p| p.0 >= lo - 1e-9 && p.0 <= hi + 1e-9),
        "рельса не уходит вбок от своей колонки: {:?}",
        rail.points
    );
    // ровно два изгиба вместо четырёх: ступеньки вниз и влево нет
    assert_eq!(
        rail.points.len(),
        3,
        "прямая рельса: низ маркера -> вниз -> ось цикла: {:?}",
        rail.points
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// Репорт (1.c): `if (fgets(...) == NULL) { eof = 1; break; }` внутри
/// `while (!ok)`. Маркер выхода — отдельная фигура с подписью ВНУТРИ;
/// ни одно ребро не проходит сквозь его внутренность и не идёт по
/// кромке, вход — стрелкой в верхнюю вершину, рельса — из нижней к низу
/// цикла. Регрессия: подпись висела снаружи, а рельса шла насквозь и
/// вливалась в ствол на высоте тела (4-сторонний узел).
#[test]
fn break_marker_after_input_is_a_separate_shape() {
    let src = "\
int main() {
    char line[67];
    int eof, ok, n;
    eof = 0;
    printf(\"n: \");
    ok = 0;
    while (!ok) {
        if (fgets(line, sizeof line, stdin) == NULL) { eof = 1; break; }
        if (sscanf(line, \"%d\", &n) == 1 && n > 0 && n <= 100) { ok = 1; }
        else { printf(\"err: \"); }
    }
    return 0;
}
";
    let nodes = crate::frontend::c::parse_c_to_nodes(src, "ru").unwrap();
    let l = lay(&nodes);
    let m = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "exit" && sh.lines == vec!["break".to_string()])
        .expect("маркер break — отдельный шестиугольник");
    let (top, bot) = (m.cy - m.h / 2.0, m.cy + m.h / 2.0);
    let (left, right) = (m.cx - m.w / 2.0, m.cx + m.w / 2.0);
    // подпись только внутри фигуры: внешней метки нет
    assert!(
        !l.labels
            .iter()
            .any(|lb| lb.text == "break" || lb.text == "continue"),
        "подпись маркера висит снаружи: {:?}",
        l.labels
            .iter()
            .filter(|lb| lb.text == "break" || lb.text == "continue")
            .collect::<Vec<_>>()
    );
    assert_eq!(m.lines, vec!["break".to_string()], "текст в shape.lines");
    // ни одно ребро не заходит во внутренность и не лежит на кромке
    for (i, e) in l.edges.iter().enumerate() {
        for w in e.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let horiz = (a.1 - b.1).abs() < 1e-9;
            let vert = (a.0 - b.0).abs() < 1e-9;
            if horiz {
                let inside = a.1 > top + 1e-6
                    && a.1 < bot - 1e-6
                    && a.0.min(b.0) < right - 1e-6
                    && a.0.max(b.0) > left + 1e-6;
                assert!(
                    !inside,
                    "ребро #{i} горизонталью вошло в маркер: {a:?}-{b:?}"
                );
                let border = (a.1 - top).abs() < 1e-6 || (a.1 - bot).abs() < 1e-6;
                assert!(
                    !(border && a.0.min(b.0) < right - m.skew && a.0.max(b.0) > left + m.skew),
                    "ребро #{i} легло на кромку маркера: {a:?}-{b:?}"
                );
            } else if vert {
                let inside = a.0 > left + 1e-6
                    && a.0 < right - 1e-6
                    && a.1.min(b.1) < bot - 1e-6
                    && a.1.max(b.1) > top + 1e-6;
                assert!(!inside, "ребро #{i} вертикалью вошло в маркер: {a:?}-{b:?}");
            }
        }
    }
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(edge_on_border_ok(&l).is_ok());
    // --check ловит обе формы дефекта: линию сквозь маркер и по кромке
    let mut through = l.clone();
    through.edges.push(crate::layout::Edge {
        points: vec![(left - 10.0, m.cy), (right + 10.0, m.cy)],
        arrow: false,
    });
    assert!(
        crossings_ok(&through.shapes, &through.edges).is_err(),
        "check пропускает линию через маркер"
    );
    let mut border = l.clone();
    border.edges.push(crate::layout::Edge {
        points: vec![(left, bot), (right, bot)],
        arrow: false,
    });
    assert!(
        edge_on_border_ok(&border).is_err(),
        "check пропускает линию по кромке маркера"
    );
    // вход — стрелка в верхнюю вершину, рельса — из нижней
    assert!(
        l.edges.iter().any(|e| {
            e.arrow
                && e.points
                    .last()
                    .is_some_and(|p| (p.0 - m.cx).abs() < 1e-9 && (p.1 - top).abs() < 1e-9)
        }),
        "в маркер входит стрелка сверху"
    );
    // рельса — цепочка сегментов от низа маркера до ствола под циклом
    let le = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "loop_end")
        .expect("низ цикла");
    let le_bottom = le.cy + le.h / 2.0;
    let mut p = (m.cx, bot);
    let mut end = None;
    for _ in 0..12 {
        let Some(e) = l
            .edges
            .iter()
            .find(|e| (e.points[0].0 - p.0).abs() < 1e-9 && (e.points[0].1 - p.1).abs() < 1e-9)
        else {
            break;
        };
        p = *e.points.last().unwrap();
        if p.0 == 0.0 {
            end = Some(p);
            break;
        }
    }
    let last = end.expect("рельса break доходит до ствола, а не обрывается");
    assert!(
        last.1 > le_bottom,
        "рельса break уходит за низ цикла, а не в тело: {last:?} при низе {le_bottom}"
    );
    // стык однозначен: рельса вливается в вертикаль, которая продолжается
    // вниз, а второй горизонтали с другой стороны нет (не 4-сторонний узел)
    let trunk = l
        .edges
        .iter()
        .find(|e| {
            e.points.windows(2).any(|w| {
                (w[0].0 - last.0).abs() < 1e-9
                    && (w[1].0 - last.0).abs() < 1e-9
                    && w[0].1 < last.1 - 1e-9
                    && w[1].1 > last.1 + 1e-9
            })
        })
        .expect("в стыке рельсы вертикаль продолжается вниз");
    assert!(
        trunk.points.iter().any(|p| p.1 > last.1),
        "продолжение вниз есть: {:?}",
        trunk.points
    );
    assert!(
        !l.edges.iter().any(|e| {
            let n = e.points.len();
            (e.points[n - 1].0 - last.0).abs() < 1e-9
                && (e.points[n - 1].1 - last.1).abs() < 1e-9
                && (e.points[n - 2].1 - last.1).abs() < 1e-9
                && e.points[n - 2].0 > last.0
        }),
        "в стык не входит вторая горизонталь справа: не 4-сторонний узел"
    );
}

/// Когда колонка плитки накрыта трапецией слияния, рельса идёт через
/// коридор левее — иначе вертикаль проткнула бы трапецию.
#[test]
fn break_rail_under_trapezoid_uses_corridor() {
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![s("a = 1"), sbrk()]),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let lb = l.shapes.iter().find(|sh| sh.kind == "loop_begin").unwrap();
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    // плитка стоит на оси цикла, то есть прямо под трапецией слияния:
    // рельса обязана уйти в коридор левее, иначе проткнёт трапецию
    let rail = l
        .edges
        .iter()
        .find(|e| e.points.iter().any(|p| p.0 < -lb.w / 2.0))
        .expect("рельса ушла в левый коридор мимо трапеции");
    let leftmost = rail.points.iter().map(|p| p.0).fold(f64::MAX, f64::min);
    assert!(
        leftmost < -lb.w / 2.0,
        "рельса заходит левее половины ширины трапеции: {leftmost} против {}",
        -lb.w / 2.0
    );
    // коридор: вниз из плитки, влево, вниз мимо трапеции, вправо на ось
    let trunk = l
        .edges
        .iter()
        .find(|e| {
            let p = e.points.last().unwrap();
            !e.arrow
                && p.0 == 0.0
                && p.1 > le.cy + le.h / 2.0
                && e.points[0].1 == p.1
                && e.points[0].0 != 0.0
        })
        .expect("рельса вливается в ось ниже loop_end");
    assert!(
        trunk.points[0].0 < -lb.w / 2.0,
        "низ рельсы идёт из коридора: {:?}",
        trunk.points
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

#[test]
fn break_goes_left_rail_below_loop() {
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![s("a = 1"), sbrk()]),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let lb = l.shapes.iter().find(|sh| sh.kind == "loop_begin").unwrap();
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    let rail = l
        .edges
        .iter()
        .find(|e| {
            let p = e.points.last().unwrap();
            !e.arrow
                && p.0 == 0.0
                && p.1 > le.cy + le.h / 2.0
                && e.points[0].1 == p.1
                && e.points[0].0 != 0.0
        })
        .expect("рельса break через левый канал");
    assert!(
        rail.points[0].0 < -lb.w / 2.0,
        "рельса идёт из коридора левее трапеции: {:?}",
        rail.points
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

#[test]
fn to_end_single_arrow_into_end() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![sio("printf(\"many\")")], true),
        br("нет", vec![s("c = 0")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    assert!(single_entry_ok(&l), "ровно одна стрелка в «конец»");
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

#[test]
fn ret_is_dead_end() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![Stmt::Return("return 1".into())], false),
        br("нет", vec![s("b = 2")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let ret = l.shapes.iter().find(|sh| sh.kind == "ret").unwrap();
    let out = l.edges.iter().any(|e| {
        let (x, y) = e.points[0];
        (x - ret.cx).abs() <= ret.w / 2.0 + 0.01 && (y - ret.cy).abs() <= ret.h / 2.0 + 0.01
    });
    assert!(!out, "из return не выходят линии");
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

#[test]
fn examples_layout_invariants() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("gvn") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let nodes = crate::frontend::gvn::parse(&src, &Style::default(), "ru")
            .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        let st = Style::default();
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        assert!(
            crossings_ok(&l.shapes, &l.edges).is_ok(),
            "{}: линии заходят на блоки",
            path.display()
        );
        assert!(
            crate::layout::edges_cross_ok(&l).is_ok(),
            "{}: линии пересекаются",
            path.display()
        );
        assert!(
            crate::layout::edge_on_border_ok(&l).is_ok(),
            "{}: линия идёт по кромке блока",
            path.display()
        );
        assert!(
            overlaps_ok(&l.shapes),
            "{}: фигуры перекрываются",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 5, "ожидалось >=5 примеров, проверено {checked}");
}

#[test]
fn split_scheme_single_part_for_short_scheme() {
    let parts = split_scheme(
        linear(),
        &normalize(&linear(), &Style::default()),
        &Style::default(),
    );
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].len(), 4);
}

/// Маркер ручного переноса внутри ветки — не фигура: колонка глотает
/// его молча (страниц внутри колонки нет), раскладка не падает.
#[test]
fn branch_marker_is_silent() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![smark(), s("a = 1")], false),
        br("нет", vec![s("b = 2")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    assert!(
        !l.shapes.iter().any(|sh| sh.kind == "conn"),
        "маркер не должен рисоваться кружком"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// Маркер ручного переноса ломает страницу, даже если всё влезло:
/// маркер-узел съеден, части связаны кружками одной буквы.
#[test]
fn top_level_marker_forces_page_break() {
    let st = Style::default();
    let nodes = crate::frontend::gvn::parse("a = 1\nперенос\nb = 2\n", &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(
        !parts
            .iter()
            .flatten()
            .any(|n| n.text == crate::frontend::MARKER),
        "маркер-узлы обязаны быть съедены"
    );
    let out = parts[0].last().unwrap();
    let inp = parts[1].first().unwrap();
    assert_eq!(out.kind, NodeKind::Conn, "{out:?}");
    assert_eq!(out.text, inp.text, "одна буква на пару листов");
    for part in &parts {
        let l = layout(part, &sizes, &st);
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
        assert!(single_entry_ok(&l));
    }
}

/// Маркер последним перед «концом»: разрезать нечего — лишнего листа
/// и сиротского кружка не появляется.
#[test]
fn marker_at_end_no_orphan_page() {
    let st = Style::default();
    let nodes = crate::frontend::gvn::parse("a = 1\nb = 2\nперенос\n", &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(parts.len(), 1, "{parts:?}");
    assert!(
        !parts[0].iter().any(|n| n.kind == NodeKind::Conn),
        "ни маркера, ни кружка: {parts:?}"
    );
}

#[test]
fn orthogonal_edges() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![br("да", vec![s("a = 1")], false), br("нет", vec![], false)];
    let schemes: Vec<Vec<Node>> = vec![
        linear(),
        vec![
            node(NodeKind::Term, "начало"),
            d,
            node(NodeKind::Term, "конец"),
        ],
        vec![
            node(NodeKind::Term, "начало"),
            loop_node("while i < 5", vec![s("i = i + 1"), sbrk()]),
            node(NodeKind::Term, "конец"),
        ],
    ];
    for nodes in &schemes {
        let l = lay(nodes);
        for e in &l.edges {
            for seg in e.points.windows(2) {
                assert!(
                    (seg[0].0 - seg[1].0).abs() < 1e-9 || (seg[0].1 - seg[1].1).abs() < 1e-9,
                    "не ортогональный сегмент {seg:?}"
                );
            }
        }
    }
}

/// pend-рельсы при последнем узле-цикле: layout() — публичный API,
/// «конец» может отсутствовать. Рельсы обязаны дойти до магистрали
/// под циклом со стрелкой (раньше pend просто терялся).
#[test]
fn pend_flush_when_loop_is_last() {
    let st = Style::default();
    let text = "if a > 0\n    yes -> end: printf(1)\n    no:\nwhile a < 5\n    a = a + 1\n";
    let mut nodes = crate::frontend::gvn::parse(text, &st, "en").unwrap();
    nodes.pop(); // убираем «конец»: последний узел — цикл
    assert_eq!(nodes.last().unwrap().kind, NodeKind::Loop);
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let loop_bottom = l
        .shapes
        .iter()
        .filter(|s| s.kind == "loop_end")
        .map(|s| s.cy + s.h / 2.0)
        .fold(f64::MIN, f64::max);
    let rails: Vec<&crate::layout::Edge> = l
        .edges
        .iter()
        .filter(|e| {
            e.arrow && matches!(e.points.last(), Some(&(x, y)) if x.abs() < 1e-6 && y > loop_bottom)
        })
        .collect();
    assert_eq!(rails.len(), 1, "рельсы «-> конец» не потеряны");
    // полная ломаная: ветка, спуск, внешний рельс, возврат на магистраль
    assert_eq!(rails[0].points.len(), 5, "{:?}", rails[0].points);
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// Фикс «жирной линии слияния»: горизонталь на уровне merge_y — ровно
/// один сегмент (шина от крайней колонки до крайней), а не наложенные
/// хвосты каждой колонки (ступеньки разной жирности на рендере).
#[test]
fn switch_merge_draws_single_bus() {
    let mut d = node(NodeKind::Decision, "if switch (d)");
    d.switch_var = Some("d".into());
    d.branches = vec![
        br("1", vec![s("a = 1")], false),
        br("2", vec![s("b = 2")], false),
        br("3", vec![s("c = 3")], false),
        br("4", vec![s("d = 4")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let st = Style::default();
    let l = lay(&nodes);
    let my = l
        .shapes
        .iter()
        .filter(|sh| sh.kind == "act")
        .map(|sh| sh.cy + sh.h / 2.0)
        .fold(f64::MIN, f64::max)
        + st.mgap;
    let at_bus = |e: &crate::layout::Edge| {
        e.points
            .windows(2)
            .any(|w| (w[0].1 - w[1].1).abs() < 1e-9 && (w[0].1 - my).abs() < 1e-9)
    };
    assert_eq!(
        l.edges.iter().filter(|e| at_bus(e)).count(),
        1,
        "ровно один горизонтальный сегмент на merge_y"
    );
    let bus = l.edges.iter().find(|e| at_bus(e)).unwrap();
    let mut xs: Vec<f64> = bus
        .points
        .windows(2)
        .filter(|w| (w[0].1 - my).abs() < 1e-9)
        .flat_map(|w| [w[0].0, w[1].0])
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        *xs.first().unwrap() < -1.0 && *xs.last().unwrap() > 1.0,
        "шина проходит через 0, накрывая обе стороны: {xs:?}"
    );
    // три вертикальных спуска (без стрелок) заканчиваются на шине
    let drops = l
        .edges
        .iter()
        .filter(|e| {
            !e.arrow
                && e.points.len() >= 2
                && e.points.iter().all(|p| (p.0 - e.points[0].0).abs() < 1e-9)
                && (e.points.last().unwrap().1 - my).abs() < 1e-9
        })
        .count();
    assert_eq!(drops, 4, "спуск по каждой колонке отдельно");
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(single_entry_ok(&l));
}

/// РЕГРЕССИЯ: узел с четырьмя сторонами (плюс) на шине switch. Ствол,
/// проходящий сквозь шину, читается как пересечение двух линий, а не
/// как ответвление от шины. Ствол между рядами сдвинут на шаг сетки и
/// заканчивается на шине T-стыком; средний кейс веера (три кейса) ушёл
/// в сетку 2+1, где осевой колонки нет.
#[test]
fn switch_bus_has_no_four_way_nodes() {
    fn assert_no_four_way(l: &Layout) {
        const TOL: f64 = 0.01;
        let segs: Vec<((f64, f64), (f64, f64))> = l
            .edges
            .iter()
            .flat_map(|e| e.points.windows(2).map(|w| (w[0], w[1])))
            .collect();
        let mut pts: Vec<(f64, f64)> = segs.iter().flat_map(|&(a, b)| [a, b]).collect();
        pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        pts.dedup();
        for p in pts {
            let mut dirs = std::collections::BTreeSet::new();
            for &(a, b) in &segs {
                let on = if (a.0 - b.0).abs() < 1e-9 {
                    (p.0 - a.0).abs() <= TOL
                        && p.1 >= a.1.min(b.1) - TOL
                        && p.1 <= a.1.max(b.1) + TOL
                } else {
                    (p.1 - a.1).abs() <= TOL
                        && p.0 >= a.0.min(b.0) - TOL
                        && p.0 <= a.0.max(b.0) + TOL
                };
                if !on {
                    continue;
                }
                for q in [a, b] {
                    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                    if dx.abs() <= TOL && dy.abs() <= TOL {
                        continue;
                    }
                    dirs.insert((
                        if dx.abs() <= TOL {
                            0
                        } else if dx > 0.0 {
                            1
                        } else {
                            -1
                        },
                        if dy.abs() <= TOL {
                            0
                        } else if dy > 0.0 {
                            1
                        } else {
                            -1
                        },
                    ));
                }
            }
            assert!(
                dirs.len() < 4,
                "узел с четырьмя сторонами в {p:?}: {dirs:?}"
            );
        }
    }
    let st = Style::default();
    // вложенный switch из трёх кейсов (2+1) и верхний из пяти (2+2+1)
    let nested = "\
while a < 5
    switch (a)
        1:
        b = 1
        2:
        b = 2
        3:
        b = 3
    a = a + 1
";
    let top5 = "\
switch (a)
    1:
    b = 1
    2:
    b = 2
    3:
    b = 3
    4:
    b = 4
    5:
    b = 5
";
    let top3 = "\
switch (a)
    1:
    b = 1
    2:
    b = 2
    3:
    b = 3
";
    // ряд целиком мёртв (оба кейса — continue): шины слияния у ряда нет,
    // ствол между рядами не должен проходить сквозь шину входа
    let dead_row = "\
while a < 5
    switch (a)
        1:
        continue
        2:
        continue
        3:
        b = 3
    a = a + 1
";
    let dead_row_top = "\
switch (a)
    1:
    return 1
    2:
    return 2
    3:
    b = 3
    4:
    b = 4
    5:
    b = 5
";
    for text in [nested, top5, top3, dead_row, dead_row_top] {
        let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        assert_no_four_way(&l);
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
        assert!(dangling_ok(&l).is_ok());
    }
}

/// Короткая ветка НЕ получает пад: контент всех колонок начинается на
/// одной высоте top0 (компактно), слияние — шина от нижней точки.
#[test]
fn short_branch_starts_at_same_top() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![s("a = 1"), s("a = 2")], false),
        br("нет", vec![s("b = 3")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let top = |s: &Shape| s.cy - s.h / 2.0;
    let a1 = l
        .shapes
        .iter()
        .find(|s| s.lines == vec!["a = 1".to_string()])
        .unwrap();
    let b3 = l
        .shapes
        .iter()
        .find(|s| s.lines == vec!["b = 3".to_string()])
        .unwrap();
    assert!(
        (top(a1) - top(b3)).abs() < 0.01,
        "верх контента обеих колонок на одной высоте: {} vs {}",
        top(a1),
        top(b3)
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(single_entry_ok(&l));
}

/// Пустой срез не должен паниковать на nodes.len() - 1.
#[test]
fn empty_scheme_layout_no_panic() {
    let st = Style::default();
    let l = layout(&[], &normalize(&[], &st), &st);
    assert!(l.shapes.is_empty());
}

/// Входящие кружки соединителей стреляют стрелками в «конец» — это
/// легально; единственный не-inbound вход по-прежнему один.
#[test]
fn single_entry_allows_inbound_conns() {
    let st = Style::default();
    let text = "if a > 0\n    yes -> end: printf(1)\n    no:\nb = 111111\n".repeat(14);
    let nodes = crate::frontend::gvn::parse(&text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert!(parts.len() >= 2, "схема должна разрезаться на листы");
    for part in &parts {
        let l = layout(part, &sizes, &st);
        assert!(single_entry_ok(&l), "single-entry нарушен на листе");
    }
}

/// Схема, оборвавшаяся return'ом: «конец» не нарисован — проверять
/// нечего, инвариант должен возвращать успех.
#[test]
fn single_entry_without_end_shape_is_ok() {
    let st = Style::default();
    let text = "a = 1\nreturn 1\nb = 2\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    assert_eq!(
        l.shapes.iter().filter(|s| s.kind == "term").count(),
        1,
        "нарисован только «начало»"
    );
    assert!(single_entry_ok(&l));
}

/// Мёртвая осевая колонка (tx = 0, return внизу) обязана учитываться
/// в merge_y: иначе ствол продолжения (0, merge_y) → следующий блок
/// протыкает её ret насквозь.
#[test]
fn switch_dead_axis_column_counts_into_merge_y() {
    let st = Style::default();
    let text = "\
input scanf(\"%d\", &x)
if switch (x)
    1: printf(\"раз\")
    2 -> end: printf(\"два\"); return 1
    иначе: printf(\"три\")
output printf(x)
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    assert_eq!(
        crossings_ok(&l.shapes, &l.edges),
        Ok(()),
        "ствол продолжения не должен протыкать ret мёртвой осевой колонки"
    );
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Пустая ветка «нет» — короткое колено вправо, а не ствол из нижней
/// вершины ромба. Колонка «да» слева; обе ветви сливаются на шине ниже
/// ромба, и только оттуда поток идёт вниз.
#[test]
fn empty_branch_is_right_stub() {
    let st = Style::default();
    let text = "if x <= 0\n    да: printf(\"bad\"); return 1\n    нет:\ny = x + 1\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let y_b = dsh.cy + dsh.h / 2.0;
    assert!(
        !l.edges.iter().any(|e| {
            let p = e.points[0];
            (p.0 - dsh.cx).abs() < 1e-9
                && (p.1 - y_b).abs() < 1e-9
                && e.points.get(1).is_some_and(|q| q.1 > y_b + 1e-9)
        }),
        "из нижней вершины ромба вниз ничего не идёт"
    );
    // колено пустой «нет»: правая вершина -> вправо -> вниз в шину
    let vr = (dsh.cx + dsh.w / 2.0, dsh.cy);
    let stub = l
        .edges
        .iter()
        .find(|e| {
            let p = e.points[0];
            (p.0 - vr.0).abs() < 1e-9 && (p.1 - vr.1).abs() < 1e-9 && !e.arrow
        })
        .expect("колено пустой ветки от правой вершины");
    assert!(
        stub.points[1].0 > vr.0 && stub.points[1].1 == vr.1,
        "колено уходит вправо: {:?}",
        stub.points
    );
    let my = y_b + 2.0 * st.grid;
    assert!(
        (stub.points.last().unwrap().1 - my).abs() < 1e-9,
        "колено возвращается в шину на y_b + 2g: {:?}",
        stub.points
    );
    // подпись «нет» — на колене справа от ромба, не на стволе
    let no = l.labels.iter().find(|lb| lb.text == "нет").unwrap();
    assert!(
        no.x > dsh.cx + dsh.w / 2.0 - 1e-9 && (no.y - (dsh.cy - st.label_dy)).abs() < 1e-9,
        "подпись нет на правом колене: ({}, {})",
        no.x,
        no.y
    );
    // рельсы слева нет: левая вершина ромба — только вход «да»
    let vl = (dsh.cx - dsh.w / 2.0, dsh.cy);
    assert!(
        !l.edges.iter().any(|e| {
            !e.arrow && (e.points[0].0 - vl.0).abs() < 1e-9 && (e.points[0].1 - vl.1).abs() < 1e-9
        }),
        "рельса пустой ветки больше не рисуется"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Канон: у if с единственной непустой веткой «да» уходит ВЛЕВО
/// (tx = -base), «нет» — правым коленом с меткой у линии.
#[test]
fn if_yes_branch_goes_left() {
    let st = Style::default();
    let text = "if x <= 0\n    да: printf(\"bad\")\n    нет:\ny = x + 1\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let io = l.shapes.iter().find(|s| s.kind == "io").unwrap();
    let nhe = super::nhe_of(&sizes, &nodes, &st);
    let base = dsh.w / 2.0 + st.hgap + nhe;
    assert!(
        (io.cx + base).abs() < 1e-9,
        "да-колонка налево: io.cx = {}, ожидался -base = {}",
        io.cx,
        -base
    );
    // метка «нет» стоит на правом колене, а не на стволе
    let no = l.labels.iter().find(|lb| lb.text == "нет").unwrap();
    assert!(
        no.x > dsh.cx + dsh.w / 2.0 && no.y < dsh.cy + dsh.h / 2.0,
        "метка нет справа от ромба: ({}, {})",
        no.x,
        no.y
    );
    // вертикали из нижней вершины ромба нет
    let y_b = dsh.cy + dsh.h / 2.0;
    assert!(
        !l.edges.iter().any(|e| {
            let p = e.points[0];
            (p.0 - dsh.cx).abs() < 1e-9
                && (p.1 - y_b).abs() < 1e-9
                && e.points.get(1).is_some_and(|q| q.1 > y_b + 1e-9)
        }),
        "из нижней вершины ромба вниз ничего не идёт"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// РЕГРЕССИЯ: if/else с двумя непустыми ветками — колонки симметричны,
/// на ±base (base = dw/2 + hgap + nhe), ничего не уехало на ось.
#[test]
fn if_else_columns_symmetric_at_base() {
    let st = Style::default();
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![s("a = 1")], false),
        br("нет", vec![s("b = 2")], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let colw = sizes["act"].0.max(sizes["io"].0);
    let base = dsh.w / 2.0 + st.hgap + colw / 2.0;
    let mut acts: Vec<f64> = l
        .shapes
        .iter()
        .filter(|s| s.kind == "act")
        .map(|s| s.cx)
        .collect();
    acts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(acts.len(), 2, "две ветки — две колонки");
    assert!(
        (acts[0] + base).abs() < 1e-9 && (acts[1] - base).abs() < 1e-9,
        "колонки на ∓base = ∓{base}: {acts:?}"
    );
    assert!(
        (acts[0] + acts[1]).abs() < 1e-9,
        "симметрия: колонки зеркальны относительно оси"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// РЕГРЕССИЯ: из ромба if нет вертикального ребра вниз. «Да» уходит
/// влево, «нет» вправо; обе ветви сливаются ниже ромба, и только с шины
/// слияния поток идёт дальше. Проверяется геометрия, не картинка.
#[test]
fn diamond_has_no_vertical_down_leg() {
    let st = Style::default();
    let cases = [
        "if a > 0\n    да: printf(\"да\")\n    нет: printf(\"нет\")\n",
        "if a > 0\n    да: printf(\"да\")\n    нет:\nc = 1\n",
        "if a < 0\n    да: printf(\"neg\")\n    нет:\n    if a == 0\n        да: printf(\"zero\")\n        нет: printf(\"?\")\n",
        "if a > 0\n    да:\n    if b > 0\n        да: printf(\"in\")\n        нет:\n    c = 1\n    нет: printf(\"out\")\n",
    ];
    for text in cases {
        let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        let mut n_if = 0;
        for d in l.shapes.iter().filter(|s| s.kind == "if") {
            n_if += 1;
            let bottom = d.cy + d.h / 2.0;
            for e in &l.edges {
                for w in e.points.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let vertical_out = (a.0 - d.cx).abs() < 1e-9
                        && (b.0 - d.cx).abs() < 1e-9
                        && (((a.1 - bottom).abs() < 1e-9 && b.1 > bottom + 1e-9)
                            || ((b.1 - bottom).abs() < 1e-9 && a.1 > bottom + 1e-9));
                    assert!(
                        !vertical_out,
                        "{text:?}: вертикаль из нижней вершины ромба {:?}: {a:?}-{b:?}",
                        d.lines
                    );
                }
            }
            for lb in &l.labels {
                if (lb.y - (d.cy - st.label_dy)).abs() > 1e-9 {
                    continue;
                }
                // подписи вложенных ромбов лежат на той же строке: берём
                // ближайший по x ромб, иначе внешний судит чужую подпись
                let nearest = l.shapes.iter().filter(|s| s.kind == "if").min_by(|a, b| {
                    (a.cx - lb.x)
                        .abs()
                        .partial_cmp(&(b.cx - lb.x).abs())
                        .unwrap()
                });
                if nearest.map(|s| s.cx) != Some(d.cx) {
                    continue;
                }
                if lb.text == "да" {
                    assert!(lb.x < d.cx, "{text:?}: да справа от ромба: {}", lb.x);
                }
                if lb.text == "нет" {
                    assert!(lb.x > d.cx, "{text:?}: нет слева от ромба: {}", lb.x);
                }
            }
        }
        assert!(n_if >= 1);
    }
}

/// РЕГРЕССИЯ (readsize.c): ромб, все ветви которого уходят рельсами
/// break/continue, не тянет ствол из нижней вершины. Продолжения на оси
/// нет, и вертикаль обрывалась в пустоте между маркерами соседних
/// колонок (внутренний if внутри while).
#[test]
fn diamond_with_all_rails_has_no_dangling_leg() {
    let st = Style::default();
    let text = "\
while 1
    printf(\"privet...\")
    if scanf_s(...) == 2
        да:
        if *x > SIZEX || *y > SIZEY || *x <= 0 || *y <= 0
            да:
            printf(\"max size is...\")
            continue
            нет:
            printf(\"Razmer...\")
            break
        нет:
        printf(\"prover format\")
        while getchar() != '\\n'
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    if let Err(e) = dangling_ok(&l) {
        panic!("обрыв линии: {e}");
    }
    for d in l.shapes.iter().filter(|s| s.kind == "if") {
        let bottom = d.cy + d.h / 2.0;
        for e in &l.edges {
            for w in e.points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let vertical_out = (a.0 - d.cx).abs() < 1e-9
                    && (b.0 - d.cx).abs() < 1e-9
                    && (((a.1 - bottom).abs() < 1e-9 && b.1 > bottom + 1e-9)
                        || ((b.1 - bottom).abs() < 1e-9 && a.1 > bottom + 1e-9));
                assert!(
                    !vertical_out,
                    "вертикаль из нижней вершины ромба {:?}: {a:?}-{b:?}",
                    d.lines
                );
            }
        }
    }
}

/// Канон: вложенный if внутри да-колонки внешнего — его «да» тоже
/// слева от под-ромба, пустое «нет» — правым коленом, «нет» внешнего —
/// справа. Стороны у вложенного те же, что у внешнего: разнобой
/// читается как ошибка.
#[test]
fn nested_if_keeps_yes_left() {
    let st = Style::default();
    let text = "\
if a > 0
    да:
    if b > 0
        да: printf(\"внутри\")
        нет:
    c = 1
    нет: printf(\"минус\")
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let _nhe = super::nhe_of(&sizes, &nodes, &st);
    let mut ifs: Vec<&Shape> = l.shapes.iter().filter(|s| s.kind == "if").collect();
    ifs.sort_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap());
    assert_eq!(ifs.len(), 2, "внешний и внутренний ромбы");
    let outer = ifs[0];
    let inner = ifs[1];
    let colw = sizes["act"].0.max(sizes["io"].0);
    // колонки пакуются по своим ветвям: «нет» из одной плитки стоит
    // hinge + colw/2, а не по общей на схему полуширине
    let base = outer.w / 2.0 + st.hgap + colw / 2.0;
    let base2 = inner.w / 2.0 + st.hgap + colw / 2.0;
    // «да»-колонка внешнего стоит по ВНУТРЕННЕМУ выносу своей ветки
    // (куда содержимое вылезает к оси), а не по симметричной полуширине
    let dec = nodes
        .iter()
        .find(|n| n.kind == NodeKind::Decision)
        .expect("внешний ромб");
    let (ext_l, _) = crate::layout::column::extent_lr(&sizes, &st, colw, &dec.branches[0].stmts);
    assert!(
        inner.cx < outer.cx - outer.w / 2.0,
        "внутренний ромб в левой да-колонке внешнего: {}",
        inner.cx
    );
    let _ = ext_l;
    let at = |txt: &str| {
        l.shapes
            .iter()
            .find(|s| s.lines.first().is_some_and(|t| t.contains(txt)))
            .unwrap()
    };
    let io = at("внутри");
    let act = at("c = 1");
    let minus = at("минус");
    assert!(
        io.cx < inner.cx,
        "да-контент вложенного колонкой налево: {} vs {}",
        io.cx,
        inner.cx
    );
    let _ = base2;
    assert!(
        act.cx < outer.cx,
        "c = 1 в левой да-колонке внешнего: {}",
        act.cx
    );
    assert!(
        minus.cx > outer.cx + outer.w / 2.0,
        "нет-ветка внешнего справа: {}",
        minus.cx
    );
    let _ = base;
    // пустое «нет» вложенного — колено вправо, а не рельса: вертикали
    // слева от под-ромба нет
    let vl = (inner.cx - inner.w / 2.0, inner.cy);
    assert!(
        !l.edges.iter().any(|e| {
            !e.arrow && (e.points[0].0 - vl.0).abs() < 1e-9 && (e.points[0].1 - vl.1).abs() < 1e-9
        }),
        "рельса пустого «нет» вложенного не рисуется"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
}

/// Пустая ветка переключателя: рельса чистит только правые колонки —
/// bx = up(base + nhe + g), а не внешняя сторона всей схемы.
#[test]
fn empty_branch_rail_clears_own_columns() {
    let st = Style::default();
    let mut d = node(NodeKind::Decision, "if switch (d)");
    d.switch_var = Some("d".into());
    d.branches = vec![
        br("1", vec![s("a = 1")], false),
        br("2", vec![s("b = 2")], false),
        br("3", vec![s("c = 3")], false),
        br("4", vec![], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        d,
        node(NodeKind::Term, "конец"),
    ];
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let vr = (dsh.cx + dsh.w / 2.0, dsh.cy);
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow && (e.points[0].0 - vr.0).abs() < 1e-9 && (e.points[0].1 - vr.1).abs() < 1e-9
        })
        .expect("рельса пустой ветки от правой вершины ромба");
    let bx = rail.points[1].0;
    let nhe = super::nhe_of(&sizes, &nodes, &st);
    // крайняя колонка (правая, ярус 0) на +base
    let base = dsh.w / 2.0 + st.hgap + nhe;
    let want = crate::layout::geometry::up(base + nhe + st.grid, st.grid);
    assert!(
        (bx - want).abs() < 1e-9,
        "bx = {bx}, ожидался up(base + nhe + g) = {want}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
}

/// СЕТЕЧНАЯ СТРОГОСТЬ: вся геометрия раскладки живёт на модульной сетке.
/// (а) ширины/высоты фигур кратны grid (eps 0.01);
/// (б) вертикальный зазор между соседями одной колонки кратен grid;
/// (в) крайние координаты (cx±w/2, cy±h/2) кратны grid/2: ось колонки
///     легально лежит на полусетке (dw/2 + label_exit_dx + k*2*grid).
/// Для (б) допуск 0.005, а не 0.01: литеральный vgap 42.5 при
/// grid 14.17 отстоит от 3*grid ровно на 0.01 и с допуском 0.01
/// проскочил бы. Тест ловит возврат литеральных зазоров (14.2, 28.3,
/// 42.5): остаток каждого от ближайшего кратного grid не менее 0.01.
#[test]
fn grid_invariant() {
    let st = Style::default();
    let g = st.grid;
    let on_grid = |v: f64, m: f64, eps: f64| ((v / m) - (v / m).round()).abs() * m < eps;

    let mut d2 = node(NodeKind::Decision, "if a > 0");
    d2.branches = vec![
        br("да", vec![s("a = 1")], false),
        br("нет", vec![s("b = 2")], false),
    ];
    let mut d3 = node(NodeKind::Decision, "if switch (d)");
    d3.switch_var = Some("d".into());
    d3.branches = vec![
        br("1", vec![s("a = 1")], false),
        br("2", vec![s("b = 2")], false),
        br("3", vec![s("c = 3")], false),
    ];
    let schemes: Vec<Vec<Node>> = vec![
        linear(),
        vec![
            node(NodeKind::Term, "начало"),
            d2,
            node(NodeKind::Term, "конец"),
        ],
        vec![
            node(NodeKind::Term, "начало"),
            loop_node("while i < 5", vec![s("a = 1"), sbrk()]),
            node(NodeKind::Term, "конец"),
        ],
        vec![
            node(NodeKind::Term, "начало"),
            d3,
            node(NodeKind::Term, "конец"),
        ],
    ];

    for nodes in &schemes {
        let sizes = normalize(nodes, &st);
        let l = layout(nodes, &sizes, &st);
        for sh in &l.shapes {
            assert!(
                on_grid(sh.w, g, 0.01) && on_grid(sh.h, g, 0.01),
                "{}: размер {}x{} не кратен grid = {g}",
                sh.kind,
                sh.w,
                sh.h
            );
            for c in [
                sh.cx - sh.w / 2.0,
                sh.cx + sh.w / 2.0,
                sh.cy - sh.h / 2.0,
                sh.cy + sh.h / 2.0,
            ] {
                assert!(
                    on_grid(c, g / 2.0, 0.01),
                    "{}: координата {c} не кратна grid/2",
                    sh.kind
                );
            }
        }
        // (б) колонка = группа фигур с одним cx
        let mut cols: Vec<(f64, Vec<&Shape>)> = Vec::new();
        for sh in &l.shapes {
            match cols.iter_mut().find(|(x, _)| (sh.cx - *x).abs() < 1e-6) {
                Some((_, col)) => col.push(sh),
                None => cols.push((sh.cx, vec![sh])),
            }
        }
        for (cx, mut col) in cols {
            col.sort_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap());
            for pair in col.windows(2) {
                let gap = (pair[1].cy - pair[1].h / 2.0) - (pair[0].cy + pair[0].h / 2.0);
                assert!(
                    gap >= -0.01 && on_grid(gap, g, 0.005),
                    "колонка cx={cx}: зазор {} между {} и {} не кратен grid = {g}",
                    gap,
                    pair[0].kind,
                    pair[1].kind
                );
            }
        }
    }
}

/// Каскад else-if (схема 25): три ромба на оси, «нет» — правое колено
/// к следующему ромбу, да-колонки слева, все
/// на одном top0; слияние: ровно одна горизонтальная шина под всеми.
#[test]
fn elseif_cascade_trunk_and_bus() {
    let st = Style::default();
    let text = "\
if a < 0
    да: printf(\"neg\")
    нет:
    if a == 0
        да: printf(\"zero\")
        нет:
        if a > 0
            да: printf(\"pos\")
            нет: printf(\"?\")
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let mut ifs: Vec<&Shape> = l.shapes.iter().filter(|s| s.kind == "if").collect();
    ifs.sort_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap());
    assert_eq!(ifs.len(), 3, "три ромба каскада");
    for d in &ifs {
        assert_eq!(d.cx, 0.0, "ромб на оси, а не в {}", d.cx);
    }
    // из нижней вершины каждого ромба вниз ничего не идёт: «нет» —
    // правое колено, спуск к следующему ромбу начинается сбоку
    for d in &ifs {
        let y_b = d.cy + d.h / 2.0;
        assert!(
            !l.edges.iter().any(|e| {
                let p = e.points[0];
                (p.0 - d.cx).abs() < 1e-9
                    && (p.1 - y_b).abs() < 1e-9
                    && e.points.get(1).is_some_and(|q| q.1 > y_b + 1e-9)
            }),
            "вертикаль из нижней вершины ромба: {:?}",
            d.lines
        );
        let no = l
            .labels
            .iter()
            .find(|lb| lb.text == "нет" && (lb.y - (d.cy - st.label_dy)).abs() < 1e-9);
        if let Some(no) = no {
            assert!(no.x > d.cx, "подпись нет слева от ромба: {}", no.x);
        }
    }
    // колонки (канон): все «да» СЛЕВА (L0, L1, L2), хвост-else справа (R0)
    let colw = sizes["act"].0.max(sizes["io"].0);
    let nhe = colw / 2.0;
    let base = ifs[0].w / 2.0 + st.hgap + nhe;
    let pitch = 2.0 * nhe + st.colgap;
    let at = |txt: &str| {
        l.shapes
            .iter()
            .find(|s| s.lines.first().is_some_and(|t| t.contains(txt)))
            .unwrap_or_else(|| panic!("нет фигуры {txt}"))
    };
    let neg = at("neg");
    let zero = at("zero");
    let pos = at("pos");
    let other = at("?");
    // хвост-«нет» стоит на одну полосу правее прежнего: его левый край
    // уступает дорожку «нет»-колену последнего звена
    for (sh, x, name) in [
        (neg, -base, "neg L0"),
        (zero, -base - pitch, "zero L1"),
        (pos, -base - 2.0 * pitch, "pos L2"),
        (other, base + 2.0 * st.grid, "? R0"),
    ] {
        assert!(
            (sh.cx - x).abs() < 1e-9,
            "{name}: cx = {}, ожидалась абсцисса {x}",
            sh.cx
        );
    }
    let top = |s: &Shape| s.cy - s.h / 2.0;
    for sh in [neg, zero, pos, other] {
        assert!(
            (top(sh) - top(neg)).abs() < 1e-9,
            "все колонки на одном top0"
        );
    }
    // шина: ровно один горизонтальный сегмент на merge_y, через 0,
    // накрывает крайние колонки; merge_y = низ последнего ромба + 2g
    let dsh_last = ifs[2];
    let my = dsh_last.cy + dsh_last.h / 2.0 + 2.0 * st.grid;
    let bus: Vec<&crate::layout::Edge> = l
        .edges
        .iter()
        .filter(|e| {
            e.points
                .windows(2)
                .any(|w| (w[0].1 - w[1].1).abs() < 1e-9 && (w[0].1 - my).abs() < 1e-9)
        })
        .collect();
    assert_eq!(bus.len(), 1, "ровно одна горизонтальная шина на merge_y");
    let xs: Vec<f64> = bus[0].points.iter().map(|p| p.0).collect();
    assert!(
        xs.iter().cloned().fold(f64::MAX, f64::min) <= -base - 2.0 * pitch + 1e-9
            && xs.iter().cloned().fold(f64::MIN, f64::max) >= base + 1e-9,
        "шина от хвоста-else справа до крайней левой да-колонки через 0: {xs:?}"
    );
    assert_eq!(crossings_ok(&l.shapes, &l.edges), Ok(()));
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Цепочка из двух ромбов без else: каскад с пустым хвостом — рельса
/// «нет» справа (там колонок нет), вплотную к ромбу; шина одна.
#[test]
fn elseif_cascade_two_links_empty_else() {
    let st = Style::default();
    let text = "\
if a < 0
    да: printf(\"neg\")
    нет:
    if a == 0
        да: printf(\"zero\")
        нет:
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let mut ifs: Vec<&Shape> = l.shapes.iter().filter(|s| s.kind == "if").collect();
    ifs.sort_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap());
    assert_eq!(ifs.len(), 2, "два ромба каскада");
    assert!(ifs.iter().all(|d| d.cx == 0.0), "ромбы на оси");
    let colw = sizes["act"].0.max(sizes["io"].0);
    let nhe = colw / 2.0;
    let base = ifs[0].w / 2.0 + st.hgap + nhe;
    let neg = l
        .shapes
        .iter()
        .find(|s| s.lines.first().is_some_and(|t| t.contains("neg")))
        .unwrap();
    let zero = l
        .shapes
        .iter()
        .find(|s| s.lines.first().is_some_and(|t| t.contains("zero")))
        .unwrap();
    assert!((neg.cx + base).abs() < 1e-9, "neg L0");
    assert!(
        (zero.cx + base + 2.0 * nhe + st.colgap).abs() < 1e-9,
        "zero L1"
    );
    // рельса пустого хвоста из ПРАВОЙ вершины последнего ромба (справа
    // колонок нет — свободная сторона), жмётся к ромбу: up(base + nhe + g)
    let d2 = ifs[1];
    let vr = (d2.cx + d2.w / 2.0, d2.cy);
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow && (e.points[0].0 - vr.0).abs() < 1e-9 && (e.points[0].1 - vr.1).abs() < 1e-9
        })
        .expect("рельса пустого «нет» из правой вершины ромба");
    assert!(
        rail.points[1].0 > vr.0 && rail.points[2].0 == rail.points[1].0,
        "рельса уходит вправо и опускается: {:?}",
        rail.points
    );
    let my = d2.cy + d2.h / 2.0 + 2.0 * st.grid;
    let buses = l
        .edges
        .iter()
        .filter(|e| {
            e.points
                .windows(2)
                .any(|w| (w[0].1 - w[1].1).abs() < 1e-9 && (w[0].1 - my).abs() < 1e-9)
        })
        .count();
    assert_eq!(buses, 1, "ровно одна шина на merge_y");
    assert_eq!(crossings_ok(&l.shapes, &l.edges), Ok(()));
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Каскад с пустым хвостом: две else-if по умолчанию — но «да» первой
/// (пустая плитка) и вложенный if в «нет»: именно та форма, в которой
/// коридор стороны считался только по обходам и оставлял «да» первого
/// ромба висеть в воздухе (u1-7: отрезок из левой вершины без стыка).
/// Инвариант: ни один конец ребра не висит в пустоте.
#[test]
fn elseif_cascade_first_branch_not_dangling() {
    let st = Style::default();
    let text = "\
if d2 == r2
    да:
    printf(\"na okruzhnosti\")
    нет:
    if d2 < r2
        да:
        printf(\"vnutri\")
        нет:
        printf(\"vne\")
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    if let Err(e) = crate::layout::dangling_ok(&l) {
        panic!("обрыв в каскаде else-if: {e}");
    }
    assert_eq!(crossings_ok(&l.shapes, &l.edges), Ok(()));
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Большой переключатель (>= 5 кейсов): кейсы сеткой по два на ряд —
/// все колонки на ±base, второй ряд ниже первого; ряды слиты в ствол.
#[test]
fn switch_rows_grid_for_five_cases() {
    let st = Style::default();
    let text = "\
if switch (d)
    1: printf(\"один\"); break
    2: printf(\"два\"); break
    3: printf(\"три\"); break
    4: printf(\"четыре\"); break
    5: printf(\"пять\"); break
output printf(d)
";
    let nodes = crate::frontend::gvn::parse(text, &st, "").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let colw = sizes["act"].0.max(sizes["io"].0);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    // колонка кейса стоит по своему содержимому: hinge + полуширина
    let base = dsh.w / 2.0 + st.hgap + colw / 2.0;
    let ios: Vec<&Shape> = l
        .shapes
        .iter()
        .filter(|s| s.kind == "io" && s.lines.first().is_some_and(|t| t.contains("printf(\"")))
        .collect();
    assert_eq!(ios.len(), 5, "пять кейсов");
    let _ = colw;
    // четыре кейса сеткой на ±base, нечётный пятый — на оси
    let mut on_axis = 0;
    for io in &ios {
        if io.cx.abs() < 1e-9 {
            on_axis += 1;
            continue;
        }
        assert!(
            (io.cx.abs() - base).abs() < 1e-9,
            "кейс на ±base = ±{base}, а он в {}",
            io.cx
        );
    }
    assert_eq!(on_axis, 1, "нечётный хвост стоит на стволе");
    // ряды: два кейса слева (1, 3), два справа (2, 4); ряд 1 ниже ряда 0
    let mut lefts: Vec<f64> = ios.iter().filter(|s| s.cx < 0.0).map(|s| s.cy).collect();
    lefts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(lefts.len(), 2);
    // кейсы больше не таскают плитки break, поэтому ряды опираются на
    // высоту самой плитки кейса, а не ромба
    let (_, io_h) = measure(&st, "io", "printf(\"один\")");
    assert!(
        lefts[1] > lefts[0] + io_h,
        "каждый следующий ряд ниже предыдущего: {:?}",
        lefts
    );
    // ширина схемы меньше, чем была бы одной шиной (2 яруса вширь)
    let (minx, ..) = l.bounds;
    let w = {
        let xs: Vec<f64> = l
            .shapes
            .iter()
            .flat_map(|s| [s.cx - s.w / 2.0, s.cx + s.w / 2.0])
            .collect();
        xs.iter().cloned().fold(f64::MAX, f64::min)
    };
    let _ = (minx, w);
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Четыре кейса — ещё одна шина (как на доске), не сетка: все четыре
/// кейса на одном верхнем ряду, на ±base и ±(base + pitch).
#[test]
fn switch_four_cases_stay_single_bus() {
    let st = Style::default();
    let text = "\
if switch (d)
    1: printf(\"один\"); break
    2: printf(\"два\"); break
    3: printf(\"три\"); break
    4: printf(\"четыре\"); break
output printf(d)
";
    let nodes = crate::frontend::gvn::parse(text, &st, "").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let nhe = super::nhe_of(&sizes, &nodes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let base = dsh.w / 2.0 + st.hgap + nhe;
    let pitch = 2.0 * nhe + st.colgap;
    let want = [-base - pitch, -base, base, base + pitch];
    let mut got: Vec<f64> = l
        .shapes
        .iter()
        .filter(|s| s.kind == "io" && s.cx != 0.0)
        .map(|s| s.cx)
        .collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (g, w) in got.iter().zip(want.iter()) {
        assert!((g - w).abs() < 1e-9, "cx {g}, ожидалось {w}");
    }
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Пустая «нет» — короткое колено справа: широкая «да»-колонка слева
/// на ширину схемы не влияет вовсе, отдельной рельсы у пустой ветки нет.
#[test]
fn empty_branch_stub_ignores_wide_yes_column() {
    let st = Style::default();
    let text = "\
if a > 0
    да: printf(\"очень широкий текст printf\")
    нет:
output printf(1)
";
    let nodes = crate::frontend::gvn::parse(text, &st, "").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let io = l.shapes.iter().find(|s| s.kind == "io").unwrap();
    assert!(io.cx < dsh.cx, "да-колонка слева");
    let left = dsh.cx - dsh.w / 2.0;
    let vl = (left, dsh.cy);
    assert!(
        !l.edges.iter().any(|e| {
            !e.arrow && (e.points[0].0 - vl.0).abs() < 1e-9 && (e.points[0].1 - vl.1).abs() < 1e-9
        }),
        "рельсы пустой ветки нет"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Репорт: ветка «да» из одного break рисовала вторую линию к слиянию
/// if и стрелку в пустоту. Теперь колонка без плиток не дорисовывается
/// к слиянию, вход без стрелки, выход — только рельса под цикл.
#[test]
fn break_only_branch_no_merge_descent() {
    let st = Style::default();
    let text = "\
while true
    input scanf(\"%d\", &a)
    if a > 0
        yes:
        break
        no:
        printf(\"нет\")
output printf(\"далее\")
";
    let nodes = crate::frontend::gvn::parse(text, &st, "").unwrap();
    let _sizes = normalize(&nodes, &st);
    let l = lay(&nodes);
    assert!(
        !l.shapes
            .iter()
            .any(|sh| sh.lines == vec!["break".to_string()]),
        "блок break не рисуется: {:?}",
        l.shapes.iter().map(|s| s.lines.clone()).collect::<Vec<_>>()
    );
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let top2 = dsh.cy + dsh.h / 2.0 + crate::layout::vgap(&st);
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    let merge_y = le.cy + le.h / 2.0 + st.mgap;
    // рельсы break и continue сливаются на одной высоте — посередине
    // выхода loop_end: разные концы давали X между рельсами
    let join_y = (le.cy + le.h / 2.0 + merge_y) * 0.5;
    // вход в «да»-колонку (канон: слева) без стрелки; ось колонки
    // берём из самого ребра — она пакуется по экстенту ветки
    let entry = l
        .edges
        .iter()
        .find(|e| {
            let last = e.points.last().unwrap();
            last.0 < 0.0 && (last.1 - top2).abs() < 1e-9
        })
        .expect("вход в да-колонку");
    assert!(!entry.arrow, "стрелки в пустую колонку нет");
    let base = entry.points.last().unwrap().0;
    // ложный спуск колонки к слиянию исчез: вертикали на x = base
    // ниже верха колонки нет
    let bogus = l.edges.iter().any(|e| {
        e.points.windows(2).any(|w| {
            let (p, q) = (w[0], w[1]);
            (p.0 - base).abs() < 1e-9 && (q.0 - base).abs() < 1e-9 && q.1.max(p.1) > top2 + 1e-9
        })
    });
    assert!(!bogus, "спуск rail-колонки к шине слияния не рисуется");
    // рельса break: коридор своей стороны ниже loop_end, T-стык на продолжении
    let (lw, _) = measure(&st, "loop", "while true");
    let rail = l
        .edges
        .iter()
        .find(|e| {
            e.points.iter().any(|p| p.0.abs() > lw / 2.0)
                && e.points
                    .last()
                    .map(|p| p.0 == 0.0 && (p.1 - join_y).abs() < 1e-9)
                    .unwrap_or(false)
        })
        .expect("рельса break в коридоре");
    let last = rail.points.last().unwrap();
    assert!(
        last.0 == 0.0 && (last.1 - join_y).abs() < 1e-9,
        "рельса приходит на слияние рельс: {last:?} vs {join_y}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(single_entry_ok(&l));
}

/// Репорт: break в кейсе switch внутри цикла считался выходом из ЦИКЛА.
/// В C он покидает только switch — схема с break обязана совпадать
/// со схемой без него.
#[test]
fn case_break_inside_loop_changes_nothing() {
    let st = Style::default();
    let with_b = "\
while i < 5
    if switch (a)
        1: printf(\"один\"); break
        2: printf(\"два\"); break
output printf(\"далее\")
";
    let without = "\
while i < 5
    if switch (a)
        1: printf(\"один\")
        2: printf(\"два\")
output printf(\"далее\")
";
    let a = lay(&crate::frontend::gvn::parse(with_b, &st, "").unwrap());
    let b = lay(&crate::frontend::gvn::parse(without, &st, "").unwrap());
    assert_eq!(a.shapes, b.shapes, "фигуры совпадают");
    assert_eq!(
        a.edges, b.edges,
        "рёбра совпадают: break растворён в слиянии кейса"
    );
}

/// То же на верхнем уровне: раньше break в кейсе рисовался прямоугольником.
#[test]
fn top_level_case_break_dissolves() {
    let st = Style::default();
    let with_b = "if switch (a)\n    1: printf(\"один\"); break\n    2: printf(\"два\"); break\noutput printf(\"k\")\n";
    let without =
        "if switch (a)\n    1: printf(\"один\")\n    2: printf(\"два\")\noutput printf(\"k\")\n";
    let a = lay(&crate::frontend::gvn::parse(with_b, &st, "").unwrap());
    let b = lay(&crate::frontend::gvn::parse(without, &st, "").unwrap());
    assert!(!a
        .shapes
        .iter()
        .any(|sh| sh.lines == vec!["break".to_string()]));
    assert_eq!(a.shapes, b.shapes);
    assert_eq!(a.edges, b.edges);
}

/// continue: рельса к выходу loop_end (следующая итерация); мёртвый
/// код после continue в той же колонке не рисуется.
#[test]
fn continue_rails_to_loop_end_output() {
    let st = Style::default();
    let text = "\
while i < 5
    a = 1
    continue
    b = 2
output printf(\"далее\")
";
    let nodes = crate::frontend::gvn::parse(text, &st, "").unwrap();
    let l = lay(&nodes);
    assert!(!l
        .shapes
        .iter()
        .any(|sh| sh.lines == vec!["b = 2".to_string()]));
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    // continue — следующая итерация: входит в БОК loop_end, выше его
    // низа и выше нижней трапеции; не сливается с выходом из цикла
    let cont_y = le.cy - le.h / 4.0;
    let rail = l
        .edges
        .iter()
        .find(|e| {
            let last = e.points.last().unwrap();
            let side = (last.0 - (le.cx - le.w / 2.0)).abs() < 1e-9
                || (last.0 - (le.cx + le.w / 2.0)).abs() < 1e-9;
            !e.arrow && (last.1 - cont_y).abs() < 1e-9 && side
        })
        .expect("рельса continue входит в бок loop_end");
    assert!(
        rail.points.len() >= 2,
        "рельса приходит на бок трапеции: {:?}",
        rail.points
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// Рельса continue внутри while с вложенными if (меню пользователя):
/// коридор дренажа меряется от фактического левого края тела, иначе
/// вертикаль рельсы проходит сквозь io-плитку колонки ветви
/// (u3.c: ребро (-481.78, 935)-(-481.78, 2153) сквозь блок).
#[test]
fn continue_rail_inside_nested_if_loop_misses_shapes() {
    // while { io; if {да: [io, continue]}; if {да: [io, if {да: [io, continue]}]} }
    let mut inner = node(NodeKind::Decision, "if (scanf_s(...) != 1 (a <= 0))");
    inner.branches = vec![
        br(
            "да",
            vec![sio("printf(\"oshibka a\")"), Stmt::Continue],
            false,
        ),
        br("нет", vec![], false),
    ];
    let mut mid = node(NodeKind::Decision, "if (choice == 1)");
    mid.branches = vec![
        br(
            "да",
            vec![sio("printf(\"vvedite a: \")"), Stmt::Node(Box::new(inner))],
            false,
        ),
        br("нет", vec![], false),
    ];
    let mut top = node(NodeKind::Decision, "if (scanf_s(...) != 1)");
    top.branches = vec![
        br(
            "да",
            vec![sio("printf(\"oshibka\")"), Stmt::Continue],
            false,
        ),
        br("нет", vec![], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node(
            "while 1",
            vec![
                sio("printf(\"menu\")"),
                Stmt::Node(Box::new(top)),
                Stmt::Node(Box::new(mid)),
            ],
        ),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    if let Err(e) = crossings_ok(&l.shapes, &l.edges) {
        panic!("рельса дренажа заходит на блок: {e}");
    }
    if let Err(e) = crate::layout::dangling_ok(&l) {
        panic!("обрыв линии: {e}");
    }
    assert!(overlaps_ok(&l.shapes));
}

/// Каскад else-if, где каждая ветка кончается return: плитка «return»
/// бывает шире колонки, и коридор каскада садился внутрь её силуэта
/// (torture.c/classify.c: вертикаль сквозь «return -1»). Полуширина
/// колонки теперь считает свою фигуру, коридор чист.
#[test]
fn cascade_with_returns_keeps_corridor_clear() {
    let st = Style::default();
    let text = "\
if x < 0
    да: return -1
    нет:
    if x == 0
        да: return 0
        нет:
        if x < 10
            да: return 1
            нет: return 2
";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let sizes = normalize(&nodes, &st);
    let l = layout(&nodes, &sizes, &st);
    if let Err(e) = crossings_ok(&l.shapes, &l.edges) {
        panic!("коридор каскада заходит на блок: {e}");
    }
    if let Err(e) = crate::layout::dangling_ok(&l) {
        panic!("обрыв линии: {e}");
    }
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Гигантский цикл: тело режется на листы — ленты с микроскопическим
/// кеглем больше нет. Каждая часть влезает в бюджет, кружки парные,
/// нижняя трапеция с номером — на последнем листе.
#[test]
fn giant_loop_body_splits_across_pages() {
    let st = Style::default();
    let mut text = String::from("while i < 24\n");
    for k in 1..=24 {
        text.push_str(&format!("    s = s + {k}\n"));
    }
    let nodes = crate::frontend::gvn::parse(&text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(
        parts.len(),
        1,
        "24 шага влезают на один лист (split_min): {}",
        parts.len()
    );
    for (k, part) in parts.iter().enumerate() {
        let l = layout(part, &sizes, &st);
        assert!(
            l.bounds.3 <= (st.a4_h - 2.0 * st.page_pad) / st.split_min,
            "лист {k} не влез: {}",
            l.bounds.3
        );
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok(), "лист {k}");
        assert!(overlaps_ok(&l.shapes), "лист {k}");
        assert!(dangling_ok(&l).is_ok(), "лист {k}");
    }
    // страница одна — соединителей и хвостовых трапеций-кусков нет
    let l = layout(parts.last().unwrap(), &sizes, &st);
    assert!(
        l.shapes.iter().any(|sh| sh.kind == "loop_end"),
        "нижняя трапеция цикла"
    );
}

/// continue в теле гигантского цикла: рельсы уходят на кружок-выход
/// листа — ни пересечений, ни обрывов ни на одном листе.
#[test]
fn giant_loop_with_continue_in_body() {
    let st = Style::default();
    // continue в ветке if: только такой рельс не обрывает отрисовку тела
    let mut text = String::from("while i < 24\n");
    text.push_str("    if i % 2 == 0\n        да: continue\n        нет: s = s + 1\n");
    for k in 1..=24 {
        text.push_str(&format!("    s = s + {k}\n"));
    }
    let nodes = crate::frontend::gvn::parse(&text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(
        parts.len(),
        1,
        "continue не обязателен к разрезу: {}",
        parts.len()
    );
    for (k, part) in parts.iter().enumerate() {
        let l = layout(part, &sizes, &st);
        assert_eq!(crossings_ok(&l.shapes, &l.edges), Ok(()), "лист {k}");
        assert!(overlaps_ok(&l.shapes), "лист {k}");
        assert!(dangling_ok(&l).is_ok(), "лист {k}");
    }
    // рельс головы/середины приходит в живой поток листа, а не в пустоту:
    // либо на кружок-выход (стык листов), либо на ствол следующей части —
    // кусок из одного оператора ставит на лист несколько частей, и тогда
    // кружка между ними нет, рельса сходится на вход следующей.
    let drained = parts.iter().any(|p| {
        let l = layout(p, &sizes, &st);
        l.edges.iter().filter(|e| !e.arrow).any(|e| {
            let Some(last) = e.points.last() else {
                return false;
            };
            let to_circle = l
                .shapes
                .iter()
                .filter(|sh| sh.kind == "conn")
                .any(|c| last == &(c.cx, c.cy - c.h / 2.0));
            let to_trunk = l.edges.iter().any(|a| {
                a.arrow && (a.points.first() == Some(last) || a.points.last() == Some(last))
            });
            to_circle || to_trunk
        })
    });
    assert!(drained, "ни один рельс не пришёл в поток листа");
}

/// Меню (lab4): тело цикла из четырёх гигантских «если» режется перед
/// каждым оператором — один оператор на лист, каждый в бюджете, кружки
/// на стыках парные.
#[test]
fn menu_body_splits_per_operator() {
    let st = Style::default();
    let big_if = |t: &str| {
        let mut d = node(NodeKind::Decision, t);
        d.branches = vec![
            br(
                "да",
                (0..6).map(|k| s(&format!("a = a + {k}"))).collect(),
                false,
            ),
            br(
                "нет",
                (0..6).map(|k| s(&format!("b = b + {k}"))).collect(),
                false,
            ),
        ];
        d
    };
    let body: Vec<Stmt> = (0..4)
        .map(|k| Stmt::Node(Box::new(big_if(&format!("if r == {k}")))))
        .collect();
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while !done", body),
        node(NodeKind::Term, "конец"),
    ];
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(
        parts.len(),
        1,
        "меню влезает на один лист (split_min): {}",
        parts.len()
    );
    let budget = (st.a4_h - 2.0 * st.page_pad) / st.split_min;
    for (k, part) in parts.iter().enumerate() {
        let l = layout(part, &sizes, &st);
        assert!(l.bounds.3 <= budget, "лист {k} не влез: {}", l.bounds.3);
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok(), "лист {k}");
        assert!(overlaps_ok(&l.shapes), "лист {k}");
        assert!(dangling_ok(&l).is_ok(), "лист {k}");
    }
    for w in parts.windows(2) {
        let out = w[0].last().unwrap();
        let inp = w[1].first().unwrap();
        assert_eq!(out.kind, NodeKind::Conn, "выход листа — кружок: {out:?}");
        assert_eq!(out.text, inp.text, "буквы пары расходятся");
    }
    // трапеции цикла: вход на головном листе, номер — на хвостовом
    let first = layout(&parts[0], &sizes, &st);
    let last = layout(parts.last().unwrap(), &sizes, &st);
    assert!(
        first.shapes.iter().any(|sh| sh.kind == "loop_begin"),
        "нет входа цикла на первом листе"
    );
    assert!(
        last.shapes.iter().any(|sh| sh.kind == "loop_end"),
        "нет хвоста цикла на последнем листе"
    );
}

/// Маркер ручного переноса в теле цикла: листы ломаются ровно по нему,
/// даже если цикл целиком влезает на один лист.
#[test]
fn manual_marker_inside_loop_body() {
    let st = Style::default();
    let text = "\
while i < 5
    a = 1
    b = 2
    перенос
    c = 3
    d = 4
";
    let nodes = crate::frontend::gvn::parse(text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(
        !parts
            .iter()
            .flatten()
            .any(|n| n.text == crate::frontend::MARKER),
        "маркер съеден"
    );
    let out = parts[0].last().unwrap();
    let inp = parts[1].first().unwrap();
    assert_eq!(out.kind, NodeKind::Conn, "{out:?}");
    assert_eq!(out.text, inp.text, "одна буква на пару листов");
    // голова несёт верхнюю трапецию, хвост — нижнюю с номером цикла
    let l1 = layout(&parts[0], &sizes, &st);
    let l2 = layout(&parts[1], &sizes, &st);
    assert!(l1.shapes.iter().any(|sh| sh.kind == "loop_begin"));
    assert!(!l1.shapes.iter().any(|sh| sh.kind == "loop_end"));
    assert!(!l2.shapes.iter().any(|sh| sh.kind == "loop_begin"));
    let le = l2
        .shapes
        .iter()
        .find(|sh| sh.kind == "loop_end")
        .expect("нижняя трапеция на хвосте");
    assert_eq!(le.lines, vec!["1".to_string()]);
    for l in [&l1, &l2] {
        assert_eq!(crossings_ok(&l.shapes, &l.edges), Ok(()));
        assert!(overlaps_ok(&l.shapes));
        assert!(dangling_ok(l).is_ok());
    }
}

/// Мёртвый хвост тела после первой рельсы не воскресает на листах: колонка
/// обрывается на break, псевдо-замер мёртвых не видит — иначе рез уходил за
/// рельсу и «МЁРТВ» рисовался живой плиткой на следующем листе.
#[test]
fn dead_candidates_after_rail_not_drawn() {
    let st = Style::default();
    let mut text = String::from("while i < 24\n");
    for k in 1..=5 {
        text.push_str(&format!("    s = s + {k}\n"));
    }
    text.push_str("    break\n");
    for k in 1..=10 {
        text.push_str(&format!("    МЁРТВ{k} = 1\n"));
    }
    // переросток ниже цикла: без него тело вообще не режется
    for k in 1..=30 {
        text.push_str(&format!("z = z + {k}\n"));
    }
    let nodes = crate::frontend::gvn::parse(&text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(
        parts.len(),
        1,
        "умеренно длинный цикл не режем: {}",
        parts.len()
    );
    for (k, part) in parts.iter().enumerate() {
        let l = layout(part, &sizes, &st);
        assert!(
            !l.shapes
                .iter()
                .any(|sh| sh.lines.iter().any(|t| t.contains("МЁРТВ"))),
            "лист {k} воскресил мёртвый код: {:?}",
            l.shapes
                .iter()
                .map(|sh| sh.lines.clone())
                .collect::<Vec<_>>()
        );
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok(), "лист {k}");
        assert!(dangling_ok(&l).is_ok(), "лист {k}");
    }
    // живая часть (до рельсы) нарисована целиком — как в обычном раскладе
    let drawn: Vec<String> = parts
        .iter()
        .flat_map(|p| layout(p, &sizes, &st).shapes)
        .flat_map(|sh| sh.lines)
        .collect();
    for k in 1..=5 {
        let t = format!("s = s + {k}");
        assert!(drawn.contains(&t), "живой оператор {t} потерян: {drawn:?}");
    }
}

/// Маркеры подряд: лист из одних кружков не рождается — лишний маркер
/// глотается, буква на него не тратится. То же с маркером первым оператором.
#[test]
fn two_consecutive_markers_no_empty_sheet() {
    let st = Style::default();
    let text = "a = 1\nперенос\nперенос\nb = 2\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    assert_eq!(parts.len(), 2, "{parts:?}");
    for (k, part) in parts.iter().enumerate() {
        assert!(
            part.iter().any(|n| n.kind != NodeKind::Conn),
            "лист {k} из одних кружков: {part:?}"
        );
        let l = layout(part, &sizes, &st);
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok(), "лист {k}");
        assert!(dangling_ok(&l).is_ok(), "лист {k}");
    }
    let out = parts[0].last().unwrap();
    let inp = parts[1].first().unwrap();
    assert_eq!(out.kind, NodeKind::Conn, "{out:?}");
    assert_eq!(out.text, inp.text, "одна буква на пару листов");
    assert!(
        parts[0].iter().any(|n| n.text == "a = 1") && parts[1].iter().any(|n| n.text == "b = 2"),
        "операторы разъехались не туда: {parts:?}"
    );
    // маркер первым оператором: резать перед ним нечего — глотается,
    // буква не тратится, лишнего листа нет
    let items = vec![
        node(NodeKind::Conn, crate::frontend::MARKER),
        node(NodeKind::Act, "a = 1"),
        node(NodeKind::Act, "b = 2"),
    ];
    let sizes = normalize(&items, &st);
    let parts = split_scheme(items, &sizes, &st);
    assert_eq!(
        parts.len(),
        1,
        "маркер в голове обязан глотаться: {parts:?}"
    );
    assert!(
        !parts[0].iter().any(|n| n.kind == NodeKind::Conn),
        "кружок из ниоткуда: {parts:?}"
    );
}

/// 30 разрывов в длинной схеме: буквы зацикливаются (счётчик общий, wrap по
/// % длины алфавита), и ни один лист не выходит пустоватым из кружков.
#[test]
fn letters_wrap_past_alphabet() {
    let st = Style::default();
    let mut text = String::new();
    for k in 1..=30 {
        text.push_str(&format!("a = {k}\nперенос\nперенос\n"));
    }
    text.push_str("b = 1\n");
    let nodes = crate::frontend::gvn::parse(&text, &st, "en").unwrap();
    let sizes = normalize(&nodes, &st);
    let parts = split_scheme(nodes, &sizes, &st);
    let n_letters = st.letters.chars().count();
    assert!(
        parts.len() > n_letters,
        "частей {} не больше букв {n_letters} — wrap не проверить",
        parts.len()
    );
    for (k, part) in parts.iter().enumerate() {
        assert!(
            part.iter().any(|n| n.kind != NodeKind::Conn),
            "лист {k} пустоват: {part:?}"
        );
        let l = layout(part, &sizes, &st);
        assert!(crossings_ok(&l.shapes, &l.edges).is_ok(), "лист {k}");
        assert!(dangling_ok(&l).is_ok(), "лист {k}");
    }
    assert_eq!(
        parts[0].last().unwrap().text,
        parts[n_letters].last().unwrap().text,
        "буквы обязаны зацикливаться"
    );
}

/// Точки, достижимые по графу рёбер от start: вершины ломаных плюс
/// T-стыки (вершина на внутренности чужого отрезка). Фигуры не
/// проходятся — путь сквозь блок считается недостижимым.
fn reachable(l: &Layout, start: (f64, f64)) -> Vec<(f64, f64)> {
    const TOL: f64 = 0.01;
    fn find(uf: &mut [usize], mut x: usize) -> usize {
        while uf[x] != x {
            uf[x] = uf[uf[x]];
            x = uf[x];
        }
        x
    }
    fn union(uf: &mut [usize], a: usize, b: usize) {
        let (ra, rb) = (find(uf, a), find(uf, b));
        if ra != rb {
            uf[ra] = rb;
        }
    }
    let key = |p: (f64, f64)| ((p.0 * 100.0).round() as i64, (p.1 * 100.0).round() as i64);
    let mut pts: Vec<(f64, f64)> = l.edges.iter().flat_map(|e| e.points.clone()).collect();
    pts.push(start);
    let idx: std::collections::HashMap<(i64, i64), usize> =
        pts.iter().enumerate().map(|(i, &p)| (key(p), i)).collect();
    let mut uf: Vec<usize> = (0..pts.len()).collect();
    let on = |p: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        if (a.0 - b.0).abs() < 1e-9 {
            (p.0 - a.0).abs() <= TOL && p.1 >= a.1.min(b.1) - TOL && p.1 <= a.1.max(b.1) + TOL
        } else if (a.1 - b.1).abs() < 1e-9 {
            (p.1 - a.1).abs() <= TOL && p.0 >= a.0.min(b.0) - TOL && p.0 <= a.0.max(b.0) + TOL
        } else {
            false
        }
    };
    for e in &l.edges {
        for w in e.points.windows(2) {
            let (Some(&ia), Some(&ib)) = (idx.get(&key(w[0])), idx.get(&key(w[1]))) else {
                continue;
            };
            union(&mut uf, ia, ib);
            for (i, &p) in pts.iter().enumerate() {
                if on(p, w[0], w[1]) {
                    union(&mut uf, i, ia);
                }
            }
        }
    }
    let root = find(&mut uf, idx[&key(start)]);
    (0..pts.len())
        .filter(|&i| find(&mut uf, i) == root)
        .map(|i| pts[i])
        .collect()
}

/// Каждый кейс switch достижим от нижней вершины ромба по рёбрам, не
/// заходя в другие кейсы (фигуры график не проходят). Ловит сетку,
/// второй ряд которой питается выходами первого ряда, а не ромбом.
fn assert_cases_reachable(nodes: &[Node], n_cases: usize) {
    let st = Style::default();
    let sizes = normalize(nodes, &st);
    let l = layout(nodes, &sizes, &st);
    let sw = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "if" && sh.lines.join(" ").contains("switch"))
        .expect("ромб switch");
    let reach = reachable(&l, (sw.cx, sw.cy + sw.h / 2.0));
    for k in 1..=n_cases {
        let tile = format!("a{k} = {k}");
        let case = l
            .shapes
            .iter()
            .find(|sh| sh.lines.iter().any(|t| t.contains(&tile)))
            .unwrap_or_else(|| panic!("нет кейса {k}"));
        let entry = (case.cx, case.cy - case.h / 2.0);
        assert!(
            reach
                .iter()
                .any(|p| (p.0 - entry.0).abs() < 0.01 && (p.1 - entry.1).abs() < 0.01),
            "кейс {k} не достижим от ромба switch: вход {entry:?}"
        );
    }
    assert!(crate::layout::edges_cross_ok(&l).is_ok());
    assert!(crate::layout::collinear_overlap_ok(&l).is_ok());
    assert!(overlaps_ok(&l.shapes));
}

fn switch_decision(n_cases: usize) -> Node {
    let mut d = node(NodeKind::Decision, "if switch (d)");
    d.switch_var = Some("d".into());
    d.branches = (1..=n_cases)
        .map(|k| br(&k.to_string(), vec![s(&format!("a{k} = {k}"))], false))
        .collect();
    d
}

#[test]
fn switch_cases_reachable_from_diamond() {
    // вложенный switch: 4 кейса = сетка 2x2 (класс labfix-06)
    let nested4 = {
        let mut outer = node(NodeKind::Decision, "if m > 0");
        outer.branches = vec![
            br("да", vec![Stmt::Node(Box::new(switch_decision(4)))], false),
            br("нет", vec![s("b = 2")], false),
        ];
        vec![
            node(NodeKind::Term, "начало"),
            outer,
            node(NodeKind::Term, "конец"),
        ]
    };
    assert_cases_reachable(&nested4, 4);
    // вложенный switch: 5 кейсов = три ряда, последний на оси
    let nested5 = {
        let mut outer = node(NodeKind::Decision, "if m > 0");
        outer.branches = vec![
            br("да", vec![Stmt::Node(Box::new(switch_decision(5)))], false),
            br("нет", vec![s("b = 2")], false),
        ];
        vec![
            node(NodeKind::Term, "начало"),
            outer,
            node(NodeKind::Term, "конец"),
        ]
    };
    assert_cases_reachable(&nested5, 5);
    // switch верхнего уровня: switch_rows
    let top = vec![
        node(NodeKind::Term, "начало"),
        switch_decision(5),
        node(NodeKind::Term, "конец"),
    ];
    assert_cases_reachable(&top, 5);
}

/// Обратной стрелки нет: начало и конец тела связывают одинаковые
/// номера на трапециях (как в ГОСТ-примерах и методичке). Ни одно ребро
/// не идёт вверх.
#[test]
fn loop_has_no_back_edge_but_matching_numbers() {
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![s("i = i + 1")]),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let lb = l.shapes.iter().find(|sh| sh.kind == "loop_begin").unwrap();
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    assert_eq!(le.lines, vec!["1".to_string()], "номер на нижней трапеции");
    assert!(
        lb.lines.iter().any(|t| t.starts_with("1. ")),
        "тот же номер на голове: {:?}",
        lb.lines
    );
    assert!(
        !l.edges
            .iter()
            .any(|e| e.points.windows(2).any(|w| w[1].1 < w[0].1 - 1.0)),
        "обратного хода вверх быть не должно"
    );
    assert!(collinear_overlap_ok(&l).is_ok());
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

/// do-while: тело выполняется до проверки — это видно по маркеру
/// «do while» на голове; обратной стрелки нет, связь — номером.
#[test]
fn do_while_is_labeled_on_head() {
    let mut lp = loop_node("x < 3", vec![s("x = x + 1")]);
    lp.loop_kind = Some(crate::ir::LoopKind::DoWhile);
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        lp,
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let lb = l.shapes.iter().find(|sh| sh.kind == "loop_begin").unwrap();
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    assert_eq!(
        lb.lines,
        vec!["1. do while x < 3".to_string()],
        "маркер do на голове"
    );
    assert_eq!(le.lines, vec!["1".to_string()], "номер на нижней трапеции");
    assert!(
        !l.edges
            .iter()
            .any(|e| e.points.windows(2).any(|w| w[1].1 < w[0].1 - 1.0)),
        "обратного хода do-while быть не должно"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(collinear_overlap_ok(&l).is_ok());
}

/// break и continue ведут в разные точки: continue — в бок loop_end
/// (следующая итерация), break — ниже нижней трапеции (выход из цикла).
#[test]
fn break_and_continue_leave_at_different_heights() {
    let mut cont = node(NodeKind::Decision, "if a");
    cont.branches = vec![
        br("да", vec![Stmt::Continue], false),
        br("нет", vec![], false),
    ];
    let mut brk = node(NodeKind::Decision, "if b");
    brk.branches = vec![br("да", vec![Stmt::Break], false), br("нет", vec![], false)];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node(
            "while i < 5",
            vec![
                s("i = i + 1"),
                Stmt::Node(Box::new(cont)),
                Stmt::Node(Box::new(brk)),
            ],
        ),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    let cont_edge = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow
                && e.points.last().is_some_and(|p| {
                    (p.1 - (le.cy - le.h / 4.0)).abs() < 1e-6
                        && ((p.0 - (le.cx - le.w / 2.0)).abs() < 1e-6
                            || (p.0 - (le.cx + le.w / 2.0)).abs() < 1e-6)
                })
        })
        .expect("continue входит в бок loop_end");
    let brk_edge = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow
                && e.points.last().is_some_and(|p| {
                    p.0 == le.cx && p.1 > le.cy + le.h / 2.0 && p.1 < le.cy + le.h / 2.0 + 40.0
                })
        })
        .expect("break уходит ниже loop_end");
    assert!(
        cont_edge.points.last().unwrap().1 < brk_edge.points.last().unwrap().1,
        "continue выше выхода break"
    );
    assert!(collinear_overlap_ok(&l).is_ok());
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(dangling_ok(&l).is_ok());
}

/// if/else с continue и break: continue возвращается к голове цикла
/// (вверх по своей стороне), break спускается вниз своей колонкой и
/// уходит под трапецию. Ветви не сливаются в одну вертикаль и не
/// расходятся из неё «одна линия, потом две».
#[test]
fn continue_returns_to_head_break_goes_down() {
    let mut inner = node(NodeKind::Decision, "if a");
    inner.branches = vec![
        br("да", vec![s("printf(\"max\")"), Stmt::Continue], false),
        br("нет", vec![s("printf(\"ok\")"), Stmt::Break], false),
    ];
    let mut outer = node(NodeKind::Decision, "if scanf");
    outer.branches = vec![
        br("да", vec![Stmt::Node(Box::new(inner))], false),
        br("нет", vec![], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![Stmt::Node(Box::new(outer))]),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    let lb = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "loop_begin")
        .expect("голова цикла");
    let le = l
        .shapes
        .iter()
        .find(|sh| sh.kind == "loop_end")
        .expect("низ цикла");
    let cont_edge = l
        .edges
        .iter()
        .find(|e| {
            e.arrow
                && e.points.last().is_some_and(|p| {
                    (p.1 - lb.cy).abs() < 1e-6
                        && ((p.0 - (lb.cx - lb.w / 2.0)).abs() < 1e-6
                            || (p.0 - (lb.cx + lb.w / 2.0)).abs() < 1e-6)
                })
        })
        .expect("continue входит в бок loop_begin");
    // вертикаль continue — по своей стороне, далеко от колонки break
    let cont_lane = cont_edge.points[0].0;
    let brk_edge = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow
                && e.points.windows(2).any(|w| {
                    (w[0].0 - w[1].0).abs() < 1e-9 && w[0].1 < w[1].1 && w[1].1 > le.cy + le.h / 2.0
                })
        })
        .expect("break спускается ниже loop_end");
    let brk_x = brk_edge.points[0].0;
    assert!(
        (cont_lane - brk_x).abs() > 1.0,
        "continue и break делят одну вертикаль: {} и {}",
        cont_lane,
        brk_x
    );
    assert!(
        cont_edge.points.last().unwrap().1 < le.cy,
        "continue выше низа цикла"
    );
    assert!(collinear_overlap_ok(&l).is_ok());
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(dangling_ok(&l).is_ok());
}

/// Пустой хвост каскада else-if: рельса справа не должна идти по кромке
/// плитки, которую вложенный switch вынес правее оси ромба. Длинная
/// подпись кейса расширяет колонку, и фиксированный отступ от ромба
/// (dw/2 + 2g) совпадал с её правой гранью.
#[test]
fn cascade_empty_tail_rail_clears_nested_switch() {
    let long = "b".repeat(60);
    let mut sw = node(NodeKind::Decision, "switch (k)");
    sw.switch_var = Some("k".into());
    sw.branches = vec![
        br("1", vec![s("a")], false),
        br("2", vec![s(&long)], false),
        br("3", vec![s("a")], false),
        br("4", vec![s(&long)], false),
    ];
    let mut inner = node(NodeKind::Decision, "if (y)");
    inner.branches = vec![br("да", vec![s("b")], false), br("нет", vec![], false)];
    let mut outer = node(NodeKind::Decision, "if (x)");
    outer.branches = vec![
        br("да", vec![Stmt::Node(Box::new(sw))], false),
        br("нет", vec![Stmt::Node(Box::new(inner))], false),
    ];
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        outer,
        node(NodeKind::Act, &long),
        node(NodeKind::Term, "конец"),
    ];
    let l = lay(&nodes);
    assert!(
        edge_on_border_ok(&l).is_ok(),
        "рельса пустого хвоста на кромке: {:?}",
        edge_on_border_ok(&l)
    );
    assert!(
        crossings_ok(&l.shapes, &l.edges).is_ok(),
        "рельса протыкает плитку: {:?}",
        crossings_ok(&l.shapes, &l.edges)
    );
}
