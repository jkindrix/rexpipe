//! Terminal User Interface (TUI) for rexpipe.
//!
//! Provides a real-time dashboard for monitoring pipeline processing,
//! visualizing pipeline steps, and displaying processing statistics.
//!
//! # Features
//!
//! - Real-time processing statistics
//! - Pipeline step visualization
//! - Progress bar with ETA
//! - Live match highlighting
//! - Interactive controls
//!
//! # Usage
//!
//! Enable the `tui` feature and use `--dashboard` flag:
//!
//! ```bash
//! rexpipe --config pipeline.toml --dashboard input.txt
//! ```

#[cfg(feature = "tui")]
use std::io::{self, Stdout};
#[cfg(feature = "tui")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "tui")]
use std::time::{Duration, Instant};

#[cfg(feature = "tui")]
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

#[cfg(feature = "tui")]
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};

/// Processing statistics for the dashboard.
#[derive(Debug, Clone, Default)]
pub struct ProcessingStats {
    /// Total lines processed
    pub lines_processed: u64,
    /// Total matches found
    pub matches_found: u64,
    /// Total transformations applied
    pub transformations_applied: u64,
    /// Lines filtered (dropped)
    pub lines_filtered: u64,
    /// Lines extracted
    pub lines_extracted: u64,
    /// Errors encountered
    pub errors: u64,
    /// Processing start time
    pub start_time: Option<Instant>,
    /// Estimated total lines (for progress)
    pub estimated_total: Option<u64>,
    /// Current file being processed
    pub current_file: Option<String>,
    /// Step statistics
    pub step_stats: Vec<StepStats>,
    /// Recent matches (for display)
    pub recent_matches: Vec<String>,
    /// Status message
    pub status: String,
    /// Is processing complete?
    pub is_complete: bool,
}

/// Statistics for a single pipeline step.
#[derive(Debug, Clone)]
pub struct StepStats {
    /// Step name or description
    pub name: String,
    /// Step type
    pub step_type: String,
    /// Matches for this step
    pub matches: u64,
    /// Transformations for this step
    pub transformations: u64,
}

impl ProcessingStats {
    /// Create new empty statistics.
    pub fn new() -> Self {
        Self {
            start_time: Some(Instant::now()),
            status: "Initializing...".to_string(),
            ..Default::default()
        }
    }

    /// Get the elapsed time since processing started.
    pub fn elapsed(&self) -> Duration {
        self.start_time.map(|t| t.elapsed()).unwrap_or_default()
    }

    /// Get the processing rate (lines per second).
    pub fn lines_per_second(&self) -> f64 {
        let elapsed = self.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.lines_processed as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Get the estimated time remaining.
    pub fn eta(&self) -> Option<Duration> {
        if let Some(total) = self.estimated_total {
            if self.lines_processed > 0 {
                let lps = self.lines_per_second();
                if lps > 0.0 {
                    let remaining = total.saturating_sub(self.lines_processed);
                    let seconds = remaining as f64 / lps;
                    return Some(Duration::from_secs_f64(seconds));
                }
            }
        }
        None
    }

    /// Get the progress percentage.
    pub fn progress(&self) -> f64 {
        if let Some(total) = self.estimated_total {
            if total > 0 {
                return (self.lines_processed as f64 / total as f64).min(1.0);
            }
        }
        0.0
    }

    /// Add a recent match for display.
    pub fn add_match(&mut self, match_text: String) {
        self.recent_matches.push(match_text);
        // Keep only the last 10 matches
        if self.recent_matches.len() > 10 {
            self.recent_matches.remove(0);
        }
    }
}

/// Shared statistics for thread-safe updates.
#[cfg(feature = "tui")]
pub type SharedStats = Arc<Mutex<ProcessingStats>>;

/// Create a new shared statistics instance.
#[cfg(feature = "tui")]
pub fn create_shared_stats() -> SharedStats {
    Arc::new(Mutex::new(ProcessingStats::new()))
}

/// Dashboard application state.
#[cfg(feature = "tui")]
pub struct Dashboard {
    /// Shared processing statistics
    stats: SharedStats,
    /// Terminal instance
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Should the dashboard exit?
    should_exit: bool,
    /// Scroll offset for matches list
    match_scroll: usize,
    /// Show help overlay?
    show_help: bool,
}

#[cfg(feature = "tui")]
impl Dashboard {
    /// Create a new dashboard instance.
    pub fn new(stats: SharedStats) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            stats,
            terminal,
            should_exit: false,
            match_scroll: 0,
            show_help: false,
        })
    }

    /// Run the dashboard main loop.
    pub fn run(&mut self) -> io::Result<()> {
        loop {
            self.draw()?;

            // Check for input with timeout
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                self.should_exit = true;
                            }
                            KeyCode::Char('?') | KeyCode::Char('h') => {
                                self.show_help = !self.show_help;
                            }
                            KeyCode::Up => {
                                if self.match_scroll > 0 {
                                    self.match_scroll -= 1;
                                }
                            }
                            KeyCode::Down => {
                                self.match_scroll += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Check if processing is complete
            {
                let stats = self.stats.lock().unwrap();
                if stats.is_complete && self.should_exit {
                    break;
                }
            }

            if self.should_exit {
                break;
            }
        }

        Ok(())
    }

    /// Draw the dashboard.
    fn draw(&mut self) -> io::Result<()> {
        let stats = self.stats.lock().unwrap().clone();
        let show_help = self.show_help;

        self.terminal.draw(|frame| {
            let area = frame.area();

            if show_help {
                draw_help(frame, area);
            } else {
                draw_main(frame, area, &stats);
            }
        })?;

        Ok(())
    }
}

