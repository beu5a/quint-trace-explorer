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

/// Which panel is focused in diff mode
#[derive(Clone, Copy, PartialEq)]
pub enum DiffFocus {
    Left,
    Right,
}

/// View mode for the application
#[derive(Clone, Copy, PartialEq)]
pub enum ViewMode {
    Single,
    Diff { left: usize, right: usize, focus: DiffFocus },
}

/// Application state
pub struct App {
    pub trace: Trace,
    pub current_state: usize,
    pub should_quit: bool,
    pub expansion: ExpansionState,
    pub cursor: usize,  // Which line is selected
    pub scroll_offset: usize,  // First visible line
    pub auto_expand: bool,  // Auto-expand changed variables on state navigation
    pub view_mode: ViewMode,
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
            view_mode: ViewMode::Single,
        }
    }

    /// Enter diff mode comparing current state with previous
    pub fn enter_diff_mode(&mut self) {
        let right = self.current_state;
        let left = if right > 0 { right - 1 } else { 0 };
        self.view_mode = ViewMode::Diff { left, right, focus: DiffFocus::Right };
    }

    /// Exit diff mode
    pub fn exit_diff_mode(&mut self) {
        if let ViewMode::Diff { right, .. } = self.view_mode {
            self.current_state = right;
        }
        self.view_mode = ViewMode::Single;
    }

    /// Toggle focus in diff mode
    pub fn toggle_diff_focus(&mut self) {
        if let ViewMode::Diff { left, right, focus } = self.view_mode {
            let new_focus = match focus {
                DiffFocus::Left => DiffFocus::Right,
                DiffFocus::Right => DiffFocus::Left,
            };
            self.view_mode = ViewMode::Diff { left, right, focus: new_focus };
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
        // Viewport height depends on view mode
        // Single: terminal height - header (1) - blank line (1)
        // Diff: terminal height - header (1) - blank line (1) - panel borders (2)
        let viewport_height = match app.view_mode {
            ViewMode::Single => terminal_height.saturating_sub(2),
            ViewMode::Diff { .. } => terminal_height.saturating_sub(4),
        };

        // Build tree lines based on view mode
        // tree_lines: used for cursor navigation and Enter toggle
        // all_expandable_paths: used for expand_all (includes both panels in diff mode)
        let (tree_lines, line_count, all_expandable_paths) = match app.view_mode {
            ViewMode::Single => {
                let diff = compute_diff_for_state(&app);
                let lines = build_tree_lines(&app, &diff, terminal_width);
                let count = lines.len();
                let paths: Vec<_> = lines.iter()
                    .filter(|l| l.expandable)
                    .map(|l| l.path.clone())
                    .collect();
                (lines, count, paths)
            }
            ViewMode::Diff { left, right, focus } => {
                // In diff mode, use focused panel's lines for navigation
                let empty_diff = DiffResult { changes: std::collections::HashMap::new() };
                let panel_width = terminal_width / 2;
                let left_lines = build_tree_lines_for_state(&app.trace, left, &app.expansion, &empty_diff, panel_width);
                let right_lines = build_tree_lines_for_state(&app.trace, right, &app.expansion, &empty_diff, panel_width);

                // Use focused panel for cursor navigation
                let focused_lines = match focus {
                    DiffFocus::Left => left_lines.clone(),
                    DiffFocus::Right => right_lines.clone(),
                };
                let count = focused_lines.len();

                // Combine paths from both panels for expand_all
                let mut all_paths: Vec<_> = left_lines.iter()
                    .filter(|l| l.expandable)
                    .map(|l| l.path.clone())
                    .collect();
                all_paths.extend(right_lines.iter()
                    .filter(|l| l.expandable)
                    .map(|l| l.path.clone()));

                (focused_lines, count, all_paths)
            }
        };

        // Ensure cursor stays within bounds
        if app.cursor >= line_count && line_count > 0 {
            app.cursor = line_count - 1;
        }

        // Ensure cursor is visible in viewport
        app.ensure_cursor_visible(viewport_height);

        let mut header_layout = HeaderLayout {
            prev_start: 0, prev_end: 0,
            next_start: 0, next_end: 0,
            expand_start: 0, expand_end: 0,
            collapse_start: 0, collapse_end: 0,
            diff_start: 0, diff_end: 0,
        };
        terminal.draw(|f| {
            header_layout = match app.view_mode {
                ViewMode::Single => render(f, &app, &tree_lines, viewport_height),
                ViewMode::Diff { left, right, focus } => render_diff(f, &app, left, right, focus, viewport_height),
            };
        })?;

        let event_context = EventContext {
            tree_lines: &tree_lines,
            all_expandable_paths: &all_expandable_paths,
            line_count,
            viewport_height,
            terminal_width,
            header_layout: &header_layout,
        };

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                handle_key_event(&mut app, key.code, &event_context);
            }
            Event::Mouse(mouse) => {
                handle_mouse_event(&mut app, mouse, &event_context);
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

/// Context needed for event handling
struct EventContext<'a> {
    tree_lines: &'a [TreeLine],
    all_expandable_paths: &'a [Vec<String>],
    line_count: usize,
    viewport_height: usize,
    terminal_width: usize,
    header_layout: &'a HeaderLayout,
}

/// Handle keyboard events
fn handle_key_event(app: &mut App, key: KeyCode, ctx: &EventContext) {
    match app.view_mode {
        ViewMode::Single => handle_single_mode_key(app, key, ctx),
        ViewMode::Diff { .. } => handle_diff_mode_key(app, key, ctx),
    }
}

/// Handle keyboard events in single view mode
fn handle_single_mode_key(app: &mut App, key: KeyCode, ctx: &EventContext) {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('d') => app.enter_diff_mode(),
        KeyCode::Left => handle_prev_state(app),
        KeyCode::Right => handle_next_state(app),
        KeyCode::Up => {
            if app.cursor > 0 {
                app.cursor -= 1;
            }
        }
        KeyCode::Down => {
            if app.cursor + 1 < ctx.line_count {
                app.cursor += 1;
            }
        }
        KeyCode::PageUp => {
            app.cursor = app.cursor.saturating_sub(ctx.viewport_height.saturating_sub(2));
        }
        KeyCode::PageDown => {
            app.cursor = (app.cursor + ctx.viewport_height.saturating_sub(2)).min(ctx.line_count.saturating_sub(1));
        }
        KeyCode::Home => {
            app.cursor = 0;
        }
        KeyCode::End => {
            if ctx.line_count > 0 {
                app.cursor = ctx.line_count - 1;
            }
        }
        KeyCode::Enter => {
            if let Some(line) = ctx.tree_lines.get(app.cursor) {
                if line.expandable {
                    app.expansion.toggle(&line.path);
                }
            }
        }
        KeyCode::Char('c') => {
            app.expansion.clear();
        }
        KeyCode::Char('e') => {
            app.expansion.expand_all(ctx.all_expandable_paths);
        }
        _ => {}
    }
}

