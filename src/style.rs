#[derive(Debug, Clone)]
pub struct Style {
    pub font: f64,
    pub char_w: f64,
    pub pad_x: f64,
    pub pad_y: f64,
    pub pitch: f64,
    pub max_chars: usize,
    pub cond_chars: usize,
    /// добор к ширине текста фигуры (act/io/loop)
    pub text_pad: f64,
    /// добор к ширине текста ромба (запас между рёбрами)
    pub cond_pad: f64,
    /// вертикальные/горизонтальные пады терминатора
    pub term_pad_v: f64,
    pub term_pad_h: f64,
    /// подрезка высоты: pitch уже содержит межстрочный запас
    pub line_slack: f64,
    pub grid: f64,
    pub vgap: f64,
    pub hgap: f64,
    pub colgap: f64,
    pub mgap: f64,
    pub jog: f64,
    pub rail: f64,
    pub rail_step: f64,
    pub conn_r: f64,
    pub conn_step: f64,
    pub aspect: f64,
    pub a4_w: f64,
    pub a4_h: f64,
    pub page_pad: f64,
    pub split_scale: f64,
    pub edge_lw: f64,
    pub label_dx: f64,
    pub label_dy: f64,
    pub label_exit_dx: f64,
    pub label_axis_dx: f64,
    pub vertex_label_dy: f64,
    pub label_gap: f64,
    pub letters: &'static str,
    pub io_words: &'static [&'static str],
}

const IO_WORDS: &[&str] = &[
    "printf", "printf_s", "scanf", "scanf_s", "scan", "print", "println", "puts", "putchar",
    "echo", "getchar", "gets", "cin", "cout", "read", "write",
];

/// Коэффициенты производных метрик от кегля: значения при font = 12 pt.
const CHAR_W_RATE: f64 = 0.61; // ширина моноширинного глифа DejaVu Sans Mono
const PITCH_RATE: f64 = 1.5; // межстрочный шаг
const PAD_X_RATE: f64 = 1.5; // поля текста по горизонтали
const PAD_Y_RATE: f64 = 1.17; // поля текста по вертикали
const TEXT_PAD_RATE: f64 = 6.0 / 12.0; // добор ширины фигуры
const COND_PAD_RATE: f64 = 26.0 / 12.0; // запас текста в ромбе
const TERM_PAD_V_RATE: f64 = 16.0 / 12.0; // высота капсулы сверх текста
const TERM_PAD_H_RATE: f64 = 22.0 / 12.0; // ширина капсулы сверх текста
const LINE_SLACK_RATE: f64 = 4.0 / 12.0; // подрезка высоты блока
const LABEL_DY_RATE: f64 = 1.0; // подпись над точкой: базовая линия
const VERTEX_LABEL_DY_RATE: f64 = 11.0 / 12.0;
const LABEL_GAP_RATE: f64 = 16.0 / 12.0;

/// Модульная сетка 5 мм (b = 2a, ГОСТ 19.701-90) при font = 12 pt.
/// Все зазоры задаются целыми долями сетки, без литералов вида 14.2.
const GRID: f64 = 14.17;

impl Style {
    /// Шаблон DEFAULT записан для font = 12 pt, lw = 1.0; произвольные
    /// значения кегля/пера — только через with_metrics, иначе
    /// производные метрики разъедутся с font.
    pub const DEFAULT: Style = Style {
        font: 12.0,
        char_w: 7.32,
        pad_x: 18.0,
        pad_y: 14.04,
        pitch: 18.0,
        max_chars: 30,
        cond_chars: 22,
        text_pad: 6.0,
        cond_pad: 26.0,
        term_pad_v: 16.0,
        term_pad_h: 22.0,
        line_slack: 4.0,
        grid: GRID,
        vgap: 3.0 * GRID,
        hgap: 2.0 * GRID,
        colgap: GRID,
        mgap: 2.0 * GRID,
        jog: GRID,
        rail: GRID,
        rail_step: 2.0 * GRID,
        conn_r: GRID,
        conn_step: GRID,
        aspect: 1.0,
        a4_w: 468.0,
        a4_h: 700.0,
        page_pad: 14.0,
        split_scale: 0.70,
        edge_lw: 1.0,
        label_dx: 14.0,
        label_dy: 12.0,
        label_exit_dx: 2.0 * GRID,
        label_axis_dx: 14.0,
        vertex_label_dy: 11.0,
        label_gap: 16.0,
        letters: "АБВГДЕЖЗИКЛМНОПРСТУФХЦЧШЭЮЯ",
        io_words: IO_WORDS,
    };

