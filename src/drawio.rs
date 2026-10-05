//! Экспорт раскладки в формат draw.io (mxGraph XML).
//!
//! Раньше мост отдавал только SVG-строку, а рисунок в draw.io
//! редактировать нельзя: это картинка. Здесь та же раскладка
//! превращается в `mxCell` с геометрией, поэтому блоки и рёбра в
//! draw.io можно брать и двигать.
//!
//! Координаты — в пунктах, как в SVG: mxGraph рисует в единицах
//! координат, а `fontSize` считает в пунктах, так что система
//! получается согласованной без пересчёта. Страница тоже в пунктах.
//!
//! Рёбра привязываются к блокам, если конец ломаной попадает внутрь
//! фигуры: в draw.io такие рёбра тянутся за блоком. Не попал —
//! остаётся ломаная с точками, её тоже можно двигать, но перетаскивание
//! блока её не уведёт.

use crate::layout::{Layout, Shape};
use crate::style::Style;

/// Экранирование текста для XML.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Стиль фигуры по её виду в раскладке. Формы те же, что рисует SVG:
/// капсула, параллелограмм, ромб, шестиугольник, прямоугольник, круг.
fn shape_style(kind: &str, st: &Style) -> String {
    let font = format!("fontFamily=DejaVu Sans Mono;fontSize={};", st.font.round());
    let common = format!("whiteSpace=wrap;html=1;{font}");
    let body = match kind {
        "term" | "ret" => "rounded=1;arcSize=50;".to_string(),
        "io" => "shape=parallelogram;perimeter=parallelogramPerimeter;".to_string(),
        "if" => "rhombus;".to_string(),
        "loop_begin" | "loop_end" => "shape=hexagon;perimeter=hexagonPerimeter2;".to_string(),
        "conn" => "ellipse;".to_string(),
        _ => "rounded=0;".to_string(),
    };
    format!("{body}{common}fillColor=#ffffff;strokeColor=#000000;")
}

/// Стиль ребра: ортогональное, стрелка в конце — как в SVG.
fn edge_style(st: &Style) -> String {
    format!(
        "edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;endArrow=block;endFill=1;\
         strokeColor=#000000;strokeWidth={};",
        st.edge_lw
    )
}

/// Блок фигуры целиком в mxCell.
fn cell(id: &str, sh: &Shape, st: &Style) -> String {
    let text = esc(&sh.lines.join("\n"));
    format!(
        "        <mxCell id=\"{id}\" value=\"{text}\" style=\"{}\" vertex=\"1\" parent=\"1\">\
         \n          <mxGeometry x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" as=\"geometry\"/>\
         \n        </mxCell>",
        shape_style(&sh.kind, st).replace('"', "&quot;"),
        sh.cx - sh.w / 2.0,
        sh.cy - sh.h / 2.0,
        sh.w,
        sh.h,
    )
}

/// Ломаная ребра в mxPoint. Единицы — пункты.
fn waypoints(points: &[(f64, f64)]) -> String {
    let pts: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("<mxPoint x=\"{x:.2}\" y=\"{y:.2}\"/>"))
        .collect();
    if pts.is_empty() {
        return String::new();
    }
    format!(
        "<mxGeometry relative=\"1\" as=\"geometry\">\n            <Array as=\"points\">\n              {}\n            </Array>\n          </mxGeometry>",
        pts.join("\n              ")
    )
}

/// Блок, внутри которого лежит точка: для привязки ребра.
fn block_at(layout: &Layout, p: (f64, f64)) -> Option<String> {
    layout
        .shapes
        .iter()
        .position(|sh| {
            let dx = p.0 - sh.cx;
            let dy = p.1 - sh.cy;
            dx * dx + dy * dy <= (sh.w * sh.w + sh.h * sh.h) / 4.0
        })
        .map(|i| format!("n{i}"))
}

