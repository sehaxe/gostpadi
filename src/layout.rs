use crate::ir::Node;
use crate::style::Style;

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

pub fn measure(_style: &Style, _kind: &str, _text: &str) -> (f64, f64) {
    (0.0, 0.0)
}

pub fn normalize(_nodes: &[Node], _style: &Style) -> std::collections::HashMap<String, (f64, f64)> {
    std::collections::HashMap::new()
}

pub fn uniform_sizes(
    _per_file: &[std::collections::HashMap<String, (f64, f64)>],
) -> std::collections::HashMap<String, (f64, f64)> {
    std::collections::HashMap::new()
}

pub fn layout(
    _nodes: &[Node],
    _sizes: &std::collections::HashMap<String, (f64, f64)>,
    _style: &Style,
) -> Layout {
    Layout {
        shapes: Vec::new(),
        edges: Vec::new(),
        labels: Vec::new(),
        bounds: (0.0, 0.0, 0.0, 0.0),
        anchors: Vec::new(),
    }
}

pub fn split_scheme(
    _nodes: Vec<Node>,
    _sizes: &std::collections::HashMap<String, (f64, f64)>,
    _style: &Style,
) -> Vec<Vec<Node>> {
    vec![_nodes]
}
