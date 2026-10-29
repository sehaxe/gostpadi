//! C++-обвязка C-фронтенда.
//!
//! Грамматика lang-c читает C, а не C++. Здесь C++-конструкции
//! маскируются пробелами или однобайтовыми заменами той же длины:
//! байтовые span'ы AST остаются координатами и в тексте подписей.
//! Строковые и символьные литералы не трогаем.

/// `namespace X { ... }` и `namespace X = Y;` — обёртка C++, у C её нет.
/// Заголовок и парная `}` маскируются пробелами (переводы строк целы),
/// содержимое остаётся на верхнем уровне: функции файла рисуются как обычно.
pub(crate) fn strip_namespaces(src: &str) -> String {
    let mut buf = src.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if !word_at(&buf, i, b"namespace") {
            i += 1;
            continue;
        }
        // `using namespace std;` — это директива, её снимает strip_using
        if prev_token(&buf, i).is_some_and(|(s, e)| &buf[s..e] == b"using") {
            i += "namespace".len();
            continue;
        }
        let Some(end) = namespace_end(&buf, i) else {
            i += "namespace".len();
            continue;
        };
        if buf[end - 1] == b'{' {
            if let Some(close) = matching_brace(&buf, end - 1) {
                let from = if prev_token(&buf, i).is_some_and(|(s, e)| &buf[s..e] == b"inline") {
                    prev_token(&buf, i).map_or(i, |(s, _)| s)
                } else {
                    i
                };
                mask_keep_newlines(&mut buf, from, end);
                mask_keep_newlines(&mut buf, close, close + 1);
                // `};` после namespace: пустое объявление C не понимает
                let mut semi = close + 1;
                while buf
                    .get(semi)
                    .is_some_and(|b| matches!(*b, b' ' | b'\t' | b'\r' | b'\n'))
                {
                    semi += 1;
                }
                if buf.get(semi) == Some(&b';') {
                    mask_keep_newlines(&mut buf, semi, semi + 1);
                }
                // внутрь блока: там могут быть вложенные namespace
                i = end;
                continue;
            }
        } else {
            mask_keep_newlines(&mut buf, i, end);
            i = end;
            continue;
        }
        i += "namespace".len();
    }
    from_bytes(buf, src)
}

/// Конец заголовка namespace: позиция после `{` или после `;` алиаса.
fn namespace_end(buf: &[u8], start: usize) -> Option<usize> {
    let mut j = skip_ws_all(buf, start + "namespace".len());
    loop {
        if buf.get(j) == Some(&b'{') {
            return Some(j + 1);
        }
        if buf.get(j) == Some(&b'=') {
            let mut k = j + 1;
            while k < buf.len() {
                if matches!(buf[k], b'"' | b'\'') {
                    k = skip_literal(buf, k);
                    continue;
                }
                if buf[k] == b';' {
                    return Some(k + 1);
                }
                k += 1;
            }
            return None;
        }
        j = ident_end(buf, j)?;
        let k = skip_ws_all(buf, j);
        if buf.get(k..k + 2) == Some(b"::") {
            j = skip_ws_all(buf, k + 2);
        } else {
            j = k;
        }
    }
}

fn skip_ws_all(buf: &[u8], mut i: usize) -> usize {
    while buf
        .get(i)
        .is_some_and(|b| matches!(*b, b' ' | b'\t' | b'\r' | b'\n'))
    {
        i += 1;
    }
    i
}

fn mask_keep_newlines(buf: &mut [u8], from: usize, to: usize) {
    for b in &mut buf[from..to] {
        if !matches!(*b, b'\n' | b'\r') {
            *b = b' ';
        }
    }
}

