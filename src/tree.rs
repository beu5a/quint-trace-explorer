use std::collections::HashSet;

use ratatui::style::Color;

use crate::diff::{DiffKind, DiffResult};

// Display thresholds as percentages of available width
const INLINE_PERCENT: usize = 80;   // Use 80% of available width for inline content
const KEY_PERCENT: usize = 35;      // Map keys get 35% of available width
const VALUE_PERCENT: usize = 45;    // Map values get 45% of available width
const PREVIEW_PERCENT: usize = 70;  // Record previews get 70% of available width

// Indent size in characters
const INDENT_SIZE: usize = 2;

/// Calculate display thresholds based on terminal width and depth
#[derive(Clone, Copy)]
struct DisplayThresholds {
    inline: usize,
    key: usize,
    value: usize,
    preview: usize,
}

impl DisplayThresholds {
    fn new(terminal_width: usize, depth: usize) -> Self {
        let indent_used = depth * INDENT_SIZE;
        let available = terminal_width.saturating_sub(indent_used).max(20);

        Self {
            inline: available * INLINE_PERCENT / 100,
            key: available * KEY_PERCENT / 100,
            value: available * VALUE_PERCENT / 100,
            preview: available * PREVIEW_PERCENT / 100,
        }
    }
}

/// Path to a node in the tree (e.g., ["system", "v1", "state"])
pub type NodePath = Vec<String>;

/// Tracks which nodes are expanded
pub struct ExpansionState {
    expanded: HashSet<NodePath>,
}

impl ExpansionState {
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
        }
    }

    pub fn is_expanded(&self, path: &NodePath) -> bool {
        self.expanded.contains(path)
    }

    pub fn toggle(&mut self, path: &NodePath) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.clone());
        }
    }

    /// Expand all ancestor paths leading to changed nodes
    /// This expands the tree to reveal all changes
    pub fn expand_to_changes(&mut self, changed_paths: &[NodePath]) {
        for path in changed_paths {
            // Expand all ancestor paths (not the leaf itself unless it has children)
            for i in 1..path.len() {
                let ancestor = path[0..i].to_vec();
                self.expanded.insert(ancestor);
            }
            // Also expand the path itself (in case it has nested changes)
            self.expanded.insert(path.clone());
        }
    }

    /// Clear all expansions
    pub fn clear(&mut self) {
        self.expanded.clear();
    }

    /// Expand all given paths
    pub fn expand_all(&mut self, paths: &[NodePath]) {
        for path in paths {
            self.expanded.insert(path.clone());
        }
    }
}

/// A single line in the rendered tree
#[derive(Clone)]
pub struct TreeLine {
    pub path: NodePath,
    pub expandable: bool,
    pub diff: DiffKind,
    pub spans: Vec<StyledSpan>,
}

/// A styled span for syntax highlighting
#[derive(Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: SpanStyle,
}

/// Style types for syntax highlighting
#[derive(Clone, Copy)]
pub enum SpanStyle {
    Default,
    #[allow(dead_code)]
    String,
    #[allow(dead_code)]
    Number,
    #[allow(dead_code)]
    Boolean,
}

impl SpanStyle {
    pub fn to_color(&self) -> Option<Color> {
        match self {
            SpanStyle::Default => None,
            SpanStyle::String => Some(Color::Cyan),
            SpanStyle::Number => Some(Color::Magenta),
            SpanStyle::Boolean => Some(Color::Blue),
        }
    }
}

impl StyledSpan {
    fn new(text: impl Into<String>, style: SpanStyle) -> Self {
        Self { text: text.into(), style }
    }

    fn default(text: impl Into<String>) -> Self {
        Self::new(text, SpanStyle::Default)
    }

    #[allow(dead_code)]
    fn string(text: impl Into<String>) -> Self {
        Self::new(text, SpanStyle::String)
    }

    #[allow(dead_code)]
    fn number(text: impl Into<String>) -> Self {
        Self::new(text, SpanStyle::Number)
    }

    #[allow(dead_code)]
    fn boolean(text: impl Into<String>) -> Self {
        Self::new(text, SpanStyle::Boolean)
    }
}

impl TreeLine {
    /// Create a TreeLine with default (unstyled) spans from text
    fn with_default_spans(path: NodePath, text: String, expandable: bool, diff: DiffKind) -> Self {
        let spans = vec![StyledSpan::default(text)];
        Self { path, expandable, diff, spans }
    }
}

