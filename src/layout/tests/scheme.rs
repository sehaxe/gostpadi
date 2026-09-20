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
        loop_node("while i < 5", vec![s("a = 1"), sbrk()]),
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

/// Пустая ветка при да-колонке слева: рельса «нет» жмётся к ромбу —
/// bx = up(dw/2 + 2g), независимо от ширины левой колонки.
#[test]
fn empty_branch_rail_hugs_diamond() {
    let st = Style::default();
    let text = "if x <= 0\n    да: printf(\"bad\"); return 1\n    нет:\ny = x + 1\n";
    let nodes = crate::frontend::gvn::parse(text, &st, "ru").unwrap();
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
    let want = crate::layout::geometry::up(dsh.w / 2.0 + 2.0 * st.grid, st.grid);
    assert!(
        (bx - want).abs() < 1e-9,
        "bx = {bx}, ожидался up(dw/2 + 2g) = {want}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
    assert!(overlaps_ok(&l.shapes));
}

/// ГОСТ 19.701: у if с единственной непустой веткой и пустой «нет»
/// контент идёт КОЛОНКОЙ НАЛЕВО (tx = -base), рельса «нет» жмётся
/// к ромбу: bx = up(dw/2 + 2g).
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
    let vr = (dsh.cx + dsh.w / 2.0, dsh.cy);
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow && (e.points[0].0 - vr.0).abs() < 1e-9 && (e.points[0].1 - vr.1).abs() < 1e-9
        })
        .expect("рельса «нет» от правой вершины ромба");
    let bx = rail.points[1].0;
    let want = crate::layout::geometry::up(dsh.w / 2.0 + 2.0 * st.grid, st.grid);
    assert!(
        (bx - want).abs() < 1e-9,
        "bx = {bx}, ожидался up(dw/2 + 2g) = {want}"
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

/// Вложенный if с одной веткой и пустой «нет» внутри да-колонки
/// внешнего: контент колонкой налево от под-ромба, под-рельса
/// зеркальна внешнему краю да-колонки вложенного.
#[test]
fn nested_if_yes_left_with_mirror_rail() {
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
    let nhe = super::nhe_of(&sizes, &nodes, &st);
    let mut ifs: Vec<&Shape> = l.shapes.iter().filter(|s| s.kind == "if").collect();
    ifs.sort_by(|a, b| a.cy.partial_cmp(&b.cy).unwrap());
    assert_eq!(ifs.len(), 2, "внешний и внутренний ромбы");
    let outer = ifs[0];
    let inner = ifs[1];
    let base = outer.w / 2.0 + st.hgap + nhe;
    // локальный экстент веток вложенного: printf = colw/2
    let colw = sizes["act"].0.max(sizes["io"].0);
    let base2 = inner.w / 2.0 + st.hgap + colw / 2.0;
    // внутренний ромб на оси да-колонки внешнего
    assert!(
        (inner.cx + base).abs() < 1e-9,
        "внутренний ромб на -base = {}, а он в {}",
        -base,
        inner.cx
    );
    let io = l
        .shapes
        .iter()
        .find(|s| s.lines.first().is_some_and(|t| t.contains("внутри")))
        .unwrap();
    let act = l
        .shapes
        .iter()
        .find(|s| s.lines.first().is_some_and(|t| t.contains("c = 1")))
        .unwrap();
    let minus = l
        .shapes
        .iter()
        .find(|s| s.lines.first().is_some_and(|t| t.contains("минус")))
        .unwrap();
    assert!(
        (io.cx - (inner.cx - base2)).abs() < 1e-9,
        "да-контент вложенного колонкой налево"
    );
    assert!(
        (act.cx - inner.cx).abs() < 1e-9,
        "c = 1 на оси да-колонки внешнего"
    );
    assert!(
        (minus.cx - base).abs() < 1e-9,
        "нет-ветка внешнего справа на +base"
    );
    // под-рельса жмётся к под-ромбу: inner.cx + up(dw2/2 + 2g)
    let vr = (inner.cx + inner.w / 2.0, inner.cy);
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow && (e.points[0].0 - vr.0).abs() < 1e-9 && (e.points[0].1 - vr.1).abs() < 1e-9
        })
        .expect("рельса пустой «нет» вложенного от его правой вершины");
    let want = inner.cx + crate::layout::geometry::up(inner.w / 2.0 + 2.0 * st.grid, st.grid);
    assert!(
        (rail.points[1].0 - want).abs() < 1e-9,
        "bx вложенного = {}, ожидался up(base2 + nhe) = {}",
        rail.points[1].0,
        want
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

/// Каскад else-if (схема 25): три ромба на оси, «нет» — вертикальный
/// ствол между ними, да-колонки чередуют стороны L0, R0, L1, R1, все
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
    // ствол: стрелка от нижней вершины ромба к верхней вершины следующего
    for w in ifs.windows(2) {
        let (y0, y1) = (w[0].cy + w[0].h / 2.0, w[1].cy - w[1].h / 2.0);
        assert!(
            l.edges.iter().any(|e| {
                e.arrow
                    && e.points.len() == 2
                    && e.points.iter().all(|p| p.0.abs() < 1e-9)
                    && (e.points[0].1 - y0).abs() < 1e-9
                    && (e.points[1].1 - y1).abs() < 1e-9
            }),
            "нет вертикального ствола между ромбами"
        );
    }
    // колонки: чередование сторон L0, R0, L1, R1 на одном top0
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
    for (sh, x, name) in [
        (neg, -base, "neg L0"),
        (zero, base, "zero R0"),
        (pos, -(base + pitch), "pos L1"),
        (other, base + pitch, "? R1"),
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
        xs.iter().cloned().fold(f64::MAX, f64::min) <= -(base + pitch) + 1e-9
            && xs.iter().cloned().fold(f64::MIN, f64::max) >= base + pitch - 1e-9,
        "шина от крайней левой до крайней правой колонки через 0: {xs:?}"
    );
    assert_eq!(crossings_ok(&l.shapes, &l.edges), Ok(()));
    assert!(overlaps_ok(&l.shapes));
    assert!(single_entry_ok(&l));
}