/// C++-обвязка, которую грамматика C не понимает. Все замены — пробелы
/// или `&`→`*` той же длины: смещения и номера строк остаются координатами
/// исходника. Строковые и символьные литералы не трогаем.
pub(crate) fn normalize_line(line: &str) -> String {
    if line.trim_start().starts_with('#') {
        return line.to_string();
    }
    let line = strip_using(line);
    let line = rewrite_constexpr(&line);
    let line = rewrite_empty_return(&line);
    let line = replace_std_types(&line);
    let line = rewrite_casts(&line);
    let line = replace_scope(&line);
    let line = rewrite_auto(&line);
    let line = rewrite_string_type(&line);
    let line = rewrite_brace_init(&line);
    rewrite_refs(&line)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn word_at(buf: &[u8], i: usize, word: &[u8]) -> bool {
    buf.get(i..i + word.len()) == Some(word)
        && (i == 0 || !is_ident_byte(buf[i - 1]))
        && buf.get(i + word.len()).is_none_or(|b| !is_ident_byte(*b))
}

fn skip_spaces(buf: &[u8], mut i: usize) -> usize {
    while buf.get(i).is_some_and(|b| *b == b' ' || *b == b'\t') {
        i += 1;
    }
    i
}

fn ident_end(buf: &[u8], i: usize) -> Option<usize> {
    let first = *buf.get(i)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut j = i + 1;
    while buf.get(j).is_some_and(|b| is_ident_byte(*b)) {
        j += 1;
    }
    Some(j)
}

fn skip_literal(buf: &[u8], i: usize) -> usize {
    let quote = buf[i];
    let mut j = i + 1;
    while j < buf.len() {
        if buf[j] == b'\\' {
            j += 2;
            continue;
        }
        if buf[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    j
}

fn mask(buf: &mut [u8], from: usize, to: usize) {
    for b in &mut buf[from..to] {
        *b = b' ';
    }
}

fn from_bytes(buf: Vec<u8>, fallback: &str) -> String {
    String::from_utf8(buf).unwrap_or_else(|_| fallback.to_string())
}

/// `using namespace std;`, `using std::cout;` — директива компилятора,
/// на схеме ей нечего изображать. Маска сохраняет длину строки.
fn strip_using(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, b"using") {
            if let Some(end) = using_end(&buf, i) {
                mask(&mut buf, i, end);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    from_bytes(buf, line)
}

fn using_end(buf: &[u8], start: usize) -> Option<usize> {
    let mut j = skip_spaces(buf, start + "using".len());
    if word_at(buf, j, b"namespace") {
        j = skip_spaces(buf, j + "namespace".len());
    }
    if buf.get(j..j + 2) == Some(b"::") {
        j = skip_spaces(buf, j + 2);
    }
    j = ident_end(buf, j)?;
    loop {
        let k = skip_spaces(buf, j);
        if buf.get(k..k + 2) == Some(b"::") {
            let m = skip_spaces(buf, k + 2);
            j = ident_end(buf, m)?;
        } else {
            j = k;
            break;
        }
    }
    (buf.get(j) == Some(&b';')).then_some(j + 1)
}

/// `constexpr` — квалификатор C++, для C это тот же `const`. Длина
/// сохраняется: `const` + четыре пробела.
fn rewrite_constexpr(line: &str) -> String {
    replace_word(line, b"constexpr", b"const    ")
}

/// `return {};` — value-initialization C++. Пустые скобки заменяются
/// нулём: `return 0 ;`. Содержимое `{...}` не трогаем.
fn rewrite_empty_return(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, b"return") {
            let open = skip_spaces(&buf, i + "return".len());
            if buf.get(open) == Some(&b'{') {
                let close = skip_spaces(&buf, open + 1);
                if buf.get(close) == Some(&b'}') {
                    buf[open] = b'0';
                    mask(&mut buf, open + 1, close + 1);
                    i = close + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    from_bytes(buf, line)
}

/// Замена слова вне литералов; длина replacement должна совпадать с word.
fn replace_word(line: &str, word: &[u8], replacement: &[u8]) -> String {
    debug_assert_eq!(word.len(), replacement.len());
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, word) {
            buf[i..i + word.len()].copy_from_slice(replacement);
            i += word.len();
            continue;
        }
        i += 1;
    }
    from_bytes(buf, line)
}

/// В подписи `std::` не нужен: `cout`, `sqrt`, `string` читаются и без
/// пространства имён. Маска той же длины, чтобы span'ы совпадали с разбором.
pub(crate) fn display_line(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, b"std") && buf.get(i + 3..i + 5) == Some(b"::") {
            let from = if i >= 2 && buf.get(i - 2..i) == Some(b"::") {
                i - 2
            } else {
                i
            };
            mask(&mut buf, from, i + 5);
            i += 5;
            continue;
        }
        i += 1;
    }
    from_bytes(buf, line)
}

/// `std::string`, `std::size_t` и потоки — типы C++ без C-аналога.
/// Заменяются на `int` с добивкой пробелами: длина та же, подпись
/// блока остаётся исходной (`std::string s`), а lang-c видит `int s`.
fn replace_std_types(line: &str) -> String {
    const TYPES: [&[u8]; 9] = [
        b"std::stringstream",
        b"std::string_view",
        b"std::ifstream",
        b"std::ofstream",
        b"std::wstring",
        b"std::istream",
        b"std::ostream",
        b"std::string",
        b"std::size_t",
    ];
    let mut line = line.to_string();
    for ty in TYPES {
        line = replace_exact(&line, ty, b"int");
    }
    line
}

/// Замена точной последовательности вне литералов с добивкой пробелами
/// до её длины: `std::string` -> `int        `.
fn replace_exact(line: &str, needle: &[u8], replacement: &[u8]) -> String {
    debug_assert!(needle.len() >= replacement.len());
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, needle) {
            buf[i..i + replacement.len()].copy_from_slice(replacement);
            mask(&mut buf, i + replacement.len(), i + needle.len());
            i += needle.len();
            continue;
        }
        i += 1;
    }
    from_bytes(buf, line)
}

