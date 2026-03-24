use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

use crate::{flatten, ratatui_color_for_name, RenderLine, TreeNode};

// ── App ───────────────────────────────────────────────────────────────────────

struct App {
    root: TreeNode,
    search: String,
    scroll: usize,
    lines: Vec<RenderLine>,
    total_files: usize,
}

impl App {
    fn new(root: TreeNode, initial_search: String) -> Self {
        let total_files = root.count_files();
        let mut app = App {
            root,
            search: initial_search,
            scroll: 0,
            lines: vec![],
            total_files,
        };
        app.refresh_lines();
        app
    }

    fn refresh_lines(&mut self) {
        let mut lines = vec![];
        flatten(&self.root, "", false, 0, &self.search, &mut lines);
        self.lines = lines;

        // Jump to first match
        if !self.search.is_empty() {
            if let Some(first) = self.lines.iter().position(|l| l.match_range.is_some()) {
                self.scroll = first.saturating_sub(2);
            }
        }
    }

    fn match_count(&self) -> usize {
        self.lines.iter().filter(|l| l.match_range.is_some()).count()
    }

    fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    fn scroll_down(&mut self, n: usize, viewport: usize) {
        let max = self.lines.len().saturating_sub(viewport);
        self.scroll = (self.scroll + n).min(max);
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn build_line(rl: &RenderLine) -> Line<'static> {
    let prefix_sty = Style::default().fg(Color::DarkGray);
    let hi_sty = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let name_color = ratatui_color_for_name(&rl.name, rl.is_dir);
    let base = if rl.is_dir {
        Style::default().fg(name_color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(name_color)
    };

    let mut spans: Vec<Span<'static>> = vec![Span::styled(rl.prefix.clone(), prefix_sty)];

    match rl.match_range {
        None => spans.push(Span::styled(rl.name.clone(), base)),
        Some((s, e)) => {
            let n = &rl.name;
            if s > 0 {
                spans.push(Span::styled(n[..s].to_string(), base));
            }
            spans.push(Span::styled(n[s..e].to_string(), hi_sty));
            if e < n.len() {
                spans.push(Span::styled(n[e..].to_string(), base));
            }
        }
    }
    Line::from(spans)
}

fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    // ── Title bar ─────────────────────────────────────────────────────────────
    let stat = if app.search.is_empty() {
        format!("  {} files ", app.total_files)
    } else if app.lines.is_empty() {
        format!("  no matches  ({} files total) ", app.total_files)
    } else {
        format!("  {} / {} ", app.match_count(), app.total_files)
    };

    let title = Line::from(vec![
        Span::styled(
            " newtree",
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(stat, Style::default().bg(Color::DarkGray).fg(Color::White)),
        Span::styled(
            "  ↑↓/PgUp/PgDn  ESC clear  q quit ",
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ),
    ]);
    f.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::DarkGray)),
        chunks[0],
    );

    // ── Tree ──────────────────────────────────────────────────────────────────
    let tree_h = chunks[1].height as usize;
    let start = app.scroll.min(app.lines.len());

    let tree_lines: Vec<Line<'static>> = if app.lines.is_empty() && !app.search.is_empty() {
        vec![Line::from(Span::styled(
            format!("  (no matches for {:?})", app.search),
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.lines[start..].iter().take(tree_h).map(build_line).collect()
    };

    f.render_widget(Paragraph::new(tree_lines), chunks[1]);

    // ── Search box ────────────────────────────────────────────────────────────
    let border_col = if app.search.is_empty() {
        Color::DarkGray
    } else {
        Color::Yellow
    };

    f.render_widget(
        Paragraph::new(format!("{}_", app.search))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Search ")
                    .border_style(Style::default().fg(border_col)),
            )
            .style(Style::default().fg(Color::White)),
        chunks[2],
    );
}

// ── Event loop ────────────────────────────────────────────────────────────────

pub fn run_tui(root: TreeNode, initial_search: String) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(root, initial_search);
    let mut viewport_h: usize = 20;

    loop {
        terminal.draw(|f| {
            viewport_h = f.area().height.saturating_sub(4) as usize;
            render(f, &app);
        })?;

        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    let m = key.modifiers;
                    match (m, key.code) {
                        (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,

                        // Quit when search is empty
                        (KeyModifiers::NONE, KeyCode::Char('q'))
                        | (KeyModifiers::NONE, KeyCode::Esc)
                            if app.search.is_empty() =>
                        {
                            break
                        }

                        // Clear search
                        (KeyModifiers::NONE, KeyCode::Esc) => {
                            app.search.clear();
                            app.scroll = 0;
                            app.refresh_lines();
                        }

                        // Backspace
                        (KeyModifiers::NONE, KeyCode::Backspace) => {
                            app.search.pop();
                            app.scroll = 0;
                            app.refresh_lines();
                        }

                        // Typing
                        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                            app.search.push(c);
                            app.refresh_lines();
                        }

                        // Scrolling
                        (KeyModifiers::NONE, KeyCode::Up) => app.scroll_up(1),
                        (KeyModifiers::NONE, KeyCode::Down) => app.scroll_down(1, viewport_h),
                        (KeyModifiers::NONE, KeyCode::PageUp) => app.scroll_up(viewport_h),
                        (KeyModifiers::NONE, KeyCode::PageDown) => {
                            app.scroll_down(viewport_h, viewport_h)
                        }
                        (KeyModifiers::NONE, KeyCode::Home) => app.scroll = 0,
                        (KeyModifiers::NONE, KeyCode::End) => {
                            app.scroll = app.lines.len().saturating_sub(viewport_h);
                        }

                        _ => {}
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
