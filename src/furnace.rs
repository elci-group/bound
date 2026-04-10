//! furnace.rs
//! Placeholder for Furnace integration: richer contextual analysis of files.

use std::path::Path;
use serde::Serialize;
use crate::expandable::ExpandableBlock;
use crate::metadata::FileMetadata;

/// Represents a structured Furnace report
#[derive(Debug, Clone, Serialize)]
pub struct FurnaceReport {
    pub file_meta: FileMetadata,
    pub notes: Vec<String>,
    pub complexity: Option<f64>, // placeholder for computed metric
}

impl FurnaceReport {
    /// Render the report as an expandable{} block
    pub fn render(&self) -> String {
        let mut content = String::new();
        content.push_str(&format!("Notes:\n"));
        for note in &self.notes {
            content.push_str(&format!("- {}\n", note));
        }
        if let Some(c) = self.complexity {
            content.push_str(&format!("Complexity: {:.2}\n", c));
        }

        ExpandableBlock::new("furnace", &content)
            .add_attr("file", &self.file_meta.relative_path)
            .add_attr("size_bytes", &self.file_meta.size_bytes.to_string())
            .render()
    }
}

/// Analyze a file (stub implementation)
pub fn analyze_file(_path: &Path, meta: &FileMetadata) -> FurnaceReport {
    // Placeholder: real logic will compute structural notes, complexity, etc.
    let notes = vec![
        "Stub analysis: content not yet analyzed.".to_string()
    ];

    FurnaceReport {
        file_meta: meta.clone(),
        notes,
        complexity: Some(0.0),
    }
}
