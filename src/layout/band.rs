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
        cuts.push(next_top);
        top = next_top;
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
    let mut pts: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
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

    let mut last_band: Option<(usize, (f64, f64))> = None;
    for e in &l.edges {
        let cnt = e.points.len();
        if cnt < 2 {
            continue;
        }
        for seg in e.points.windows(2) {
            let (p1, p2) = (seg[0], seg[1]);
            if (p1.1 - p2.1).abs() < EPS {
                // горизонтальный отрезок полосу не пересекает
                let k = band_of(p1.1);
                push(&mut pts[k], (p1.0, p1.1 - starts[k]));
                push(&mut pts[k], (p2.0, p2.1 - starts[k]));
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
                push(&mut pts[k], (prev.0, prev.1 - starts[k]));
                push(&mut pts[k], (prev.0, c - starts[k]));
                seams.push((k, prev.0, c));
                prev = (prev.0, c);
            }
            let k = band_of(prev.1);
            push(&mut pts[k], (prev.0, prev.1 - starts[k]));
            push(&mut pts[k], (p2.0, p2.1 - starts[k]));
        }
        // стрелка стоит на последнем отрезке ребра, чей конец попал
        // в эту полосу: полоса без конца стрелки не получает
        last_band = Some((band_of(e.points[cnt - 1].1), e.points[cnt - 1]));
    }

    let mut edges: Vec<Vec<Edge>> = Vec::with_capacity(n);
    for (k, band_pts) in pts.iter().enumerate() {
        // стрелка — только на последнем отрезке той полосы, в которую
        // пришёлся конец ребра
        let tail = matches!(last_band, Some((b, p)) if b == k
            && band_pts.last().is_some_and(|&q| q.0 - p.0 < EPS && q.1 - p.1 < EPS));
        let n_seg = band_pts.len() / 2;
        edges.push(
            band_pts
                .chunks(2)
                .enumerate()
                .filter(|c| c.1.len() == 2)
                .map(|(i, c)| Edge {
                    points: vec![c[0], c[1]],
                    arrow: tail && i + 1 == n_seg,
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
fn push(out: &mut Vec<(f64, f64)>, p: (f64, f64)) {
    if out
        .last()
        .is_some_and(|&q| (q.0 - p.0).abs() < EPS && (q.1 - p.1).abs() < EPS)
    {
        return;
    }
    out.push(p);
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

    /// Мелкая схема режется один лист — полосы не нужны.
    #[test]
    fn small_scheme_stays_one_band() {
        let (nodes, st) = laid("int main(void){ x = 1; }");
        let sizes = normalize(&nodes, &st);
        let l = layout(&nodes, &sizes, &st);
        assert_eq!(band_split(&l, &st).len(), 1);
    }
}