/// Цепочка из двух ромбов без else: каскад с пустым хвостом — рельса
/// «нет» на следующей свободной стороне (левой), вплотную к ромбу
/// с очисткой левых колонок; шина одна.
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
    assert!((zero.cx - base).abs() < 1e-9, "zero R0");
    // рельса пустого хвоста из ЛЕВОЙ вершины последнего ромба, жмётся
    // к ромбу и чистит левые колонки: up(base + nhe + g)
    let d2 = ifs[1];
    let vl = (d2.cx - d2.w / 2.0, d2.cy);
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow && (e.points[0].0 - vl.0).abs() < 1e-9 && (e.points[0].1 - vl.1).abs() < 1e-9
        })
        .expect("рельса пустого «нет» из левой вершины ромба");
    let want = -crate::layout::geometry::up(base + nhe + st.grid, st.grid);
    assert!(
        (rail.points[1].0 - want).abs() < 1e-9,
        "bx = {}, ожидался -up(base + nhe) = {want}",
        rail.points[1].0
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
    let nhe = super::nhe_of(&sizes, &nodes, &st);
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let base = dsh.w / 2.0 + st.hgap + nhe;
    let ios: Vec<&Shape> = l
        .shapes
        .iter()
        .filter(|s| s.kind == "io" && s.cx != 0.0)
        .collect();
    assert_eq!(ios.len(), 5, "пять кейсов");
    // все колонки строго на ±base — ширина сетки постоянна
    for io in &ios {
        assert!(
            ((io.cx - base).abs() < 1e-9) || ((io.cx + base).abs() < 1e-9),
            "кейс на ±base = ±{base}, а он в {}",
            io.cx
        );
    }
    // ряды: три кейса слева (1, 3, 5), два справа (2, 4); ряд 1 ниже ряда 0
    let mut lefts: Vec<f64> = ios.iter().filter(|s| s.cx < 0.0).map(|s| s.cy).collect();
    lefts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(lefts.len(), 3);
    // кейсы больше не таскают плитки break, поэтому ряды опираются на
    // высоту самой плитки кейса, а не ромба
    let (_, io_h) = measure(&st, "io", "printf(\"один\")");
    assert!(
        lefts[1] > lefts[0] + io_h && lefts[2] > lefts[1] + io_h,
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

/// Рельса пустой ветки не разлетается за широкой левой колонкой:
/// жмётся к ромбу на up(dw/2 + 2g).
#[test]
fn empty_rail_hugs_despite_wide_left_column() {
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
    let vr = (dsh.cx + dsh.w / 2.0, dsh.cy);
    let rail = l
        .edges
        .iter()
        .find(|e| {
            !e.arrow && (e.points[0].0 - vr.0).abs() < 1e-9 && (e.points[0].1 - vr.1).abs() < 1e-9
        })
        .expect("рельса пустой ветки");
    let bx = rail.points[1].0;
    let want = crate::layout::geometry::up(dsh.w / 2.0 + 2.0 * st.grid, st.grid);
    assert!((bx - want).abs() < 1e-9, "bx = {bx}, ожидался {want}");
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
    let sizes = normalize(&nodes, &st);
    let l = lay(&nodes);
    assert!(
        !l.shapes
            .iter()
            .any(|sh| sh.lines == vec!["break".to_string()]),
        "блок break не рисуется: {:?}",
        l.shapes.iter().map(|s| s.lines.clone()).collect::<Vec<_>>()
    );
    let dsh = l.shapes.iter().find(|s| s.kind == "if").unwrap();
    let nhe = super::nhe_of(&sizes, &nodes, &st);
    let base = dsh.w / 2.0 + st.hgap + nhe;
    let top2 = dsh.cy + dsh.h / 2.0 + st.vgap;
    let le = l.shapes.iter().find(|sh| sh.kind == "loop_end").unwrap();
    let merge_y = le.cy + le.h / 2.0 + st.mgap;
    // вход в «да»-колонку без стрелки
    let entry = l
        .edges
        .iter()
        .find(|e| {
            let last = e.points.last().unwrap();
            (last.0 + base).abs() < 1e-9 && (last.1 - top2).abs() < 1e-9
        })
        .expect("вход в да-колонку");
    assert!(!entry.arrow, "стрелки в пустую колонку нет");
    // ложный спуск колонки к слиянию исчез: вертикали на x = -base
    // ниже верха колонки нет
    let bogus = l.edges.iter().any(|e| {
        e.points.windows(2).any(|w| {
            let (p, q) = (w[0], w[1]);
            (p.0 + base).abs() < 1e-9 && (q.0 + base).abs() < 1e-9 && q.1.max(p.1) > top2 + 1e-9
        })
    });
    assert!(!bogus, "спуск rail-колонки к шине слияния не рисуется");
    // рельса break: левый канал ниже loop_end, T-стык на продолжении
    let (lw, _) = measure(&st, "loop", "while true");
    let rail = l
        .edges
        .iter()
        .find(|e| {
            e.points.iter().any(|p| p.0 < -lw / 2.0)
                && e.points
                    .last()
                    .map(|p| p.0 == 0.0 && (p.1 - merge_y).abs() < 1e-9)
                    .unwrap_or(false)
        })
        .expect("рельса break в левом канале");
    let last = rail.points.last().unwrap();
    assert!(
        last.0 == 0.0 && (last.1 - merge_y).abs() < 1e-9,
        "рельса приходит на выход цикла: {last:?} vs {merge_y}"
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
    let le_bottom = le.cy + le.h / 2.0;
    let rail = l
        .edges
        .iter()
        .find(|e| {
            let last = e.points.last().unwrap();
            last.0 == 0.0 && last.1 > le_bottom + 1e-9 && e.points.len() >= 4
        })
        .expect("рельса continue к выходу loop_end");
    let merge_y = le_bottom + st.mgap;
    let yj = rail.points.last().unwrap().1;
    assert!(
        yj < merge_y,
        "T-стык на выходе loop_end, не ниже: {yj} vs {merge_y}"
    );
    assert!(crossings_ok(&l.shapes, &l.edges).is_ok());
}
