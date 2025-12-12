use std::io;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, MouseEventKind, MouseButton, EnableMouseCapture, DisableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;

use crate::diff::{compute_diff, DiffKind, DiffResult};
use crate::loader::Trace;
use crate::tree::{ExpansionState, TreeLine, render_value};

/// Application state
pub struct App {
    pub trace: Trace,
    pub current_state: usize,
    pub should_quit: bool,
    pub expansion: ExpansionState,
    pub cursor: usize,  // Which line is selected
    pub scroll_offset: usize,  // First visible line
    pub auto_expand: bool,  // Auto-expand changed variables on state navigation
}

impl App {
    pub fn new(trace: Trace, auto_expand: bool) -> Self {
        Self {
            trace,
            current_state: 0,
            should_quit: false,
            expansion: ExpansionState::new(),
            cursor: 0,
            scroll_offset: 0,
            auto_expand,
        }
    }

    /// Ensure cursor is visible within the viewport
    pub fn ensure_cursor_visible(&mut self, viewport_height: usize) {
        // Keep some padding at top/bottom
        let padding = 2;

        if self.cursor < self.scroll_offset + padding {
            // Cursor is above viewport
            self.scroll_offset = self.cursor.saturating_sub(padding);
        } else if self.cursor >= self.scroll_offset + viewport_height - padding {
            // Cursor is below viewport
            self.scroll_offset = self.cursor.saturating_sub(viewport_height - padding - 1);
        }
    }
}

