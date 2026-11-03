//! Pins how much of this repository's own code the turn-only memory's screen redacts.
//!
//! A memory that quotes what it kept is useless if the screen in front of it
//! takes a type name for a key. The shared `screen_text` takes about a quarter
//! of the identifiers of 20 characters or more in `crates/**/*.rs`; the memory's
//! screen may take 1%. The sources are read the way the architecture and
//! filemap tests read them, so a change that swaps one screen for the other, or
//! adds a rule that reaches ordinary code, fails here.

use std::path::{Path, PathBuf};

use polaris_core::secret_screen::{FilterResult, screen_for_memory, screen_text};
use regex::Regex;

#[path = "support/repository_files.rs"]
mod repository_files;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cannot derive the repository root from CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// The lines of `crates/**/*.rs` that hold an identifier of 20 characters or
/// more, as the lines a tool result would carry.
fn lines_with_long_identifiers(root: &Path, identifier: &Regex) -> Vec<String> {
    let mut lines = Vec::new();
    for path in repository_files::repository_files(root, true) {
        if !path.starts_with("crates/") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(root.join(&path)) else {
            continue;
        };
        lines.extend(
            body.lines()
                .filter(|l| identifier.is_match(l))
                .map(str::to_owned),
        );
    }
    lines
}

/// How many of the identifiers on `lines` the screen took out, counted where
/// each stands: the identifier is gone from what the screen returned, or the
/// whole line was dropped.
fn taken_out(lines: &[String], identifier: &Regex, screen: fn(&str) -> FilterResult) -> usize {
    lines
        .iter()
        .map(|line| match screen(line) {
            FilterResult::Keep(_) => 0,
            FilterResult::Drop => identifier.find_iter(line).count(),
            FilterResult::Redacted(out) => identifier
                .find_iter(line)
                .filter(|found| !out.contains(found.as_str()))
                .count(),
        })
        .sum()
}

#[test]
fn the_memory_screen_takes_at_most_one_in_a_hundred_of_the_repositorys_long_identifiers() {
    let root = repo_root();
    let identifier = Regex::new(r"[A-Za-z_][A-Za-z0-9_]{19,}").expect("compiles");
    let lines = lines_with_long_identifiers(&root, &identifier);
    let total: usize = lines.iter().map(|l| identifier.find_iter(l).count()).sum();
    assert!(
        total > 10_000,
        "the repository's sources were not read: {total} identifiers"
    );

    let memory = taken_out(&lines, &identifier, screen_for_memory);
    assert!(
        memory * 100 <= total,
        "screen_for_memory took out {memory} of {total} identifiers; the limit is 1%"
    );

    // The measure sees redaction: the screen it must not drift toward takes
    // far more than the limit.
    let shared = taken_out(&lines, &identifier, screen_text);
    assert!(
        shared * 10 > total,
        "screen_text took out only {shared} of {total}: the measure no longer sees redaction"
    );
}
