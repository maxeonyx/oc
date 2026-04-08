use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crate::service::SessionService;

use super::model::DashboardSnapshot;
use super::render;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct DashboardState {
    pub snapshot: DashboardSnapshot,
    pub selected_index: usize,
}

pub fn run(service: &SessionService) -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let initial_selected_session_id = default_selected_session_id(service)?;
    let mut state = DashboardState::load(service, initial_selected_session_id)?;
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(|frame| render::render(frame, &state))?;

        let wait = POLL_INTERVAL.saturating_sub(last_refresh.elapsed());
        if event::poll(wait)? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    match key_event.code {
                        KeyCode::Esc => return Ok(()),
                        KeyCode::Up => state.move_selection_up(),
                        KeyCode::Down => state.move_selection_down(),
                        KeyCode::Enter => {
                            if let Some(session_id) = state.selected_session_id() {
                                service.activate_target(&session_id.to_string())?;
                                state = DashboardState::load(service, Some(session_id))?;
                                last_refresh = Instant::now();
                            }
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_refresh.elapsed() >= POLL_INTERVAL {
            let selected_session_id = state.selected_session_id();
            state = DashboardState::load(service, selected_session_id)?;
            last_refresh = Instant::now();
        }
    }
}

impl DashboardState {
    fn load(service: &SessionService, selected_session_id: Option<i64>) -> Result<Self> {
        let snapshot = DashboardSnapshot::from_session_entries(service.list_dashboard_sessions()?);
        let selected_index = select_index(&snapshot, selected_session_id);

        Ok(Self {
            snapshot,
            selected_index,
        })
    }

    pub fn selected_session_id(&self) -> Option<i64> {
        self.snapshot
            .rows
            .get(self.selected_index)
            .map(|row| row.session_id)
    }

    pub fn move_selection_up(&mut self) {
        if self.snapshot.rows.is_empty() {
            return;
        }

        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn move_selection_down(&mut self) {
        if self.snapshot.rows.is_empty() {
            return;
        }

        let last_index = self.snapshot.rows.len().saturating_sub(1);
        self.selected_index = (self.selected_index + 1).min(last_index);
    }
}

fn default_selected_session_id(service: &SessionService) -> Result<Option<i64>> {
    let matches = service.current_directory_matches()?;
    Ok(matches.first().map(|session| session.id))
}

fn select_index(snapshot: &DashboardSnapshot, selected_session_id: Option<i64>) -> usize {
    match selected_session_id {
        Some(session_id) => snapshot
            .rows
            .iter()
            .position(|row| row.session_id == session_id)
            .unwrap_or(0),
        None => 0,
    }
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable terminal raw mode")?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen)
            .context("failed to enter alternate screen")?;

        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;
        terminal.clear().context("failed to clear terminal")?;

        Ok(Self { terminal })
    }

    fn draw<F>(&mut self, render_fn: F) -> Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal
            .draw(render_fn)
            .context("failed to draw TUI")?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
