//! Инварианты раскладки: топология циклов-трапеций, рельсы break,
//! single-entry в «конец», тупики return, проверки на примерах.

use super::*;
use crate::layout::{
    crossings_ok, layout, measure, normalize, overlaps_ok, single_entry_ok, split_scheme,
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
    let st = Style::default();
    let (lw, lh) = measure(&st, "loop", "while i < 5");
    let l = lay(&nodes);
    let want = (0.5 * lh).min(0.25 * lw);
    let lb = l.shapes.iter().find(|sh| sh.kind == "loop_begin").unwrap();
    assert_eq!(lb.lines, vec!["while i < 5".to_string()]);
    assert!(lb.skew > 0.0, "срез верхних углов у loop_begin");
    assert_eq!(lb.skew, want, "Δ = min(0.5*h, 0.25*w)");
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    assert_eq!(
        le.lines,
        vec!["1".to_string()],
        "номер цикла на нижнем шестиугольнике"
    );
    assert_eq!(le.skew, want, "loop_end — зеркальный срез той же величины");
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

#[test]
fn break_goes_left_rail_below_loop() {
    let nodes = vec![
        node(NodeKind::Term, "начало"),
        loop_node("while i < 5", vec![s("a = 1"), s("break")]),
        node(NodeKind::Term, "конец"),
    ];
    let st = Style::default();
    let l = lay(&nodes);
    let (lw, _) = measure(&st, "loop", "while i < 5");
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    let rail = l
        .edges
        .iter()
        .find(|e| !e.arrow && e.points.iter().any(|p| p.0 < -lw / 2.0))
        .expect("рельса break через левый канал");
    let last = rail.points.last().unwrap();
    assert!(
        last.0 == 0.0 && last.1 > le.cy + le.h / 2.0,
        "рельса сливается с магистралью ниже loop_end: {last:?}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}

#[test]
fn to_end_single_arrow_into_end() {
    let mut d = node(NodeKind::Decision, "if a > 0");
    d.branches = vec![
        br("да", vec![s("printf(\"many\")")], true),
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
        br("да", vec![s("return 1")], false),
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
            loop_node("while i < 5", vec![s("i = i + 1"), s("break")]),
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
    assert_eq!(drops, 3, "спуск по каждой колонке отдельно");
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(single_entry_ok(&l));
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

/// Пустая ветка без правых колонок (одна колонка слева): рельса «нет»
/// отходит от вершины ромба ровно на 2g — локальный экстент вместо
/// глобального запаса «на все колонки всех ромбов схемы».
#[test]
fn empty_branch_rail_hugs_diamond() {
    let st = Style::default();
    let text = "if x <= 0\n    да: printf(\"bad\"); return 1\n    нет:\ny = x + 1\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
    let l = lay(&nodes);
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
    let want = crate::layout::geometry::up(dsh.w / 2.0 + 2.0 * st.grid, st.grid);
    assert!(
        (bx - want).abs() < 1e-9,
        "bx = {bx}, ожидался up(dw/2 + 2g) = {want}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
}

/// Пустая ветка при наличии правой колонки: bx = правая кромка крайней
/// правой колонки (tx + nhe) + 2g, рельса не цепляет собственные колонки.
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
    // nhe — по той же формуле, что в Ctx::new
    let colw = sizes["act"].0.max(sizes["io"].0);
    let nhe = nodes[1]
        .branches
        .iter()
        .filter(|b| !b.stmts.is_empty())
        .map(|b| crate::layout::column::extent(&sizes, &st, colw, &b.stmts))
        .fold(colw / 2.0, f64::max);
    let right_tx = l.shapes.iter().map(|s| s.cx).fold(f64::MIN, f64::max);
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
    let want = crate::layout::geometry::up(right_tx + nhe + 2.0 * st.grid, st.grid);
    assert!(
        (bx - want).abs() < 1e-9,
        "bx = {bx}, ожидался up(rightmost_tx + nhe + 2g) = {want}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
}
