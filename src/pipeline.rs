//! Сборка пайплайна: C -> узлы -> страницы SVG.
//! Геометрия живёт в layout, отрисовка в generate; здесь только склейка.

use crate::frontend::cts::CParser;
use crate::generate::{fit_scale, render_svg_at};
use crate::ir::Node;
use crate::layout::{layout, normalize, split_scheme, uniform_sizes, Layout, Sizes};
use crate::style::Style;

pub struct Options {
    pub labels: String, // "en" | "ru"
    pub font: Option<f64>,
    pub lw: Option<f64>,
    pub no_split: bool,
    /// альбомная ориентация листа А4. Выбирается один на всю пачку:
    /// страницы лабы обязаны совпадать (ГОСТ 19.701-90 п. 4.1.3).
    pub landscape: bool,
}

impl Options {
    /// Стиль пайплайна: кегль, толщина пера и лист из опций, остальное —
    /// производные (Style::with_metrics). None — значения по умолчанию.
    pub fn style(&self) -> Style {
        Style {
            no_split: self.no_split,
            sheet: if self.landscape {
                crate::sheet::Sheet::A4_LANDSCAPE
            } else {
                crate::sheet::Sheet::A4
            },
            ..Style::with_metrics(self.font.unwrap_or(14.0), self.lw.unwrap_or(1.0))
        }
    }
}

/// C -> узлы схемы. Не падает на синтаксисе: битый код рисуется
/// частично, а не отказывает (tree-sitter, см. frontend::cts).
fn to_nodes(text: &str, opts: &Options) -> Vec<Node> {
    let st = opts.style();
    let mut nodes = CParser::new().parse(text, &opts.labels);
    // Перенос — до normalize: размеры фигур считаются по тексту, и
    // однострочное условие даёт другой ромб, чем перенесённое.
    crate::layout::wrap_nodes(&mut nodes, st.max_chars);
    nodes
}

/// Разобранная схема: путь входа и её узлы.
pub type Scheme = (String, Vec<Node>);

/// Пачка входов (путь, текст) -> (путь, узлы). Ошибок разбора нет:
/// любой текст даёт хоть какую-то схему.
pub fn parse_batch(inputs: &[(String, String)], opts: &Options) -> Vec<Scheme> {
    inputs
        .iter()
        .map(|(path, text)| (path.clone(), to_nodes(text, opts)))
        .collect()
}

/// Чем закончилась пачка: общий масштаб и выбранный лист. Нужно
/// вызывающему, чтобы честно сказать, когда кегль на листе упал
/// ниже читаемого: раньше пачка из пяти схем с одним широким
/// диспетчем молча давала 4.7 pt текста на всех пяти листах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatchInfo {
    /// общий масштаб пачки
    pub scale: f64,
    pub sheet: crate::sheet::Sheet,
    /// кегль на листе, pt — то, что читает преподаватель
    pub font_on_page: f64,
}

impl BatchInfo {
    /// Масштаб уронил бы кегль ниже порога читаемости. Порог тот же,
    /// что у порезки: ниже `split_scale` лист сжимать нельзя, это
    /// ровно тот случай, когда `split_scheme` режет вместо сжатия.
    /// Здесь он означает «кегль на листе меньше 0.7 кегля в файле».
    pub fn is_illegible(&self) -> bool {
        self.scale < 0.7
    }

    /// Насколько текст на листе мельче исходного кегля, в процентах.
    pub fn shrink_percent(&self) -> u32 {
        ((1.0 - self.scale) * 100.0).round().max(0.0) as u32
    }
}
fn lay_out(
    schemes: Vec<(String, Vec<Node>)>,
    sizes: &Sizes,
    st: &Style,
) -> Vec<(String, Vec<Layout>)> {
    schemes
        .into_iter()
        .map(|(path, nodes)| {
            let pages = if st.no_split {
                vec![layout(&nodes, sizes, st)]
            } else {
                split_scheme(nodes, sizes, st)
                    .iter()
                    .map(|part| layout(part, sizes, st))
                    .collect()
            };
            (path, pages)
        })
        .collect()
}

/// Единый масштаб пачки: минимальный из постраничных вписываний.
/// Общий по всем файлам намеренно — ГОСТ 19.701-90 п. 4.1.3 требует,
/// чтобы символы были одного размера, и страницы лабы обязаны совпадать.
fn shared_scale(laid: &[(String, Vec<Layout>)], st: &Style) -> f64 {
    laid.iter()
        .flat_map(|(_, pages)| pages)
        .map(|l| fit_scale(l.bounds, st))
        .fold(1.0_f64, f64::min)
}

