//! Сборка пайплайна: текст (.gvn), C или C++ -> узлы -> страницы SVG.
//! Геометрия живёт в layout, отрисовка в generate; здесь только склейка.

use crate::error::ParseError;
use crate::frontend::{CodeFrontend, Frontend, GvnFrontend, InputKind, Lang};
use crate::generate::{fit_scale, render_svg_at, render_svg_tight};
use crate::ir::Node;
use crate::layout::{
    collinear_overlap_ok, crossings_ok, dangling_ok, edge_on_border_ok, edges_cross_ok, layout,
    normalize, overlaps_ok, single_entry_ok, split_scheme, strip_markers, uniform_sizes, Edge,
    Label, Layout, Sizes,
};
use crate::style::Style;

pub struct Options {
    pub labels: String, // "en" | "ru"
    pub font: Option<f64>,
    pub lw: Option<f64>,
    pub no_split: bool,
}

impl Options {
    /// Стиль пайплайна: кегль и толщина пера из опций, остальное —
    /// производные (Style::with_metrics). None — значения по умолчанию.
    pub fn style(&self) -> Style {
        Style {
            no_split: self.no_split,
            ..Style::with_metrics(self.font.unwrap_or(12.0), self.lw.unwrap_or(1.0))
        }
    }
}

/// Схема пайплайна: путь (файл или «файл#функция»), узлы и подпись.
/// Подпись — сигнатура функции для коробки «предопределённый процесс»
/// над схемой; у main и .gvn-входа её нет.
#[derive(Clone, Debug)]
pub struct Scheme {
    pub path: String,
    pub nodes: Vec<Node>,
    pub title: Option<String>,
}

/// Путь схемы -> (вход, функция): «dir/main.c#main» -> («dir/main.c», Some("main")).
/// Функции нет (gvn-вход) — второй элемент None.
pub fn scheme_parts(path: &str) -> (&str, Option<&str>) {
    path.split_once('#')
        .map_or((path, None), |(a, b)| (a, Some(b)))
}

/// Проверка схем на инварианты ГОСТ-раскладки: линии под прямыми углами,
/// без пересечений и обрывов, в «конец» — одна стрелка. Без рисования.
/// Возвращает по схеме имя и «что не так» (Ok — всё чисто, с числом блоков).
pub fn validate(schemes: &[Scheme], st: &Style) -> Vec<(String, Result<String, String>)> {
    schemes
        .iter()
        .map(|s| {
            let (inp, fun) = scheme_parts(&s.path);
            let name = match fun {
                Some(f) => format!("{inp}#{f}"),
                None => inp.to_string(),
            };
            let nodes = &s.nodes;
            let l = layout(nodes, &normalize(nodes, st), st);
            let res = || {
                crossings_ok(&l.shapes, &l.edges)
                    .map_err(|e| format!("линии заходят на блоки: {e}"))?;
                edges_cross_ok(&l).map_err(|e| format!("линии пересекаются: {e}"))?;
                collinear_overlap_ok(&l).map_err(|e| format!("двойная линия: {e}"))?;
                edge_on_border_ok(&l).map_err(|e| format!("линия идёт по кромке блока: {e}"))?;
                dangling_ok(&l).map_err(|e| format!("обрыв линии: {e}"))?;
                if !overlaps_ok(&l.shapes) {
                    return Err("фигуры перекрываются".to_string());
                }
                if !single_entry_ok(&l) {
                    return Err("в «конец» входит не одна стрелка".to_string());
                }
                Ok(format!("ok: {} blocks", nodes.len()))
            };
            (name, res())
        })
        .collect()
}

/// Пачка входов (путь, текст, вид входа) -> схемы.
/// Ошибка разбора возвращается вместе с путём входа.
/// Код с несколькими функциями даёт по схеме на функцию:
/// путь схемы «файл.c#имя_функции» (решение 06.10: рисуем все функции).
pub fn parse_batch(
    inputs: &[(String, String, InputKind)],
    opts: &Options,
) -> Result<Vec<Scheme>, (String, ParseError)> {
    let st = opts.style();
    let mut out = Vec::with_capacity(inputs.len());
    for (path, text, kind) in inputs {
        let frontend: Box<dyn Frontend> = match kind {
            InputKind::Gvn => Box::new(GvnFrontend),
            InputKind::C => Box::new(CodeFrontend { lang: Lang::C }),
            InputKind::Cpp => Box::new(CodeFrontend { lang: Lang::Cpp }),
        };
        let funcs = match frontend.parse(text, &st, &opts.labels) {
            Ok(f) => f,
            Err(e) => return Err((path.clone(), e)),
        };
        for f in funcs {
            // main — схема самого входа (main.c -> main.svg), прочие
            // функции получают суффикс: main.c#sum_neg -> main-sum_neg.svg.
            // main — программа, а не функция: подписи нет.
            let (scheme_path, title) = match f.name.as_deref() {
                Some(name) if name != "main" => (format!("{path}#{name}"), f.signature),
                _ => (path.clone(), None),
            };
            out.push(Scheme {
                path: scheme_path,
                nodes: f.nodes,
                title,
            });
        }
    }
    Ok(out)
}

