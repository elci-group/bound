//! expandable.rs
//! Provides helper functions to wrap content in expandable{} blocks.

use std::collections::HashMap;

/// Represents a generic expandable block
pub struct ExpandableBlock {
    pub tag: String,
    pub attributes: HashMap<String, String>,
    pub content: String,
}

impl ExpandableBlock {
    /// Create a new expandable block
    pub fn new(tag: &str, content: &str) -> Self {
        ExpandableBlock {
            tag: tag.to_string(),
            attributes: HashMap::new(),
            content: content.to_string(),
        }
    }

    /// Add a key-value attribute
    pub fn add_attr(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }

    /// Render the block as a string
    pub fn render(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("expandable{{"));
        lines.push(format!("  type: {}", self.tag));

        for (k, v) in &self.attributes {
            lines.push(format!("  {}: {}", k, v));
        }

        if !self.content.is_empty() {
            lines.push("  content: |".to_string());
            for line in self.content.lines() {
                lines.push(format!("    {}", line));
            }
        }

        lines.push("}".to_string());
        lines.join("\n")
    }
}

/// Convenience function: wrap content in a simple expandable block
pub fn wrap_expandable(tag: &str, content: &str) -> String {
    ExpandableBlock::new(tag, content).render()
}
