use std::env;
use std::path::{Path, PathBuf};
use std::process;

use gostpadi::error::ParseError;
use gostpadi::frontend::{input_kind, InputKind};
use gostpadi::pipeline::{self, Options};

mod serve;

use std::io::Write;

/// stdout с тихим выходом при оборванной трубе («gostpadi ... | head»):
/// std игнорирует SIGPIPE, запись возвращает BrokenPipe — выходим 0.
fn out(s: &str) {
    let stdout = std::io::stdout();
    let mut w = stdout.lock();
    if let Err(e) = w.write_all(s.as_bytes()) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            process::exit(0);
        }
        eprintln!("ошибка вывода: {e}");
        process::exit(1);
    }
}

/// Версия — единственный источник истины: Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Внимание: `\`-перенос внутри строкового литерала съедает отступы
/// следующей строки — ветки шаблона обязаны остаться с отступом в 4
/// пробела, иначе `--template | gostpadi - --check` не сойдётся.
const TEMPLATE: &str = "\
#gostpadi 1
# One line = one block; top to bottom. Five words: input, output, if, yes/no.
input scanf(\"%d\", &a)
c = a * 2
if c > 10
    yes: printf(\"many\"); break
    no: c = 0
output printf(\"c = %d\", c)
";

const HELP: &str = "gostpadi 2.0.0 — блок-схемы по ГОСТ 19.701 из кода C или .gvn

ИСПОЛЬЗОВАНИЕ:
    gostpadi схема.gvn [ещё.gvn|код.c ...] [-o out.svg|папка/] [флаги]
    gostpadi serve [--port=8787]   локальный HTTP-дверь для агентов

ФЛАГИ:
    -o <путь>       выход: файл.svg (один вход) или папка/ (пачка)
    --labels=ru|en  язык надписей (по умолчанию en)
    --font=N        кегль текста в pt (по умолчанию 12), растит всю геометрию
    --lw=N          толщина линий и усиков стрелок (по умолчанию 1.0)
    --no-split      не резать длинную схему на листы: один лист, в А4
                    вписывает общий масштаб пачки (для вставки в отчёт)
    --landscape     листы А4 альбомные: широкие схемы читаются крупнее
    --no-fix        строгий разбор: не выправлять табы, CRLF, BOM, пробелы
                    в директивах (по умолчанию вход мягко нормализуется)
    --trim          в отчёт: SVG ровно по содержимому, размер в мм,
                    без листа А4 и без ужатия
    --drawio        файл draw.io (.drawio) с редактируемыми блоками
    --check         только проверить, не рисовать
    --template      заготовка .gvn на stdout
    -h, --help      эта справка
    -V, --version   версия

    gostpadi serve [--port=N]
        локальный сервер (только 127.0.0.1) с JSON-дверью движка:
        POST /render — код -> страницы SVG, POST /lint — проверка,
        POST /export?mode=1|2|3 — SVG в мм / draw.io на схему / книга на пачку; ?lang=c|gvn.
";

const USAGE: &str = "использование: gostpadi схема.gvn [ещё.gvn|код.c ...] [-o out.svg|папка/] [--labels=ru|en] [--font=N] [--lw=N] [--no-split] [--landscape] [--no-fix] [--trim] [--drawio] [--check] [--template] [-h] [-V]";

/// Базовый путь результата схемы: ".../stem.svg" (суффиксы листов добавит
/// page_path). Имя схемы несёт и вход, и функцию («1/main.c#main») —
/// папкой считается -o с косой чертой или существующая папка; пачка с
/// голым именем кладёт результат рядом с каждым входом; у одинаковых
/// stem имя родительской папки — префикс.
fn base_for(
    inp: &str,
    fun: Option<&str>,
    output: Option<&str>,
    folder: bool,
    batch: bool,
    dup: bool,
    same_dir: bool,
) -> PathBuf {
    let mut stem = Path::new(inp)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    // функция в имени, чтобы main.c#add и main.c#mul не перезаписали
    // друг друга: имя Linux-безопасное
    if let Some(f) = fun.filter(|f| !f.is_empty()) {
        let safe: String = f
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        stem = format!("{stem}-{safe}");
    }
    if folder && dup {
        if let Some(parent) = Path::new(inp).parent().and_then(|p| p.file_name()) {
            stem = format!("{}-{}", parent.to_string_lossy(), stem);
        }
    }
    match output {
        Some(o) if folder => PathBuf::from(o).join(format!("{stem}.svg")),
        // Пачка в один файл-базу: у входов из одной папки одинаковое
        // имя out.svg затирало предыдущий — уникальный суффикс из
        // вход/функции. У входов из разных папок имя остаётся прежним.
        Some(o) if batch => {
            let dir = Path::new(inp).parent().unwrap_or(Path::new(""));
            if same_dir || fun.is_some() {
                let p = Path::new(o);
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "scheme".into());
                let ext = p
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                dir.join(format!("{name}-{stem}{ext}"))
            } else {
                dir.join(o)
            }
        }
        // -o файл: у входа с функциями имя схемы — имя файла с суффиксом
        // функции, иначе все схемы писались в один -o и выживала последняя
        Some(o) if fun.is_some() => {
            let p = Path::new(o);
            let name = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "scheme".into());
            p.with_file_name(format!(
                "{name}-{}.svg",
                stem.rsplit_once('-').map_or("", |(_, f)| f)
            ))
        }
        Some(o) => PathBuf::from(o),
        None => Path::new(inp).with_file_name(format!("{stem}.svg")),
    }
}

/// Лист k: база, base-2.svg, base-3.svg...
fn page_path(base: &Path, k: usize) -> PathBuf {
    if k == 0 {
        return base.to_path_buf();
    }
    let s = base.to_string_lossy();
    let stem = s.strip_suffix(".svg").unwrap_or(&s);
    PathBuf::from(format!("{stem}-{}.svg", k + 1))
}

/// «файл:строка: сообщение» — как render_file в gostpadi.py.
fn report_parse(path: &str, e: &ParseError) {
    let loc = match (e.line, e.col) {
        (Some(l), Some(c)) => format!("{path}:{l}:{c}: "),
        (Some(l), None) => format!("{path}:{l}: "),
        (None, _) => format!("{path}: "),
    };
    let src = e
        .src
        .as_deref()
        .map(|s| format!(" ({})", s.trim()))
        .unwrap_or_default();
    eprintln!("{loc}{}{src}", e.msg);
}

fn die_parse(path: &str, e: &ParseError) -> ! {
    report_parse(path, e);
    process::exit(1)
}

/// --font=N / --lw=N: число > 0, иначе usage-ошибка (exit 2).
fn num_flag(arg: &str, name: &str) -> f64 {
    let v = arg[name.len() + 1..].trim();
    match v.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => n,
        _ => {
            eprintln!("{arg}: {name} требует положительное число");
            process::exit(2);
        }
    }
}

