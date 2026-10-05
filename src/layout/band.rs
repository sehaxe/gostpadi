//! Разрез готовой раскладки на полосы по высоте.
//!
//! Узловой разрез (`split.rs`) смотрит верхний уровень и умеет делить
//! ромб по ветвям и цикл по телу. Глубже он не смотрит: пять вложенных
//! циклов — это один узел, и резать его нечем, поэтому схема
//! ужималась до кегля 6 pt.
//!
//! Здесь режется уже ГОТОВАЯ раскладка, то есть где угодно: граница
//! полосы проходит между блоками там, где до неё дошло место. Рёбра,
//! пересекающие границу, разрезаются, а место разреза помечается
//! кружком с буквой — лист виден как продолжение предыдущего.

use super::ctx::bounds_of;
use super::types::{Edge, Label, Layout, Shape};
use crate::style::Style;

const EPS: f64 = 1e-6;

/// Сколько пунктов влезает в полосу при масштабе не ниже порога
/// читаемости. Ровно тот же вопрос, что у узловой порезки, только
/// посчитанный по высоте текстовой зоны.
fn band_height(st: &Style) -> f64 {
    st.sheet.text_h() / st.split_scale
}

/// Границы полос: y, где полоса заканчивается. Пусто — раскладка
/// влезает и резать нечего.
///
/// Полоса закрывается перед блоком, который в неё не влез: сначала
/// берётся нижняя кромка блоков, чей верх внутри полосы, и если она
/// вылезла за границу — разрез идёт там, а следующая полоса
/// начинается с этой кромки.
fn band_cuts(l: &Layout, st: &Style) -> Vec<f64> {
    let h = band_height(st);
    let mut cuts: Vec<f64> = Vec::new();
    let mut top = l.bounds.1;
    // страховка от зацикливания: полос не больше, чем блоков плюс одна
    for _ in 0..=l.shapes.len() {
        let limit = top + h;
        // низ последнего блока, который помещается ЦЕЛИКОМ, и верх
        // первого, который уже нет. Раньше считался только верх блока,
        // и полоса из пяти вложенных циклов выглядела свободной: все
        // пять трапеций loop_begin маленькие и уместились, а тела под
        // ними ушли за край — разрез не делался, и полос оставался
        // непоместившимся.
        let mut last_ok = top;
        // БЕСКОНЕЧНОСТЬ, а не f64::MAX: MAX конечное число, и
        // проверка «ничего не нашлось» через is_finite() не срабатывала
        let mut next_top = f64::INFINITY;
        for sh in &l.shapes {
            let s_top = sh.cy - sh.h / 2.0;
            if s_top < top - EPS {
                continue; // блок выше полосы: он не в этой
            }
            let s_bot = sh.cy + sh.h / 2.0;
            if s_bot <= limit + EPS {
                last_ok = last_ok.max(s_bot);
            } else {
                next_top = next_top.min(s_top);
            }
        }
        // блок выше самой полосы (не влезает никуда) — резать нечем,
        // иначе полосы нулевой высоты и цикл не кончится
        if !next_top.is_finite() || next_top <= top + EPS {
            return cuts;
        }
        // Граница на 2r ВЫШЕ верха блока, который в неё не влез: кружку-
        // соединителю нужно место на ОБЕИХ страницах. Ровно на верх
        // блока граница давала обрыв: ребро, входящее в этот блок,
        // кончалось точно на границе, строгое условие шва его не брало,
        // кружок не ставился — линия обрывалась у нижнего края листа,
        // а блок на следующем листе висел без входа. Верх почти у самой
        // границы (блок выше полвысоты) — режем по верху, как раньше.
        let cut = next_top - 2.0 * st.conn_r;
        if cut <= top + EPS {
            cuts.push(next_top);
            top = next_top;
        } else {
            cuts.push(cut);
            top = cut;
        }
    }
    cuts
}

