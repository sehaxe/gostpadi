use std::env;
use std::path::{Path, PathBuf};
use std::process;

use gostpadi::ir::Node;
use gostpadi::layout::{crossings_ok, layout, normalize, overlaps_ok, single_entry_ok};
use gostpadi::pipeline::{self, Options};

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

const HELP: &str = "gostpadi 2.0.0 — блок-схемы по ГОСТ 19.701 из C-кода

ИСПОЛЬЗОВАНИЕ:
    gostpadi код.c [ещё.c ...] [-o out.svg|папка/] [флаги]

ФЛАГИ:
    -o <путь>       выход: файл.svg (один вход) или папка/ (пачка)
    --labels=ru|en  язык надписей (по умолчанию en)
    --font=N        кегль текста в pt (по умолчанию 14), растит всю геометрию
    --lw=N          толщина линий и усиков стрелок (по умолчанию 1.0)
    --no-split      не резать длинную схему на листы: один лист, в А4
                    вписывает общий масштаб пачки (для вставки в отчёт)
    --landscape     альбомный лист А4 297x210 вместо книжного 210x297
    --check         только проверить, не рисовать
    --trim          без листа A4: схема ровно по содержимому, размер
                    в миллиметрах — для вставки в отчёт
    --drawio        файл draw.io (.drawio) с редактируемыми блоками
    -h, --help      эта справка
    -V, --version   версия
";

const USAGE: &str = "использование: gostpadi код.c [ещё.c ...] [-o out.svg|папка/] [--labels=ru|en] [--font=N] [--lw=N] [--no-split] [--landscape] [--check] [--trim] [--drawio] [-h] [-V]";

/// Базовый путь результата входа: ".../stem.svg" (суффиксы листов добавит
/// page_path). Папкой считается -o с косой чертой или существующая папка;
/// пачка с голым именем кладёт результат рядом с каждым входом; у
/// одинаковых stem имя родительской папки — префикс.
fn base_for(inp: &str, output: Option<&str>, folder: bool, batch: bool, dup: bool) -> PathBuf {
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
        Some(o) if batch => Path::new(inp).parent().unwrap_or(Path::new("")).join(o),
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
    let mut trim = false;
    let mut drawio = false;
    let mut landscape = false;
    let mut check = false;

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
            "--check" => check = true,
            "--no-split" => no_split = true,
            "--trim" => trim = true,
            "--drawio" => drawio = true,
            "--landscape" => landscape = true,
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

    if inputs.is_empty() {
        eprintln!("{USAGE}");
        process::exit(2);
    }

    // читаем входы: формат только C, тип по расширению не определяется
    let mut sources: Vec<(String, String)> = Vec::with_capacity(inputs.len());
    for inp in &inputs {
        match std::fs::read_to_string(inp) {
            Ok(text) => sources.push((inp.clone(), text)),
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
        landscape,
    };
    let st = opts.style();

    if check {
        let schemes = pipeline::parse_batch(&sources, &opts);
        let multi = schemes.len() > 1;
        for (inp, (_, nodes)) in inputs.iter().zip(&schemes) {
            let l = layout(nodes, &normalize(nodes, &st), &st);
            let bad = |msg: String| -> ! {
                eprintln!("{inp}: {msg}");
                process::exit(1);
            };
            if let Err(e) = crossings_ok(&l.shapes, &l.edges) {
                bad(format!("линии заходят на блоки: {e}"));
            }
            if !overlaps_ok(&l.shapes) {
                bad("фигуры перекрываются".into());
            }
            if !single_entry_ok(&l) {
                bad("в «конец» входит не одна стрелка".into());
            }
            if multi {
                out(&format!("{inp}: ok: {} blocks\n", nodes.len()));
            } else {
                out(&format!("ok: {} blocks\n", nodes.len()));
            }
        }
        return;
    }

    // Разбора с ошибкой не бывает: битый C рисуется частично. Пустой
    // вход (нет ни одного блока) рисуем пустым листом и предупреждаем —
    // молча выдать схему из ничего хуже, чем сказать об этом.
    let schemes: Vec<(String, Vec<Node>)> = pipeline::parse_batch(&sources, &opts);
    let mut empty: Vec<&str> = Vec::new();
    for (path, nodes) in &schemes {
        if nodes
            .iter()
            .all(|n| n.branches.is_empty() && n.body.is_none())
            && nodes.len() <= 2
        {
            empty.push(path);
        }
    }
    for path in &empty {
        eprintln!("{path}: в коде не найден int main(...) — схема пустая");
    }
    // Если пуст ВСЁ — схем не получилось вовсе, и «готово» в коде выхода
    // враньё: наружу ушёл бы лист с двумя терминаторами и ничего
    // больше. Частично пустая пачка — норма, там предупреждения
    // достаточно и остальные файлы отрисованы.
    if !empty.is_empty() && empty.len() == schemes.len() {
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

    let mut failed = false;

    // --trim и --drawio отдают по одному файлу на вход, без листа A4:
    // лист удобен для печати, но в отчёт его не вставишь — схема
    // занимает часть страницы, а вокруг пустое поле. Размер задаётся
    // в миллиметрах, вписывания нет, поэтому у всех файлов пачки
    // физический размер совпадает.
    if trim || drawio {
        let produced = if drawio {
            pipeline::render_drawio_batch(schemes, &st)
        } else {
            pipeline::render_tight_batch(schemes, &st)
        };
        let ext = if drawio { "drawio" } else { "svg" };
        for ((path, data), inp) in produced.into_iter().zip(&inputs) {
            let _ = path;
            let base =
                base_for(inp, output.as_deref(), folder, inputs.len() > 1, dup).with_extension(ext);
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

    let (rendered, info) = pipeline::render_batch(schemes, &st);
    for ((_, pages), inp) in rendered.into_iter().zip(&inputs) {
        let base = base_for(inp, output.as_deref(), folder, inputs.len() > 1, dup);
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
    // Молча выдать 4.7 pt текста нельзя: на листе это нечитаемо, и
    // причина обычно одна — слишком широкий блок, который порезка
    // не может разрезать (резать можно только между узлами).
    if info.is_illegible() {
        eprintln!(
            "внимание: общий масштаб {:.3} — кегль на листе {:.1} pt вместо {} pt, \
             схема читается с трудом. Порезка идёт только между блоками верхнего \
             уровня, поэтому мешает один слишком широкий блок: разнесите его \
             (например поделите кейсы switch на два блока) либо уберите длинные \
             подписи кейсов.",
            info.scale, info.font_on_page, st.font
        );
    }
    if failed {
        process::exit(1);
    }
}
