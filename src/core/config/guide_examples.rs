//! CI verification of the KDL examples in the user guide (`docs/guide`).
//!
//! Every fenced ```` ```kdl ```` block in the guide's Markdown chapters must
//! be a **complete user config**: it is parsed and validated through exactly
//! the path a real user config takes (`parse_user_kdl` + `Config::with_user`).
//! Fragments that are not meant to load on their own — error demonstrations,
//! partial snippets — are annotated ```` ```kdl,ignore ```` and skipped.
//!
//! This keeps the guide honest: a config example that stops loading breaks
//! `cargo test`, so a broken example can never ship.

use super::loader::Config;
use super::source::parse_user_kdl;
use std::path::{Path, PathBuf};

/// A fenced code block extracted from a Markdown document.
#[derive(Debug, PartialEq, Eq)]
struct FencedBlock {
    /// 1-based line number of the opening fence.
    line: usize,
    /// The fence's info string (`kdl`, `kdl,ignore`, `bash`, …).
    info: String,
    /// The block's content, newline-terminated per line.
    body: String,
}

/// All ``` fenced blocks of `md`, in order. Handles indented fences
/// (blockquotes / lists) by trimming leading whitespace before matching.
fn extract_fenced_blocks(md: &str) -> Vec<FencedBlock> {
    let mut blocks = Vec::new();
    let mut open: Option<FencedBlock> = None;
    for (i, line) in md.lines().enumerate() {
        let trimmed = line.trim_start();
        match open.as_mut() {
            Some(block) => {
                if trimmed.starts_with("```") {
                    blocks.push(open.take().expect("checked above"));
                } else {
                    block.body.push_str(line);
                    block.body.push('\n');
                }
            }
            None => {
                if let Some(info) = trimmed.strip_prefix("```") {
                    open = Some(FencedBlock {
                        line: i + 1,
                        info: info.trim().to_string(),
                        body: String::new(),
                    });
                }
            }
        }
    }
    blocks
}

/// Every chapter file of every book under `docs/guide` — any `*.md` inside a
/// `src` directory (skipping mdBook's `book` output directories).
fn guide_chapter_files() -> Vec<PathBuf> {
    let guide = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/guide");
    let mut files = Vec::new();
    collect_chapters(&guide, false, &mut files);
    files.sort();
    files
}

fn collect_chapters(dir: &Path, in_src: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name == "book" {
                continue; // mdBook build output
            }
            collect_chapters(&path, in_src || name == "src", out);
        } else if in_src && path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

#[test]
fn every_guide_kdl_example_loads_as_a_user_config() {
    let files = guide_chapter_files();
    if files.is_empty() {
        // The crate package excludes docs/ (Cargo.toml `exclude`), so a
        // `cargo test` run from a crates.io / distro source tree has no
        // guide to verify. Only a git checkout must hard-fail here.
        if !Path::new(env!("CARGO_MANIFEST_DIR")).join(".git").exists() {
            eprintln!("guide examples: docs/guide not present (packaged source) — skipping");
            return;
        }
        panic!("no guide chapters found — was docs/guide moved?");
    }

    let mut verified = 0usize;
    let mut failures = Vec::new();
    for file in &files {
        let md = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        for block in extract_fenced_blocks(&md) {
            if block.info != "kdl" {
                continue; // not a config example, or marked kdl,ignore
            }
            let origin = format!("{}:{}", file.display(), block.line);
            let result = parse_user_kdl(&block.body, Path::new(&origin))
                .and_then(|doc| Config::with_user(&doc, PathBuf::from(&origin)));
            match result {
                Ok(_) => verified += 1,
                Err(e) => failures.push(format!("{origin}: {e:#}")),
            }
        }
    }

    assert!(
        failures.is_empty(),
        "guide config examples failed to load:\n{}",
        failures.join("\n")
    );
    // Guard against the extractor silently matching nothing: the config
    // chapters ship dozens of verified examples in each language.
    assert!(
        verified >= 30,
        "only {verified} ```kdl examples found — extraction is likely broken"
    );
}

#[cfg(test)]
mod extractor_tests {
    use super::*;

    #[test]
    fn extracts_blocks_with_info_and_line_numbers() {
        let md = "# T\n\n```kdl\ntheme \"x\"\n```\n\ntext\n\n```bash\nls\n```\n";
        let blocks = extract_fenced_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].line, 3);
        assert_eq!(blocks[0].info, "kdl");
        assert_eq!(blocks[0].body, "theme \"x\"\n");
        assert_eq!(blocks[1].line, 9);
        assert_eq!(blocks[1].info, "bash");
        assert_eq!(blocks[1].body, "ls\n");
    }

    #[test]
    fn ignore_annotation_is_a_distinct_info_string() {
        let md = "```kdl,ignore\nbroken (\n```\n```kdl\nicons \"none\"\n```\n";
        let blocks = extract_fenced_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].info, "kdl,ignore");
        assert_eq!(blocks[1].info, "kdl");
    }

    #[test]
    fn indented_fences_and_unclosed_blocks() {
        // An indented fence (e.g. inside a list) still opens a block; a
        // block left unclosed at EOF is dropped rather than panicking.
        let md = "- item\n  ```kdl\n  theme \"x\"\n  ```\n```text\ndangling\n";
        let blocks = extract_fenced_blocks(md);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].info, "kdl");
        assert_eq!(blocks[0].body, "  theme \"x\"\n");
    }

    #[test]
    fn text_between_blocks_is_not_captured() {
        let md = "```kdl\na \"1\"\n```\nnot in a block\n```kdl\nb \"2\"\n```\n";
        let blocks = extract_fenced_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].body, "a \"1\"\n");
        assert_eq!(blocks[1].body, "b \"2\"\n");
    }

    #[test]
    fn finds_the_guide_chapters() {
        let files = guide_chapter_files();
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        // Both books' recipe chapters must be picked up.
        assert!(
            names.iter().filter(|n| *n == "config-recipes.md").count() >= 2,
            "expected config-recipes.md in en and ja, got: {names:?}"
        );
    }
}
