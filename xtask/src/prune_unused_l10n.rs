use std::collections::HashSet;
use std::fs;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Args;
use ignore::WalkBuilder;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use xshell::{cmd, Shell};

use crate::util::workspace_root;

const DEFAULT_PROJECT_ROOT: &str = "app";
const DEFAULT_ARB: &str = "lib/l10n/app_en.arb";
const DEFAULT_SEARCH_ROOTS: &[&str] = &["lib", "test"];
const DEFAULT_EXTENSIONS: &[&str] = &[".dart", ".kt", ".swift", ".java", ".m", ".mm"];
const DEFAULT_EXCLUDE_DIRS: &[&str] = &["lib/l10n"];
const DEFAULT_INCLUDE_FILES: &[&str] = &["lib/l10n/app_localizations_extension.dart"];

#[derive(Args, Debug)]
pub(crate) struct PruneArgs {
    /// Rewrite ARB file(s). Without this flag the task only reports unused keys.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    apply: bool,
    /// Print a line whenever keys are found in a file.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    verbose: bool,
    /// Keep @metadata entries even if their base key is removed.
    #[arg(long = "keep-metadata", action = clap::ArgAction::SetTrue)]
    keep_metadata: bool,
    /// Resolve relative paths against this directory.
    #[arg(long = "project-root", default_value = DEFAULT_PROJECT_ROOT)]
    project_root: String,
    /// Canonical ARB file to inspect.
    #[arg(long, default_value = DEFAULT_ARB)]
    arb: String,
    /// Additional ARB files that should be pruned alongside the canonical file.
    #[arg(long = "mirror-arb", value_name = "path")]
    mirror_arb: Vec<String>,
    /// Directories to scan for localization usages.
    #[arg(
        long = "search-root",
        value_name = "path",
        default_values = DEFAULT_SEARCH_ROOTS
    )]
    search_root: Vec<String>,
    /// File extensions to include while scanning.
    #[arg(long = "ext", value_name = ".dart", default_values = DEFAULT_EXTENSIONS)]
    ext: Vec<String>,
    /// Directories to skip when searching for usages.
    #[arg(
        long = "exclude-dir",
        value_name = "path",
        default_values = DEFAULT_EXCLUDE_DIRS
    )]
    exclude_dir: Vec<String>,
    /// Files that are always scanned even if they live in excluded directories.
    #[arg(
        long = "include-file",
        value_name = "path",
        default_values = DEFAULT_INCLUDE_FILES
    )]
    include_file: Vec<String>,
    /// Require a clean git workspace (unless --allow-dirty), prune with --apply, then run flutter checks.
    #[arg(long = "safe", action = clap::ArgAction::SetTrue)]
    safe: bool,
    /// Skip the git clean check (useful when running with --safe).
    #[arg(long = "allow-dirty", action = clap::ArgAction::SetTrue)]
    allow_dirty: bool,
}

pub(crate) fn run(args: PruneArgs) -> Result<()> {
    if args.safe && !args.apply {
        bail!("--safe requires --apply so changes can be written.");
    }

    let shell = Shell::new()?;

    let project_root = resolve_relative(workspace_root(), &args.project_root);
    shell.change_dir(project_root.as_std_path());

    let resolve = |input: &str| resolve_relative(project_root.as_ref(), input);

    let arb_path = resolve(&args.arb);
    if !arb_path.exists() {
        bail!("ARB file not found: {}", arb_path);
    }

    let search_roots: Vec<Utf8PathBuf> =
        args.search_root.iter().map(|root| resolve(root)).collect();
    let include_exts: HashSet<String> = args
        .ext
        .iter()
        .map(|ext| {
            if ext.starts_with('.') {
                ext.clone()
            } else {
                format!(".{ext}")
            }
        })
        .collect();
    let exclude_dirs: Vec<Utf8PathBuf> = args.exclude_dir.iter().map(|dir| resolve(dir)).collect();
    let include_files: HashSet<Utf8PathBuf> =
        args.include_file.iter().map(|file| resolve(file)).collect();

    let keep_metadata = args.keep_metadata;
    let verbose = args.verbose;
    let apply = args.apply;
    let safe_mode = args.safe;
    let allow_dirty = args.allow_dirty;

    if safe_mode && !allow_dirty {
        ensure_clean_git_workspace(&shell, project_root.as_ref())?;
    }

    let keys = load_keys(&arb_path)?;
    if keys.is_empty() {
        println!("No keys found in {}.", arb_path);
        return Ok(());
    }

    let candidate_files =
        collect_candidate_files(&search_roots, &include_exts, &exclude_dirs, &include_files);

    if candidate_files.is_empty() {
        bail!("No files matched the provided search criteria.");
    }

    let unused_keys = find_unused_keys(&keys, &candidate_files, verbose, project_root.as_ref());

    if unused_keys.is_empty() {
        println!("✅ All localization keys are referenced.");
        return Ok(());
    }

    println!("Found {} unused key(s):", unused_keys.len());
    let mut sorted_keys = unused_keys.iter().cloned().collect::<Vec<_>>();
    sorted_keys.sort();
    for key in sorted_keys {
        println!(" • {key}");
    }

    if !apply {
        println!("\nDry-run mode; pass --apply to remove them.");
        return Ok(());
    }

    let mut target_set: HashSet<Utf8PathBuf> = HashSet::new();
    target_set.insert(arb_path.clone());
    for mirror in args.mirror_arb.iter().map(|path| resolve(path)) {
        target_set.insert(mirror);
    }
    for sibling in discover_sibling_arbs(&arb_path) {
        target_set.insert(sibling);
    }
    let total_files = target_set.len();

    let mut total_removed = 0usize;
    for target in target_set {
        if !target.exists() {
            eprintln!("Skipping missing ARB: {}", target);
            continue;
        }
        total_removed += prune_arb_file(&target, &unused_keys, keep_metadata)?;
    }

    println!("\nRemoved {total_removed} entries across {total_files} file(s).");

    run_flutter_command(&shell, &["gen-l10n"], project_root.as_ref())?;
    let analyze_targets = build_analyze_targets(&search_roots, project_root.as_ref());
    let mut args = vec!["analyze".to_string()];
    args.extend(analyze_targets);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_flutter_command(&shell, &arg_refs, project_root.as_ref())?;

    Ok(())
}

