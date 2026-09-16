#[derive(Debug, Clone)]
pub struct Style {
    pub font: f64,
    pub font_stack: &'static [&'static str],
    pub char_w: f64,
    pub pad_x: f64,
    pub pad_y: f64,
    pub pitch: f64,
    pub max_chars: usize,
    pub cond_chars: usize,
    pub grid: f64,
    pub vgap: f64,
    pub hgap: f64,
    pub colgap: f64,
    pub mgap: f64,
    pub jog: f64,
    pub rail: f64,
    pub rail_step: f64,
    pub term_round: f64,
    pub conn_r: f64,
    pub conn_from_end: f64,
    pub conn_step: f64,
    pub aspect: f64,
    pub a4_w: f64,
    pub a4_h: f64,
    pub page_pad: f64,
    pub split_scale: f64,
    pub dpi: u32,
    pub max_px_per_pt: f64,
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

const FONT_STACK: &[&str] = &["DejaVu Sans Mono", "Noto Sans CJK TC", "DejaVu Sans"];
const IO_WORDS: &[&str] = &[
    "printf", "scanf", "scan", "print", "println", "puts", "putchar", "echo", "getchar", "gets",
    "cin", "cout", "read", "write",
];

impl Style {
    pub const DEFAULT: Style = Style {
        font: 12.0,
        font_stack: FONT_STACK,
        char_w: 7.3,
        pad_x: 18.0,
        pad_y: 14.0,
        pitch: 18.0,
        max_chars: 30,
        cond_chars: 22,
        grid: 14.17,
        vgap: 42.5,
        hgap: 28.3,
        colgap: 14.2,
        mgap: 28.3,
        jog: 14.2,
        rail: 14.2,
        rail_step: 28.3,
        term_round: 14.0,
        conn_r: 14.2,
        conn_from_end: 28.3,
        conn_step: 14.2,
        aspect: 1.0,
        a4_w: 468.0,
        a4_h: 700.0,
        page_pad: 14.0,
        split_scale: 0.70,
        dpi: 200,
        max_px_per_pt: 3.5,
        edge_lw: 1.0,
        label_dx: 14.0,
        label_dy: 12.0,
        label_exit_dx: 28.34,
        label_axis_dx: 14.0,
        vertex_label_dy: 11.0,
        label_gap: 16.0,
        letters: "АБВГДЕЖЗИКЛМНОПРСТУФХЦЧШЭЮЯ",
        io_words: IO_WORDS,
    };

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
    #[test]
    fn is_io_detects() {
        let s = Style::default();
        assert!(s.is_io("printf(\"hi\")"));
        assert!(s.is_io("scanf(\"%d\", &a)"));
        assert!(!s.is_io("a = printf + 1"));
        assert!(!s.is_io("myprintf"));
    }
}