/// `static_cast<T>(x)` — C++-каст. Для разбора `(T)(x)`:
/// `static_cast` маскируется в `(`, `<` и `>` — в пробел и `)`.
/// Спан выражения начинается на `(`, поэтому подпись остаётся исходной.
fn rewrite_casts(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        let Some(kind) = cast_word_at(&buf, i) else {
            i += 1;
            continue;
        };
        let open = skip_spaces(&buf, i + kind);
        if buf.get(open) != Some(&b'<') {
            i += kind;
            continue;
        }
        let Some(close) = matching_angle(&buf, open) else {
            i += kind;
            continue;
        };
        buf[i] = b'(';
        mask(&mut buf, i + 1, i + kind);
        buf[open] = b' ';
        buf[close] = b')';
        i = close + 1;
    }
    from_bytes(buf, line)
}

fn cast_word_at(buf: &[u8], i: usize) -> Option<usize> {
    for word in [
        b"static_cast".as_slice(),
        b"reinterpret_cast",
        b"const_cast",
        b"dynamic_cast",
    ] {
        if word_at(buf, i, word) {
            return Some(word.len());
        }
    }
    None
}

fn matching_angle(buf: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(buf, i);
            continue;
        }
        match buf[i] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `cpplabs::readNumber` — `::` в C нет. Заменяем на `__` той же длины:
/// имя становится обычным идентификатором, а подпись остаётся с `::`
/// (span начинается на первом имени). `std::` в подписи уже замаскирован.
fn replace_scope(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if buf.get(i..i + 2) == Some(b"::") {
            buf[i] = b'_';
            buf[i + 1] = b'_';
            i += 2;
            continue;
        }
        i += 1;
    }
    from_bytes(buf, line)
}

/// `auto x = ...` — тип выводится компилятором, для разбора годится `int`.
/// `auto` перед `[` (structured binding) и `auto int` (storage class C)
/// не трогаем.
fn rewrite_auto(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, b"auto") {
            let next = skip_spaces(&buf, i + "auto".len());
            let starts_type = buf.get(next).is_some_and(|b| {
                b.is_ascii_alphabetic() || *b == b'_' || matches!(*b, b'*' | b'&')
            });
            if starts_type && !starts_c_type(&buf, next) {
                buf[i..i + "auto".len()].copy_from_slice(b"int ");
                i += "auto".len();
                continue;
            }
        }
        i += 1;
    }
    from_bytes(buf, line)
}

fn starts_c_type(buf: &[u8], i: usize) -> bool {
    const WORDS: [&[u8]; 14] = [
        b"void",
        b"char",
        b"short",
        b"int",
        b"long",
        b"float",
        b"double",
        b"signed",
        b"unsigned",
        b"_Bool",
        b"bool",
        b"const",
        b"volatile",
        b"struct",
    ];
    WORDS.iter().any(|w| word_at(buf, i, w))
}

/// `string s` (после `using namespace std;`) — тип C++, для разбора `int`.
/// Подпись остаётся `string s`: span начинается на том же месте.
fn rewrite_string_type(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if word_at(&buf, i, b"string") {
            let next = skip_spaces(&buf, i + "string".len());
            let is_type = buf.get(next).is_some_and(|b| {
                b.is_ascii_alphabetic() || *b == b'_' || matches!(*b, b'*' | b'&')
            });
            if is_type {
                buf[i..i + "string".len()].copy_from_slice(b"int   ");
                i += "string".len();
                continue;
            }
        }
        i += 1;
    }
    from_bytes(buf, line)
}

/// `double x{};` и `double y {expr};` — инициализация C++.
/// Пустые скобки без `=` убираются (объявление без значения), с `=` —
/// дают `0`; `{expr}` становится `= expr` (или `(expr)` после `=`).
fn rewrite_brace_init(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if buf[i] != b'{' {
            i += 1;
            continue;
        }
        let Some(close) = matching_brace(&buf, i) else {
            i += 1;
            continue;
        };
        let after = skip_spaces(&buf, close + 1);
        let terminated = after >= buf.len() || matches!(buf[after], b';' | b',' | b')');
        if !terminated || !brace_is_initializer(&buf, i) {
            i = close + 1;
            continue;
        }
        let content = &buf[i + 1..close];
        let empty = content.iter().all(|b| matches!(*b, b' ' | b'\t'));
        // `{1, 2}` — список, `{ stmt; }` — блок: и то и другое не `{expr}`
        if has_top_level_comma(content) || has_semicolon(content) {
            i = close + 1;
            continue;
        }
        let after_eq = prev_token(&buf, i).is_some_and(|(s, e)| &buf[s..e] == b"=");
        if empty {
            if after_eq {
                buf[i] = b'0';
                mask(&mut buf, i + 1, close + 1);
            } else {
                mask(&mut buf, i, close + 1);
            }
        } else if after_eq {
            buf[i] = b'(';
            buf[close] = b')';
        } else {
            buf[i] = b'=';
            // `;` вместо `}`: AST-спан инициализатора кончается перед
            // терминатором, поэтому в подпись не попадает лишняя скобка.
            // Для `,`/`)` терминатор оставляем пробелом — там `;` нельзя.
            buf[close] = if buf[after] == b';' { b';' } else { b' ' };
        }
        i = close + 1;
    }
    from_bytes(buf, line)
}