/// Handle keyboard events in diff view mode
fn handle_diff_mode_key(app: &mut App, key: KeyCode, ctx: &EventContext) {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('d') => app.exit_diff_mode(),
        KeyCode::Tab => app.toggle_diff_focus(),
        KeyCode::Left => handle_prev_state(app),
        KeyCode::Right => handle_next_state(app),
        KeyCode::Up => {
            if app.cursor > 0 {
                app.cursor -= 1;
            }
        }
        KeyCode::Down => {
            if app.cursor + 1 < ctx.line_count {
                app.cursor += 1;
            }
        }
        KeyCode::PageUp => {
            app.cursor = app.cursor.saturating_sub(ctx.viewport_height.saturating_sub(4));
        }
        KeyCode::PageDown => {
            app.cursor = (app.cursor + ctx.viewport_height.saturating_sub(4)).min(ctx.line_count.saturating_sub(1));
        }
        KeyCode::Home => {
            app.cursor = 0;
        }
        KeyCode::End => {
            if ctx.line_count > 0 {
                app.cursor = ctx.line_count - 1;
            }
        }
        KeyCode::Enter => {
            if let Some(line) = ctx.tree_lines.get(app.cursor) {
                if line.expandable {
                    app.expansion.toggle(&line.path);
                }
            }
        }
        KeyCode::Char('c') => {
            app.expansion.clear();
        }
        KeyCode::Char('e') => {
            app.expansion.expand_all(ctx.all_expandable_paths);
        }
        _ => {}
    }
}

