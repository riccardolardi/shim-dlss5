//! One bounded walk of a game folder, shared by every analyser.
//!
//! The tree is collected once (depth-limited, known junk folders skipped) and
//! then queried in memory, so analysing a game touches the disk once for
//! directory listings no matter how many questions are asked of it.

use std::path::{Path, PathBuf};

/// Folders that never hold the game executable or anything we look for.
const SKIP_DIRS: &[&str] = &[
    "_commonredist",
    "commonredist",
    "redist",
    "redists",
    "directx",
    "dotnet",
    "vcredist",
    "prerequisites",
    "__installer",
    "installers",
    "support",
    "crashreportclient",
    "crashreporter",
    "crashreport",
    ".egstore",
    "saved",
    "logs",
    "screenshots",
    "soundtrack",
    "ost",
    "artbook",
    "manual",
    "extras",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    /// Path relative to the root, `/`-separated and lowercased, for matching.
    pub rel: String,
    /// File name, lowercased.
    pub name: String,
    pub depth: usize,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub root: PathBuf,
    pub files: Vec<Entry>,
    pub dirs: Vec<Entry>,
}

impl Tree {
    /// Breadth-first, at most `max_depth` levels below `root` (0 = root only).
    /// Unreadable folders are skipped and logged, never fatal.
    pub fn collect(root: &Path, max_depth: usize) -> Self {
        let mut tree = Tree {
            root: root.to_path_buf(),
            ..Default::default()
        };
        let mut queue = vec![(root.to_path_buf(), 0usize)];
        while let Some((dir, depth)) = queue.pop() {
            let read = match std::fs::read_dir(&dir) {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!(dir = %dir.display(), %e, "folder skipped");
                    continue;
                }
            };
            for entry in read.filter_map(|e| e.ok()) {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else { continue };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let rel = relative(root, &path);
                if meta.is_dir() {
                    tree.dirs.push(Entry {
                        path: path.clone(),
                        rel,
                        name: name.clone(),
                        depth: depth + 1,
                        size: 0,
                    });
                    if depth < max_depth && !SKIP_DIRS.contains(&name.as_str()) {
                        queue.push((path, depth + 1));
                    }
                } else if meta.is_file() {
                    tree.files.push(Entry {
                        path,
                        rel,
                        name,
                        depth: depth + 1,
                        size: meta.len(),
                    });
                }
            }
        }
        tree.files.sort_by(|a, b| a.rel.cmp(&b.rel));
        tree.dirs.sort_by(|a, b| a.rel.cmp(&b.rel));
        tree
    }

    pub fn has_file(&self, name_lower: &str) -> bool {
        self.files.iter().any(|f| f.name == name_lower)
    }

    pub fn has_dir(&self, name_lower: &str) -> bool {
        self.dirs.iter().any(|d| d.name == name_lower)
    }

    pub fn files_where<'a>(
        &'a self,
        pred: impl Fn(&Entry) -> bool + 'a,
    ) -> impl Iterator<Item = &'a Entry> + 'a {
        self.files.iter().filter(move |f| pred(f))
    }

    pub fn find_file(&self, name_lower: &str) -> Option<&Entry> {
        self.files.iter().find(|f| f.name == name_lower)
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

#[cfg(test)]
pub mod testing {
    use std::path::Path;

    /// Create every listed file (empty, or with the given bytes), making folders.
    pub fn make_tree(root: &Path, files: &[(&str, &[u8])]) {
        for (rel, bytes) in files {
            let path = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, bytes).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::make_tree;

    #[test]
    fn collects_files_and_dirs_with_depth_and_lowercase_rel() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(
            tmp.path(),
            &[
                ("Game.exe", b"x"),
                ("Binaries/Win64/Game-Win64-Shipping.exe", b"xy"),
                ("_CommonRedist/vcredist/vc.exe", b""),
                ("Engine/Binaries/ThirdParty/x/y/deep.dll", b""),
            ],
        );
        let tree = Tree::collect(tmp.path(), 5);
        let rels: Vec<&str> = tree.files.iter().map(|f| f.rel.as_str()).collect();
        assert!(rels.contains(&"game.exe"));
        assert!(rels.contains(&"binaries/win64/game-win64-shipping.exe"));
        assert!(!rels.iter().any(|r| r.contains("vcredist")), "{rels:?}");
        assert!(
            tree.has_dir("_commonredist"),
            "skipped dirs are still listed"
        );
        let shipping = tree.find_file("game-win64-shipping.exe").unwrap();
        assert_eq!(shipping.depth, 3);
        assert_eq!(shipping.size, 2);
        assert!(tree.has_file("deep.dll"));
    }

    #[test]
    fn depth_limit_is_respected() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(tmp.path(), &[("a/b/c/d.txt", b"")]);
        assert!(Tree::collect(tmp.path(), 3).has_file("d.txt"));
        assert!(!Tree::collect(tmp.path(), 2).has_file("d.txt"));
    }

    #[test]
    fn missing_root_gives_an_empty_tree() {
        let tree = Tree::collect(Path::new("Z:/definitely/not/here"), 2);
        assert!(tree.files.is_empty() && tree.dirs.is_empty());
    }
}
