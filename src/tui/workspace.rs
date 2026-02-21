use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub path: String,
    pub content: String,
    pub terms: HashSet<String>,
    pub modified_unix: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIndex {
    pub root: PathBuf,
    pub entries: Vec<IndexedFile>,
    pub indexed_files: usize,
    pub skipped_files: usize,
    pub built_unix: u64,
}

#[derive(Debug, Clone)]
struct CandidateFile {
    path: PathBuf,
    rel: String,
    modified_unix: u64,
    size: u64,
}

impl WorkspaceIndex {
    pub fn build(root: &Path, max_files: usize, max_file_bytes: u64) -> Self {
        Self::build_from_existing(root, max_files, max_file_bytes, None)
    }

    pub fn build_incremental(
        root: &Path,
        cache_path: &Path,
        max_files: usize,
        max_file_bytes: u64,
    ) -> Self {
        let cached = Self::load_cache(cache_path).ok();
        let index = Self::build_from_existing(root, max_files, max_file_bytes, cached.as_ref());
        let _ = index.save_cache(cache_path);
        index
    }

    pub fn load_cache(cache_path: &Path) -> Result<Self, String> {
        let raw =
            fs::read_to_string(cache_path).map_err(|e| format!("failed to read cache: {}", e))?;
        serde_json::from_str(&raw).map_err(|e| format!("failed to parse cache: {}", e))
    }

    pub fn save_cache(&self, cache_path: &Path) -> Result<(), String> {
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("failed to create cache dir: {}", e))?;
        }
        let raw =
            serde_json::to_string(self).map_err(|e| format!("failed to encode cache: {}", e))?;
        fs::write(cache_path, raw).map_err(|e| format!("failed to write cache: {}", e))
    }

    pub fn retrieve(&self, query: &str, top_k: usize, max_chars: usize) -> Vec<(String, String)> {
        let query_terms = extract_terms(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut scored = self
            .entries
            .iter()
            .filter_map(|entry| {
                let overlap = query_terms.intersection(&entry.terms).count();
                if overlap == 0 {
                    None
                } else {
                    Some((overlap, entry))
                }
            })
            .collect::<Vec<_>>();

        scored.sort_by(|a, b| b.0.cmp(&a.0));

        let mut out = Vec::new();
        for (_, entry) in scored.into_iter().take(top_k) {
            let mut snippet = entry.content.clone();
            if snippet.len() > max_chars {
                let mut end = max_chars;
                while end > 0 && !snippet.is_char_boundary(end) {
                    end -= 1;
                }
                snippet.truncate(end);
                snippet.push_str("\n[...truncated]");
            }
            out.push((entry.path.clone(), snippet));
        }
        out
    }

    pub fn search_paths(&self, query: &str, top_k: usize) -> Vec<String> {
        let q = query.to_lowercase();
        let mut out = self
            .entries
            .iter()
            .filter(|e| e.path.to_lowercase().contains(&q))
            .map(|e| e.path.clone())
            .take(top_k)
            .collect::<Vec<_>>();

        if out.is_empty() {
            let terms = extract_terms(query);
            let mut scored = self
                .entries
                .iter()
                .filter_map(|entry| {
                    let score = terms.intersection(&entry.terms).count();
                    if score == 0 {
                        None
                    } else {
                        Some((score, entry.path.clone()))
                    }
                })
                .collect::<Vec<_>>();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            out = scored.into_iter().take(top_k).map(|(_, p)| p).collect();
        }

        out
    }

    fn build_from_existing(
        root: &Path,
        max_files: usize,
        max_file_bytes: u64,
        existing: Option<&WorkspaceIndex>,
    ) -> Self {
        let mut indexed_files = 0usize;
        let mut skipped_files = 0usize;

        let candidates = collect_candidates(root, max_files, max_file_bytes, &mut skipped_files);

        let existing_map = existing
            .map(|idx| {
                idx.entries
                    .iter()
                    .map(|e| (e.path.clone(), e.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        let mut entries = Vec::new();
        for candidate in candidates {
            if indexed_files >= max_files {
                break;
            }

            if let Some(old) = existing_map.get(&candidate.rel) {
                if old.modified_unix == candidate.modified_unix && old.size == candidate.size {
                    entries.push(old.clone());
                    indexed_files += 1;
                    continue;
                }
            }

            let content = match fs::read_to_string(&candidate.path) {
                Ok(c) => c,
                Err(_) => {
                    skipped_files += 1;
                    continue;
                }
            };

            entries.push(IndexedFile {
                path: candidate.rel,
                terms: extract_terms(&content),
                content,
                modified_unix: candidate.modified_unix,
                size: candidate.size,
            });
            indexed_files += 1;
        }

        Self {
            root: root.to_path_buf(),
            entries,
            indexed_files,
            skipped_files,
            built_unix: unix_now(),
        }
    }
}

fn collect_candidates(
    root: &Path,
    max_files: usize,
    max_file_bytes: u64,
    skipped_files: &mut usize,
) -> Vec<CandidateFile> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let read_dir = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => {
                *skipped_files += 1;
                continue;
            }
        };

        for item in read_dir.flatten() {
            let path = item.path();
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();

            if path.is_dir() {
                if is_ignored_dir(&file_name) {
                    continue;
                }
                stack.push(path);
                continue;
            }

            if out.len() >= max_files || !path.is_file() || !looks_like_text_file(&path) {
                *skipped_files += 1;
                continue;
            }

            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => {
                    *skipped_files += 1;
                    continue;
                }
            };

            if metadata.len() > max_file_bytes {
                *skipped_files += 1;
                continue;
            }

            let rel = path
                .strip_prefix(root)
                .ok()
                .unwrap_or(&path)
                .display()
                .to_string();

            out.push(CandidateFile {
                path,
                rel,
                modified_unix: metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                size: metadata.len(),
            });
        }

        if out.len() >= max_files {
            break;
        }
    }

    out
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".venv"
            | "venv"
            | ".idea"
            | ".vscode"
            | "dist"
            | "build"
    )
}

fn looks_like_text_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();

    if ext.is_empty() {
        return true;
    }

    matches!(
        ext.as_str(),
        "rs" | "py"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "java"
            | "go"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "cs"
            | "rb"
            | "php"
            | "sh"
            | "zsh"
            | "toml"
            | "yaml"
            | "yml"
            | "json"
            | "md"
            | "txt"
            | "sql"
            | "html"
            | "css"
            | "scss"
    )
}

fn extract_terms(input: &str) -> HashSet<String> {
    let mut terms = HashSet::new();
    let mut buf = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            buf.push(ch.to_ascii_lowercase());
        } else if !buf.is_empty() {
            if buf.len() >= 3 {
                terms.insert(buf.clone());
            }
            buf.clear();
        }
    }

    if !buf.is_empty() && buf.len() >= 3 {
        terms.insert(buf);
    }

    terms
}