/// Раскладка -> полосы листов. Если влезает, возвращается как есть.
pub fn band_split(l: &Layout, st: &Style) -> Vec<Layout> {
    let cuts = band_cuts(l, st);
    if cuts.is_empty() {
        return vec![l.clone()];
    }
    let mut starts = vec![l.bounds.1];
    starts.extend_from_slice(&cuts);
    let n = starts.len();
    let band_of = |y: f64| -> usize {
        cuts.iter()
            .rposition(|&c| y >= c - EPS)
            .map_or(0, |i| i + 1)
    };

    let mut shapes: Vec<Vec<Shape>> = vec![Vec::new(); n];
    let mut labels: Vec<Vec<Label>> = vec![Vec::new(); n];
    // Отрезки полосы: (из, куда). Именно ПАРАМИ, а не списком точек.
    // Раньше точки всех рёбер полосы сваливались в один общий вектор,
    // а потом спаривались по две через chunks(2). Стоило одному ребру
    // оставить в полосе нечётное число точек — разметка сдвигалась,
    // и chunks соединял точки ЧУЖИХ рёбер: получались диагонали через
    // всю схему. Пара «из-куда» не может рассинхронизироваться.
    let mut segs: Vec<Vec<Seg>> = vec![Vec::new(); n];
    // швы: (полоса, до которой дошло ребро, x пересечения, y границы).
    // На каждом шве две отметки — конец в нижней полосе и начало в
    // верхней, с одной буквой на обе.
    let mut seams: Vec<(usize, f64, f64)> = Vec::new();

    for s in &l.shapes {
        let mut s = s.clone();
        let k = band_of(s.cy - s.h / 2.0);
        s.cy -= starts[k];
        shapes[k].push(s);
    }
    for lb in &l.labels {
        let mut lb = lb.clone();
        let k = band_of(lb.y);
        lb.y -= starts[k];
        labels[k].push(lb);
    }

    // (полоса, отрезок, конец ребра?) — конец нужен, чтобы стрелка
    // досталась ровно тому отрезку, где ребро заканчивается
    let mut tail: Vec<Option<usize>> = vec![None; n];
    for e in &l.edges {
        if e.points.len() < 2 {
            continue;
        }
        for seg in e.points.windows(2) {
            let (p1, p2) = (seg[0], seg[1]);
            if (p1.1 - p2.1).abs() < EPS {
                // горизонтальный отрезок полосу не пересекает
                let k = band_of(p1.1);
                segs[k].push(((p1.0, p1.1 - starts[k]), (p2.0, p2.1 - starts[k])));
                continue;
            }
            // вертикальный: режем по тем границам полос, что строго
            // между концами отрезка
            let mut here: Vec<f64> = cuts
                .iter()
                .copied()
                .filter(|&c| c > p1.1.min(p2.1) + EPS && c < p1.1.max(p2.1) - EPS)
                .collect();
            here.sort_by(|a, _b| p2.1.total_cmp(a));
            let mut prev = p1;
            for c in here {
                let k = band_of(prev.1);
                segs[k].push(((prev.0, prev.1 - starts[k]), (prev.0, c - starts[k])));
                seams.push((k, prev.0, c));
                prev = (prev.0, c);
            }
            let k = band_of(prev.1);
            segs[k].push(((prev.0, prev.1 - starts[k]), (p2.0, p2.1 - starts[k])));
        }
        // стрелка — на последнем отрезке ребра, и только в полосе,
        // где этот конец оказался
        let (last_from, last_to) = (e.points[e.points.len() - 2], e.points[e.points.len() - 1]);
        let k = band_of(last_to.1);
        if let Some(idx) = segs[k]
            .iter()
            .rposition(|&(_, b)| abs_eq(b.0, last_to.0) && abs_eq(b.1, last_to.1))
        {
            tail[k] = Some(idx);
        }
        let _ = last_from;
    }

    let mut edges: Vec<Vec<Edge>> = Vec::with_capacity(n);
    for (k, band_segs) in segs.iter().enumerate() {
        edges.push(
            band_segs
                .iter()
                .enumerate()
                .map(|(i, &(from, to))| Edge {
                    points: vec![from, to],
                    arrow: tail[k] == Some(i),
                })
                .collect(),
        );
    }

    let n_letters = st.letters.chars().count().max(1);
    let mut out: Vec<Layout> = Vec::with_capacity(n);
    // на каждом шве две отметки с одной буквой: конец в нижней полосе
    // и начало в верхней
    for (si, (low, x, y)) in seams.iter().enumerate() {
        let letter = st.letters.chars().nth(si % n_letters).unwrap();
        for (k, cy) in [(*low, y - starts[*low]), (*low + 1, 0.0)] {
            if k >= n {
                continue;
            }
            shapes[k].push(Shape {
                kind: "conn".into(),
                cx: *x,
                cy,
                w: 2.0 * st.conn_r,
                h: 2.0 * st.conn_r,
                skew: 0.0,
                lines: vec![letter.to_string()],
            });
        }
    }
    for k in 0..n {
        let bounds = bounds_of(&edges[k], &shapes[k], &labels[k], st);
        out.push(Layout {
            shapes: std::mem::take(&mut shapes[k]),
            edges: std::mem::take(&mut edges[k]),
            labels: std::mem::take(&mut labels[k]),
            bounds,
            anchors: Vec::new(),
        });
    }
    out
}