    /// Все текстовые метрики от кегля, линии — от толщины пера:
    /// measure/render берут их из полей, поэтому смена font растит
    /// геометрию согласованно, смена lw — только толщину линий и усиков.
    pub fn with_metrics(font: f64, edge_lw: f64) -> Style {
        let mut s = Style::DEFAULT;
        s.font = font;
        s.edge_lw = edge_lw;
        s.char_w = font * CHAR_W_RATE;
        s.pitch = font * PITCH_RATE;
        s.pad_x = font * PAD_X_RATE;
        s.pad_y = font * PAD_Y_RATE;
        s.text_pad = font * TEXT_PAD_RATE;
        s.cond_pad = font * COND_PAD_RATE;
        s.term_pad_v = font * TERM_PAD_V_RATE;
        s.term_pad_h = font * TERM_PAD_H_RATE;
        s.line_slack = font * LINE_SLACK_RATE;
        s.label_dy = font * LABEL_DY_RATE;
        s.vertex_label_dy = font * VERTEX_LABEL_DY_RATE;
        s.label_gap = font * LABEL_GAP_RATE;
        // зазоры кратны модульной сетке (см. GRID)
        s.vgap = 3.0 * s.grid;
        s.hgap = 2.0 * s.grid;
        s.colgap = s.grid;
        s.mgap = 2.0 * s.grid;
        s.jog = s.grid;
        s.rail = s.grid;
        s.rail_step = 2.0 * s.grid;
        s.conn_r = s.grid;
        s.conn_step = s.grid;
        s.label_exit_dx = 2.0 * s.grid;
        s
    }

    pub fn is_io(&self, text: &str) -> bool {
        let t = text.trim_start();
        for w in self.io_words {
            if let Some(rest) = t.strip_prefix(*w) {
                if rest.is_empty() {
                    return true;
                }
                let c = rest.chars().next().unwrap();
                if !c.is_alphanumeric() && c != '_' {
                    return true;
                }
            }
        }
        false
    }
}

impl Default for Style {
    fn default() -> Self {
        Style::DEFAULT
    }
}

pub const DEFAULT: Style = Style::DEFAULT;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_values() {
        let s = Style::default();
        assert!((s.font - 12.0).abs() < 1e-9);
        assert!((s.grid - 14.17).abs() < 1e-9);
        assert!((s.label_exit_dx - s.grid * 2.0).abs() < 0.01);
        assert_eq!(s.max_chars, 30);
        assert_eq!(s.cond_chars, 22);
    }
    /// Все зазоры кратны модульной сетке: возврат литералов (42.5, 28.3,
    /// 14.2) ломает сеточный инвариант раскладки.
    #[test]
    fn gaps_are_grid_multiples() {
        for (src, s) in [
            ("default", Style::default()),
            ("with_metrics", Style::with_metrics(24.0, 2.0)),
        ] {
            let g = s.grid;
            for (name, v, k) in [
                ("vgap", s.vgap, 3.0),
                ("hgap", s.hgap, 2.0),
                ("colgap", s.colgap, 1.0),
                ("mgap", s.mgap, 2.0),
                ("jog", s.jog, 1.0),
                ("rail", s.rail, 1.0),
                ("rail_step", s.rail_step, 2.0),
                ("conn_r", s.conn_r, 1.0),
                ("conn_step", s.conn_step, 1.0),
                ("label_exit_dx", s.label_exit_dx, 2.0),
            ] {
                assert!(
                    (v - k * g).abs() < 1e-9,
                    "{src}: {name} = {v}, ожидалось {k} * grid = {}",
                    k * g
                );
            }
        }
    }
    #[test]
    fn is_io_detects() {
        let s = Style::default();
        assert!(s.is_io("printf(\"hi\")"));
        assert!(s.is_io("scanf(\"%d\", &a)"));
        assert!(s.is_io("scanf_s(\"%d\", &a)"), "суффиксная _s-версия scanf");
        assert!(s.is_io("printf_s(\"hi\")"), "суффиксная _s-версия printf");
        assert!(
            !s.is_io("scanf_sx(\"%d\", &a)"),
            "_s не ловит длиннее слово"
        );
        assert!(!s.is_io("a = printf + 1"));
        assert!(!s.is_io("myprintf"));
    }
    /// DEFAULT — это with_metrics(12, 1): шаблон и конструктор согласованы.
    #[test]
    fn default_is_with_metrics_at_12pt() {
        let d = Style::default();
        let m = Style::with_metrics(12.0, 1.0);
        for (a, b) in [
            (d.char_w, m.char_w),
            (d.pitch, m.pitch),
            (d.pad_x, m.pad_x),
            (d.pad_y, m.pad_y),
            (d.text_pad, m.text_pad),
            (d.cond_pad, m.cond_pad),
            (d.term_pad_v, m.term_pad_v),
            (d.term_pad_h, m.term_pad_h),
            (d.line_slack, m.line_slack),
            (d.label_dy, m.label_dy),
            (d.vertex_label_dy, m.vertex_label_dy),
            (d.label_gap, m.label_gap),
        ] {
            assert!((a - b).abs() < 1e-9, "{a} != {b}");
        }
    }
    /// Смена кегля растит все производные метрики согласованно.
    #[test]
    fn with_metrics_scales_from_font() {
        let s = Style::with_metrics(24.0, 2.5);
        assert!((s.char_w - 24.0 * 0.61).abs() < 1e-9);
        assert!((s.pitch - 24.0 * 1.5).abs() < 1e-9);
        assert!((s.pad_x - 24.0 * 1.5).abs() < 1e-9);
        assert!((s.pad_y - 24.0 * 1.17).abs() < 1e-9);
        assert!((s.edge_lw - 2.5).abs() < 1e-9);
        // сетка и страница — от кегля не зависят
        assert!((s.grid - 14.17).abs() < 1e-9);
        assert!((s.a4_w - 468.0).abs() < 1e-9);
    }
}
