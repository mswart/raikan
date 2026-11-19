//! Keeps the layer direction of docs/architecture.md §3 honest: a module may only refer to
//! modules of its own or a lower layer. The legacy implementation is visible to adapters only.

use std::path::Path;

/// (module, layer). `domain` and `trace` sit beside everything.
const LAYERS: &[(&str, u8)] = &[
    ("domain", 0),
    ("trace", 0),
    ("variants", 1),
    ("engine", 2),
    ("knowledge", 3),
    ("conventions", 4),
    ("policy", 5),
    ("search", 6),
    ("agent", 7),
    ("adapters", 8),
];

const LEGACY: &[&str] = &[
    "game",
    "hyphenated",
    "card_quantum",
    "position_set",
    "dump_strategies",
    "PositionSet",
    "CardQuantum",
];

fn layer(module: &str) -> Option<u8> {
    LAYERS.iter().find(|(m, _)| *m == module).map(|(_, l)| *l)
}

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn layers_only_depend_downwards() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for &(module, own) in LAYERS {
        let dir = src.join(module);
        if !dir.exists() {
            continue;
        }
        let mut files = Vec::new();
        sources(&dir, &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap();
                for (i, _) in code.match_indices("crate::") {
                    let rest = &code[i + "crate::".len()..];
                    let rest = rest.trim_start_matches('{');
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    let target = layer(&name);
                    let bad = match target {
                        Some(t) => t > own && t != 0,
                        None => LEGACY.contains(&name.as_str()) && module != "adapters",
                    };
                    if bad {
                        violations.push(format!(
                            "{}:{}: {module} uses crate::{name}",
                            file.display(),
                            n + 1
                        ));
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "layer violations:\n{}",
        violations.join("\n")
    );
}
