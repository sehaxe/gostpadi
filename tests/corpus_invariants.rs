//! Корпус целиком: каждая схема раскладывается без новых дефектов
//! (X-пересечения рёбер, линия по кромке блока) и рисуется в SVG.
//! `pipeline::validate` — тот же набор инвариантов, что у `--check` и
//! serve /lint; corpus-тест держит их обязательными для всего корпуса.

use gostpadi::frontend::{input_kind, InputKind};
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn corpus_has_no_edge_crossings_or_border_lines() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.to_str()
                .is_some_and(|s| input_kind(s) != InputKind::Gvn || s.ends_with(".gvn"))
        })
        .collect();
    files.sort();
    assert!(files.len() >= 24, "корпус мал: {}", files.len());

    let opts = gostpadi::pipeline::Options {
        labels: "ru".into(),
        font: None,
        lw: None,
        no_split: false,
    };
    let st = opts.style();
    let out = std::env::temp_dir().join(format!("gostpadi-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    for f in &files {
        let p = f.to_str().unwrap();
        let text = std::fs::read_to_string(f).unwrap();
        let inputs = vec![(p.to_string(), text, input_kind(p))];
        let schemes = gostpadi::pipeline::parse_batch(&inputs, &opts)
            .unwrap_or_else(|(path, e)| panic!("{path}: разбор: {e:?}"));
        for (name, res) in gostpadi::pipeline::validate(&schemes, &st) {
            if let Err(e) = res {
                panic!("{name}: {e}");
            }
        }
        // Листы, которые реально печатаются: те же два инварианта на
        // разрезанных частях с едиными размерами пачки.
        let normed: Vec<_> = schemes
            .iter()
            .map(|s| gostpadi::layout::normalize(&s.nodes, &st))
            .collect();
        let sizes = gostpadi::layout::uniform_sizes(&normed);
        for s in &schemes {
            let parts = gostpadi::layout::split_scheme(s.nodes.clone(), &sizes, &st);
            for (i, part) in parts.iter().enumerate() {
                let l = gostpadi::layout::layout(part, &sizes, &st);
                if let Err(e) = gostpadi::layout::edges_cross_ok(&l) {
                    panic!("{} лист {i}: пересечение линий: {e}", s.path);
                }
                if let Err(e) = gostpadi::layout::edge_on_border_ok(&l) {
                    panic!("{} лист {i}: линия по кромке: {e}", s.path);
                }
            }
        }
        let r = Command::new(env!("CARGO_BIN_EXE_gostpadi"))
            .args([p, "-o", out.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "{p} render: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let chk = Command::new(env!("CARGO_BIN_EXE_gostpadi"))
            .args(["--check", p])
            .output()
            .unwrap();
        assert!(
            chk.status.success(),
            "{p} --check: {}",
            String::from_utf8_lossy(&chk.stderr)
        );
    }
    std::fs::remove_dir_all(&out).ok();
}
