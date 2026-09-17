use crate::ir::{Node, NodeKind, Stmt};
use crate::style::Style;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Shape {
    pub kind: String,
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub points: Vec<(f64, f64)>,
    pub arrow: bool,
}

#[derive(Debug, Clone)]
pub struct Label {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub ha: String,
}

#[derive(Debug, Clone)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub shapes: Vec<Shape>,
    pub edges: Vec<Edge>,
    pub labels: Vec<Label>,
    pub bounds: (f64, f64, f64, f64),
    pub anchors: Vec<Anchor>,
}

pub type Sizes = BTreeMap<&'static str, (f64, f64)>;

/// Округление вверх до модульной сетки 5 мм (b = 2a, ГОСТ 19.701-90).
/// Поправка 1e-9 отсекает float-мусор вида 3.0000000001.
fn up(v: f64, g: f64) -> f64 {
    (v / g - 1e-9).ceil() * g
}

/// (ширина, высота) фигуры по её тексту; порт gostpadi.py measure().
/// Текст уже перенесён — строки разделяются \n.
pub fn measure(st: &Style, kind: &str, text: &str) -> (f64, f64) {
    let g = st.grid;
    let ls: Vec<&str> = text.split('\n').collect();
    let tw = ls.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * st.char_w;
    let n = ls.len() as f64;
    match kind {
        "term" | "ret" => {
            let h = (2.0 * g).max(up(n * st.pitch * 0.8 + 16.0, g));
            (up(tw + st.pad_x + 22.0, g).max(2.0 * h), h)
        }
        "conn" => (2.0 * st.conn_r, 2.0 * st.conn_r),
        "loop" => {
            // шестиугольник «подготовка»: торцы скошены под 45° (по h/2)
            let h = (2.0 * g).max(up(n * st.pitch + st.pad_y - 4.0, g));
            let w = up(tw + st.pad_x + 6.0, g) + h;
            (w.max(2.0 * h), h)
        }
        "if" => {
            // текст между рёбрами ромба; боковые стороны под 45°: h = aspect * w
            let ymax = (n - 1.0) * st.pitch / 2.0 + 6.0;
            let need = tw + 26.0;
            let w = up((need + ymax * 2.0 / st.aspect).max(11.0 * g), g);
            (w, w * st.aspect)
        }
        _ => {
            let h = (2.0 * g).max(up(n * st.pitch + st.pad_y - 4.0, g));
            (up(tw + st.pad_x + 6.0, g).max(2.0 * h), h)
        }
    }
}

fn put(sizes: &mut Sizes, st: &Style, kind: &'static str, text: &str) {
    let (w, h) = measure(st, kind, text);
    let e = sizes.entry(kind).or_insert((w, h));
    e.0 = e.0.max(w);
    e.1 = e.1.max(h);
}

fn put_items(sizes: &mut Sizes, st: &Style, items: &[Stmt]) {
    for it in items {
        match it {
            Stmt::Node(nd) => put_node(sizes, st, nd),
            Stmt::Text(t) => {
                let kind = if st.is_io(t) { "io" } else { "act" };
                put(sizes, st, kind, t);
            }
        }
    }
}

fn put_node(sizes: &mut Sizes, st: &Style, nd: &Node) {
    put(sizes, st, kind_name(&nd.kind), &nd.text);
    for br in &nd.branches {
        put_items(sizes, st, &br.stmts);
    }
    if let Some(body) = &nd.body {
        put_items(sizes, st, body);
    }
}

fn kind_name(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::Term => "term",
        NodeKind::Io => "io",
        NodeKind::Act => "act",
        NodeKind::Decision => "if",
        NodeKind::Loop => "loop",
        NodeKind::Return => "ret",
        NodeKind::Conn => "conn",
    }
}

/// Максимальный размер на каждый тип фигур — единый для всей схемы.
/// Порт gostpadi.py normalize(); базовые размеры всегда присутствуют.
pub fn normalize(nodes: &[Node], st: &Style) -> Sizes {
    let mut sizes = Sizes::new();
    for nd in nodes {
        put_node(&mut sizes, st, nd);
    }
    for (kind, sample) in [
        ("term", "x"),
        ("ret", "return 1"),
        ("io", "x"),
        ("act", "x"),
        ("if", "x"),
        ("loop", "x"),
        ("conn", "А"),
    ] {
        put(&mut sizes, st, kind, sample);
    }
    sizes
}

/// Один размер фигур на всю пачку схем: по каждому типу — максимум.
pub fn uniform_sizes(per_file: &[Sizes]) -> Sizes {
    let mut out = Sizes::new();
    for sizes in per_file {
        for (k, &(w, h)) in sizes {
            let e = out.entry(k).or_insert((w, h));
            e.0 = e.0.max(w);
            e.1 = e.1.max(h);
        }
    }
    out
}