#[cfg(feature = "tui")]
impl Drop for Dashboard {
    fn drop(&mut self) {
        // Restore terminal
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

/// Draw the main dashboard view.
#[cfg(feature = "tui")]
fn draw_main(frame: &mut Frame, area: Rect, stats: &ProcessingStats) {
    // Create layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(3),  // Progress bar
            Constraint::Min(8),     // Main content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Header
    draw_header(frame, chunks[0], stats);

    // Progress bar
    draw_progress(frame, chunks[1], stats);

    // Main content area
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),  // Statistics
            Constraint::Percentage(60),  // Matches
        ])
        .split(chunks[2]);

    draw_statistics(frame, main_chunks[0], stats);
    draw_matches(frame, main_chunks[1], stats);

    // Footer
    draw_footer(frame, chunks[3]);
}

/// Draw the header.
#[cfg(feature = "tui")]
fn draw_header(frame: &mut Frame, area: Rect, stats: &ProcessingStats) {
    let title = if let Some(ref file) = stats.current_file {
        format!(" rexpipe Dashboard - {} ", file)
    } else {
        " rexpipe Dashboard ".to_string()
    };

    let status_style = if stats.is_complete {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(&stats.status, status_style),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, area);
}

/// Draw the progress bar.
#[cfg(feature = "tui")]
fn draw_progress(frame: &mut Frame, area: Rect, stats: &ProcessingStats) {
    let progress = stats.progress();
    let eta_text = if let Some(eta) = stats.eta() {
        format!(" ETA: {:?}", eta)
    } else {
        String::new()
    };

    let label = format!(
        "{:.1}% ({} / {}){}",
        progress * 100.0,
        stats.lines_processed,
        stats.estimated_total.unwrap_or(0),
        eta_text
    );

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Progress "))
        .gauge_style(Style::default().fg(Color::Cyan))
        .ratio(progress)
        .label(label);

    frame.render_widget(gauge, area);
}

/// Draw the statistics panel.
#[cfg(feature = "tui")]
fn draw_statistics(frame: &mut Frame, area: Rect, stats: &ProcessingStats) {
    let elapsed = stats.elapsed();
    let lps = stats.lines_per_second();

    let stats_text = vec![
        Line::from(vec![
            Span::styled("Lines Processed: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.lines_processed.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Matches Found: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.matches_found.to_string(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Transformations: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.transformations_applied.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("Lines Filtered: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.lines_filtered.to_string(),
                Style::default().fg(Color::Magenta),
            ),
        ]),
        Line::from(vec![
            Span::styled("Errors: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.errors.to_string(),
                if stats.errors > 0 {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Elapsed: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.1}s", elapsed.as_secs_f64()),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Rate: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.0} lines/sec", lps),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title(" Statistics "))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

/// Draw the matches panel.
#[cfg(feature = "tui")]
fn draw_matches(frame: &mut Frame, area: Rect, stats: &ProcessingStats) {
    let items: Vec<ListItem> = stats
        .recent_matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let content = if m.len() > 60 {
                format!("{}...", &m[..57])
            } else {
                m.clone()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:2}. ", i + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(content, Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Recent Matches "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(list, area);
}

/// Draw the footer.
#[cfg(feature = "tui")]
fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::White)),
        Span::raw(" Quit "),
        Span::styled(" ? ", Style::default().fg(Color::Black).bg(Color::White)),
        Span::raw(" Help "),
        Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::White)),
        Span::raw(" Scroll "),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, area);
}

/// Draw the help overlay.
#[cfg(feature = "tui")]
fn draw_help(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  rexpipe Dashboard Help",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Keyboard Controls:"),
        Line::from(""),
        Line::from("    q, Esc    Quit dashboard"),
        Line::from("    ?, h      Toggle this help"),
        Line::from("    ↑, ↓      Scroll matches"),
        Line::from(""),
        Line::from("  The dashboard shows real-time processing"),
        Line::from("  statistics as files are processed."),
        Line::from(""),
        Line::from("  Press any key to close this help."),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .style(Style::default().bg(Color::DarkGray)),
        )
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));

    // Center the help dialog
    let help_area = centered_rect(60, 60, area);
    frame.render_widget(paragraph, help_area);
}

/// Create a centered rectangle for dialogs.
#[cfg(feature = "tui")]
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Run the dashboard in a background thread while processing continues.
#[cfg(feature = "tui")]
pub fn run_dashboard_async(stats: SharedStats) -> std::thread::JoinHandle<io::Result<()>> {
    std::thread::spawn(move || {
        let mut dashboard = Dashboard::new(stats)?;
        dashboard.run()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_stats_default() {
        let stats = ProcessingStats::new();
        assert_eq!(stats.lines_processed, 0);
        assert_eq!(stats.matches_found, 0);
        assert!(!stats.is_complete);
    }

    #[test]
    fn test_processing_stats_progress() {
        let mut stats = ProcessingStats::new();
        stats.estimated_total = Some(100);
        stats.lines_processed = 50;
        assert!((stats.progress() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_add_match() {
        let mut stats = ProcessingStats::new();
        for i in 0..15 {
            stats.add_match(format!("match {}", i));
        }
        // Should only keep last 10
        assert_eq!(stats.recent_matches.len(), 10);
        assert_eq!(stats.recent_matches[0], "match 5");
    }
}
