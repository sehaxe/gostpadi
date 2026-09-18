//! WASM-мост для сайта: C-ABI поверх пайплайна, ноль зависимостей.
//! Протокол: указатель+длина входа, флаги, на выходе буфер с JSON
//! {"ok":true,"sheets":[...]} либо {"ok":false,"error":{...}}.
//! Память: буфер выдаёт Rust, освобождение — gostpadi_free.

use crate::pipeline::{self, Options};

/// JSON-строка в буфер: кавычки, слэш и управляющие символы.
fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::write(out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Текст схемы -> ответ в формате JSON (листы SVG либо ошибка разбора).
fn render_result(text: &str, is_c: bool, ru: bool, lw: f64, font: f64) -> String {
    let opts = Options {
        labels: if ru { "ru" } else { "en" }.into(),
        font: (font > 0.0).then_some(font),
        lw: (lw > 0.0).then_some(lw),
    };
    match pipeline::render_text(text, is_c, &opts) {
        Ok(pages) => {
            let mut out = String::with_capacity(pages.iter().map(|p| p.len()).sum::<usize>() + 32);
            out.push_str("{\"ok\":true,\"sheets\":[");
            for (i, p) in pages.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_str(&mut out, p);
            }
            out.push_str("]}");
            out
        }
        Err(e) => {
            let mut out = String::with_capacity(128);
            out.push_str("{\"ok\":false,\"error\":{\"msg\":");
            json_str(&mut out, &e.msg);
            out.push_str(",\"line\":");
            match e.line {
                Some(l) => out.push_str(&l.to_string()),
                None => out.push_str("null"),
            }
            out.push_str(",\"col\":");
            match e.col {
                Some(c) => out.push_str(&c.to_string()),
                None => out.push_str("null"),
            }
            out.push_str(",\"src\":");
            match &e.src {
                Some(s) => json_str(&mut out, s),
                None => out.push_str("null"),
            }
            out.push_str("}}");
            out
        }
    }
}

/// Вход: (ptr, len) в UTF-8; is_c/ru — флаги; lw/font — 0 = дефолт.
/// Возврат: указатель на буфер с JSON, длина записана в *out_len.
/// Паника движка (кривой ввод недопустим по контракту парсеров) —
/// wasm-trap: страница покажет «движок упал» и перезапустит модуль.
#[no_mangle]
pub extern "C" fn gostpadi_render(
    ptr: *const u8,
    len: usize,
    is_c: u8,
    ru: u8,
    lw: f64,
    font: f64,
    out_len: *mut usize,
) -> *mut u8 {
    if ptr.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = if len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    let text = String::from_utf8_lossy(bytes);
    let json = render_result(&text, is_c != 0, ru != 0, lw, font);
    let boxed = json.into_bytes().into_boxed_slice();
    let p = boxed.as_ptr() as *mut u8;
    let n = boxed.len();
    std::mem::forget(boxed);
    unsafe { *out_len = n };
    p
}

/// Буфер для данных со стороны JS (вход, out_len): пишет страница.
#[no_mangle]
pub extern "C" fn gostpadi_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Освобождение буфера из gostpadi_render/gostpadi_alloc.
#[no_mangle]
pub extern "C" fn gostpadi_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe { drop(Vec::from_raw_parts(ptr, len, len)) };
    }
}