pub fn layout(_nodes: &[Node], _sizes: &Sizes, _style: &Style) -> Layout {
    Layout {
        shapes: Vec::new(),
        edges: Vec::new(),
        labels: Vec::new(),
        bounds: (0.0, 0.0, 0.0, 0.0),
        anchors: Vec::new(),
    }
}

pub fn split_scheme(_nodes: Vec<Node>, _sizes: &Sizes, _style: &Style) -> Vec<Vec<Node>> {
    vec![_nodes]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: NodeKind, text: &str) -> Node {
        Node::new(kind, text)
    }

    #[test]
    fn measure_on_grid() {
        let st = Style::default();
        // conn задан как 2*conn_r и в Python не кратен сетке (28.4 vs 2*grid=28.34)
        for kind in ["term", "ret", "io", "act", "if", "loop"] {
            for text in ["x", "a = 1", "printf(hello world)\nfoo(a, b)"] {
                let (w, h) = measure(&st, kind, text);
                let q = |v: f64| (v / st.grid).round() - v / st.grid;
                assert!(q(w).abs() < 1e-9, "{kind} w={w} off-grid");
                if kind != "if" && kind != "conn" {
                    assert!(q(h).abs() < 1e-9, "{kind} h={h} off-grid");
                }
            }
        }
    }

    #[test]
    fn term_min_height_two_grids() {
        let st = Style::default();
        let (_, h) = measure(&st, "term", "x");
        assert!(h >= 2.0 * st.grid);
    }

    #[test]
    fn if_height_tracks_aspect() {
        let st = Style::default();
        let (w, h) = measure(&st, "if", "long condition a < b and c > d ?");
        assert!((h - w * st.aspect).abs() < 1e-9);
    }

    #[test]
    fn normalize_all_seven_kinds() {
        let st = Style::default();
        let nodes = vec![
            node(NodeKind::Term, "начало"),
            node(NodeKind::Act, "a = 1\nprintf(x)\nif a>0\n    да: b = 2"),
            node(NodeKind::Return, "конец"),
        ];
        let sizes = normalize(&nodes, &st);
        for k in ["term", "ret", "io", "act", "if", "loop", "conn"] {
            let s = sizes.get(k).unwrap_or_else(|| panic!("missing {k}"));
            assert!(s.0 > 0.0 && s.1 > 0.0, "{k} zero size");
        }
        // в схеме нет io-узлов: базовый "x" остаётся меньше act
        assert!(sizes["io"].0 < sizes["act"].0);
    }

    #[test]
    fn normalize_recurses_branches_and_body() {
        let st = Style::default();
        // короткие тексты узлов, длинные строки в ветке и body: тогда
        // ширина типа доказывает, что рекурсия в branches/body прошла
        let mut nd = node(NodeKind::Decision, "c?");
        let mut inner = node(NodeKind::Act, "d = 1");
        inner.body = Some(vec![Stmt::Text("a very long body line indeed xx".into())]);
        nd.branches.push(crate::ir::Branch {
            label: "да".into(),
            stmts: vec![Stmt::Text("printf(long io statement here ok)".into())],
            to_end: false,
            link: None,
        });
        let sizes = normalize(&[nd, inner], &st);
        let (w, _) = measure(&st, "io", "printf(long io statement here ok)");
        assert_eq!(sizes["io"].0, w);
        let (w2, _) = measure(&st, "act", "a very long body line indeed xx");
        assert_eq!(sizes["act"].0, w2);
    }

    #[test]
    fn text_stmt_classified_io_vs_act() {
        let st = Style::default();
        let mut nd = node(NodeKind::Decision, "c?");
        nd.branches.push(crate::ir::Branch {
            label: "да".into(),
            stmts: vec![Stmt::Text("printf(x)".into()), Stmt::Text("a = 1".into())],
            to_end: false,
            link: None,
        });
        let sizes = normalize(&[nd], &st);
        // io: printf; act: "a = 1" шире базового "x", значит классификация сработала
        let (w, _) = measure(&st, "act", "a = 1");
        assert_eq!(sizes["act"].0, w);
        let (wi, _) = measure(&st, "io", "printf(x)");
        assert_eq!(sizes["io"].0, wi);
    }

    #[test]
    fn uniform_takes_max() {
        let st = Style::default();
        let mut a = normalize(&[node(NodeKind::Term, "начало")], &st);
        let mut b = normalize(&[node(NodeKind::Term, "начало")], &st);
        a.insert("if", (100.0, 100.0));
        b.insert("if", (200.0, 70.0));
        let u = uniform_sizes(&[a, b]);
        assert_eq!(u["if"], (200.0, 100.0));
    }
}