/// Экспорт для отчёта: `render_tight_batch` — SVG по содержимому в
/// миллиметрах (одна схема на вход, вставка «миллиметр в миллиметр»),
/// `render_drawio_batch` — книга draw.io с редактируемыми блоками.
/// Оба идут через ту же раскладку, что и экран: вид один, отличается
/// только вывод.

/// Узлы пачки -> раскладки, общие на все файлы: размеры фигур
/// единые (uniform_sizes), как у обычного рендера. Подпись ложится на
/// единственный лист.
fn export_layouts(schemes: Vec<Scheme>, st: &Style) -> Vec<(String, Vec<Layout>)> {
    let normed: Vec<Sizes> = schemes.iter().map(|s| normalize(&s.nodes, st)).collect();
    let sizes = uniform_sizes(&normed);
    schemes
        .into_iter()
        .map(|s| {
            // порезка не нужна: страниц нет, схема одна. Маркеры ручного
            // переноса — тоже: кружок-соединитель на одном листе сирота
            let mut nodes = s.nodes;
            strip_markers(&mut nodes);
            let st = Style {
                no_split: true,
                ..st.clone()
            };
            let mut l = layout(&nodes, &sizes, &st);
            l.title = s.title;
            (s.path, vec![l])
        })
        .collect()
}

/// SVG по содержимому, без листа: для вставки в отчёт.
pub fn render_tight_batch(schemes: Vec<Scheme>, st: &Style) -> Vec<(String, String)> {
    export_layouts(schemes, st)
        .into_iter()
        .map(|(path, parts)| {
            let joined = join_parts(&parts);
            (path, render_svg_tight(&joined, st))
        })
        .collect()
}

/// Книга draw.io по той же пачке: один файл со всеми листами.
/// Имя файла схемы без расширения — заголовок страницы draw.io.
fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Страницы одной схемы -> имя книги: «main» или «main — сигнатура».
fn book_title(stem: &str, pages: &[Layout]) -> String {
    match pages.first().and_then(|l| l.title.clone()) {
        Some(t) => format!("{stem} — {t}"),
        None => stem.to_string(),
    }
}

pub fn render_drawio_batch(schemes: Vec<Scheme>, st: &Style) -> Vec<(String, String)> {
    let (laid, scale) = paginate(&schemes, st);
    laid.into_iter()
        .map(|(path, pages)| {
            let name = book_title(&stem_of(&path), &pages);
            // имена страниц даёт book(): лист 1..N добавляется сам
            let named: Vec<(String, Layout)> =
                pages.into_iter().map(|l| (String::new(), l)).collect();
            (path, crate::drawio::book(&name, &named, st, scale))
        })
        .collect()
}

/// Вся пачка — в одну книгу draw.io: каждая схема и каждый её лист
/// отдельной страницей. Один файл на всю лабу вместо папки обрывков.
pub fn render_drawio_book(schemes: Vec<Scheme>, st: &Style) -> String {
    let (laid, scale) = paginate(&schemes, st);
    let sections: Vec<(String, Vec<Layout>)> = laid
        .into_iter()
        .map(|(path, pages)| (book_title(&stem_of(&path), &pages), pages))
        .collect();
    crate::drawio::book_all(&sections, st, scale)
}