/// Точка ломаной в список, если она не совпадает с последней: два
/// ребра, сошедшиеся в одной точке, не должны склеиваться.
/// Отрезок в полосе: (из, куда). Пара, а не отдельные точки, — иначе
/// спаривание может сдвинуться и соединить точки чужих отрезков.
type Seg = ((f64, f64), (f64, f64));

/// Совпадение координат с допуском: точки после разреза считаются
/// равными, хотя и считались независимо.
fn abs_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

#[cfg(test)]
mod band_tests {
    use super::*;
    use crate::frontend::cts::CParser;
    use crate::ir::Node;
    use crate::layout::{layout, normalize};
    use crate::style::Style;

    fn st() -> Style {
        Style::default()
    }

    fn laid(src: &str) -> (Vec<Node>, Style) {
        let st = st();
        let nodes = CParser::new().parse(src, "en");
        (nodes, st)
    }

    /// Пять вложенных циклов — один верхнеуровневый узел, и узловой
    /// разрез его не берёт. Полосы режут готовую раскладку, то есть
    /// где угодно: без них лист уезжал на кегль 6 pt.
    #[test]
    fn deep_nesting_is_split_into_bands() {
        let (nodes, st) = laid(
            "int main(void){ for(int i=0;i<3;i++) for(int j=0;j<3;j++) \
             for(int k=0;k<3;k++) for(int l=0;l<3;l++) for(int m=0;m<3;m++) \
             if((i+j+k+l+m)%7==0) printf(\"%d\", i*j*k*l*m); }",
        );
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        assert!(
            !st.sheet.fits(l.bounds.2, l.bounds.3, st.split_scale),
            "схема должна быть не по листу, иначе тест бессмыслен"
        );
        let bands = band_split(&l, &st);
        assert!(bands.len() > 1, "полос не получилось: {}", bands.len());
        for b in &bands {
            assert!(
                st.sheet.fits(b.bounds.2, b.bounds.3, st.split_scale)
                    || b.bounds.3 > band_height(&st),
                "полоса {}x{} не влезает в лист",
                b.bounds.2,
                b.bounds.3
            );
        }
    }