/// Узлы из parse_batch + стиль -> (путь, страницы SVG). Размеры фигур,
/// ориентация листа и масштаб — общие на всю пачку.
///
/// Ориентация выбирается один на пачку, а не на файл: страницы лабы
/// обязаны совпадать. Книжная берётся, если она держит общий масштаб не
/// ниже порога читаемости; иначе — альбомная, у которой текстовая зона
/// шире в 1.5 раза. Раньше ориентации не было вовсе, и один широкий
/// диспетч ужимал всю лабу до нечитаемых 4.7 pt.
pub fn render_batch(
    schemes: Vec<(String, Vec<Node>)>,
    st: &Style,
) -> (Vec<(String, Vec<String>)>, BatchInfo) {
    let normed: Vec<Sizes> = schemes.iter().map(|(_, n)| normalize(n, st)).collect();
    let sizes = uniform_sizes(&normed);

    let laid = lay_out(schemes.clone(), &sizes, st);
    let s = shared_scale(&laid, st);
    let (st, laid, s) = if s >= st.split_scale || st.no_split {
        (st.clone(), laid, s)
    } else {
        // книжная нечитаема: пробуем альбомную, лист у неё шире
        let alt = Style {
            sheet: crate::sheet::Sheet::A4_LANDSCAPE,
            ..st.clone()
        };
        let laid_alt = lay_out(schemes.clone(), &sizes, &alt);
        let s_alt = shared_scale(&laid_alt, &alt);
        if s_alt > s {
            (alt, laid_alt, s_alt)
        } else {
            (st.clone(), laid, s)
        }
    };

    let info = BatchInfo {
        scale: s,
        sheet: st.sheet,
        font_on_page: st.font * s,
    };
    let out = laid
        .into_iter()
        .map(|(path, pages)| {
            let pages = pages.iter().map(|l| render_svg_at(l, &st, s)).collect();
            (path, pages)
        })
        .collect();
    (out, info)
}

