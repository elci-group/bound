
//! tree.rs
//! Generates a directory-style tree representation of a list of files.

use std::path::{Path, PathBuf};

/// Generates a textual tree from a list of files relative to a root
pub fn generate_tree(root: &Path, files: &[PathBuf]) -> String {
    // Collect relative paths and sort
    let mut rel_paths: Vec<PathBuf> = files
        .iter()
        .map(|p| p.strip_prefix(root).unwrap_or(p).to_path_buf())
        .collect();
    rel_paths.sort();

    let mut tree_lines = Vec::new();
    let mut last_components: Vec<String> = Vec::new();

    for path in rel_paths {
        let components: Vec<String> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();

        // Find common prefix with last path
        let mut common = 0;
        while common < components.len() && common < last_components.len() {
            if components[common] != last_components[common] {
                break;
            }
            common += 1;
        }

        // Print new components
        for comp in &components[common..] {
            let indent = "    ".repeat(common);
            tree_lines.push(format!("{}{}", indent, comp));
            common += 1;
        }

        last_components = components;
    }

    format!("🌳 PROJECT TREE (root: {})\n\n{}\n", root.display(), tree_lines.join("\n"))
}