/// Склеивает части порезки в одну раскладку: вертикальные смещения
/// частей складываются, иначе блоки наложатся.
fn join_parts(parts: &[Layout]) -> Layout {
    let Some(first) = parts.first() else {
        return Layout::default();
    };
    if parts.len() == 1 {
        return first.clone();
    }
    let mut out = first.clone();
    let mut shift = 0.0f64;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            shift = part.bounds.1 - (out.bounds.1 + out.bounds.3);
        }
        for sh in &part.shapes {
            let mut sh = sh.clone();
            sh.cy += shift;
            out.shapes.push(sh);
        }
        for e in &part.edges {
            out.edges.push(Edge {
                points: e.points.iter().map(|(x, y)| (*x, y + shift)).collect(),
                arrow: e.arrow,
            });
        }
        for lb in &part.labels {
            out.labels.push(Label {
                x: lb.x,
                y: lb.y + shift,
                text: lb.text.clone(),
                ha: lb.ha.clone(),
            });
        }
    }
    let xs: Vec<f64> = out
        .shapes
        .iter()
        .flat_map(|sh| [sh.cx - sh.w / 2.0, sh.cx + sh.w / 2.0])
        .collect();
    let ys: Vec<f64> = out
        .shapes
        .iter()
        .flat_map(|sh| [sh.cy - sh.h / 2.0, sh.cy + sh.h / 2.0])
        .collect();
    let minx = xs.iter().cloned().fold(f64::MAX, f64::min);
    let miny = ys.iter().cloned().fold(f64::MAX, f64::min);
    let maxx = xs.iter().cloned().fold(f64::MIN, f64::max);
    let maxy = ys.iter().cloned().fold(f64::MIN, f64::max);
    out.bounds = (minx, miny, maxx - minx, maxy - miny);
    out
}

/// Узлы из parse_batch + стиль -> (путь, страницы SVG). Длинные схемы
/// режутся на листы А4 (или одним листом при no_split — тогда высокий
/// лист ужимает общий масштаб); размеры фигур и масштаб общие на всю
/// пачку: figures одного типа во всех файлах — одного визуального размера.
pub fn render_batch(schemes: Vec<Scheme>, st: &Style) -> Vec<(String, Vec<String>)> {
    let (laid, s) = paginate(&schemes, st);
    laid.into_iter()
        .map(|(path, pages)| {
            let pages = pages.iter().map(|l| render_svg_at(l, st, s)).collect();
            (path, pages)
        })
        .collect()
}

/// Постраничная раскладка пачки и её единый масштаб.
///
/// Масштаб пачки — минимум постраничных вписываний, и рез идёт под
/// него: бюджет листа = А4/масштаб. При бюджете 1.0 лист режется на
/// 672pt и рисуется на 0.22 — четыре пятых А4 пустые.
///
/// Эталон реза (бюджет 1.0) — верхняя граница масштаба: заполнение не
/// должно стоить кегля. У высокой узкой схемы слияние листов дало бы
/// 0.22 вместо 1.0 — лучше лишний лист, чем нечитаемая схема. А у
/// схемы, упёршейся в ширину листа, масштаб задан шириной и от числа
/// листов не зависит — там заполнение бесплатно.
fn paginate(schemes: &[Scheme], st: &Style) -> (Vec<(String, Vec<Layout>)>, f64) {
    let normed: Vec<Sizes> = schemes.iter().map(|s| normalize(&s.nodes, st)).collect();
    let sizes = uniform_sizes(&normed);
    let ref_laid = laid_pages(schemes, &sizes, st, 1.0);
    let s_ref = batch_scale(&ref_laid, st);
    let mut cut = st.split_min;
    let mut laid = laid_pages(schemes, &sizes, st, cut);
    let mut s = batch_scale(&laid, st);
    // масштаб выше бюджета реза — листы и так полные; ниже — лист
    // упёрся в ширину и недозаполнен по высоте: перерезать под
    // фактический масштаб (бюджет растёт, листов меньше)
    for _ in 0..2 {
        if s + 0.005 >= cut {
            break;
        }
        cut = s;
        laid = laid_pages(schemes, &sizes, st, cut);
        s = batch_scale(&laid, st);
    }
    if s < s_ref * 0.97 {
        laid = ref_laid;
        s = s_ref;
    }
    // README обещает предупреждение, а не тихо нечитаемый лист
    if s < st.split_min {
        eprintln!(
            "предупреждение: масштаб {s:.3} ниже читаемого {:.2} — кегль {:.1} pt; \
             уменьшите схему или разрешите рез на листы",
            st.split_min,
            st.font * s
        );
    }
    (laid, s)
}