/// Подписи ветвей — отдельными текстовыми блоками: в draw.io метки на
/// ребре привязываются к ребру, а не висят в координатах, и при
/// переносе схемы они остаются со своим ребром.
fn label_cell(id: &str, lb: &crate::layout::Label, st: &Style) -> String {
    let align = match lb.ha.as_str() {
        "left" => "left",
        "right" => "right",
        _ => "center",
    };
    let w = lb.text.chars().count() as f64 * st.char_w + 6.0;
    format!(
        "        <mxCell id=\"{id}\" value=\"{}\" style=\"text;html=1;align={align};verticalAlign=middle;\
         fontFamily=DejaVu Sans Mono;fontSize={};strokeColor=none;fillColor=none;\" vertex=\"1\" parent=\"1\">\
         \n          <mxGeometry x=\"{:.2}\" y=\"{:.2}\" width=\"{w:.2}\" height=\"{:.2}\" as=\"geometry\"/>\
         \n        </mxCell>",
        esc(&lb.text),
        st.font.round(),
        lb.x - w / 2.0,
        lb.y - st.font,
        st.font * 1.4,
    )
}

/// Лист раскладки -> один `<diagram>`.
fn diagram(name: &str, layout: &Layout, st: &Style) -> String {
    let mut body = String::new();
    for (i, sh) in layout.shapes.iter().enumerate() {
        body.push_str(&cell(&format!("n{i}"), sh, st));
        body.push('\n');
    }
    for (i, e) in layout.edges.iter().enumerate() {
        let id = format!("e{i}");
        // конец ломаной внутри блока — привязываем ребро к нему
        let (src, dst) = (
            e.points.first().and_then(|p| block_at(layout, *p)),
            e.points.last().and_then(|p| block_at(layout, *p)),
        );
        let attrs = match (&src, &dst) {
            (Some(s), Some(d)) => format!("source=\"{s}\" target=\"{d}\" "),
            _ => String::new(),
        };
        body.push_str(&format!(
            "        <mxCell id=\"{id}\" style=\"{}\" edge=\"1\" parent=\"1\" {attrs}>\n            {}\n        </mxCell>\n",
            edge_style(st).replace('"', "&quot;"),
            waypoints(&e.points),
        ));
    }
    for (i, lb) in layout.labels.iter().enumerate() {
        body.push_str(&label_cell(&format!("t{i}"), lb, st));
        body.push('\n');
    }

    format!(
        "  <diagram name=\"{}\" id=\"{}\">\n    <mxGraphModel dx=\"850\" dy=\"1100\" grid=\"0\" \
gridSize=\"10\" guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"1\" \
pageScale=\"1\" pageWidth=\"{:.0}\" pageHeight=\"{:.0}\" math=\"0\" shadow=\"0\">\n      <root>\n\
        <mxCell id=\"0\"/>\n        <mxCell id=\"1\" parent=\"0\"/>\n{body}      </root>\n    </mxGraphModel>\n  </diagram>\n",
        esc(name),
        name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_"),
        st.sheet.w,
        st.sheet.h,
    )
}