/// Handle mouse events
fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent, ctx: &EventContext) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let row = mouse.row as usize;
            let col = mouse.column as usize;

            if row == 0 {
                // Header button clicks
                let layout = ctx.header_layout;
                if col >= layout.diff_start && col < layout.diff_end {
                    match app.view_mode {
                        ViewMode::Single => app.enter_diff_mode(),
                        ViewMode::Diff { .. } => app.exit_diff_mode(),
                    }
                } else if col >= layout.prev_start && col < layout.prev_end {
                    handle_prev_state(app);
                } else if col >= layout.next_start && col < layout.next_end {
                    handle_next_state(app);
                } else if col >= layout.expand_start && col < layout.expand_end {
                    app.expansion.expand_all(ctx.all_expandable_paths);
                } else if col >= layout.collapse_start && col < layout.collapse_end {
                    app.expansion.clear();
                }
            } else if row >= 2 {
                handle_content_click(app, row, col, ctx);
            }
        }
        MouseEventKind::ScrollUp => {
            app.scroll_offset = app.scroll_offset.saturating_sub(3);
            if app.view_mode == ViewMode::Single {
                if app.cursor >= app.scroll_offset + ctx.viewport_height {
                    app.cursor = (app.scroll_offset + ctx.viewport_height).saturating_sub(1);
                }
            }
        }
        MouseEventKind::ScrollDown => {
            let max_scroll = ctx.line_count.saturating_sub(ctx.viewport_height);
            app.scroll_offset = (app.scroll_offset + 3).min(max_scroll);
            if app.view_mode == ViewMode::Single {
                if app.cursor < app.scroll_offset {
                    app.cursor = app.scroll_offset;
                }
            }
        }
        _ => {}
    }
}

/// Navigate to previous state (used by both keyboard and mouse)
fn handle_prev_state(app: &mut App) {
    match app.view_mode {
        ViewMode::Single => {
            if app.current_state > 0 {
                app.current_state -= 1;
                app.cursor = 0;
                app.scroll_offset = 0;
                if app.auto_expand {
                    auto_expand_changes(app);
                }
            }
        }
        ViewMode::Diff { left, right, focus } => {
            match focus {
                DiffFocus::Left => {
                    if left > 0 {
                        app.view_mode = ViewMode::Diff { left: left - 1, right, focus };
                        app.scroll_offset = 0;
                    }
                }
                DiffFocus::Right => {
                    if right > 0 {
                        app.view_mode = ViewMode::Diff { left, right: right - 1, focus };
                        app.scroll_offset = 0;
                    }
                }
            }
        }
    }
}

/// Navigate to next state (used by both keyboard and mouse)
fn handle_next_state(app: &mut App) {
    match app.view_mode {
        ViewMode::Single => {
            if app.current_state + 1 < app.trace.states.len() {
                app.current_state += 1;
                app.cursor = 0;
                app.scroll_offset = 0;
                if app.auto_expand {
                    auto_expand_changes(app);
                }
            }
        }
        ViewMode::Diff { left, right, focus } => {
            let max_state = app.trace.states.len().saturating_sub(1);
            match focus {
                DiffFocus::Left => {
                    if left < max_state {
                        app.view_mode = ViewMode::Diff { left: left + 1, right, focus };
                        app.scroll_offset = 0;
                    }
                }
                DiffFocus::Right => {
                    if right < max_state {
                        app.view_mode = ViewMode::Diff { left, right: right + 1, focus };
                        app.scroll_offset = 0;
                    }
                }
            }
        }
    }
}