fn resolve_relative(base: &Utf8Path, raw: &str) -> Utf8PathBuf {
    let path = Utf8PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn load_keys(path: &Utf8Path) -> Result<Vec<String>> {
    let raw = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
    let data: serde_json::Map<String, Value> =
        serde_json::from_str(&raw).with_context(|| format!("Failed to parse {path}"))?;
    Ok(data
        .keys()
        .filter(|key| !key.starts_with('@'))
        .cloned()
        .collect())
}

fn collect_candidate_files(
    search_roots: &[Utf8PathBuf],
    include_extensions: &HashSet<String>,
    exclude_dirs: &[Utf8PathBuf],
    include_files: &HashSet<Utf8PathBuf>,
) -> Vec<Utf8PathBuf> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();

    for root in search_roots {
        if !root.exists() {
            continue;
        }
        let mut builder = WalkBuilder::new(root);
        builder.hidden(false).follow_links(false);
        let walker = builder.build();
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    eprintln!("⚠️  Skipping entry under {}: {}", root, err);
                    continue;
                }
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let path = match Utf8PathBuf::from_path_buf(entry.into_path()) {
                Ok(path) => path,
                Err(os_string) => {
                    eprintln!("⚠️  Skipping non-UTF8 path: {:?}", os_string);
                    continue;
                }
            };
            if include_files.contains(&path) {
                if seen.insert(path.clone()) {
                    files.push(path);
                }
                continue;
            }
            if is_excluded(&path, exclude_dirs) {
                continue;
            }
            match path.extension() {
                Some(ext) => {
                    let normalized = format!(".{ext}");
                    if !include_extensions.contains(&normalized) {
                        continue;
                    }
                }
                None => continue,
            }
            if seen.insert(path.clone()) {
                files.push(path);
            }
        }
    }

    for include in include_files {
        if include.exists() && seen.insert(include.clone()) {
            files.push(include.clone());
        }
    }

    files
}

fn is_excluded(path: &Utf8Path, exclude_dirs: &[Utf8PathBuf]) -> bool {
    exclude_dirs
        .iter()
        .any(|dir| path.starts_with(dir.as_path()))
}

fn find_unused_keys(
    keys: &[String],
    files: &[Utf8PathBuf],
    verbose: bool,
    project_root: &Utf8Path,
) -> HashSet<String> {
    let mut unused: HashSet<String> = keys.iter().cloned().collect();
    for file in files {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("⚠️  Skipping {}: {error}", file);
                continue;
            }
        };
        let hits: Vec<String> = unused
            .iter()
            .filter(|key| text.contains(*key))
            .cloned()
            .collect();
        if !hits.is_empty() {
            for hit in &hits {
                unused.remove(hit);
            }
            if verbose {
                let rel = relative_path(file, project_root);
                eprintln!("✔ Found {} key(s) in {rel}", hits.len());
            }
            if unused.is_empty() {
                break;
            }
        }
    }
    unused
}

fn ensure_clean_git_workspace(shell: &Shell, working_directory: &Utf8Path) -> Result<()> {
    let output = cmd!(shell, "git status --porcelain")
        .read()
        .with_context(|| format!("Failed to run git status in {working_directory}"))?;
    if !output.trim().is_empty() {
        bail!(
            "Safe mode: working tree is dirty. Commit or stash your changes before pruning localizations."
        );
    }
    Ok(())
}