/// Постраничная раскладка пачки с бюджетом листа А4/`scale`.
fn laid_pages(
    schemes: &[Scheme],
    sizes: &Sizes,
    st: &Style,
    scale: f64,
) -> Vec<(String, Vec<Layout>)> {
    let cut_st = Style {
        split_scale: scale,
        ..st.clone()
    };
    schemes
        .iter()
        .map(|s| {
            let mut pages = if st.no_split {
                // без порезки маркер ручного переноса рисовать нечем: лист один
                let mut nodes = s.nodes.clone();
                strip_markers(&mut nodes);
                vec![layout(&nodes, sizes, &cut_st)]
            } else {
                split_scheme(s.nodes.clone(), sizes, &cut_st)
                    .iter()
                    .map(|part| layout(part, sizes, &cut_st))
                    .collect()
            };
            // подпись функции — на первом листе: продолжение начинается
            // с кружка-соединителя, коробке там некуда стрелять
            if let Some(first) = pages.first_mut() {
                first.title = s.title.clone();
            }
            (s.path.clone(), pages)
        })
        .collect()
}

/// Единый масштаб пачки: минимальное постраничное вписывание.
fn batch_scale(laid: &[(String, Vec<Layout>)], st: &Style) -> f64 {
    laid.iter()
        .flat_map(|(_, pages)| pages)
        .map(|l| fit_scale(l, st))
        .fold(1.0_f64, f64::min)
}

/// Один вход -> страницы SVG. `is_c` — обратная совместимость wasm-моста.
pub fn render_text(text: &str, is_c: bool, opts: &Options) -> Result<Vec<String>, ParseError> {
    let kind = if is_c { InputKind::C } else { InputKind::Gvn };
    render_text_kind(text, kind, opts)
}

/// Один вход с явным видом: C, C++ или .gvn.
pub fn render_text_kind(
    text: &str,
    kind: InputKind,
    opts: &Options,
) -> Result<Vec<String>, ParseError> {
    match parse_batch(&[(String::new(), text.to_string(), kind)], opts) {
        Ok(schemes) => Ok(render_batch(schemes, &opts.style()).remove(0).1),
        Err((_, e)) => Err(e),
    }
}