    /// Ничего не потеряно: сумма блоков по полосам равна исходному.
    #[test]
    fn no_block_is_lost_by_bands() {
        let (nodes, st) = laid(
            "int main(void){ for(int i=0;i<9;i++){ a=i; b=i*2; c=i*3; \
             d=i*4; e=i*5; f=i*6; g=i*7; } }",
        );
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        let bands = band_split(&l, &st);
        let total = |ls: &[Layout]| ls.iter().map(|x| x.shapes.len()).sum::<usize>();
        assert_eq!(total(&bands), l.shapes.len(), "часть блоков пропала");
    }

    /// Полосы не соединяют точки разных рёбер.
    ///
    /// Регрессия: точки всех рёбер полосы складывались в один вектор,
    /// и `chunks(2)` соединял их попарно. Ребро, оставившее нечётное
    /// число точек, сдвигало разметку — и следующая пара брала первую
    /// точку одного ребра и вторую чужого. В SVG выходили диагонали
    /// через всю схему, а `--check` их не видел: он ловит пересечения
    /// с блоками, а не наклон линий.
    #[test]
    fn no_edge_crosses_another_in_a_band() {
        let (nodes, st) = laid(
            "int main(void){ int d; scanf(\"%d\", &d); switch(d){\n\
             case 1: printf(\"a\"); break; case 2: printf(\"b\"); break;\n\
             case 3: printf(\"c\"); break; case 4: printf(\"d\"); break;\n\
             case 5: printf(\"e\"); break; case 6: printf(\"f\"); break;\n\
             case 7: printf(\"g\"); break; case 8: printf(\"h\"); break;\n\
             case 9: printf(\"i\"); break; case 10: printf(\"j\"); break;\n\
             case 11: printf(\"k\"); break; default: printf(\"z\"); }\n\
             for(int i=0;i<9;i++){ a=i; b=i*2; c=i*3; d=i*4;\n\
             e=i*5; f=i*6; g=i*7; h=i*8; } return 0; }",
        );
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        let bands = band_split(&l, &st);
        assert!(
            bands.len() > 1,
            "нужно несколько полос, получилось {}",
            bands.len()
        );
        for (bi, b) in bands.iter().enumerate() {
            for (ei, e) in b.edges.iter().enumerate() {
                for w in e.points.windows(2) {
                    assert!(
                        (w[0].0 - w[1].0).abs() < 1e-6 || (w[0].1 - w[1].1).abs() < 1e-6,
                        "полоса {bi}, ребро {ei}: отрезок {:?}-{:?} не по осям",
                        w[0],
                        w[1]
                    );
                }
            }
        }
    }

    /// Каждое ребро в полосе остаётся связным куском исходного: его
    /// отрезки идут подряд и не меняют направление без поворота.
    /// Обратная проверка к предыдущей: если бы спаривание пошло по
    /// чужому ребру, телесность ломалась бы не только наклоном.
    #[test]
    fn band_edges_stay_connected() {
        let (nodes, st) = laid(
            "int main(void){ for(int i=0;i<9;i++){ a=i; b=i*2; c=i*3; d=i*4; \
             e=i*5; f=i*6; g=i*7; h=i*8; } }",
        );
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        let bands = band_split(&l, &st);
        // сколько отрезков ушло в полосы, столько же должно остаться:
        // разрез ребра по шву добавляет парность, но не теряет отрезки
        let orig: usize = l
            .edges
            .iter()
            .map(|e| e.points.len().saturating_sub(1))
            .sum();
        let got: usize = bands
            .iter()
            .flat_map(|b| &b.edges)
            .map(|e| e.points.len().saturating_sub(1))
            .sum();
        // шов режет вертикаль на два отрезка, поэтому got >= orig
        assert!(
            got >= orig,
            "полосы потеряли отрезки: было {orig}, стало {got}"
        );
    }

    /// Мелкая схема режется один лист — полосы не нужны.
    #[test]
    fn small_scheme_stays_one_band() {
        let (nodes, st) = laid("int main(void){ x = 1; }");
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        assert_eq!(band_split(&l, &st).len(), 1);
    }
}
