#![allow(clippy::all, clippy::pedantic, clippy::nursery)]
use std::env;
use std::path::{Path, PathBuf};
use std::process;

use gostpadi::error::ParseError;
use gostpadi::layout::{layout, normalize};
use gostpadi::pipeline::{self, Options};
use gostpadi::style::Style;

const VERSION: &str = "2.0.0";

const TEMPLATE: &str = "#gostpadi 1\n\
# One line = one block; top to bottom. Five words: input, output, if, yes/no.\n\
input scanf(\"%d\", &a)\n\
c = a * 2\n\
if c > 10\n\
    yes: printf(\"many\"); break\n\
    no: c = 0\n\
output printf(\"c = %d\", c)\n";

const HELP: &str = "gostpadi 2.0.0 — блок-схемы по ГОСТ 19.701 из кода C или .gvn

ИСПОЛЬЗОВАНИЕ:
    gostpadi схема.gvn [ещё.gvn|код.c ...] [-o out.svg|папка/] [флаги]

ФЛАГИ:
    -o <путь>       выход: файл.svg (один вход) или папка/ (пачка)
    --labels=ru|en  язык надписей (по умолчанию en)
    --check         только проверить, не рисовать
    --template      заготовка .gvn на stdout
    -h, --help      эта справка
    -V, --version   версия
";

const USAGE: &str = "использование: gostpadi схема.gvn [ещё.gvn|код.c ...] [-o out.svg|папка/] [--labels=ru|en] [--check] [--template] [-h] [-V]";

/// Базовый путь результата входа: ".../stem.svg" (суффиксы листов добавит
/// page_path). Папкой считается -o с косой чертой, существующая папка
/// или пачка входов; у одинаковых stem имя родительской папки — префикс.
fn base_for(inp: &str, output: Option<&str>, folder: bool, dup: bool) -> PathBuf {
    let mut stem = Path::new(inp)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if folder && dup {
        if let Some(parent) = Path::new(inp).parent().and_then(|p| p.file_name()) {
            stem = format!("{}-{}", parent.to_string_lossy(), stem);
        }
    }
    match output {
        Some(o) if folder => PathBuf::from(o).join(format!("{stem}.svg")),
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

/// «файл:строка: сообщение» — как render_file в gostpadi.py; exit 1.
fn die_parse(path: &str, e: &ParseError) -> ! {
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
    process::exit(1)
}

fn main() {
    let argv: Vec<String> = env::args().collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut labels = "en".to_string();
    let mut check = false;
    let mut template = false;

    let mut i = 1;
    while i < argv.len() {
        let a = argv[i].clone();
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                process::exit(0);
            }
            "-V" | "--version" => {
                println!("gostpadi {VERSION}");
                process::exit(0);
            }
            "--template" => template = true,
            "--check" => check = true,
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
            _ if a.starts_with('-') && a != "-" => {
                eprintln!("{USAGE}");
                process::exit(2);
            }
            _ => inputs.push(a),
        }
        i += 1;
    }

    if template {
        print!("{TEMPLATE}");
        process::exit(0);
    }
    if inputs.is_empty() {
        eprintln!("{USAGE}");
        process::exit(2);
    }

    // читаем входы; расширение определяет тип (.c -> C, остальное .gvn)
    let mut sources: Vec<(String, String, bool)> = Vec::with_capacity(inputs.len());
    for inp in &inputs {
        match std::fs::read_to_string(inp) {
            Ok(text) => {
                let is_c = Path::new(inp)
                    .extension()
                    .map(|e| e == "c")
                    .unwrap_or(false);
                sources.push((inp.clone(), text, is_c));
            }
            Err(e) => {
                eprintln!("не удалось открыть {inp}: {e}");
                process::exit(1);
            }
        }
    }

    let opts = Options { labels };
    let st = Style::default();

    if check {
        let schemes = match pipeline::parse_batch(&sources, &opts) {
            Ok(s) => s,
            Err((path, e)) => die_parse(&path, &e),
        };
        let multi = schemes.len() > 1;
        for (inp, (_, nodes)) in inputs.iter().zip(&schemes) {
            layout(nodes, &normalize(nodes, &st), &st);
            if multi {
                println!("{inp}: ok: {} blocks", nodes.len());
            } else {
                println!("ok: {} blocks", nodes.len());
            }
        }
        return;
    }

    let schemes = match pipeline::parse_batch(&sources, &opts) {
        Ok(s) => s,
        Err((path, e)) => die_parse(&path, &e),
    };

    let folder = match &output {
        Some(o) => o.ends_with('/') || Path::new(o).is_dir() || inputs.len() > 1,
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

    let mut failed = false;
    for ((_, pages), inp) in pipeline::render_batch(schemes).into_iter().zip(&inputs) {
        let base = base_for(inp, output.as_deref(), folder, dup);
        for (k, svg) in pages.iter().enumerate() {
            let target = page_path(&base, k);
            if let Err(e) = std::fs::write(&target, svg) {
                eprintln!("не удалось записать {}: {e}", target.display());
                failed = true;
                continue;
            }
            println!("{}", target.display());
        }
    }
    if failed {
        process::exit(1);
    }
}