/// Get diff marker for a DiffKind
fn diff_marker(diff: DiffKind) -> &'static str {
    match diff {
        DiffKind::Added => "+ ",
        _ => "",
    }
}

/// Format name prefix: "name: " or "" if name is empty
fn name_prefix(name: &str, diff: DiffKind) -> String {
    let marker = diff_marker(diff);
    if name.is_empty() {
        marker.to_string()
    } else {
        format!("{}{}: ", marker, name)
    }
}

/// Format name prefix with icon: "icon name: " or "icon " if name is empty
fn name_prefix_with_icon(icon: &str, name: &str, diff: DiffKind) -> String {
    let marker = diff_marker(diff);
    if name.is_empty() {
        format!("{}{} ", marker, icon)
    } else {
        format!("{}{} {}: ", marker, icon, name)
    }
}

/// Render an itf::Value into tree lines
pub fn render_value(
    name: &str,
    value: &itf::Value,
    path: NodePath,
    expansion: &ExpansionState,
    diff: &DiffResult,
    depth: usize,
    terminal_width: usize,
) -> Vec<TreeLine> {
    let indent = "  ".repeat(depth);
    let expanded = expansion.is_expanded(&path);
    let diff_kind = diff.get(&path);
    let prefix = name_prefix(name, diff_kind);
    let thresholds = DisplayThresholds::new(terminal_width, depth);

    match value {
        // Leaf values - not expandable
        itf::Value::Bool(b) => {
            let text = format!("{}{}{}", indent, prefix, b);
            vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
        }
        itf::Value::Number(n) => {
            let text = format!("{}{}{}", indent, prefix, n);
            vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
        }
        itf::Value::String(s) => {
            let text = format!("{}{}\"{}\"", indent, prefix, s);
            vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
        }
        itf::Value::BigInt(n) => {
            let text = format!("{}{}{}", indent, prefix, n);
            vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
        }

        // Record - expandable
        itf::Value::Record(fields) => {
            // Check for sum type pattern: {tag: String, value: X}
            if let Some((tag, inner_value)) = detect_sum_type(fields) {
                // Display as Tag(preview of value)
                let inner_preview = format_value_full(inner_value, thresholds.preview)
                    .unwrap_or_else(|| format_value_short(inner_value));

                // Check if inner value can be fully inlined
                let can_inline = format_value_full(inner_value, thresholds.preview).is_some();

                if can_inline {
                    let text = format!("{}{}{}({})", indent, prefix, tag, inner_preview);
                    vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
                } else {
                    let icon = if expanded { "▼" } else { "▶" };
                    let icon_prefix = name_prefix_with_icon(icon, name, diff_kind);
                    let text = format!("{}{}{}({})", indent, icon_prefix, tag, inner_preview);
                    let mut lines = vec![TreeLine::with_default_spans(path.clone(), text, true, diff_kind)];
                    if expanded {
                        // Only show the inner value's contents, skip tag
                        let mut value_path = path.clone();
                        value_path.push("value".to_string());
                        lines.extend(render_value_children(inner_value, value_path, expansion, diff, depth + 1, terminal_width));
                    }
                    lines
                }
            } else if let Some(inline) = format_value_full(value, thresholds.inline) {
                // Small record, show inline without expand
                let text = format!("{}{}{}", indent, prefix, inline);
                vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
            } else {
                // Large record, show preview and allow expand
                let icon = if expanded { "▼" } else { "▶" };
                let icon_prefix = name_prefix_with_icon(icon, name, diff_kind);
                let preview = format_record_preview(fields, thresholds.preview);
                let text = format!("{}{}{}", indent, icon_prefix, preview);
                let mut lines = vec![TreeLine::with_default_spans(path.clone(), text, true, diff_kind)];
                if expanded {
                    for (field_name, field_value) in fields.iter() {
                        let mut child_path = path.clone();
                        child_path.push(field_name.clone());
                        lines.extend(render_value(field_name, field_value, child_path, expansion, diff, depth + 1, terminal_width));
                    }
                }
                lines
            }
        }

        // Map - expandable
        itf::Value::Map(pairs) => {
            if pairs.is_empty() {
                // Empty map, no expand needed
                let text = format!("{}{}Map(empty)", indent, prefix);
                vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
            } else {
                let icon = if expanded { "▼" } else { "▶" };
                let icon_prefix = name_prefix_with_icon(icon, name, diff_kind);
                let text = format!("{}{}Map({} entries)", indent, icon_prefix, pairs.len());
                let mut lines = vec![TreeLine::with_default_spans(path.clone(), text, true, diff_kind)];
                if expanded {
                    for (i, (key, val)) in pairs.iter().enumerate() {
                        // Try to show full key, fall back to short
                        let key_str = format_value_full(key, thresholds.key)
                            .unwrap_or_else(|| format_value_short(key));
                        let mut child_path = path.clone();
                        child_path.push(format!("{}", i));
                        let child_diff = diff.get(&child_path);

                        // Try to format value fully inline
                        let val_full = format_value_full(val, thresholds.value);
                        let can_inline = val_full.is_some();
                        let val_display = val_full.unwrap_or_else(|| format_value_short(val));

                        let marker = diff_marker(child_diff);
                        let entry_text = if can_inline {
                            // Simple value, no icon needed
                            format!("{}  {}{} -> {}", indent, marker, key_str, val_display)
                        } else {
                            // Complex value, show expand icon
                            let entry_icon = if expansion.is_expanded(&child_path) { "▼" } else { "▶" };
                            format!("{}  {}{} {} -> {}", indent, marker, entry_icon, key_str, val_display)
                        };

                        lines.push(TreeLine::with_default_spans(child_path.clone(), entry_text, !can_inline, child_diff));

                        // If value can't be inlined and this entry is expanded, show children
                        if !can_inline && expansion.is_expanded(&child_path) {
                            // Render value's children directly (skip the value's own header)
                            let child_lines = render_value_children(val, child_path, expansion, diff, depth + 2, terminal_width);
                            lines.extend(child_lines);
                        }
                    }
                }
                lines
            }
        }

        // Set - expandable only if content can't be shown inline
        itf::Value::Set(items) => {
            let count = items.iter().count();
            let all_simple = all_simple(items.iter());
            let inline = if all_simple {
                format_collection_inline(items.iter(), "{ ", " }", thresholds.inline)
            } else {
                None
            };

            // If we can show inline, no need for expand/collapse
            if let Some(ref inline_str) = inline {
                let text = format!("{}{}{}", indent, prefix, inline_str);
                vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
            } else {
                // Complex or too long - needs expand/collapse
                let icon = if expanded { "▼" } else { "▶" };
                let icon_prefix = name_prefix_with_icon(icon, name, diff_kind);
                let text = format!("{}{}Set({} items)", indent, icon_prefix, count);
                let mut lines = vec![TreeLine::with_default_spans(path.clone(), text, true, diff_kind)];

                if expanded {
                    // Show each item without index (sets are unordered)
                    for (i, item) in items.iter().enumerate() {
                        let mut child_path = path.clone();
                        child_path.push(format!("{}", i));
                        lines.extend(render_value("", item, child_path, expansion, diff, depth + 1, terminal_width));
                    }
                }
                lines
            }
        }

        // List - expandable only if content can't be shown inline
        itf::Value::List(items) => {
            let all_simple = all_simple(items.iter());
            let inline = if all_simple {
                format_collection_inline(items.iter(), "[", "]", thresholds.inline)
            } else {
                None
            };

            // If we can show inline, no need for expand/collapse
            if let Some(ref inline_str) = inline {
                let text = format!("{}{}{}", indent, prefix, inline_str);
                vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
            } else {
                // Complex or too long - needs expand/collapse
                let icon = if expanded { "▼" } else { "▶" };
                let icon_prefix = name_prefix_with_icon(icon, name, diff_kind);
                let text = format!("{}{}List({} items)", indent, icon_prefix, items.len());
                let mut lines = vec![TreeLine::with_default_spans(path.clone(), text, true, diff_kind)];

                if expanded {
                    // Lists keep indexes since order matters
                    for (i, item) in items.iter().enumerate() {
                        let mut child_path = path.clone();
                        child_path.push(format!("{}", i));
                        lines.extend(render_value(&format!("[{}]", i), item, child_path, expansion, diff, depth + 1, terminal_width));
                    }
                }
                lines
            }
        }

        // Tuple - expandable
        itf::Value::Tuple(items) => {
            let icon = if expanded { "▼" } else { "▶" };
            let icon_prefix = name_prefix_with_icon(icon, name, diff_kind);
            let text = format!("{}{}Tuple({} items)", indent, icon_prefix, items.len());
            let mut lines = vec![TreeLine::with_default_spans(path.clone(), text, true, diff_kind)];
            if expanded {
                for (i, item) in items.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(format!("{}", i));
                    lines.extend(render_value(&format!("[{}]", i), item, child_path, expansion, diff, depth + 1, terminal_width));
                }
            }
            lines
        }

        itf::Value::Unserializable(u) => {
            let text = format!("{}{}{:?}", indent, prefix, u);
            vec![TreeLine::with_default_spans(path, text, false, diff_kind)]
        }
    }
}