/// Handle clicks on tree content area
fn handle_content_click(app: &mut App, row: usize, col: usize, ctx: &EventContext) {
    match app.view_mode {
        ViewMode::Single => {
            let clicked_line = app.scroll_offset + (row - 2);
            if clicked_line < ctx.line_count {
                app.cursor = clicked_line;
                if let Some(line) = ctx.tree_lines.get(clicked_line) {
                    if line.expandable {
                        app.expansion.toggle(&line.path);
                    }
                }
            }
        }
        ViewMode::Diff { left, right, .. } => {
            let half_width = ctx.terminal_width / 2;
            let new_focus = if col < half_width {
                DiffFocus::Left
            } else {
                DiffFocus::Right
            };
            app.view_mode = ViewMode::Diff { left, right, focus: new_focus };

            // Row 0 = header, Row 1 = empty, Row 2 = panel border, Row 3+ = content
            if row >= 3 {
                let clicked_line = app.scroll_offset + (row - 3);
                let empty_diff = DiffResult { changes: std::collections::HashMap::new() };
                let state_idx = if new_focus == DiffFocus::Left { left } else { right };
                let panel_lines = build_tree_lines_for_state(&app.trace, state_idx, &app.expansion, &empty_diff, half_width);

                if clicked_line < panel_lines.len() {
                    app.cursor = clicked_line;
                    if let Some(line) = panel_lines.get(clicked_line) {
                        if line.expandable {
                            app.expansion.toggle(&line.path);
                        }
                    }
                }
            }
        }
    }
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

/// Clickable regions in the header
struct HeaderLayout {
    prev_start: usize,
    prev_end: usize,
    next_start: usize,
    next_end: usize,
    expand_start: usize,
    expand_end: usize,
    collapse_start: usize,
    collapse_end: usize,
    diff_start: usize,
    diff_end: usize,
}

/// Build header line and return (Line, HeaderLayout)
fn build_header<'a>(
    state_text: &str,
    middle_text: &str,
    diff_btn_text: &str,
    header_style: ratatui::style::Style,
    button_style: ratatui::style::Style,
) -> (ratatui::text::Line<'a>, HeaderLayout) {
    use ratatui::text::Span;

    let mut pos = 0;

    let space1 = " ";
    pos += space1.len();

    let prev_start = pos;
    let prev_btn = "[◀]";
    pos += prev_btn.chars().count();
    let prev_end = pos;

    pos += state_text.len();

    let next_start = pos;
    let next_btn = "[▶]";
    pos += next_btn.chars().count();
    let next_end = pos;

    pos += middle_text.len();

    let expand_start = pos;
    let expand_btn = "[+all]";
    pos += expand_btn.len();
    let expand_end = pos;

    let space2 = " ";
    pos += space2.len();

    let collapse_start = pos;
    let collapse_btn = "[-all]";
    pos += collapse_btn.len();
    let collapse_end = pos;

    let sep2 = " | ";
    pos += sep2.len();

    let diff_start = pos;
    pos += diff_btn_text.len();
    let diff_end = pos;

    let suffix = " | q quit ";

    let header = ratatui::text::Line::from(vec![
        Span::styled(space1.to_string(), header_style),
        Span::styled(prev_btn.to_string(), button_style),
        Span::styled(state_text.to_string(), header_style),
        Span::styled(next_btn.to_string(), button_style),
        Span::styled(middle_text.to_string(), header_style),
        Span::styled(expand_btn.to_string(), button_style),
        Span::styled(space2.to_string(), header_style),
        Span::styled(collapse_btn.to_string(), button_style),
        Span::styled(sep2.to_string(), header_style),
        Span::styled(diff_btn_text.to_string(), button_style),
        Span::styled(suffix.to_string(), header_style),
    ]);

    let layout = HeaderLayout {
        prev_start,
        prev_end,
        next_start,
        next_end,
        expand_start,
        expand_end,
        collapse_start,
        collapse_end,
        diff_start,
        diff_end,
    };

    (header, layout)
}

fn render(frame: &mut Frame, app: &App, tree_lines: &[TreeLine], viewport_height: usize) -> HeaderLayout {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    let header_style = Style::default()
        .bg(Color::Indexed(56))  // Pastel purple
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let button_style = Style::default()
        .bg(Color::Indexed(56))
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    // Build scroll indicator
    let total_lines = tree_lines.len();
    let scroll_info = if total_lines > viewport_height {
        format!(" [{}-{}/{}]", app.scroll_offset + 1, (app.scroll_offset + viewport_height).min(total_lines), total_lines)
    } else {
        String::new()
    };

    let auto_indicator = if app.auto_expand { " [auto]" } else { "" };
    let state_text = format!(" State {}/{}{}{} ", app.current_state + 1, app.trace.states.len(), auto_indicator, scroll_info);
    let middle_text = " | ";

    let (header, header_layout) = build_header(&state_text, middle_text, "[diff]", header_style, button_style);

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

    header_layout
}

/// Build tree lines for a specific state index
fn build_tree_lines_for_state(
    trace: &Trace,
    state_idx: usize,
    expansion: &ExpansionState,
    diff: &DiffResult,
    terminal_width: usize,
) -> Vec<TreeLine> {
    let mut tree_lines = Vec::new();
    if let Some(state) = trace.states.get(state_idx) {
        for (name, value) in &state.values {
            let path = vec![name.clone()];
            tree_lines.extend(render_value(name, value, path, expansion, diff, 0, terminal_width));
        }
    }
    tree_lines
}

