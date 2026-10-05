//! CLI: выходы, коды возврата, сообщения об ошибках.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gostpadi"))
        .args(args)
        .output()
        .unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let d = env::temp_dir().join(format!("gostpadi-cli-{}-{name}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}

const OK_C: &str =
    "int main(void) {\n    int a;\n    scanf(\"%d\", &a);\n    a = a * 2;\n    printf(\"c\", a);\n    return 0;\n}\n";

#[test]
fn check_ok_exits_zero() {
    let d = tmp("check-ok");
    let f = d.join("ok.c");
    fs::write(&f, OK_C).unwrap();
    let out = run(&["--check", f.to_str().unwrap()]);
    assert!(out.status.success());
    // начало, конец и три оператора; объявление `int a;` без значения
    // не рисуется
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("ok: 5 blocks"),
        "{:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// ГЛАВНОЕ: битый C рисуется, а не отказывает. Строгий парсер на этом
/// коде завершался ошибкой разбора и схемы не было вовсе — теперь
/// `--check` проходит и говорит, сколько блоков получилось.
#[test]
fn broken_syntax_still_renders() {
    let d = tmp("check-broken");
    let f = d.join("broken.c");
    fs::write(
        &f,
        "int main(void) {\n    int c = 1;\n    if {\n        c = 2;\n    }\n}\n",
    )
    .unwrap();
    let out = run(&["--check", f.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("blocks"), "{stdout}");

    // и без --check рисуется SVG
    let svg = run(&[
        f.to_str().unwrap(),
        "-o",
        d.join("broken.svg").to_str().unwrap(),
    ]);
    assert!(
        svg.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&svg.stderr)
    );
    let text = std::fs::read_to_string(d.join("broken.svg")).unwrap();
    assert!(text.starts_with("<?xml"), "не SVG");
    assert!(!text.contains("NaN"), "NaN в разметке");
}

#[test]
fn no_args_exits_two() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn unknown_flag_exits_two() {
    let out = run(&["--wat"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn version_flag() {
    let out = run(&["-V"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "gostpadi 2.0.0"
    );
}

#[test]
fn help_flag_short() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("ИСПОЛЬЗОВАНИЕ"));
    assert!(help.contains("--labels=ru|en"));
}

#[test]
fn render_to_file_creates_svg() {
    let d = tmp("render-file");
    let src = d.join("in.c");
    fs::write(&src, OK_C).unwrap();
    let out_path = d.join("out.svg");
    let out = run(&[src.to_str().unwrap(), "-o", out_path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg = fs::read_to_string(&out_path).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn render_examples_main_c_to_tmp() {
    let d = tmp("main-c");
    let out_path = d.join("out.svg");
    let out = run(&["examples/main.c", "-o", out_path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg = fs::read_to_string(&out_path).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn batch_into_folder_with_dup_stems() {
    let d = tmp("batch");
    fs::create_dir_all(d.join("1")).unwrap();
    fs::create_dir_all(d.join("2")).unwrap();
    let a = d.join("1/main.c");
    let b = d.join("2/main.c");
    fs::write(&a, "int main(){printf(\"hi\");return 0;}").unwrap();
    fs::write(&b, "int main(){printf(\"bye\");return 0;}").unwrap();
    let dir = d.join("out");
    fs::create_dir_all(&dir).unwrap();
    let out = run(&[
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        "-o",
        dir.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("1-main.svg").exists());
    assert!(dir.join("2-main.svg").exists());
}

/// -o без косой черты и не существующая папка — basename-префикс:
/// результат рядом с КАЖДЫМ входом (как Python), не директория.
/// Голое имя относительно входов — гоняем с current_dir во временной папке.
#[test]
fn batch_bare_o_writes_next_to_each_input() {
    let d = tmp("bare-o");
    fs::create_dir_all(d.join("a")).unwrap();
    fs::create_dir_all(d.join("b")).unwrap();
    fs::write(d.join("a/x.c"), OK_C).unwrap();
    fs::write(d.join("b/y.c"), OK_C).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gostpadi"))
        .args(["a/x.c", "b/y.c", "-o", "out.svg"])
        .current_dir(&d)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !d.join("out.svg").exists(),
        "-o не должен был стать файлом в cwd"
    );
    assert!(d.join("a/out.svg").exists());
    assert!(d.join("b/out.svg").exists());
}

/// Пачка: сбойный вход пропускается с сообщением, остальные рендерятся,
/// код возврата 1 (порт render_many).
#[test]
fn batch_skips_bad_input_exit_one() {
    let d = tmp("batch-skip");
    let ok = d.join("ok.c");
    let bad = d.join("bad.c");
    fs::write(&ok, OK_C).unwrap();
    // битый синтаксис: рисуется частично, но не роняет остальные
    fs::write(
        &bad,
        "int main(void) {\n    int c = 1;\n    if {\n    }\n}\n",
    )
    .unwrap();
    let out = run(&[ok.to_str().unwrap(), bad.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(d.join("ok.svg").exists(), "ok.svg не создан");
    assert!(d.join("bad.svg").exists(), "bad.svg не создан");
}

/// Нечитаемый результат обязан быть назван: кегль на листе считается и
/// печатается, молча выдать 4 pt нельзя.
#[test]
fn illegible_scale_is_reported() {
    let d = tmp("illegible");
    let f = d.join("wide.c");
    let mut src = String::from("int main(void) { int d = 1;\nswitch (d) {\n");
    for k in 1..=9 {
        src.push_str(&format!(
            "case {k}: printf(\"очень длинный текст кейса {k}\"); break;\n"
        ));
    }
    src.push_str("}\n}\n");
    fs::write(&f, src).unwrap();
    let out = run(&[f.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&out.stderr);
    // либо впихнулось читаемо, либо честно сказано про кегль
    assert!(
        !err.is_empty(),
        "широкая схема дана без единого предупреждения про кегль"
    );
}

/// `| head`: обрыв трубы не паника (SIGPIPE игнорируется std, BrokenPipe
/// обрабатывается тихим exit 0).
#[test]
fn broken_pipe_exits_zero() {
    let d = tmp("sigpipe");
    let mut paths: Vec<String> = Vec::new();
    for i in 0..300 {
        let f = d.join(format!("f{i}.c"));
        fs::write(&f, OK_C).unwrap();
        paths.push(f.to_str().unwrap().to_string());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_gostpadi"))
        .args(&paths)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // закрываем трубу до того, как ребёнок допишет 300 строк
    drop(child.stdout.take());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0), "паника на BrokenPipe даёт 101");
}

fn render_svg(d: &std::path::Path, name: &str, extra: &[&str]) -> String {
    let src = d.join(format!("{name}.c"));
    fs::write(&src, OK_C).unwrap();
    let out_path = d.join(format!("{name}.svg"));
    let mut args = vec![src.to_str().unwrap(), "-o", out_path.to_str().unwrap()];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::read_to_string(&out_path).unwrap()
}

fn rect_widths(svg: &str) -> Vec<f64> {
    let mut out: Vec<f64> = svg
        .split("<rect")
        // skip(2): мусор до первого <rect и белая подложка страницы
        .skip(2)
        .filter_map(|p| {
            p.split("width=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|v| v.parse::<f64>().ok())
        })
        .collect();
    out.sort_by(|a, b| b.partial_cmp(a).unwrap());
    out
}

/// --font=20: в SVG font-size="20", фигуры крупнее, чем при дефолте 14.
#[test]
fn font_flag_grows_svg() {
    let d = tmp("font16");
    let svg20 = render_svg(&d, "f20", &["--font=20"]);
    assert!(
        svg20.contains("font-size=\"20\""),
        "нет font-size=\"20\": {:?}",
        svg20.matches("font-size").take(3).collect::<Vec<_>>()
    );
    let svg_def = render_svg(&d, "fdef", &[]);
    assert!(
        svg_def.contains("font-size=\"14\""),
        "кегль по умолчанию 14"
    );
    assert!(
        rect_widths(&svg20)[0] > rect_widths(&svg_def)[0],
        "rect при font=20 крупнее: {} vs {}",
        rect_widths(&svg20)[0],
        rect_widths(&svg_def)[0]
    );
}

/// --lw=2.5: толщина пера 2.5 и на линиях, и на усиках стрелок.
#[test]
fn lw_flag_sets_stroke_width() {
    let d = tmp("lw25");
    let svg = render_svg(&d, "lw", &["--lw=2.5"]);
    assert!(svg.contains("stroke-width=\"2.5\""), "{svg}");
    // усики — path с тем же пером
    assert!(svg.contains("<path"), "нет усиков");
    let paths_lw: Vec<&str> = svg
        .split("<path")
        .skip(1)
        .filter_map(|p| {
            p.split("stroke-width=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
        })
        .collect();
    assert!(paths_lw.iter().all(|&w| w == "2.5"), "{paths_lw:?}");
}

/// --font=0 и --lw=-1 — usage-ошибка с сообщением, exit 2.
#[test]
fn invalid_font_lw_exits_two() {
    let d = tmp("bad-flags");
    let src = d.join("in.c");
    fs::write(&src, OK_C).unwrap();
    for flag in ["--font=0", "--font=-3", "--lw=-1", "--font=abc"] {
        let out = run(&[src.to_str().unwrap(), flag]);
        assert_eq!(out.status.code(), Some(2), "{flag}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!err.is_empty(), "{flag}: нет сообщения");
    }
}

/// Файл БЕЗ main, но с функцией, рисуется: тело функции и есть схема.
/// Раньше такой файл давал пустой лист и отказ, хотя рисовать было что.
#[test]
fn file_without_main_but_with_function_is_drawn() {
    let d = tmp("no-main-fn");
    let f = d.join("helper.c");
    fs::write(&f, "int helper(int x) { return x + 1; }\n").unwrap();
    let out = run(&[f.to_str().unwrap(), "-o", d.join("o.svg").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(d.join("o.svg").exists(), "схема не создана");
    let svg = fs::read_to_string(d.join("o.svg")).unwrap();
    assert!(svg.contains("x + 1"), "тело функции не нарисовано: {svg}");
}

/// Рисовать нечего: ни одной функции с телом. «Готово» в коде выхода
/// было бы враньём — наружу уходит лист с двумя терминаторами.
#[test]
fn nothing_to_draw_exits_one_and_says_so() {
    let d = tmp("no-main");
    let f = d.join("decls.c");
    fs::write(&f, "int a;\nstruct S { int x; };\n").unwrap();
    let out = run(&[f.to_str().unwrap(), "-o", d.join("o.svg").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "рисовать нечего — это провал");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("рисовать нечего"), "причина не названа: {err}");
}

/// Частично пустая пачка — норма: остальные файлы отрисованы,
/// предупреждения достаточно, выход 0.
#[test]
fn batch_with_one_mainless_file_still_succeeds() {
    let d = tmp("no-main-batch");
    let good = d.join("good.c");
    let bad = d.join("helper.c");
    fs::write(&good, OK_C).unwrap();
    fs::write(&bad, "int helper(void) { return 1; }\n").unwrap();
    // слеш в конце: без него -o это имя файла, а не папки
    let out_dir = format!("{}/", d.join("out").display());
    let out = run(&[
        good.to_str().unwrap(),
        bad.to_str().unwrap(),
        "-o",
        &out_dir,
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(d.join("out/good.svg").exists(), "good.svg не создан");
    // файл без main рисуется по своей функции, а не пустым листом:
    // имя файла получает суффикс функции — helper-helper.svg
    assert!(
        d.join("out/helper-helper.svg").exists(),
        "схема функции helper не создана: {:?}",
        std::fs::read_dir(d.join("out"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("main не найден"),
        "файл без main разбирается как функция, а не как пустой: {err}"
    );
}