/// Render just the children of a value (without the header line)
/// Used when expanding map entries where the header is already shown
fn render_value_children(
    value: &itf::Value,
    path: NodePath,
    expansion: &ExpansionState,
    diff: &DiffResult,
    depth: usize,
    terminal_width: usize,
) -> Vec<TreeLine> {
    let thresholds = DisplayThresholds::new(terminal_width, depth);

    match value {
        itf::Value::Record(fields) => {
            let mut lines = Vec::new();
            for (field_name, field_value) in fields.iter() {
                let mut field_path = path.clone();
                field_path.push(field_name.clone());
                lines.extend(render_value(field_name, field_value, field_path, expansion, diff, depth, terminal_width));
            }
            lines
        }
        itf::Value::Set(items) => {
            let mut lines = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let mut item_path = path.clone();
                item_path.push(format!("{}", i));
                lines.extend(render_value("", item, item_path, expansion, diff, depth, terminal_width));
            }
            lines
        }
        itf::Value::List(items) => {
            let mut lines = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let mut item_path = path.clone();
                item_path.push(format!("{}", i));
                lines.extend(render_value(&format!("[{}]", i), item, item_path, expansion, diff, depth, terminal_width));
            }
            lines
        }
        itf::Value::Map(pairs) => {
            let mut lines = Vec::new();
            let indent = "  ".repeat(depth);
            for (i, (k, v)) in pairs.iter().enumerate() {
                // Try to show full key, fall back to short
                let k_str = format_value_full(k, thresholds.key)
                    .unwrap_or_else(|| format_value_short(k));
                let v_full = format_value_full(v, thresholds.value);
                let can_inline = v_full.is_some();
                let v_display = v_full.unwrap_or_else(|| format_value_short(v));
                let mut entry_path = path.clone();
                entry_path.push(format!("{}", i));
                let entry_diff = diff.get(&entry_path);
                let marker = diff_marker(entry_diff);

                let text = format!("{}{}{} -> {}", indent, marker, k_str, v_display);
                lines.push(TreeLine::with_default_spans(entry_path.clone(), text, !can_inline, entry_diff));

                if !can_inline && expansion.is_expanded(&entry_path) {
                    lines.extend(render_value_children(v, entry_path, expansion, diff, depth + 1, terminal_width));
                }
            }
            lines
        }
        itf::Value::Tuple(items) => {
            let mut lines = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let mut item_path = path.clone();
                item_path.push(format!("{}", i));
                lines.extend(render_value(&format!("[{}]", i), item, item_path, expansion, diff, depth, terminal_width));
            }
            lines
        }
        // Simple values have no children
        _ => Vec::new(),
    }
}