fn main() {
    // args_os: не-UTF8 аргумент не должен паниковать
    let argv: Vec<String> = env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut labels = "en".to_string();
    let mut font: Option<f64> = None;
    let mut lw: Option<f64> = None;
    let mut no_split = false;
    let mut landscape = false;
    let mut trim = false;
    let mut drawio = false;
    let mut check = false;
    let mut template = false;
    let mut serve_mode = false;
    let mut port = 8787u16;

    let mut i = 1;
    while i < argv.len() {
        let a = argv[i].clone();
        match a.as_str() {
            "-h" | "--help" => {
                out(HELP);
                process::exit(0);
            }
            "-V" | "--version" => {
                out(&format!("gostpadi {VERSION}\n"));
                process::exit(0);
            }
            "--template" => template = true,
            "--check" => check = true,
            "--no-split" => no_split = true,
            "--landscape" => landscape = true,
            "--no-fix" => {
                if cfg!(feature = "autofix") {
                    gostpadi::frontend::set_autofix(false);
                } else {
                    eprintln!("--no-fix: мягкий фикс не вкомпилирован, разбор и так строгий");
                }
            }
            "--trim" => trim = true,
            "--drawio" => drawio = true,
            "serve" if inputs.is_empty() => serve_mode = true,
            _ if a.starts_with("--port=") => {
                port = a["--port=".len()..].parse().unwrap_or_else(|_| {
                    eprintln!("{a}: порт должен быть числом 1..65535");
                    process::exit(2);
                });
            }
            "-o" | "--output" => {
                i += 1;
                if i >= argv.len() {
                    eprintln!("{a}: нужно имя файла");
                    process::exit(2);
                }
                output = Some(argv[i].clone());
            }
            _ if a.starts_with("--output=") => output = Some(a["--output=".len()..].to_string()),
            _ if a.starts_with("--labels=") => {
                let v = a["--labels=".len()..].to_string();
                if v != "ru" && v != "en" {
                    eprintln!("--labels={v}: поддерживаются ru и en");
                    process::exit(2);
                }
                labels = v;
            }
            _ if a.starts_with("--font=") => {
                font = Some(num_flag(&a, "--font"));
            }
            _ if a.starts_with("--lw=") => {
                lw = Some(num_flag(&a, "--lw"));
            }
            _ if a.starts_with('-') && a != "-" => {
                eprintln!("{USAGE}");
                process::exit(2);
            }
            _ => inputs.push(a),
        }
        i += 1;
    }

    if template {
        out(TEMPLATE);
        process::exit(0);
    }
    if serve_mode {
        if port == 0 {
            eprintln!("serve: порт 0 запрещён");
            process::exit(2);
        }
        serve::serve(port);
        return;
    }
    if inputs.is_empty() {
        eprintln!("{USAGE}");
        process::exit(2);
    }

    // читаем входы; расширение определяет тип (C/C++ -> разбор кода, остальное .gvn)
    let mut sources: Vec<(String, String, InputKind)> = Vec::with_capacity(inputs.len());
    for inp in &inputs {
        match std::fs::read_to_string(inp) {
            Ok(text) => {
                let kind = input_kind(inp);
                sources.push((inp.clone(), text, kind));
            }
            Err(e) => {
                eprintln!("не удалось открыть {inp}: {e}");
                process::exit(1);
            }
        }
    }

    let opts = Options {
        labels,
        font,
        lw,
        no_split,
    };
    let mut st = opts.style();
    if landscape {
        // А4 альбом: те же поля, ширина и высота местами
        std::mem::swap(&mut st.a4_w, &mut st.a4_h);
    }

    if check {
        let schemes = match pipeline::parse_batch(&sources, &opts) {
            Ok(s) => s,
            Err((path, e)) => die_parse(&path, &e),
        };
        let multi = schemes.len() > 1;
        for s in &schemes {
            let (inp, fun) = pipeline::scheme_parts(&s.path);
            let name = match fun {
                Some(f) => format!("{inp}#{f}"),
                None => inp.to_string(),
            };
            // один список проверок на CLI, serve /lint и corpus-тест:
            // раньше CLI дублировал его вручную и отставал от инвариантов
            match pipeline::validate(std::slice::from_ref(s), &st)
                .into_iter()
                .next()
            {
                Some((_, Ok(_))) => {}
                Some((_, Err(msg))) => {
                    eprintln!("{name}: {msg}");
                    process::exit(1);
                }
                None => {}
            }
            if multi {
                out(&format!("{name}: ok: {} blocks\n", s.nodes.len()));
            } else {
                out(&format!("ok: {} blocks\n", s.nodes.len()));
            }
        }
        return;
    }

    // пачка: сбойный вход не останавливает остальные (порт render_many)
    let mut schemes: Vec<pipeline::Scheme> = Vec::new();
    let mut parse_failed = false;
    for src in &sources {
        match pipeline::parse_batch(std::slice::from_ref(src), &opts) {
            Ok(mut s) => schemes.append(&mut s),
            Err((path, e)) => {
                report_parse(&path, &e);
                parse_failed = true;
            }
        }
    }
    if schemes.is_empty() {
        process::exit(1);
    }

    let folder = match &output {
        Some(o) => o.ends_with('/') || Path::new(o).is_dir(),
        None => false,
    };
    if folder {
        if let Some(o) = &output {
            if let Err(e) = std::fs::create_dir_all(o) {
                eprintln!("не удалось создать папку {o}: {e}");
                process::exit(1);
            }
        }
    }
    // одинаковые stem не перезаписывают друг друга: 1/main.c -> 1-main.svg
    let stems: Vec<String> = inputs
        .iter()
        .map(|i| {
            Path::new(i)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        })
        .collect();
    let dup = stems
        .iter()
        .any(|s| stems.iter().filter(|t| *t == s).count() > 1);
    // Один и тот же родитель у двух входов: голое `-o out.svg` иначе
    // положило бы обе схемы в один файл и оставило последнюю.
    let parents: Vec<PathBuf> = inputs
        .iter()
        .map(|i| Path::new(i).parent().unwrap_or(Path::new("")).to_path_buf())
        .collect();
    let same_dir = parents
        .iter()
        .any(|p| parents.iter().filter(|q| *q == p).count() > 1);

    let mut failed = parse_failed;
    // --trim и --drawio отдают по одному файлу на схему, без листа А4:
    // лист удобен для печати, но в отчёт его не вставишь — схема
    // занимает часть страницы, а вокруг пустое поле. tight — размер в
    // миллиметрах без ужатия, drawio — редактируемые блоки.
    if trim || drawio {
        let produced = if drawio {
            pipeline::render_drawio_batch(schemes, &st)
        } else {
            pipeline::render_tight_batch(schemes, &st)
        };
        let ext = if drawio { "drawio" } else { "svg" };
        for (scheme, data) in produced {
            let (inp, fun) = pipeline::scheme_parts(&scheme);
            let base = base_for(
                inp,
                fun,
                output.as_deref(),
                folder,
                inputs.len() > 1,
                dup,
                same_dir,
            )
            .with_extension(ext);
            if let Err(e) = std::fs::write(&base, data) {
                eprintln!("не удалось записать {}: {e}", base.display());
                failed = true;
                continue;
            }
            out(&format!("{}\n", base.display()));
        }
        if failed {
            process::exit(1);
        }
        return;
    }

    for (scheme, pages) in pipeline::render_batch(schemes, &st) {
        let (inp, fun) = pipeline::scheme_parts(&scheme);
        let base = base_for(
            inp,
            fun,
            output.as_deref(),
            folder,
            inputs.len() > 1,
            dup,
            same_dir,
        );
        for (k, svg) in pages.iter().enumerate() {
            let target = page_path(&base, k);
            if let Err(e) = std::fs::write(&target, svg) {
                eprintln!("не удалось записать {}: {e}", target.display());
                failed = true;
                continue;
            }
            out(&format!("{}\n", target.display()));
        }
    }
    if failed {
        process::exit(1);
    }
}
