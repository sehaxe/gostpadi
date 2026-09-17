//! SVG-генератор по ГОСТ 19.701-90: раскладка -> валидный SVG.

pub mod arrow;
pub mod edges;
pub mod shapes;
pub mod svg;
pub mod text;

pub use svg::{fit_scale, render_svg, render_svg_at};

#[cfg(test)]
mod tests;
