//! Сборка пайплайна: текст (.gvn или C) -> узлы -> страницы SVG.
//! Геометрия живёт в layout, отрисовка в generate; здесь только склейка.

use crate::error::ParseError;
use crate::frontend::c::c_to_gvn;
use crate::frontend::gvn::parse;
use crate::generate::render_svg;
use crate::ir::Node;
use crate::layout::{layout, normalize, split_scheme, uniform_sizes, Sizes};
use crate::style::Style;

pub struct Options {
    pub labels: String, // "en" | "ru"
}

fn to_nodes(text: &str, is_c: bool, opts: &Options, st: &Style) -> Result<Vec<Node>, ParseError> {
    let gvn = if is_c {
        c_to_gvn(text, &opts.labels)?
    } else {
        text.to_string()
    };
    parse(&gvn, st, &opts.labels)
}

/// Пачка входов (путь, текст, это C?) -> (путь, узлы).
/// Ошибка разбора возвращается вместе с путём входа.
pub fn parse_batch(
    inputs: &[(String, String, bool)],
    opts: &Options,
) -> Result<Vec<(String, Vec<Node>)>, (String, ParseError)> {
    let st = Style::default();
    let mut out = Vec::with_capacity(inputs.len());
    for (path, text, is_c) in inputs {
        match to_nodes(text, *is_c, opts, &st) {
            Ok(nodes) => out.push((path.clone(), nodes)),
            Err(e) => return Err((path.clone(), e)),
        }
    }
    Ok(out)
}

/// Узлы из parse_batch -> (путь, страницы SVG). Длинные схемы режутся
/// на листы А4; размеры фигур общие на всю пачку (как render_many).
pub fn render_batch(schemes: Vec<(String, Vec<Node>)>) -> Vec<(String, Vec<String>)> {
    let st = Style::default();
    let normed: Vec<Sizes> = schemes.iter().map(|(_, n)| normalize(n, &st)).collect();
    let sizes = uniform_sizes(&normed);
    schemes
        .into_iter()
        .map(|(path, nodes)| {
            let pages = split_scheme(nodes, &sizes, &st)
                .iter()
                .map(|part| render_svg(&layout(part, &sizes, &st), &st))
                .collect();
            (path, pages)
        })
        .collect()
}

/// Один вход -> страницы SVG.
pub fn render_text(text: &str, is_c: bool, opts: &Options) -> Result<Vec<String>, ParseError> {
    match parse_batch(&[(String::new(), text.to_string(), is_c)], opts) {
        Ok(schemes) => Ok(render_batch(schemes).remove(0).1),
        Err((_, e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            labels: "en".into(),
        }
    }

    fn rect_widths(svg: &str) -> Vec<f64> {
        let mut out = Vec::new();
        for part in svg.split("width=\"").skip(1) {
            if let Some(num) = part.split('"').next() {
                if let Ok(v) = num.parse::<f64>() {
                    out.push(v);
                }
            }
        }
        out
    }

    #[test]
    fn linear_gvn_renders_one_page() {
        let pages = render_text(
            "input scanf(\"%d\", &a)\nc = a * 2\noutput printf(\"c = %d\", c)\n",
            false,
            &opts(),
        )
        .unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].starts_with("<?xml"));
    }

    #[test]
    fn c_source_renders_one_page() {
        let pages = render_text("int main(){printf(\"hi\");return 0;}", true, &opts()).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].starts_with("<?xml"));
    }

    #[test]
    fn batch_uses_uniform_sizes() {
        // широкая плитка в A должна раздуть act-размер и в B
        let a = "x = aaaaaaaa * bbbbbbbb * cccccccc * dddddddd\noutput printf(1)\n";
        let b = "input scanf(1)\nx = 1\noutput printf(2)\n";
        let schemes = parse_batch(
            &[
                ("a.gvn".into(), a.into(), false),
                ("b.gvn".into(), b.into(), false),
            ],
            &opts(),
        )
        .unwrap();
        let out = render_batch(schemes);
        assert_eq!(out.len(), 2);
        let (mut wa, mut wb) = (rect_widths(&out[0].1[0]), rect_widths(&out[1].1[0]));
        wa.sort_by(|x, y| y.partial_cmp(x).unwrap());
        wb.sort_by(|x, y| y.partial_cmp(x).unwrap());
        assert_eq!(
            wa[0], wb[0],
            "самая широкая фигура одинакова в обеих схемах"
        );
    }
}