fn matching_brace(buf: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(buf, i);
            continue;
        }
        match buf[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn brace_is_initializer(buf: &[u8], open: usize) -> bool {
    let Some((start, end)) = prev_token(buf, open) else {
        return false;
    };
    let tok = &buf[start..end];
    if tok == b"=" {
        return true;
    }
    if tok.len() == 1 && !is_ident_byte(tok[0]) {
        return false;
    }
    if matches!(
        tok,
        b"return"
            | b"else"
            | b"do"
            | b"try"
            | b"switch"
            | b"case"
            | b"struct"
            | b"class"
            | b"union"
            | b"enum"
            | b"namespace"
    ) {
        return false;
    }
    !prev_token(buf, start).is_some_and(|(ps, pe)| {
        matches!(
            &buf[ps..pe],
            b"struct" | b"class" | b"union" | b"enum" | b"namespace"
        )
    })
}

fn has_top_level_comma(content: &[u8]) -> bool {
    let mut depth = 0usize;
    let mut i = 0;
    while i < content.len() {
        if matches!(content[i], b'"' | b'\'') {
            i = skip_literal(content, i);
            continue;
        }
        match content[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

fn has_semicolon(content: &[u8]) -> bool {
    let mut i = 0;
    while i < content.len() {
        if matches!(content[i], b'"' | b'\'') {
            i = skip_literal(content, i);
            continue;
        }
        if content[i] == b';' {
            return true;
        }
        i += 1;
    }
    false
}

/// Ссылка `int &a` — не C. В объявлении меняем `&` на `*` (та же длина):
/// для схемы это указатель, для разбора — законный декларатор. Выражения
/// (`a & b`, `&x` в вызове) не трогаем: там перед `&` не тип.
fn rewrite_refs(line: &str) -> String {
    let mut buf = line.as_bytes().to_vec();
    let mut i = 0;
    while i < buf.len() {
        if matches!(buf[i], b'"' | b'\'') {
            i = skip_literal(&buf, i);
            continue;
        }
        if buf[i] == b'&' {
            if buf.get(i + 1) == Some(&b'&') {
                i += 2;
                continue;
            }
            if is_ref_declarator(&buf, i) {
                buf[i] = b'*';
            }
        }
        i += 1;
    }
    from_bytes(buf, line)
}

fn is_ref_declarator(buf: &[u8], amp: usize) -> bool {
    let after = skip_spaces(buf, amp + 1);
    let Some(end) = ident_end(buf, after) else {
        return false;
    };
    let term = skip_spaces(buf, end);
    let declarator_end = term >= buf.len()
        || matches!(
            buf[term],
            b'=' | b',' | b';' | b')' | b']' | b'[' | b'{' | b':'
        );
    declarator_end && type_before(buf, amp)
}

fn prev_token(buf: &[u8], pos: usize) -> Option<(usize, usize)> {
    let mut j = pos;
    while j > 0 && matches!(buf[j - 1], b' ' | b'\t') {
        j -= 1;
    }
    if j == 0 {
        return None;
    }
    let end = j;
    if is_ident_byte(buf[j - 1]) {
        while j > 0 && is_ident_byte(buf[j - 1]) {
            j -= 1;
        }
    } else {
        j -= 1;
    }
    Some((j, end))
}

fn type_before(buf: &[u8], pos: usize) -> bool {
    let mut pos = pos;
    loop {
        let Some((start, end)) = prev_token(buf, pos) else {
            return false;
        };
        let tok = &buf[start..end];
        if matches!(tok, b"*" | b"const" | b"volatile") {
            pos = start;
            continue;
        }
        if matches!(
            tok,
            b"void"
                | b"char"
                | b"short"
                | b"int"
                | b"long"
                | b"float"
                | b"double"
                | b"signed"
                | b"unsigned"
                | b"_Bool"
                | b"bool"
                | b"string"
        ) {
            return true;
        }
        return prev_token(buf, start)
            .is_some_and(|(ps, pe)| matches!(&buf[ps..pe], b"struct" | b"union" | b"enum"));
    }
}
