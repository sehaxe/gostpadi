//! serve: локальный HTTP-дверь движка — агентам и скриптам.
//!
//! Слушает только 127.0.0.1: движок не светится наружу. Обмен — JSON,
//! та же форма, что сайт получает через wasm (pipeline::batch_json).
//!
//! Маршруты:
//!   GET  /health → ok
//!   POST /render?lang=c|cpp|gvn&labels=ru|en&font=&lw=&no_split=1   body=код
//!   POST /lint?lang=…  — инварианты раскладки (аналог CLI --check)
//!   POST /export?mode=1|2|3&lang=… — 1: SVG в мм, 2: draw.io на схему,
//!                                  3: одна книга draw.io на всю пачку
//!
//! lang — язык входа: по умолчанию «c», для .gvn укажи lang=gvn.
//! Пачечная семантика как у CLI: сбойный вход не роняет ответ —
//! он попадает в «errors» (или «empty»), HTTP-статус остаётся 200.

use gostpadi::frontend::InputKind;
use gostpadi::pipeline::{self, Options};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

pub fn serve(port: u16) {
    let addr = format!("127.0.0.1:{port}");
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("serve: {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!("gostpadi serve: http://{addr}");
    println!("GET  /health  — жив ли");
    println!("POST /render  — код -> страницы SVG (JSON, как у сайта)");
    println!("POST /lint    — проверка схемы без рисования");
    println!("POST /export  — mode=1: SVG в мм, mode=2: draw.io, mode=3: книга на пачку");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || handle(s));
            }
            Err(e) => eprintln!("serve: {e}"),
        }
    }
}

/// Один HTTP-запрос: читаем заголовки, ждём ровно Content-Length тела,
/// роутим, отвечаем и закрываем соединение.
fn handle(mut stream: TcpStream) {
    // Локальный клиент, который замолчал на середине запроса, не должен
    // держать поток сервера вечно: заголовок/тело ждём ограниченное время.
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) => return, // клиент ушёл до заголовка
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(p) = find_head(&buf) {
                    break p;
                }
                if buf.len() > 64 * 1024 {
                    let _ = reply(
                        &stream,
                        431,
                        r#"{"ok":false,"msg":"заголовки слишком велики"}"#,
                    );
                    return;
                }
            }
            Err(_) => return,
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let wanted_body = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().ok())
                .flatten()
        })
        .unwrap_or(0);
    if wanted_body > 4 * 1024 * 1024 {
        let _ = reply(&stream, 413, r#"{"ok":false,"msg":"тело больше 4 МБ"}"#);
        return;
    }
    let body_start = (head_end + 4).min(buf.len());
    let mut have = buf.len() - body_start;
    while have < wanted_body {
        match stream.read(&mut chunk) {
            Ok(0) => {
                let _ = reply(&stream, 400, r#"{"ok":false,"msg":"обрыв тела запроса"}"#);
                return;
            }
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                have += n;
            }
            Err(_) => {
                let _ = reply(&stream, 400, r#"{"ok":false,"msg":"обрыв тела запроса"}"#);
                return;
            }
        }
    }
    let request_line = head.lines().next().unwrap_or("");
    let body: Vec<u8> = buf[body_start..body_start + wanted_body.min(have)].to_vec();
    let (status, content) = route(request_line, &body);
    let _ = reply(&stream, status, &content);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn find_head(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn reply(stream: &TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let mut s = stream;

    let text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "OK",
    };
    let mut out = format!("HTTP/1.1 {status} {text}\r\n");
    out.push_str("Content-Type: application/json; charset=utf-8\r\n");
    out.push_str("Access-Control-Allow-Origin: *\r\n");
    out.push_str("X-Content-Type-Options: nosniff\r\n");
    out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    out.push_str("Connection: close\r\n\r\n");
    let mut wrapped = out.into_bytes();
    wrapped.extend_from_slice(body.as_bytes());
    s.write_all(&wrapped)
}

fn parse_query(q: &str) -> BTreeMap<String, String> {
    q.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) => (k.to_owned(), v.to_owned()),
            None => (kv.to_owned(), String::new()),
        })
        .collect()
}

fn opts_from(q: &BTreeMap<String, String>) -> Options {
    Options {
        labels: match q.get("labels").map(String::as_str) {
            Some("ru") => "ru".to_string(),
            _ => "en".to_string(),
        },
        font: q
            .get("font")
            .and_then(|v| v.parse().ok())
            .filter(|f| *f > 0.0),
        lw: q
            .get("lw")
            .and_then(|v| v.parse().ok())
            .filter(|f| *f > 0.0),
        no_split: q
            .get("no_split")
            .map(|v| *v == "1" || *v == "true")
            .unwrap_or(false),
    }
}