/// JSON-ответ пачки: страницы каждой схемы + два вида отказов
/// («empty» для входа без рисуемых операторов, «errors» для прочего).
/// У ошибки есть `kind` (`syntax`/`unsupported`/`empty`) — клиент не
/// угадывает по тексту сообщения. Одна форма для wasm и serve.
pub fn batch_json(
    files: &[(String, Vec<String>)],
    empty: &[(String, ParseError)],
    errors: &[(String, ParseError)],
) -> String {
    fn esc(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }
    let file = |(name, pages): &(String, Vec<String>)| {
        let sheets = pages
            .iter()
            .map(|p| format!("\"{}\"", esc(p)))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"name\":\"{}\",\"sheets\":[{sheets}]}}", esc(name))
    };
    let err_item = |(name, e): &(String, ParseError)| {
        format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"msg\":\"{}\",\"line\":{},\"col\":{},\"src\":{}}}",
            esc(name),
            e.kind.as_str(),
            esc(&e.msg),
            e.line.map_or("null".into(), |l| l.to_string()),
            e.col.map_or("null".into(), |c| c.to_string()),
            match &e.src {
                Some(s) => format!("\"{}\"", esc(s)),
                None => "null".to_string(),
            }
        )
    };
    let empty: Vec<String> = empty.iter().map(err_item).collect();
    let errs: Vec<String> = errors.iter().map(err_item).collect();
    let files: Vec<String> = files.iter().map(file).collect();
    format!(
        "{{\"ok\":true,\"files\":[{}],\"empty\":[{}],\"errors\":[{}]}}",
        files.join(","),
        empty.join(","),
        errs.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::gvn::parse;

    fn opts() -> Options {
        Options {
            labels: "en".into(),
            font: None,
            lw: None,
            no_split: false,
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

    /// Вид ошибки — полем `kind`, а не догадкой по тексту.
    #[test]
    fn batch_json_carries_error_kind() {
        let json = batch_json(
            &[],
            &[("a.c".into(), ParseError::empty("нет операторов"))],
            &[],
        );
        assert!(json.contains("\"kind\":\"empty\""), "{json}");
    }

    #[test]
    fn batch_uses_uniform_sizes() {
        // широкая плитка в A должна раздуть act-размер и в B
        let a = "x = aaaaaaaa * bbbbbbbb * cccccccc * dddddddd\noutput printf(1)\n";
        let b = "input scanf(1)\nx = 1\noutput printf(2)\n";
        let schemes = parse_batch(
            &[
                ("a.gvn".into(), a.into(), InputKind::Gvn),
                ("b.gvn".into(), b.into(), InputKind::Gvn),
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
                ("a.gvn".into(), a.into(), InputKind::Gvn),
                ("b.gvn".into(), b.into(), InputKind::Gvn),
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

    /// Экспорт для отчёта: SVG по содержимому — без листа и без
    /// вписывания, размер в миллиметрах (в Word встаёт 1:1).
    #[test]
    fn tight_export_has_mm_and_no_scaling() {
        let schemes = parse_batch(
            &[(
                "a.gvn".into(),
                "input scanf(1)\nx = 2\noutput printf(3)\n".into(),
                InputKind::Gvn,
            )],
            &opts(),
        )
        .unwrap();
        let out = render_tight_batch(schemes, &opts().style());
        assert_eq!(out.len(), 1);
        let svg = &out[0].1;
        assert!(svg.contains("width=\"") && svg.contains("mm\""), "нет мм");
        assert!(!svg.contains("scale("), "tight-экспорт без вписывания");
        assert!(svg.contains("<rect"), "нет подложки");
    }

    /// Draw.io-книга — редактируемый XML со ГОСТ-формами, одна на вход.
    #[test]
    fn drawio_export_is_editable_mxfile() {
        let schemes = parse_batch(
            &[("a.gvn".into(), "x = 1\ny = 2\n".into(), InputKind::Gvn)],
            &opts(),
        )
        .unwrap();
        let out = render_drawio_batch(schemes, &opts().style());
        assert_eq!(out.len(), 1);
        let xml = &out[0].1;
        assert!(xml.contains("<mxfile host=\"gostpadi\">"));
        assert!(xml.contains("vertex=\"1\""), "блоки не редактируемые");
        assert!(
            xml.matches("<diagram ").count() == 1,
            "одна схема — один diagram"
        );
    }

    /// mode=3: страниц в книге ровно столько, сколько SVG-листов у пачки,
    /// и каждая схема/лист — отдельный `<diagram>` с именем.
    #[test]
    fn drawio_book_page_count_matches_svg_sheets() {
        let long: String = (0..120).map(|i| format!("a = {i}\n")).collect();
        let schemes = parse_batch(
            &[
                (
                    "short.gvn".into(),
                    "x = 1\noutput printf(1)\n".into(),
                    InputKind::Gvn,
                ),
                ("long.gvn".into(), long, InputKind::Gvn),
            ],
            &opts(),
        )
        .unwrap();
        let st = opts().style();
        let sheets: usize = render_batch(schemes.clone(), &st)
            .iter()
            .map(|(_, p)| p.len())
            .sum();
        assert!(sheets > 2, "длинная схема должна резаться: {sheets}");
        let xml = render_drawio_book(schemes, &st);
        assert_eq!(
            xml.matches("<diagram ").count(),
            sheets,
            "число страниц книги не равно числу листов"
        );
        assert!(
            xml.contains("short") && xml.contains("long"),
            "нет имён схем"
        );
    }

    /// Маркер ручного переноса `// перенос` до SVG не доезжает: на
    /// страницах — кружки-соединители, слова «перенос» нет.
    #[test]
    fn marker_never_reaches_pages() {
        let src = "int main(void) {\n    int a = 1;\n    // перенос\n    int b = 2;\n}";
        let pages = render_text(src, true, &opts()).unwrap();
        assert_eq!(pages.len(), 2, "маркер ломает страницу");
        for p in &pages {
            assert!(!p.contains("перенос"), "маркер попал в SVG");
            assert!(p.contains("<circle"), "нет кружка-соединителя");
        }
    }

    /// no_split: маркер съедается — одна страница, ни одного кружка
    /// (--trim/--drawio идут тем же стрипом).
    #[test]
    fn no_split_swallows_markers() {
        let src = "int main(void) {\n    int a = 1;\n    // перенос\n    int b = 2;\n}";
        let mut o = opts();
        o.no_split = true;
        let pages = render_text(src, true, &o).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(!pages[0].contains("перенос"), "маркер попал в SVG");
        assert!(!pages[0].contains("<circle"), "сиротский кружок-маркер");
        // экспорт отчёта — тот же стрип
        let schemes = parse_batch(&[("a.c".into(), src.into(), InputKind::C)], &opts()).unwrap();
        let xml = &render_drawio_batch(schemes, &opts().style())[0].1;
        assert!(!xml.contains("перенос"), "маркер попал в draw.io");
    }

    /// Высота листа в pt (`height="…pt"` из шапки SVG).
    fn page_height(svg: &str) -> f64 {
        svg.split("height=\"")
            .nth(1)
            .and_then(|p| p.split("pt").next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }

    /// Рез идёт на равные доли: «полный лист + огрызок» — это лишний
    /// полупустой лист, а не экономия. Шестьдесят операторов при бюджете
    /// листа 3054pt (масштаб 0.22) — два листа примерно по 2240, второй
    /// не из двух фигур.
    #[test]
    fn cut_balances_pages_instead_of_full_plus_runt() {
        let st = opts().style();
        let text: String = (0..60).map(|i| format!("a = {i}\n")).collect();
        let nodes = parse(&text, &st, "en").unwrap();
        let sizes = normalize(&nodes, &st);
        let cut_st = Style {
            split_scale: 0.22,
            ..st.clone()
        };
        let parts = split_scheme(nodes, &sizes, &cut_st);
        let h: Vec<f64> = parts
            .iter()
            .map(|p| layout(p, &sizes, &st).bounds.3)
            .collect();
        let budget = (st.a4_h - 2.0 * st.page_pad) / 0.22;
        assert_eq!(parts.len(), 2, "60 операторов — два листа: {h:?}");
        assert!(h.iter().all(|&v| v <= budget), "лист не влез: {h:?}");
        assert!(
            h[1] >= 0.85 * h[0],
            "второй лист — огрызок: {h:?} (нужны равные доли)"
        );
    }

    /// Четыре лабы одним прогоном (запрос пользователя: меньше 32 листов
    /// на отчёт). Лист режется под тот масштаб, с которым рисуется, —
    /// иначе он ужимается до 0.22 и четыре пятых А4 пустуют.
    #[test]
    fn lab_batch_cuts_for_real_scale() {
        let st = opts().style();
        let mut schemes = Vec::new();
        for n in 1..=4 {
            let path = format!("{}/corpus/oap-lab{n}.c", env!("CARGO_MANIFEST_DIR"));
            let text = std::fs::read_to_string(&path).unwrap();
            let inputs = vec![(format!("lab{n}"), text, InputKind::C)];
            schemes.extend(parse_batch(&inputs, &opts()).unwrap());
        }
        let normed: Vec<Sizes> = schemes.iter().map(|s| normalize(&s.nodes, &st)).collect();
        let sizes = uniform_sizes(&normed);
        let out = render_batch(schemes.clone(), &st);
        let sheets: usize = out.iter().map(|(_, pages)| pages.len()).sum();
        assert!(sheets < 32, "листов {sheets}: экономии нет");
        for (name, pages) in &out {
            let s: f64 = scales(&pages[0])[0].parse().unwrap();
            assert!(s >= 0.20, "{name}: масштаб {s} ниже пола 0.20");
            for (i, svg) in pages.iter().enumerate() {
                let h = page_height(svg);
                // лист-продолжение заполнен по высоте, иначе рез был зря.
                // Половина — предел, когда схема чуть выше листа: 1.15
                // бюджета режется на два листа по 0.57, а на один лист
                // масштаб упал бы ниже пола 0.20.
                if pages.len() > 1 {
                    assert!(
                        h >= 0.5 * st.a4_h,
                        "{name} лист {i}: {h:.0}pt из {:.0} — полупустой",
                        st.a4_h
                    );
                }
            }
        }
        // каждый лист — валидная ГОСТ-схема: линии не заходят на блоки,
        // нет обрывов и второго входа
        for s in &schemes {
            let name = &s.path;
            let cut_st = Style {
                split_scale: st.split_min,
                ..st.clone()
            };
            let parts = split_scheme(s.nodes.clone(), &sizes, &cut_st);
            for (i, part) in parts.iter().enumerate() {
                let l = layout(part, &sizes, &st);
                assert_eq!(
                    crossings_ok(&l.shapes, &l.edges),
                    Ok(()),
                    "{name} лист {i}: линии заходят на блок"
                );
                assert!(overlaps_ok(&l.shapes), "{name} лист {i}: фигуры налезли");
                assert!(dangling_ok(&l).is_ok(), "{name} лист {i}: обрыв линии");
                assert!(single_entry_ok(&l), "{name} лист {i}: второй вход");
            }
        }
    }
}
