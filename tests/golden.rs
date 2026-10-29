//! Эталон SVG: C++-схема не должна незаметно поехать после правок
//! раскладки или C++-нормализатора. `golden/cpp-lab.svg` — main из
//! `corpus/cpp-lab.cpp`, снятый тем же CLI, что видит пользователь.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gostpadi-golden-{}-{name}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn cpp_main_scheme_matches_golden() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("corpus/cpp-lab.cpp");
    let out = tmp("cpp");
    let r = Command::new(env!("CARGO_BIN_EXE_gostpadi"))
        .args([src.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "render: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let got = fs::read_to_string(out.join("cpp-lab.svg")).unwrap();
    let want = fs::read_to_string(root.join("golden/cpp-lab.svg")).unwrap();
    assert_eq!(got, want, "C++-схема разошлась с эталоном");
    fs::remove_dir_all(&out).ok();
}