/// Detect sum type pattern: {tag: String, value: X}
/// Returns (tag_value, inner_value) if detected
fn detect_sum_type(fields: &itf::value::Record) -> Option<(&str, &itf::Value)> {
    // Must have exactly 2 fields: "tag" and "value"
    if fields.len() != 2 {
        return None;
    }

    let tag_value = fields.get("tag")?;
    let inner_value = fields.get("value")?;

    // tag must be a string
    if let itf::Value::String(tag_str) = tag_value {
        Some((tag_str.as_str(), inner_value))
    } else {
        None
    }
}

/// Format a preview of a record showing first few fields
fn format_record_preview(fields: &itf::value::Record, max_len: usize) -> String {
    if fields.is_empty() {
        return "{ }".to_string();
    }

    let mut parts = Vec::new();
    let mut total_len = 4; // for "{ " and " }"

    for (key, val) in fields.iter() {
        let val_short = format_value_short(val);
        let part = format!("{}: {}", key, val_short);

        if total_len + part.len() + 2 > max_len && !parts.is_empty() {
            // Would exceed max, stop and add ellipsis
            parts.push("...".to_string());
            break;
        }

        total_len += part.len() + 2; // +2 for ", "
        parts.push(part);
    }

    format!("{{ {} }}", parts.join(", "))
}