/// Run the TUI application
pub fn run(trace: Trace, auto_expand: bool) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut app = App::new(trace, auto_expand);

    // Event loop
    while !app.should_quit {
        // Get terminal dimensions
        let terminal_size = terminal.size()?;
        let terminal_width = terminal_size.width as usize;
        let terminal_height = terminal_size.height as usize;
        // Viewport height = terminal height - header (1 line) - blank line (1 line)
        let viewport_height = terminal_height.saturating_sub(2);

        // Compute diff with previous state
        let diff = compute_diff_for_state(&app);

        // Build tree lines for current state
        let tree_lines = build_tree_lines(&app, &diff, terminal_width);
        let line_count = tree_lines.len();

        // Ensure cursor stays within bounds
        if app.cursor >= line_count && line_count > 0 {
            app.cursor = line_count - 1;
        }

        // Ensure cursor is visible in viewport
        app.ensure_cursor_visible(viewport_height);

        terminal.draw(|f| render(f, &app, &tree_lines, viewport_height))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Left => {
                        if app.current_state > 0 {
                            app.current_state -= 1;
                            app.cursor = 0;
                            app.scroll_offset = 0;
                            if app.auto_expand {
                                auto_expand_changes(&mut app);
                            }
                        }
                    }
                    KeyCode::Right => {
                        if app.current_state + 1 < app.trace.states.len() {
                            app.current_state += 1;
                            app.cursor = 0;
                            app.scroll_offset = 0;
                            if app.auto_expand {
                                auto_expand_changes(&mut app);
                            }
                        }
                    }
                    KeyCode::Up => {
                        if app.cursor > 0 {
                            app.cursor -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if app.cursor + 1 < line_count {
                            app.cursor += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        // Move cursor up by viewport height
                        app.cursor = app.cursor.saturating_sub(viewport_height.saturating_sub(2));
                    }
                    KeyCode::PageDown => {
                        // Move cursor down by viewport height
                        app.cursor = (app.cursor + viewport_height.saturating_sub(2)).min(line_count.saturating_sub(1));
                    }
                    KeyCode::Home => {
                        app.cursor = 0;
                    }
                    KeyCode::End => {
                        if line_count > 0 {
                            app.cursor = line_count - 1;
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(line) = tree_lines.get(app.cursor) {
                            if line.expandable {
                                app.expansion.toggle(&line.path);
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        // Collapse all
                        app.expansion.clear();
                    }
                    KeyCode::Char('e') => {
                        // Expand all expandable nodes
                        let expandable_paths: Vec<_> = tree_lines
                            .iter()
                            .filter(|l| l.expandable)
                            .map(|l| l.path.clone())
                            .collect();
                        app.expansion.expand_all(&expandable_paths);
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        // Convert screen row to tree line index
                        // Row 0 = header, Row 1 = blank line, Row 2+ = tree content
                        let row = mouse.row as usize;
                        if row >= 2 {
                            let clicked_line = app.scroll_offset + (row - 2);
                            if clicked_line < line_count {
                                // Select the line
                                app.cursor = clicked_line;
                                // Toggle expand if expandable
                                if let Some(line) = tree_lines.get(clicked_line) {
                                    if line.expandable {
                                        app.expansion.toggle(&line.path);
                                    }
                                }
                            }
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        // Scroll up (move view up = show earlier content)
                        app.scroll_offset = app.scroll_offset.saturating_sub(3);
                        // Keep cursor in view
                        if app.cursor >= app.scroll_offset + viewport_height {
                            app.cursor = (app.scroll_offset + viewport_height).saturating_sub(1);
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        // Scroll down (move view down = show later content)
                        let max_scroll = line_count.saturating_sub(viewport_height);
                        app.scroll_offset = (app.scroll_offset + 3).min(max_scroll);
                        // Keep cursor in view
                        if app.cursor < app.scroll_offset {
                            app.cursor = app.scroll_offset;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Cleanup
    io::stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Compute diff between current state and previous state
fn compute_diff_for_state(app: &App) -> DiffResult {
    use std::collections::HashMap;

    if app.current_state == 0 {
        // First state - no diff
        return DiffResult { changes: HashMap::new() };
    }

    let prev = &app.trace.states[app.current_state - 1].values;
    let curr = &app.trace.states[app.current_state].values;
    compute_diff(prev, curr)
}

/// Auto-expand the tree to reveal all changes in the current state
fn auto_expand_changes(app: &mut App) {
    // Clear previous expansions and expand to current changes
    app.expansion.clear();

    let diff = compute_diff_for_state(app);
    let changed_paths: Vec<_> = diff.changes.keys().cloned().collect();
    app.expansion.expand_to_changes(&changed_paths);
}

/// Build tree lines for the current state
fn build_tree_lines(app: &App, diff: &DiffResult, terminal_width: usize) -> Vec<TreeLine> {
    let mut tree_lines = Vec::new();
    if let Some(state) = app.trace.states.get(app.current_state) {
        for (name, value) in &state.values {
            let path = vec![name.clone()];
            tree_lines.extend(render_value(name, value, path, &app.expansion, diff, 0, terminal_width));
        }
    }
    tree_lines
}

fn render(frame: &mut Frame, app: &App, tree_lines: &[TreeLine], viewport_height: usize) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    let auto_indicator = if app.auto_expand { " [auto-expand]" } else { "" };
    let header_style = Style::default()
        .bg(Color::Indexed(56))  // Pastel purple
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    // Build scroll indicator
    let total_lines = tree_lines.len();
    let scroll_info = if total_lines > viewport_height {
        format!(" [{}-{}/{}]", app.scroll_offset + 1, (app.scroll_offset + viewport_height).min(total_lines), total_lines)
    } else {
        String::new()
    };

    // Build header with padding to fill width
    let header_text = format!(
        " State {}/{}{}{} | ←/→ state | ↑/↓/PgUp/PgDn cursor | Enter toggle | e/c expand/collapse | q quit ",
        app.current_state + 1,
        app.trace.states.len(),
        auto_indicator,
        scroll_info
    );
    let header = Line::from(Span::styled(header_text, header_style));

    let mut lines: Vec<Line> = vec![header, Line::from("")];

    // Only render visible lines based on scroll offset
    let visible_lines = tree_lines
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(viewport_height);

    // Render tree lines with cursor highlighting, diff colors, and syntax highlighting
    for (i, tree_line) in visible_lines {
        let is_selected = i == app.cursor;
        let bg_color = if is_selected { Some(Color::DarkGray) } else { None };

        // Get base diff color
        let diff_color = match tree_line.diff {
            DiffKind::Added => Some(Color::Green),
            DiffKind::Removed => Some(Color::Red),
            DiffKind::Modified => Some(Color::Yellow),
            DiffKind::Unchanged => None,
        };

        // Build styled spans
        let styled_spans: Vec<Span> = tree_line.spans.iter().map(|span| {
            // Syntax color takes precedence for unchanged items, diff color for changed
            let fg_color = if diff_color.is_some() {
                diff_color
            } else {
                span.style.to_color()
            };

            let mut style = Style::default();
            if let Some(fg) = fg_color {
                style = style.fg(fg);
            }
            if let Some(bg) = bg_color {
                style = style.bg(bg);
            }
            Span::styled(&span.text, style)
        }).collect();

        lines.push(Line::from(styled_spans));
    }

    let paragraph = ratatui::widgets::Paragraph::new(lines);
    frame.render_widget(paragraph, frame.area());
}
