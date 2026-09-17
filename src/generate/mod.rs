//! SVG-генератор по ГОСТ 19.701-90: раскладка -> валидный SVG.

pub mod arrow;
pub mod edges;
pub mod shapes;
pub mod svg;
pub mod text;

pub use svg::render_svg;

#[cfg(test)]
mod tests;