/// Short format for map keys
fn format_value_short(value: &itf::Value) -> String {
    match value {
        itf::Value::Bool(b) => b.to_string(),
        itf::Value::Number(n) => n.to_string(),
        itf::Value::String(s) => format!("\"{}\"", s),
        itf::Value::BigInt(n) => n.to_string(),
        itf::Value::Record(_) => "{ ... }".to_string(),
        itf::Value::Map(_) => "Map(...)".to_string(),
        itf::Value::Set(_) => "{ ... }".to_string(),
        itf::Value::List(_) => "[...]".to_string(),
        itf::Value::Tuple(_) => "(...)".to_string(),
        itf::Value::Unserializable(_) => "<?>".to_string(),
    }
}

/// Check if a value is "simple" (can be shown inline)
fn is_simple(value: &itf::Value) -> bool {
    matches!(
        value,
        itf::Value::Bool(_)
            | itf::Value::Number(_)
            | itf::Value::String(_)
            | itf::Value::BigInt(_)
    )
}

/// Format a value fully (not just preview) - returns None if too complex/long
fn format_value_full(value: &itf::Value, max_len: usize) -> Option<String> {
    let result = match value {
        itf::Value::Bool(b) => b.to_string(),
        itf::Value::Number(n) => n.to_string(),
        itf::Value::String(s) => format!("\"{}\"", s),
        itf::Value::BigInt(n) => n.to_string(),
        itf::Value::Record(fields) => {
            if fields.is_empty() {
                "{ }".to_string()
            } else {
                let parts: Vec<String> = fields
                    .iter()
                    .filter_map(|(k, v)| {
                        format_value_full(v, max_len).map(|fv| format!("{}: {}", k, fv))
                    })
                    .collect();
                if parts.len() != fields.len() {
                    return None; // Some field couldn't be formatted
                }
                format!("{{ {} }}", parts.join(", "))
            }
        }
        itf::Value::Set(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| format_value_full(v, max_len))
                .collect();
            if parts.len() != items.iter().count() {
                return None;
            }
            format!("{{ {} }}", parts.join(", "))
        }
        itf::Value::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| format_value_full(v, max_len))
                .collect();
            if parts.len() != items.len() {
                return None;
            }
            format!("[{}]", parts.join(", "))
        }
        itf::Value::Map(pairs) => {
            if pairs.is_empty() {
                "Map()".to_string()
            } else {
                let parts: Vec<String> = pairs
                    .iter()
                    .filter_map(|(k, v)| {
                        let fk = format_value_full(k, max_len)?;
                        let fv = format_value_full(v, max_len)?;
                        Some(format!("{} -> {}", fk, fv))
                    })
                    .collect();
                if parts.len() != pairs.len() {
                    return None;
                }
                format!("Map({})", parts.join(", "))
            }
        }
        itf::Value::Tuple(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| format_value_full(v, max_len))
                .collect();
            if parts.len() != items.len() {
                return None;
            }
            format!("({})", parts.join(", "))
        }
        itf::Value::Unserializable(_) => return None,
    };

    if result.len() <= max_len {
        Some(result)
    } else {
        None
    }
}

/// Format a collection inline: {a, b, c} or [1, 2, 3]
fn format_collection_inline<'a>(
    items: impl Iterator<Item = &'a itf::Value>,
    open: &str,
    close: &str,
    max_len: usize,
) -> Option<String> {
    let formatted: Vec<String> = items.map(format_value_short).collect();
    let joined = formatted.join(", ");

    // Only inline if total length is reasonable
    if joined.len() <= max_len {
        Some(format!("{}{}{}", open, joined, close))
    } else {
        None
    }
}

/// Check if all items in a collection are simple
fn all_simple<'a>(items: impl Iterator<Item = &'a itf::Value>) -> bool {
    items.fold(true, |acc, v| acc && is_simple(v))
}