/// Compute diff between two specific states
fn compute_diff_between(trace: &Trace, left_idx: usize, right_idx: usize) -> DiffResult {
    use std::collections::HashMap;

    if left_idx >= trace.states.len() || right_idx >= trace.states.len() {
        return DiffResult { changes: HashMap::new() };
    }

    let left = &trace.states[left_idx].values;
    let right = &trace.states[right_idx].values;
    compute_diff(left, right)
}

/// Render side-by-side diff view
fn render_diff(
    frame: &mut Frame,
    app: &App,
    left_idx: usize,
    right_idx: usize,
    focus: DiffFocus,
    viewport_height: usize,
) -> HeaderLayout {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::layout::{Layout, Constraint, Direction};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let header_style = Style::default()
        .bg(Color::Indexed(56))
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let button_style = Style::default()
        .bg(Color::Indexed(56))
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let state_text = format!(" State {} vs {} ", left_idx + 1, right_idx + 1);
    let middle_text = " | Tab:switch | ";

    let (header, header_layout) = build_header(&state_text, middle_text, "[exit]", header_style, button_style);

    // Calculate panel width (half of terminal minus border)
    let area = frame.area();
    let panel_width = (area.width as usize) / 2;

    // Compute diff: comparing left to right (what changed from left to right)
    let diff_left_to_right = compute_diff_between(&app.trace, left_idx, right_idx);

    // Empty diff for showing states without diff coloring
    let empty_diff = DiffResult { changes: std::collections::HashMap::new() };

    // Build tree lines for each side
    let left_lines = build_tree_lines_for_state(&app.trace, left_idx, &app.expansion, &empty_diff, panel_width.saturating_sub(4));
    let right_lines = build_tree_lines_for_state(&app.trace, right_idx, &app.expansion, &diff_left_to_right, panel_width.saturating_sub(4));

    // Split layout: header + two panels side by side
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let panel_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[2]);

    // Render header
    frame.render_widget(Paragraph::new(header), main_chunks[0]);

    // Style for focused/unfocused borders
    let focused_style = Style::default().fg(Color::Yellow);
    let unfocused_style = Style::default().fg(Color::DarkGray);

    let left_border_style = if focus == DiffFocus::Left { focused_style } else { unfocused_style };
    let right_border_style = if focus == DiffFocus::Right { focused_style } else { unfocused_style };

    // Build left panel content with cursor highlighting
    let left_content: Vec<Line> = left_lines
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(viewport_height)
        .map(|(i, tree_line)| {
            let is_cursor = focus == DiffFocus::Left && i == app.cursor;
            let bg_color = if is_cursor { Some(Color::DarkGray) } else { None };
            let styled_spans: Vec<Span> = tree_line.spans.iter().map(|span| {
                let mut style = Style::default();
                if let Some(bg) = bg_color {
                    style = style.bg(bg);
                }
                Span::styled(&span.text, style)
            }).collect();
            Line::from(styled_spans)
        })
        .collect();

    // Build right panel content with diff highlighting and cursor
    let right_content: Vec<Line> = right_lines
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(viewport_height)
        .map(|(i, tree_line)| {
            let is_cursor = focus == DiffFocus::Right && i == app.cursor;
            let bg_color = if is_cursor { Some(Color::DarkGray) } else { None };
            let diff_color = match tree_line.diff {
                DiffKind::Added => Some(Color::Green),
                DiffKind::Removed => Some(Color::Red),
                DiffKind::Modified => Some(Color::Yellow),
                DiffKind::Unchanged => None,
            };
            let styled_spans: Vec<Span> = tree_line.spans.iter().map(|span| {
                let mut style = Style::default();
                if let Some(fg) = diff_color {
                    style = style.fg(fg);
                }
                if let Some(bg) = bg_color {
                    style = style.bg(bg);
                }
                Span::styled(&span.text, style)
            }).collect();
            Line::from(styled_spans)
        })
        .collect();

    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_style(left_border_style)
        .title(format!(" State {} ", left_idx + 1));

    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_style(right_border_style)
        .title(format!(" State {} ", right_idx + 1));

    let left_para = Paragraph::new(left_content).block(left_block);
    let right_para = Paragraph::new(right_content).block(right_block);

    frame.render_widget(left_para, panel_chunks[0]);
    frame.render_widget(right_para, panel_chunks[1]);

    header_layout
}