/// Вход для одного HTTP-обращения: имя схемы синтетическое, язык берём
/// из ?lang — детерминизм вместо гадания по содержимому.
fn as_input(lang: &str, text: &str) -> Result<(String, String, InputKind), String> {
    match lang {
        "c" => Ok(("код.c".to_string(), text.to_owned(), InputKind::C)),
        "cpp" => Ok(("код.cpp".to_string(), text.to_owned(), InputKind::Cpp)),
        "gvn" => Ok(("схема.gvn".to_string(), text.to_owned(), InputKind::Gvn)),
        other => Err(format!("lang={other}: только c, cpp и gvn")),
    }
}

/// Локальный esc: тот же словарь экранирования, что в pipeline::batch_json.
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

pub fn route(request_line: &str, body: &[u8]) -> (u16, String) {
    enum Mode {
        Tight,
        Drawio,
        Book,
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, raw_query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let q = parse_query(raw_query);

    if method == "GET" && path == "/health" {
        return (200, "{\"ok\":true,\"msg\":\"gostpadi serve\"}".to_string());
    }
    if path != "/render" && path != "/lint" && path != "/export" {
        return (
            404,
            "{\"ok\":false,\"msg\":\"маршруты: GET /health, POST /render, /lint, /export\"}"
                .to_string(),
        );
    }
    if method != "POST" {
        return (
            405,
            format!("{{\"ok\":false,\"msg\":\"метод {method} не подходит: нужен POST\"}}"),
        );
    }
    // Параметры режима проверяются до разбора тела: кривой mode — ошибка
    // запроса (400), даже если код внутри тоже не разбирается.
    let export_mode = if path == "/export" {
        match q.get("mode").map(String::as_str) {
            Some("2") => Some(Mode::Drawio),
            Some("3") => Some(Mode::Book),
            Some("1") | None => Some(Mode::Tight),
            Some(other) => {
                return (
                    400,
                    format!(
                        "{{\"ok\":false,\"error\":\"mode: только 1|2|3, получено {}\"}}",
                        esc(other)
                    ),
                )
            }
        }
    } else {
        None
    };
    let text = match std::str::from_utf8(body) {
        Ok(t) => t,
        Err(_) => {
            return (
                400,
                "{\"ok\":false,\"msg\":\"тело запроса не в UTF-8\"}".to_string(),
            )
        }
    };
    let input = match as_input(q.get("lang").map(String::as_str).unwrap_or("c"), text) {
        Ok(v) => v,
        Err(e) => return (400, format!("{{\"ok\":false,\"msg\":\"{}\"}}", esc(&e))),
    };

    let args = opts_from(&q);
    let st = args.style();
    let schemes = match pipeline::parse_batch(std::slice::from_ref(&input), &args) {
        Ok(s) => s,
        Err((_, e)) => {
            // полный отказ разбора тоже не рождает ошибку HTTP: агент
            // читает «errors[].msg», а не угадывает по коду статуса
            let out = pipeline::batch_json(&[], &[], &[("".into(), e)]);
            return (200, out);
        }
    };

    if path == "/lint" {
        let checks: Vec<String> = pipeline::validate(&schemes, &st)
            .into_iter()
            .map(|(name, res)| match res {
                Ok(msg) => format!(
                    "{{\"name\":\"{}\",\"ok\":true,\"msg\":\"{}\"}}",
                    esc(&name),
                    esc(&msg)
                ),
                Err(msg) => format!(
                    "{{\"name\":\"{}\",\"ok\":false,\"msg\":\"{}\"}}",
                    esc(&name),
                    esc(&msg)
                ),
            })
            .collect();
        return (
            200,
            format!("{{\"ok\":true,\"checks\":[{}]}}", checks.join(",")),
        );
    }

    match path {
        // render_batch не умеет падать: раскладка схемы инвариантна сама
        "/render" => {
            let pages = pipeline::render_batch(schemes, &st);
            (200, pipeline::batch_json(&pages, &[], &[]))
        }
        "/export" => {
            let mode = export_mode.expect("mode проверен выше");
            // mode=3 — одна книга на всю пачку: столько же страниц,
            // сколько SVG-листов, но в одном файле
            let out: Vec<(String, Vec<String>)> = match mode {
                Mode::Book => vec![(
                    "schemes.drawio".to_string(),
                    vec![pipeline::render_drawio_book(schemes, &st)],
                )],
                Mode::Drawio => pipeline::render_drawio_batch(schemes, &st)
                    .into_iter()
                    .map(|(n, xml)| (n, vec![xml]))
                    .collect(),
                Mode::Tight => pipeline::render_tight_batch(schemes, &st)
                    .into_iter()
                    .map(|(n, svg)| (n, vec![svg]))
                    .collect(),
            };
            (200, pipeline::batch_json(&out, &[], &[]))
        }
        _ => (
            404,
            "{\"ok\":false,\"msg\":\"маршрут не найден\"}".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "int main(void){\nint a;\na = 1;\na = a + 2;\nreturn 0;\n}\n";

    fn post(path: &str, body: &str) -> (u16, String) {
        route(&format!("POST {path} HTTP/1.1"), body.as_bytes())
    }

    #[test]
    fn health_and_404() {
        let (s, b) = route("GET /health HTTP/1.1", b"");
        assert_eq!(s, 200);
        assert!(b.contains("gostpadi"));
        let (s, _) = route("GET /nope HTTP/1.1", b"");
        assert_eq!(s, 404);
    }

    #[test]
    fn method_not_allowed() {
        let (s, b) = route("GET /render HTTP/1.1", b"");
        assert_eq!(s, 405);
        assert!(b.contains("POST"));
    }

    #[test]
    fn render_c_gives_pages() {
        let (s, b) = post("/render", MAIN);
        assert_eq!(s, 200);
        assert!(b.contains("<svg"));
        assert!(!b.contains("\"errors\":[{"), "чистый код — ошибок нет: {b}"); // поля есть, элементы пусты
    }

    #[test]
    fn render_gvn_needs_lang() {
        let (s, b) = post("/render?lang=gvn", "x = 1\noutput printf(1)\n");
        assert_eq!(s, 200);
        assert!(b.contains("<svg"));
    }

    /// `lang=cpp` — тот же код-фронтенд: using/std::/cin/cout не отказ.
    #[test]
    fn render_cpp_uses_code_frontend() {
        let src = concat!(
            "using namespace std;\n",
            "int main() {\n",
            "    int a;\n",
            "    cin >> a;\n",
            "    std::cout << a;\n",
            "    return 0;\n",
            "}\n",
        );
        let (s, b) = post("/render?lang=cpp", src);
        assert_eq!(s, 200);
        assert!(b.contains("<svg"), "{b}");
        assert!(b.contains("cout &lt;&lt; a"), "{b}");
    }

    #[test]
    fn parse_error_comes_as_errors_json() {
        let (s, b) = post("/render", "int main(void){ if (1 { return 0; } }\n");
        assert_eq!(s, 200);
        assert!(b.contains("\"errors\":"));
        assert!(b.contains("\"msg\""));
    }

    #[test]
    fn lint_reports_ok() {
        let (s, b) = post("/lint?lang=gvn", "x = 1\noutput printf(1)\n");
        assert_eq!(s, 200);
        assert!(b.contains("\"ok\":true"));
        assert!(b.contains("ok:"));
    }

    #[test]
    fn export_drawio_carries_blocks() {
        let (s, b) = post("/export?mode=2", MAIN);
        assert_eq!(s, 200);
        assert!(b.contains("mxGraphModel") || b.contains("<mx"));
    }

    #[test]
    fn lang_gate_rejects_junk() {
        let (s, b) = post("/render?lang=asm", "nop");
        assert_eq!(s, 400);
        assert!(b.contains("lang=asm"));
    }

    /// mode проверяется до разбора тела: мусорный код не маскирует
    /// опечатку в режиме экспорта.
    #[test]
    fn invalid_mode_is_400_before_parse() {
        let (s, b) = post("/export?mode=9", "не Си");
        assert_eq!(s, 400, "{b}");
        assert!(b.contains("mode"), "{b}");
    }

    /// mode=3 — одна книга на всю пачку: схемы и их листы становятся
    /// страницами одного файла.
    #[test]
    fn export_mode_3_is_one_book_with_pages() {
        let src = "int add(int a, int b) { int s = a + b; return s; }\n\
                   int mul(int a, int b) { int p = a * b; return p; }\n\
                   int main(void) { printf(\"%d\", add(1, 2)); return 0; }\n";
        let (s, b) = post("/export?mode=3", src);
        assert_eq!(s, 200);
        assert!(b.contains("\"name\":\"schemes.drawio\""), "{b}");
        assert_eq!(b.matches("<diagram ").count(), 3, "три схемы — три листа");
        assert!(b.contains("pageWidth=\\\"595.276\\\""), "нет точного А4");
        assert!(
            b.contains("id=\\\"title\\\" value=\\\"add(int a, int b)\\\""),
            "нет подписи функции"
        );
    }

    /// Не-UTF8 тело — 400, а не паника from_utf8.
    #[test]
    fn non_utf8_body_is_400() {
        let (s, b) = route("POST /render HTTP/1.1", &[0xff, 0xfe, 0x80]);
        assert_eq!(s, 400);
        assert!(b.contains("UTF-8"), "{b}");
    }

    /// Пустой вход — не 500: движок отвечает 200 и человеческой ошибкой.
    #[test]
    fn empty_input_is_reported_not_crash() {
        let (s, b) = post("/render?lang=gvn", "");
        assert_eq!(s, 200);
        assert!(b.contains("\"errors\":[{"), "{b}");
        assert!(b.contains("схема пустая"), "{b}");
    }

    /// mode=1 — SVG по содержимому в миллиметрах, без вписывания.
    #[test]
    fn export_mode_1_is_tight_svg_in_mm() {
        let (s, b) = post("/export?mode=1&lang=gvn", "x = 1\noutput printf(1)\n");
        assert_eq!(s, 200);
        assert!(b.contains("mm\\\""), "нет миллиметров: {b}");
        assert!(!b.contains("scale("), "tight-экспорт с вписыванием");
    }

    /// font=inf раньше ронял поток раскладки (пустой ответ), NaN писал
    /// в SVG. Теперь — конечный дефолт.
    #[test]
    fn non_finite_font_and_lw_stay_finite() {
        for q in ["/render?lang=gvn&font=inf", "/render?lang=gvn&lw=inf"] {
            let (s, b) = post(q, "x = 1\n");
            assert_eq!(s, 200, "{q}");
            assert!(!b.contains("NaN") && !b.contains("inf"), "{q}: {b}");
        }
    }

    /// Сырой HTTP: статусы, обрыв тела и таймаут засыпавшего клиента.
    mod http {
        use super::*;
        use std::io::{Read, Write};
        use std::net::{Shutdown, TcpListener};
        use std::time::Duration;

        /// Запускает handle() на эфемерном порту; возвращает адрес и
        /// обработчик потока.
        fn server() -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = l.local_addr().unwrap();
            let t = std::thread::spawn(move || {
                let (s, _) = l.accept().unwrap();
                handle(s);
            });
            (addr, t)
        }

        fn send(addr: std::net::SocketAddr, raw: &[u8], shutdown: bool) -> String {
            let mut c = std::net::TcpStream::connect(addr).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            c.write_all(raw).unwrap();
            if shutdown {
                c.shutdown(Shutdown::Write).unwrap();
            }
            let mut out = Vec::new();
            let _ = c.read_to_end(&mut out);
            String::from_utf8_lossy(&out).into_owned()
        }

        #[test]
        fn oversized_header_431_and_body_413() {
            let (addr, t) = server();
            let mut req = b"GET /health HTTP/1.1\r\nX: ".to_vec();
            req.extend(std::iter::repeat_n(b'a', 70 * 1024));
            assert!(send(addr, &req, false).starts_with("HTTP/1.1 431"));
            t.join().unwrap();

            let (addr, t) = server();
            let r = send(
                addr,
                b"POST /render HTTP/1.1\r\nContent-Length: 5000000\r\n\r\n",
                false,
            );
            assert!(r.starts_with("HTTP/1.1 413"), "{r}");
            t.join().unwrap();
        }

        #[test]
        fn truncated_body_is_400() {
            let (addr, t) = server();
            let r = send(
                addr,
                b"POST /render HTTP/1.1\r\nContent-Length: 100\r\n\r\n0123456789",
                true,
            );
            assert!(r.starts_with("HTTP/1.1 400"), "{r}");
            t.join().unwrap();
        }

        /// Клиент замолчал на середине заголовка: поток не должен
        /// висеть вечно, соединение закрывается по таймауту.
        #[test]
        fn silent_client_does_not_leak_thread() {
            let (addr, t) = server();
            let mut c = std::net::TcpStream::connect(addr).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
            c.write_all(b"GET /health HTTP/1.1\r\nX: partial").unwrap();
            let mut out = Vec::new();
            let _ = c.read_to_end(&mut out);
            t.join().expect("поток сервера завис на молчащем клиенте");
        }
    }
}
