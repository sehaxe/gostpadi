mod measure;
mod scheme;

use crate::ir::{Branch, Node, NodeKind, Stmt};
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
    Stmt::Text(t.into())
}

fn lay(nodes: &[Node]) -> Layout {
    let st = Style::default();
    let sizes = normalize(nodes, &st);
    layout(nodes, &sizes, &st)
}

fn linear() -> Vec<Node> {
    vec![
        node(NodeKind::Term, "начало"),
        node(NodeKind::Act, "a = 1"),
        node(NodeKind::Act, "b = 2"),
        node(NodeKind::Term, "конец"),
    ]
}

fn loop_node(text: &str, body: Vec<Stmt>) -> Node {
    let mut nd = node(NodeKind::Loop, text);
    nd.body = Some(body);
    nd
}
