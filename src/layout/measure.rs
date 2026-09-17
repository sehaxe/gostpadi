use super::geometry::up;
use super::Sizes;
use crate::ir::{Node, NodeKind, Stmt};
use crate::style::Style;

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
            // размер заголовка цикла; используется ОБЕИМ трапециям
            // loop_begin/loop_end (верх/низ, ГОСТ паттерн 3.4)
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

pub(super) fn put(sizes: &mut Sizes, st: &Style, kind: &'static str, text: &str) {
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

pub(super) fn kind_name(k: &NodeKind) -> &'static str {
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