fn prune_arb_file(
    path: &Utf8Path,
    unused_keys: &HashSet<String>,
    keep_metadata: bool,
) -> Result<usize> {
    let contents = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
    let data: serde_json::Map<String, Value> =
        serde_json::from_str(&contents).with_context(|| format!("Failed to parse {path}"))?;

    let base_count = unused_keys
        .iter()
        .filter(|key| data.contains_key(*key))
        .count();
    let metadata_count = if keep_metadata {
        0
    } else {
        unused_keys
            .iter()
            .map(|key| format!("@{key}"))
            .filter(|meta| data.contains_key(meta))
            .count()
    };
    let removed = base_count + metadata_count;

    if removed == 0 {
        return Ok(0);
    }

    let preserved = remove_keys_preserving_whitespace(&contents, unused_keys, keep_metadata);
    fs::write(path, preserved).with_context(|| format!("Failed to write {path}"))?;
    Ok(removed)
}

fn relative_path(target: &Utf8Path, root: &Utf8Path) -> String {
    target
        .strip_prefix(root)
        .map(|path| path.to_string())
        .unwrap_or_else(|_| target.to_string())
}

fn run_flutter_command(shell: &Shell, args: &[&str], working_directory: &Utf8Path) -> Result<()> {
    let label = args.join(" ");
    println!("Safe mode: running flutter {label} …");
    if args.is_empty() {
        bail!("flutter command requires arguments");
    }
    cmd!(shell, "flutter {args...}")
        .run()
        .with_context(|| format!("flutter {label} failed in {working_directory}"))
}

fn build_analyze_targets(search_roots: &[Utf8PathBuf], project_root: &Utf8Path) -> Vec<String> {
    let mut targets = Vec::new();
    for path in search_roots {
        if path.is_dir() {
            targets.push(relative_path(path, project_root));
        }
    }
    if targets.is_empty() {
        for fallback in ["lib", "test"] {
            let candidate = project_root.join(fallback);
            if candidate.is_dir() {
                targets.push(relative_path(&candidate, project_root));
            }
        }
    }
    targets
}

fn discover_sibling_arbs(arb_path: &Utf8Path) -> Vec<Utf8PathBuf> {
    let mut siblings = Vec::new();
    if let Some(directory) = arb_path.parent() {
        if let Ok(entries) = directory.read_dir_utf8() {
            for entry in entries.flatten() {
                let path = entry.path().to_path_buf();
                if path == arb_path {
                    continue;
                }
                if path.extension() == Some("arb") {
                    siblings.push(path);
                }
            }
        }
    }
    siblings
}

fn remove_keys_preserving_whitespace(
    content: &str,
    unused_keys: &HashSet<String>,
    keep_metadata: bool,
) -> String {
    static KEY_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^\"([^\"]+)\":"#).unwrap());
    static TRAILING_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r",(\s*})").unwrap());

    let lines: Vec<&str> = content.split('\n').collect();
    let mut kept = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let trimmed_left = line.trim_start();
        if !trimmed_left.starts_with('"') {
            kept.push(line.to_string());
            index += 1;
            continue;
        }

        let mut should_remove = false;
        if let Some(captures) = KEY_PATTERN.captures(trimmed_left) {
            let key_name = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            let (base_key, is_metadata) = if let Some(stripped) = key_name.strip_prefix('@') {
                (stripped, true)
            } else {
                (key_name, false)
            };
            if unused_keys.contains(base_key) && (!is_metadata || !keep_metadata) {
                should_remove = true;
            }
        }

        if !should_remove {
            kept.push(line.to_string());
            index += 1;
            continue;
        }

        let mut brace_depth = initial_brace_depth(trimmed_left);
        while brace_depth > 0 && index + 1 < lines.len() {
            index += 1;
            brace_depth += line_brace_delta(lines[index]);
        }
        index += 1;
    }

    let mut output = kept.join("\n");
    output = TRAILING_COMMA.replace_all(&output, "$1").into_owned();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn initial_brace_depth(line: &str) -> i32 {
    if !starts_object_value(line) {
        return 0;
    }
    line_brace_delta(line)
}

fn starts_object_value(line: &str) -> bool {
    if let Some(colon_index) = line.find(':') {
        for ch in line[colon_index + 1..].chars() {
            if ch.is_whitespace() {
                continue;
            }
            return ch == '{';
        }
    }
    false
}

fn line_brace_delta(line: &str) -> i32 {
    let mut delta = 0;
    let mut in_string = false;
    let mut is_escaped = false;
    for ch in line.chars() {
        if is_escaped {
            is_escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                is_escaped = true;
            }
            '"' => {
                in_string = !in_string;
            }
            '{' if !in_string => delta += 1,
            '}' if !in_string => delta -= 1,
            _ => {}
        }
    }
    delta
}
