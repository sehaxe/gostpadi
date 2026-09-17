//! Сборка пайплайна: текст (.gvn или C) -> узлы -> страницы SVG.
//! Геометрия живёт в layout, отрисовка в generate; здесь только склейка.

use crate::error::ParseError;
use crate::frontend::c::c_to_gvn;
use crate::frontend::gvn::parse;
use crate::generate::{fit_scale, render_svg_at};
use crate::ir::Node;
use crate::layout::{layout, normalize, split_scheme, uniform_sizes, Layout, Sizes};
use crate::style::Style;

pub struct Options {
    pub labels: String, // "en" | "ru"
    pub font: Option<f64>,
    pub lw: Option<f64>,
}

impl Options {
    /// Стиль пайплайна: кегль и толщина пера из опций, остальное —
    /// производные (Style::with_metrics). None — значения по умолчанию.
    pub fn style(&self) -> Style {
        Style::with_metrics(self.font.unwrap_or(12.0), self.lw.unwrap_or(1.0))
    }
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
    let st = opts.style();
    let mut out = Vec::with_capacity(inputs.len());
    for (path, text, is_c) in inputs {
        match to_nodes(text, *is_c, opts, &st) {
            Ok(nodes) => out.push((path.clone(), nodes)),
            Err(e) => return Err((path.clone(), e)),
        }
    }
    Ok(out)
}

/// Узлы из parse_batch + стиль -> (путь, страницы SVG). Длинные схемы
/// режутся на листы А4; размеры фигур и масштаб общие на всю пачку:
/// figures одного типа во всех файлах — одного визуального размера.
pub fn render_batch(schemes: Vec<(String, Vec<Node>)>, st: &Style) -> Vec<(String, Vec<String>)> {
    let normed: Vec<Sizes> = schemes.iter().map(|(_, n)| normalize(n, st)).collect();
    let sizes = uniform_sizes(&normed);
    let laid: Vec<(String, Vec<Layout>)> = schemes
        .into_iter()
        .map(|(path, nodes)| {
            let pages = split_scheme(nodes, &sizes, st)
                .iter()
                .map(|part| layout(part, &sizes, st))
                .collect();
            (path, pages)
        })
        .collect();
    // единый масштаб пачки: минимальный из постраничных вписываний
    let s = laid
        .iter()
        .flat_map(|(_, pages)| pages)
        .map(|l| fit_scale(l.bounds, st))
        .fold(1.0_f64, f64::min);
    laid.into_iter()
        .map(|(path, pages)| {
            let pages = pages.iter().map(|l| render_svg_at(l, st, s)).collect();
            (path, pages)
        })
        .collect()
}

/// Один вход -> страницы SVG.
pub fn render_text(text: &str, is_c: bool, opts: &Options) -> Result<Vec<String>, ParseError> {
    match parse_batch(&[(String::new(), text.to_string(), is_c)], opts) {
        Ok(schemes) => Ok(render_batch(schemes, &opts.style()).remove(0).1),
        Err((_, e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            labels: "en".into(),
            font: None,
            lw: None,
        }
    }

    fn rect_dims(svg: &str) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        // skip(2): мусор до первого <rect, затем белая подложка страницы
        for part in svg.split("<rect").skip(2) {
            let w = part
                .split("width=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|v| v.parse::<f64>().ok());
            let h = part
                .split("height=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|v| v.parse::<f64>().ok());
            if let (Some(w), Some(h)) = (w, h) {
                out.push((w, h));
            }
        }
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }

    fn metric(svg: &str, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        let pat = format!("{key}=\"");
        for part in svg.split(&pat).skip(1) {
            if let Some(v) = part.split('"').next() {
                out.push(v.to_string());
            }
        }
        out
    }

    /// Значение scale(...) из transform: "translate(.. ..) scale(s)".
    fn scales(svg: &str) -> Vec<String> {
        svg.split("scale(")
            .skip(1)
            .filter_map(|p| p.split(')').next().map(|v| v.to_string()))
            .collect()
    }

    fn rect_widths(svg: &str) -> Vec<f64> {
        let mut out = Vec::new();
        // skip(2): шапка до <rect и белая подложка страницы
        for part in svg.split("<rect").skip(2) {
            if let Some(num) = part
                .split("width=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
            {
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
        let out = render_batch(schemes, &opts().style());
        assert_eq!(out.len(), 2);
        let (mut wa, mut wb) = (rect_widths(&out[0].1[0]), rect_widths(&out[1].1[0]));
        wa.sort_by(|x, y| y.partial_cmp(x).unwrap());
        wb.sort_by(|x, y| y.partial_cmp(x).unwrap());
        assert_eq!(
            wa[0], wb[0],
            "самая широкая фигура одинакова в обеих схемах"
        );
    }

    /// Batch-атомарность: масштаб и кегль во всех файлах пачки
    /// посимвольно одинаковы, фигуры одного типа — одного размера.
    #[test]
    fn batch_schemas_share_scale_font_and_rect_sizes() {
        // широкая схема (переключатель) заставит пачку масштабироваться
        let a = "if switch (d)\n    1: printf(\"один\"); break\n    2: printf(\"два\"); break\n    3: printf(\"три\"); break\n    4: printf(\"четыре\"); break\n    иначе: printf(\"много\"); break\noutput printf(d)\n";
        let b = "input scanf(1)\nx = 1\noutput printf(2)\n";
        let schemes = parse_batch(
            &[
                ("a.gvn".into(), a.into(), false),
                ("b.gvn".into(), b.into(), false),
            ],
            &opts(),
        )
        .unwrap();
        let out = render_batch(schemes, &opts().style());
        assert_eq!(out.len(), 2);
        let sa = scales(&out[0].1[0]);
        let sb = scales(&out[1].1[0]);
        assert_eq!(sa, sb, "scale в пачке одинаков посимвольно");
        assert!(
            !sa.is_empty() && sa[0] != "1",
            "схема A реально масштабирована"
        );
        let fa = metric(&out[0].1[0], "font-size");
        let fb = metric(&out[1].1[0], "font-size");
        // значений может быть разное количество (фигур в A больше),
        // но сами значения кегля — одно и то же множество
        let mut ua = fa.clone();
        ua.sort();
        ua.dedup();
        let mut ub = fb.clone();
        ub.sort();
        ub.dedup();
        assert_eq!(ua, ub, "font-size в пачке одинаков посимвольно");
        // блоков разное количество (в A switch с 5 ветками), но каждый
        // размер фигуры из A встречается и в B — фигуры одного типа
        // одного размера во всей пачке
        let mut ra = rect_dims(&out[0].1[0]);
        ra.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ra.dedup();
        let mut rb = rect_dims(&out[1].1[0]);
        rb.sort_by(|a, b| a.partial_cmp(b).unwrap());
        rb.dedup();
        assert!(ra.iter().all(|d| rb.contains(d)), "{ra:?} vs {rb:?}");
    }
}
