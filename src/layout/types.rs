//! Публичные типы результата раскладки.

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

/// Якорь узла для разбивки схемы на листы: y — низ потока узла.
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

use std::collections::BTreeMap;