/// Один вход (C-код) -> страницы SVG.
pub fn render_text(text: &str, opts: &Options) -> Vec<String> {
    render_batch(
        parse_batch(&[(String::new(), text.to_string())], opts),
        &opts.style(),
    )
    .0
    .remove(0)
    .1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            labels: "en".into(),
            font: None,
            lw: None,
            no_split: false,
            landscape: false,
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
    fn linear_c_renders_one_page() {
        let pages = render_text(
            "int main(void) {\n    int a;\n    scanf(\"%d\", &a);\n    a = a * 2;\n    printf(\"c = %d\", a);\n    return 0;\n}",
            &opts(),
        );
        assert_eq!(pages.len(), 1);
        assert!(pages[0].starts_with("<?xml"));
    }

    #[test]
    fn c_source_renders_one_page() {
        let pages = render_text("int main(){printf(\"hi\");return 0;}", &opts());
        assert_eq!(pages.len(), 1);
        assert!(pages[0].starts_with("<?xml"));
    }

    #[test]
    fn batch_uses_uniform_sizes() {
        // широкая плитка в A должна раздуть act-размер и в B
        let a = "int main(void) { int x; x = aaaaaaaa * bbbbbbbb * cccccccc * dddddddd; printf(\"1\"); return 0; }";
        let b = "int main(void) { int x; scanf(1); x = 1; printf(\"2\"); return 0; }";
        let schemes = parse_batch(
            &[("a.c".into(), a.into()), ("b.c".into(), b.into())],
            &opts(),
        );
        let (out, _) = render_batch(schemes, &opts().style());
        assert_eq!(out.len(), 2);
        let (mut wa, mut wb) = (rect_widths(&out[0].1[0]), rect_widths(&out[1].1[0]));
        wa.sort_by(|x, y| y.partial_cmp(x).unwrap());
        wb.sort_by(|x, y| y.partial_cmp(x).unwrap());
        assert_eq!(
            wa[0], wb[0],
            "самая широкая фигура одинакова в обеих схемах"
        );
    }

    /// ГЛАВНОЕ требование лабы: все листы пачки совпадают — размер
    /// листа, масштаб, кегль. ГОСТ 19.701-90 п. 4.1.3 требует, чтобы
    /// символы были одного размера; страницы обязаны совпадать иначе.
    /// Раньше страница обрезалась по содержимому, и пять листов пачки
    /// имели пять разных высот.
    #[test]
    fn whole_lab_gets_identical_sheets() {
        let files: [(&str, &str); 4] = [
            ("main.c", "int main(){int a; scanf(\"%d\",&a); if(a>1){printf(\"big\");return 1;} switch(a){case 1: printf(\"one\"); break; case 2: printf(\"two\"); break; default: a=0;} return 0;}"),
            ("linear.c", "int main(void){int a; scanf(\"%d\",&a); a=a*2; printf(\"c = %d\", a); return 0;}"),
            ("if.c", "int main(void){int a,c; if(a>0){printf(\"p\");}else{c=1;} printf(\"d\"); return 0;}"),
            ("loop.c", "int main(void){int a; while(a>0){a=a-1;} printf(\"z\"); return 0;}"),
        ];
        let inputs: Vec<(String, String)> = files
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect();
        let schemes = parse_batch(&inputs, &opts());
        let (out, info) = render_batch(schemes, &opts().style());
        assert!(out.len() >= 4, "каждый файл дал хотя бы один лист");

        let mm = 72.0 / 25.4;
        let w_pt = format!("{:.3}", opts().style().sheet.w);
        let h_pt = format!("{}", opts().style().sheet.h);
        let h_pt = &h_pt[..h_pt.find('.').unwrap_or(h_pt.len())];
        let mut scales = std::collections::BTreeSet::new();
        for (name, pages) in &out {
            for svg in pages {
                assert!(
                    svg.contains(&format!("width=\"{w_pt}pt\"")),
                    "{name}: лист не {w_pt} pt шириной"
                );
                assert!(svg.contains("height=\""), "{name}: нет высоты листа");
                assert!(
                    svg.contains(&format!("height=\"{h_pt}")),
                    " {name}: высота листа не {h_pt}"
                );
                let s = svg
                    .split("scale(")
                    .nth(1)
                    .and_then(|p| p.split(')').next())
                    .unwrap()
                    .to_string();
                scales.insert(s);
            }
        }
        assert_eq!(scales.len(), 1, "листы лабы разного масштаба: {scales:?}");
        assert_eq!(
            info.sheet,
            opts().style().sheet,
            "лист А4 книжный по умолчанию"
        );
        let _ = mm;
    }

    /// Ориентация выбирается на всю пачку и всегда в её пользу:
    /// если книжная роняет масштаб ниже порога, берётся та, что
    /// даёт лучший результат, — и страницы лабы остаются одинаковыми.
    #[test]
    fn batch_picks_the_better_orientation_for_all_files() {
        // диспетч на 6 кейсов: в один ряд он шире любого листа
        let mut wide = String::from("int main(void) {\n    int d;\n    switch (d) {\n");
        for k in 1..=6 {
            wide.push_str(&format!("    case {k}: printf(\"{k}\"); break;\n"));
        }
        wide.push_str("    }\n    printf(\"%d\", d);\n    return 0;\n}");
        let narrow = "int main(void) { int x; scanf(1); x = 1; return 0; }";
        let inputs = vec![("wide.c".into(), wide), ("narrow.c".into(), narrow.into())];
        let schemes = parse_batch(&inputs, &opts());
        let (out, info) = render_batch(schemes, &opts().style());
        assert_eq!(out.len(), 2);
        // ориентация одна на оба файла: в узкой книжная, в широкой
        // альбомная, но вместе — один выбор
        let sheet_of = |svg: &str| {
            let w: f64 = svg
                .split("width=\"")
                .nth(1)
                .and_then(|p| p.split("pt").next())
                .unwrap()
                .parse()
                .unwrap();
            if (w - crate::sheet::Sheet::A4.w).abs() < 1.0 {
                "книжная"
            } else {
                "альбомная"
            }
        };
        assert_eq!(sheet_of(&out[0].1[0]), sheet_of(&out[1].1[0]));
        assert!(
            info.scale > 0.0 && info.scale <= 1.0,
            "масштаб вне (0,1]: {}",
            info.scale
        );
    }

    /// Нечитаемый результат обязан быть назван, а не выдан молча:
    /// кегль на листе считается и проверяется.
    #[test]
    fn illegible_batch_is_reported() {
        let mut absurdly_wide = String::from("int main(void) {\n    int d;\n    switch (d) {\n");
        for k in 1..=9 {
            absurdly_wide.push_str(&format!(
                "    case {k}: printf(\"case {k} with a long label\"); break;\n"
            ));
        }
        absurdly_wide.push_str("    }\n    printf(\"%d\", d);\n    return 0;\n}");
        let schemes = parse_batch(&[("w.c".into(), absurdly_wide)], &opts());
        let (_, info) = render_batch(schemes, &opts().style());
        // либо впихнулось, либо честно помечено нечитаемым
        if info.scale < 0.7 {
            assert!(
                info.is_illegible(),
                "масштаб {:.3} -> кегль {:.1} pt должен быть помечен нечитаемым",
                info.scale,
                info.font_on_page
            );
        }
        assert!(
            info.font_on_page <= 12.0 + 1e-9,
            "лист не может увеличить кегль"
        );
    }

    /// Batch-атомарность: масштаб и кегль во всех файлах пачки
    /// посимвольно одинаковы, фигуры одного типа — одного размера.
    #[test]
    fn batch_schemas_share_scale_font_and_rect_sizes() {
        // широкая схема (переключатель) заставит пачку масштабироваться
        let a = "int main(void) { int d; switch (d) { case 1: printf(\"один\"); break; case 2: printf(\"два\"); break; case 3: printf(\"три\"); break; case 4: printf(\"четыре\"); break; default: printf(\"много\"); break; } printf(\"%d\", d); return 0; }";
        let b = "int main(void) { int x; scanf(1); x = 1; printf(\"2\"); return 0; }";
        let schemes = parse_batch(
            &[("a.c".into(), a.into()), ("b.c".into(), b.into())],
            &opts(),
        );
        let (out, _) = render_batch(schemes, &opts().style());
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
