//! The `ve-core` skill (skills/ve-core) documents every method, and its links
//! resolve. A method added to the registry without docs fails here.

use std::fs;
use std::path::{Path, PathBuf};

fn skill_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/ve-core")
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

#[test]
fn every_method_is_documented() {
    let methods_dir = skill_dir().join("methods");
    let index = fs::read_to_string(methods_dir.join("index.md")).unwrap();
    let mut files = Vec::new();
    markdown_files(&methods_dir, &mut files);
    let details: String =
        files.iter().filter(|p| !p.ends_with("index.md")).map(|p| fs::read_to_string(p).unwrap()).collect::<Vec<_>>().join("\n");

    let registry = ve_daemon::build_registry();
    let mut missing = Vec::new();
    let ui_docs = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/ve-ui/SKILL.md")).unwrap_or_default();
    for spec in registry.specs() {
        let name = format!("`{}`", spec.name);
        // `ui.*` is documented in skills/ve-ui (it is meant to become a plugin), not in ve-core.
        if spec.name.starts_with("ui.") {
            if !ui_docs.contains(&name) {
                missing.push(format!("{} not in skills/ve-ui/SKILL.md", spec.name));
            }
            continue;
        }
        if !index.contains(&name) {
            missing.push(format!("{} not in methods/index.md", spec.name));
        }
        if !details.contains(&name) {
            missing.push(format!("{} has no section in methods/*.md", spec.name));
        }
    }
    assert!(missing.is_empty(), "undocumented methods:\n{}", missing.join("\n"));
}

#[test]
fn relative_links_resolve() {
    let mut files = Vec::new();
    markdown_files(&skill_dir(), &mut files);
    let mut broken = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        // Markdown links: `](target)`.
        for part in text.split("](").skip(1) {
            let target = part.split(')').next().unwrap_or("");
            if target.starts_with("http") || target.is_empty() {
                continue;
            }
            let path = target.split('#').next().unwrap_or("");
            if path.is_empty() {
                continue;
            }
            if !file.parent().unwrap().join(path).exists() {
                broken.push(format!("{}: {target}", file.display()));
            }
        }
    }
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
}
