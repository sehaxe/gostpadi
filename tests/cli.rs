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

const OK_GVN: &str = "input scanf(\"%d\", &a)\nc = a * 2\noutput printf(\"c\", c)\n";

#[test]
fn check_ok_exits_zero() {
    let d = tmp("check-ok");
    let f = d.join("ok.gvn");
    fs::write(&f, OK_GVN).unwrap();
    let out = run(&["--check", f.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok: 5 blocks"));
}

#[test]
fn check_bad_exits_one_with_line() {
    let d = tmp("check-bad");
    let f = d.join("bad.gvn");
    // строка 2 с неожиданным отступом
    fs::write(&f, "input scanf(\"%d\", &a)\n    c = 1\n").unwrap();
    let out = run(&["--check", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!("{}:2:", f.display())),
        "stderr: {err}"
    );
    assert!(err.contains("indentation"), "stderr: {err}");
}

#[test]
fn template_prints_gvn_stub() {
    let out = run(&["--template"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("input scanf"));
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
    let src = d.join("in.gvn");
    fs::write(&src, OK_GVN).unwrap();
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
fn render_examples_board2_if_to_tmp() {
    let d = tmp("board2");
    let out_path = d.join("out.svg");
    let out = run(&["examples/board2_if.gvn", "-o", out_path.to_str().unwrap()]);
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
    fs::write(d.join("a/x.gvn"), OK_GVN).unwrap();
    fs::write(d.join("b/y.gvn"), OK_GVN).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gostpadi"))
        .args(["a/x.gvn", "b/y.gvn", "-o", "out.svg"])
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
    let ok = d.join("ok.gvn");
    let bad = d.join("bad.gvn");
    fs::write(&ok, OK_GVN).unwrap();
    fs::write(&bad, "    c = 1\n").unwrap();
    let out = run(&[ok.to_str().unwrap(), bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("bad.gvn"), "stderr: {err}");
    assert!(d.join("ok.svg").exists(), "ok.svg не создан");
}

/// `| head`: обрыв трубы не паника (SIGPIPE игнорируется std, BrokenPipe
/// обрабатывается тихим exit 0).
#[test]
fn broken_pipe_exits_zero() {
    let d = tmp("sigpipe");
    let mut paths: Vec<String> = Vec::new();
    for i in 0..300 {
        let f = d.join(format!("f{i}.gvn"));
        fs::write(&f, OK_GVN).unwrap();
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
