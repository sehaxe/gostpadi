mod measure;
mod scheme;

use crate::frontend::c::parse_c_to_nodes;
use crate::ir::{Branch, Node, NodeKind, Stmt, TileKind};
use crate::layout::{layout, normalize, Layout, Shape, Sizes};
use crate::style::Style;

/// nhe как в Ctx::new: max(colw/2, экстенты непустых веток верхнего уровня).
fn nhe_of(sizes: &Sizes, nodes: &[Node], st: &Style) -> f64 {
    let colw = sizes["act"].0.max(sizes["io"].0);
    let mut nhe = colw / 2.0;
    for nd in nodes {
        if nd.kind == NodeKind::Decision {
            for b in &nd.branches {
                if !b.stmts.is_empty() {
                    nhe = nhe.max(crate::layout::column::extent(sizes, st, colw, &b.stmts));
                }
            }
        }
    }
    nhe
}

fn node(kind: NodeKind, text: &str) -> Node {
    Node::new(kind, text)
}

fn br(label: &str, stmts: Vec<Stmt>, to_end: bool) -> Branch {
    Branch {
        label: label.into(),
        stmts,
        to_end,
        link: None,
    }
}

fn s(t: &str) -> Stmt {
    Stmt::Tile {
        kind: TileKind::Act,
        text: t.into(),
    }
}

fn sio(t: &str) -> Stmt {
    Stmt::Tile {
        kind: TileKind::Io,
        text: t.into(),
    }
}

fn sbrk() -> Stmt {
    Stmt::Break
}

fn lay(nodes: &[Node]) -> Layout {
    let st = Style::default();
    let sizes = normalize(nodes, &st);
    layout(nodes, &sizes, &st)
}

/// Узлы схемы из C-кода: тест пишет только тело `main`, обёртку и язык
/// подписей добавляет помощник. Раньше тесты раскладки собирали узлы из
/// внутреннего формата `.gvn`, которого в движке больше нет.
///
/// Терминаторы «начало»/«конец» добавляются здесь: `parse_c_to_nodes`
/// отдаёт только тело main (их ставил парсер `.gvn`), а раскладка без
/// «конца» не проверяет single-entry. Это обход дыры движка, а не
/// контракт фронтенда.
pub(super) fn nodes(body: &str) -> Vec<Node> {
    with_terms(
        parse_c_to_nodes(&format!("int main(void) {{\n{body}\n}}"), "en")
            .expect("разбор C-фикстуры"),
        "Start",
        "End",
    )
}

fn with_terms(mut body: Vec<Node>, start: &str, end: &str) -> Vec<Node> {
    let mut v = Vec::with_capacity(body.len() + 2);
    v.push(node(NodeKind::Term, start));
    v.append(&mut body);
    v.push(node(NodeKind::Term, end));
    v
}

fn linear() -> Vec<Node> {
    vec![
        node(NodeKind::Term, "начало"),
        node(NodeKind::Act, "a = 1"),
        node(NodeKind::Act, "b = 2"),
        node(NodeKind::Term, "конец"),
    ]
}

/// Габарит раскладки обязан накрывать всё, что рисуется: иначе
/// содержимое вылезает за поля листа и `fit_scale` вписывает не то.
///
/// Подпись с якорем «left» рисуется как `text-anchor=start` и растёт
/// вправо на ВСЮ ширину; «right» — влево; «center» — в обе стороны на
/// половину. Раньше для left/right габарит брал половину ширины, и
/// подпись «default» у правого края схемы вылезала за поле А4.
pub(crate) fn assert_bounds_cover_labels(l: &Layout, st: &Style, what: &str) {
    let (minx, miny, w, h) = l.bounds;
    let (maxx, maxy) = (minx + w, miny + h);
    for lb in &l.labels {
        let lw = lb.text.chars().count() as f64 * st.char_w;
        let (x0, x1) = match lb.ha.as_str() {
            "left" => (lb.x, lb.x + lw),
            "right" => (lb.x - lw, lb.x),
            _ => (lb.x - lw / 2.0, lb.x + lw / 2.0),
        };
        assert!(
            x0 >= minx - 1e-6 && x1 <= maxx + 1e-6 && lb.y >= miny - 1e-6 && lb.y <= maxy + 1e-6,
            "{what}: подпись {:?} (anchor={:?}) выходит за габарит {:?}",
            lb.text,
            lb.ha,
            l.bounds
        );
    }
}

/// Настоящие раскладки со всеми тремя видами подписей: ветка с
/// меткой слева, справа и по центру. Проверка идёт через интерфейс
/// раскладки, а не через внутренние формулы.
#[test]
fn real_layouts_bounds_cover_every_label() {
    let st = Style::default();
    let cases: Vec<(&str, Vec<Node>)> = vec![
        ("ветка с длинной меткой справа", {
            let mut n = br("ветка с очень длинной меткой", vec![s("x = 1")], false);
            n.stmts = vec![s("x = 1")];
            vec![
                node(NodeKind::Term, "начало"),
                node(NodeKind::Decision, "a > 1"),
                node(NodeKind::Term, "конец"),
            ]
            .tap_decision(vec![n, br("нет", vec![s("y = 2")], false)])
        }),
        ("диспетч с default", {
            let mut nd = node(NodeKind::Decision, "switch (a)");
            nd.switch_var = Some("a".into());
            nd.branches = vec![
                br("a = 1", vec![s("printf(1)")], false),
                br("a = 2", vec![s("printf(2)")], false),
                br("default", vec![s("a = 10")], false),
            ];
            vec![
                node(NodeKind::Term, "начало"),
                nd,
                node(NodeKind::Term, "конец"),
            ]
        }),
    ];
    for (what, nodes) in cases {
        let l = layout(&nodes, &normalize(&nodes, &st), &st);
        assert!(!l.labels.is_empty(), "{what}: в раскладке нет подписей");
        assert_bounds_cover_labels(&l, &st, what);
    }
}

/// Небольшой помощник, чтобы собрать Решение с ветками в одну строку.
trait TapDecision {
    fn tap_decision(self, branches: Vec<Branch>) -> Vec<Node>;
}

impl TapDecision for Vec<Node> {
    fn tap_decision(self, branches: Vec<Branch>) -> Vec<Node> {
        let mut v = self;
        if let Some(nd) = v.get_mut(1) {
            nd.branches = branches;
        }
        v
    }
}

fn loop_node(text: &str, body: Vec<Stmt>) -> Node {
    let mut nd = node(NodeKind::Loop, text);
    nd.body = Some(body);
    nd
}
