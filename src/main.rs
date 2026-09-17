#![allow(clippy::all, clippy::pedantic, clippy::nursery)]
use std::env;
use std::process;

const VERSION: &str = "1.2.2";

const TEMPLATE: &str = "#gostpadi 1
# One line = one block; top to bottom. Five words: input, output, if, yes/no.
input scanf(\"%d\", &a)
c = a * 2
if c > 10
    yes: printf(\"many\"); break
    no: c = 0
output printf(\"c = %d\", c)
";

const HELP: &str = r#"gostpadi — рисовальщик аккуратных блок-схем из своего
текстового формата .gvn или прямо из кода на C.

Идея: вы описываете только текст блоков и порядок — раскладку библиотека
делает сама, строго по шаблону:

    - основной поток — вертикальная линия по центру;
    - «да» ветвления уходит налево, «нет»/очередной case — направо,
      средняя ветка switch — прямо вниз;
    - инструкции ветки через «;» превращаются в отдельные плитки
      колонки (printf(a) -> break);
    - все линии строго под 90° и одной толщины, стрелки входят в фигуры;
    - все фигуры одного типа в схеме имеют одинаковый размер;
    - если схема не влезает в лист А4, она сама режется на части:
      часть кончается кружком «А», следующая начинается кружком «А».

Формат .gvn (построчный, ключевые слова только английские; «Start»
и «End» добавляются сами; первой строкой можно написать «#gostpadi 1» —
это просто подпись формата):

    # comment (# and // both work)
    input scanf("%d", &a)             # parallelogram (input)
    b = a / 100000 + ...              # rectangle (action); keyword optional
    output printf("Result...")        # parallelogram (output)
    if a < 100000 || a > 999999       # diamond; branches indented 4 spaces
        yes -> end: printf("Err..."); return 1  # branch goes straight to End
        no:                           # empty branch = label on the line
    switch (status)                   # any number of cases
        1: printf("December...")      # label becomes «status = 1»
        default: printf("Impossible...")         # default branch

Строка без ключевого слова тоже действие; printf/scanf и т.п. сами
становятся вводом-выводом. Условие ромба автоматически оформляется
как «if (...)» (кроме уже начинающихся с if/switch).

Использование:
    gostpadi схема.gvn                     # -> схема.png
    gostpadi a.gvn b.gvn c.gvn             # пачка файлов за раз
    gostpadi схема.gvn -o отчёт/рис.png    # своё имя PNG
    gostpadi схема.gvn --show              # показать прямо в терминале
    gostpadi схема.gvn --auto --scale=2    # канвас по контенту, крупнее
    gostpadi --template > новая.gvn        # заготовка схемы
Опции: --auto, --scale=N, --font=N, --lw=N (толщина всех линий),
--dpi=N, --show (kitty/WezTerm/Ghostty/iTerm2), --template, -o, --version.

или из python:
    import gostpadi
    gostpadi.render(open("схема.gvn").read(), "результат.png")

Зависимости: matplotlib и pycparser (ставятся сами при запуске через uv).
"#;

fn main() {
    let argv: Vec<String> = env::args().collect();
    let mut args: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut page_auto = false;
    let mut _show = false;
    let mut template = false;
    let mut _gvn = false;
    let mut labels = "en".to_string();
    let mut zoom: Option<f64> = None;
    let mut font: Option<f64> = None;
    let mut lw: Option<f64> = None;
    let mut dpi: Option<u32> = None;

    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if a == "--auto" {
            page_auto = true;
        } else if a == "--show" {
            _show = true;
        } else if a == "--template" {
            template = true;
        } else if a == "--gvn" {
            _gvn = true;
        } else if a.starts_with("--labels=") {
            let v = a[9..].to_string();
            if v != "ru" && v != "en" {
                eprintln!("--labels={}: поддерживаются ru и en", v);
                process::exit(2);
            }
            labels = v;
        } else if a == "-o" || a == "--output" {
            i += 1;
            if i >= argv.len() {
                eprintln!("{}: нужно имя файла", a);
                process::exit(2);
            }
            output = Some(argv[i].clone());
        } else if a.starts_with("--output=") {
            output = Some(a[9..].to_string());
        } else if a.starts_with("--scale=") {
            let v = a[8..].parse::<f64>().unwrap_or(f64::NAN);
            if !v.is_finite() || v <= 0.0 {
                eprintln!("--scale={}: значение должно быть > 0", &a[8..]);
                process::exit(2);
            }
            zoom = Some(v);
        } else if a.starts_with("--font=") {
            let v = a[7..].parse::<f64>().unwrap_or(f64::NAN);
            if !v.is_finite() || v <= 0.0 {
                eprintln!("--font={}: значение должно быть > 0", &a[7..]);
                process::exit(2);
            }
            font = Some(v);
        } else if a.starts_with("--lw=") {
            let v = a[5..].parse::<f64>().unwrap_or(f64::NAN);
            if !v.is_finite() || v <= 0.0 {
                eprintln!("--lw={}: значение должно быть > 0", &a[5..]);
                process::exit(2);
            }
            lw = Some(v);
        } else if a.starts_with("--dpi=") {
            let v = a[6..].parse::<i64>().unwrap_or(-1);
            if v <= 0 {
                eprintln!("--dpi={}: значение должно быть > 0", &a[6..]);
                process::exit(2);
            }
            dpi = Some(v as u32);
        } else if a == "-h" || a == "--help" {
            print!("{}", HELP);
            process::exit(0);
        } else if a == "-v" || a == "--version" {
            println!("gostpadi {}", VERSION);
            process::exit(0);
        } else if a.starts_with('-') && a != "-" {
            eprintln!("неизвестная опция: {}", a);
            process::exit(2);
        } else {
            args.push(a.clone());
        }
        i += 1;
        let _ = (page_auto, zoom, font, lw, dpi);
    }

    if labels != "ru" && labels != "en" {
        eprintln!("--labels={}: поддерживаются ru и en", labels);
        process::exit(2);
    }
    if let Some(v) = zoom {
        if v <= 0.0 {
            eprintln!("--scale={}: значение должно быть > 0", v);
            process::exit(2);
        }
    }
    if let Some(v) = font {
        if v <= 0.0 {
            eprintln!("--font={}: значение должно быть > 0", v);
            process::exit(2);
        }
    }
    if let Some(v) = lw {
        if v <= 0.0 {
            eprintln!("--lw={}: значение должно быть > 0", v);
            process::exit(2);
        }
    }
    if let Some(v) = dpi {
        if v == 0 {
            eprintln!("--dpi={}: значение должно быть > 0", v);
            process::exit(2);
        }
    }

    if template {
        print!("{}", TEMPLATE);
        process::exit(0);
    }

    if args.is_empty() {
        eprintln!("использование: gostpadi схема.gvn | код.c [результат.png] [ещё.gvn ...] [-o результат.png] [--auto] [--scale=3] [--font=12] [--lw=1.1] [--dpi=200] [--show] [--gvn] [--template]");
        eprintln!("несколько файлов: фигуры и масштаб общие, -o — имя результата рядом с каждым входом или папка");
        process::exit(2);
    }

    if args.len() == 2 && output.is_none() {
        let second = args[1].to_lowercase();
        if second.ends_with(".png")
            || second.ends_with(".jpg")
            || second.ends_with(".jpeg")
            || second.ends_with(".svg")
        {
            output = Some(args.pop().unwrap());
        }
    }

    // stub: parse first file and report nodes
    let first = &args[0];
    let src = match std::fs::read_to_string(first) {
        Ok(s) => s,
        Err(e) => {
            // if file doesn't exist, try treating as inline gvn? For bootstrap tests where file may not exist, just attempt parse of arg as text?
            // Check if arg is like inline? Prefer error code 1
            eprintln!("не удалось открыть: {}", e);
            process::exit(1);
        }
    };
    let _ = output;
    let style = gostpadi::style::Style::default();
    // if .c file, try c_to_gvn stub -> would error "not implemented", but for .gvn we parse directly
    let text = if first.ends_with(".c") {
        match gostpadi::frontend::c::c_to_gvn(&src, &labels) {
            Ok(gvn) => gvn,
            Err(e) => {
                eprintln!("ошибка: {}", e);
                process::exit(1);
            }
        }
    } else {
        src
    };
    match gostpadi::frontend::gvn::parse(&text, &style, &labels) {
        Ok(nodes) => {
            println!("parsed: {} nodes", nodes.len());
            process::exit(0);
        }
        Err(e) => {
            let where_prefix = if let Some(l) = e.line {
                format!("{}:{}: ", first, l)
            } else {
                format!("{}: ", first)
            };
            eprintln!("{}{}", where_prefix, e);
            process::exit(1);
        }
    }
}