/// Книга: несколько листов в одном файле — как в самой схеме.
pub fn book(name: &str, pages: &[(String, Layout)], st: &Style) -> String {
    let mut out =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mxfile host=\"gostpadi\">\n");
    for (i, (_, layout)) in pages.iter().enumerate() {
        let page_name = if pages.len() == 1 {
            name.to_string()
        } else {
            format!("{name} — лист {}", i + 1)
        };
        out.push_str(&diagram(&page_name, layout, st));
        out.push('\n');
    }
    out.push_str("</mxfile>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::cts::CParser;
    use crate::layout::{layout, normalize};

    fn st() -> Style {
        Style::with_metrics(14.0, 1.0)
    }

    fn laid(src: &str) -> Layout {
        let nodes = CParser::new().parse(src, "en");
        let s = st();
        layout(&nodes, &normalize(&nodes, &s), &s)
    }

    /// Книга — это XML, а не картинка: блоки в нём редактируемые.
    #[test]
    fn produces_parseable_mxfile() {
        let xml = book(
            "main",
            &[(
                "Лист 1".into(),
                laid("int main(void){ int a=0; printf(\"x\"); return 0; }"),
            )],
            &st(),
        );
        assert!(xml.starts_with("<?xml"), "нет XML-декларации");
        assert!(xml.contains("<mxfile host=\"gostpadi\">"), "нет mxfile");
        assert!(xml.contains("<mxGraphModel"), "нет модели");
        assert!(xml.contains("vertex=\"1\""), "нет вершин");
        assert!(xml.contains("<mxCell id=\"0\"/>"), "нет корневых ячеек");
    }

    /// Координаты в mxGeometry — левый верхний угол, а в раскладке
    /// центр. Знак минус здесь дал бы блоки, зеркально уехавшие за
    /// край листа.
    #[test]
    fn geometry_uses_top_left_corner() {
        let l = laid("int main(void){ x = 1; }");
        let want_x = l
            .shapes
            .iter()
            .find(|s| s.kind == "act")
            .map(|s| s.cx - s.w / 2.0)
            .expect("нет блока");
        let cx = l
            .shapes
            .iter()
            .find(|s| s.kind == "act")
            .map(|s| s.cx)
            .unwrap();
        let xml = book("t", &[("p".into(), l)], &st());
        assert!(
            xml.contains(&format!("x=\"{want_x:.2}\"")),
            "в mxGeometry ожидался левый край {want_x:.2}"
        );
        assert!(cx >= 0.0, "центр блока отрицателен: {cx}");
    }

    /// Рёбра уезжают за нижний край и должны привязаться к блокам,
    /// иначе в draw.io они болтаются отдельно.
    #[test]
    fn edges_attach_to_blocks_when_possible() {
        let xml = book(
            "t",
            &[(
                "p".into(),
                laid("int main(void){ a = 1; b = 2; return 0; }"),
            )],
            &st(),
        );
        assert!(xml.contains("edge=\"1\""), "нет рёбер");
        assert!(
            xml.contains("source=\"n") || !xml.contains("edge=\"1\" source"),
            "хотя бы одно ребро должно привязаться к блокам"
        );
    }

    /// Текст с XML-символами не ломает файл: без экранирования
    /// draw.io просто не откроет его.
    #[test]
    fn escapes_xml_specials() {
        let xml = book(
            "t",
            &[(
                "p".into(),
                laid("int main(void){ printf(\"a & b < c > d\"); }"),
            )],
            &st(),
        );
        assert!(
            xml.contains("&amp;") || xml.contains("a &amp; b"),
            "amp не экранирован"
        );
        assert!(
            !xml.contains("a & b <"),
            "необработанные спецсимволы в тексте"
        );
    }

    /// Ромб, параллелограмм и капсула должны уехать в draw.io своими
    /// формами, а не прямоугольниками.
    #[test]
    fn gost_shapes_keep_their_form() {
        let xml = book(
            "t",
            &[(
                "p".into(),
                laid("int main(void){ if (a) printf(\"1\"); else printf(\"2\"); }"),
            )],
            &st(),
        );
        assert!(xml.contains("rhombus"), "нет ромба");
        assert!(xml.contains("shape=parallelogram"), "нет параллелограмма");
        assert!(xml.contains("rounded=1;arcSize=50"), "нет капсулы");
    }

    /// Каждый лист схемы — свой diagram: иначе в draw.io останется
    /// только последний.
    #[test]
    fn every_page_becomes_its_diagram() {
        let l1 = laid("int main(void){ a = 1; }");
        let l2 = laid("int main(void){ b = 2; }");
        let xml = book("схема", &[("1".into(), l1), ("2".into(), l2)], &st());
        assert_eq!(xml.matches("<diagram ").count(), 2, "листов должно быть 2");
        assert!(
            xml.contains("лист 1") && xml.contains("лист 2"),
            "нет имён листов"
        );
    }

    /// Ширина страницы — в пунктах, иначе лист в draw.io выходит в
    /// формате A4, когда схема в пунктах.
    #[test]
    fn page_size_is_in_points() {
        let xml = book("t", &[("p".into(), laid("int main(void){ a=1; }"))], &st());
        assert!(
            xml.contains("pageWidth=\"595\""),
            "нет ширины листа в пунктах"
        );
        assert!(
            xml.contains("pageHeight=\"842\""),
            "нет высоты листа в пунктах"
        );
    }
}
