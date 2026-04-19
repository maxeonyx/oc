use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crate::cli::RequestedAction;
use crate::commands;
use crate::service::SessionService;

use super::command::{CommandParseError, parse_command};
use super::filter::{build_view, summary_for_view, totals_scope_label};
use super::input::{InputIntent, map_key_event};
use super::render;
use super::render::Theme;
use super::selection::{
    SelectedSession, available_actions, cycle_action_for_row, default_selected_identity,
    index_for_selected_identity, preferred_action_for_row, selected_identity_at,
};
use super::types::{
    DashboardAction, DashboardSnapshot, DashboardSummary, DashboardView, InputMode,
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct DashboardState {
    pub snapshot: DashboardSnapshot,
    pub view: DashboardView,
    pub selected_index: usize,
    pub selected_action: DashboardAction,
    pub input_mode: InputMode,
    pub input_text: String,
    pub status_message: Option<String>,
    pub current_directory: Option<PathBuf>,
    pub theme: Theme,
    pending_restart: Option<PendingRestart>,
    selected_identity: Option<SelectedSession>,
}

struct PendingRestart {
    target: String,
    receiver: Receiver<Result<()>>,
}

pub fn run(service: &SessionService, status_message: Option<String>) -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let theme = render::detect_theme();
    let mut state = DashboardState::load(
        service,
        None,
        DashboardAction::Attach,
        InputMode::Filter,
        String::new(),
        status_message,
        theme,
    )?;
    let mut last_refresh = Instant::now();

    loop {
        state.poll_background_work(service)?;
        terminal.draw(|frame| render::render(frame, &state))?;

        let wait = POLL_INTERVAL.saturating_sub(last_refresh.elapsed());
        if event::poll(wait)? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    let command_mode = state.input_mode == InputMode::Command;
                    match handle_input(service, &mut state, map_key_event(key_event, command_mode))?
                    {
                        DashboardLoopAction::Continue => {}
                        DashboardLoopAction::Exit => return Ok(()),
                        DashboardLoopAction::RunInteractiveAction(action) => {
                            run_interactive_action(service, &mut state, &mut terminal, action)?;
                        }
                    }
                    last_refresh = Instant::now();
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_refresh.elapsed() >= POLL_INTERVAL {
            state.refresh(service)?;
            last_refresh = Instant::now();
        }
    }
}

fn handle_input(
    service: &SessionService,
    state: &mut DashboardState,
    intent: InputIntent,
) -> Result<DashboardLoopAction> {
    match intent {
        InputIntent::ClearInput => state.clear_input(service)?,
        InputIntent::Quit => return state.handle_quit(service),
        InputIntent::MoveUp => state.move_selection_up(),
        InputIntent::MoveDown => state.move_selection_down(),
        InputIntent::CycleLeft => state.cycle_action(-1),
        InputIntent::CycleRight => state.cycle_action(1),
        InputIntent::Submit => return state.execute_enter(service),
        InputIntent::Backspace => state.handle_backspace(service)?,
        InputIntent::EnterCommandMode => state.enter_command_mode(),
        InputIntent::InsertChar(character) => {
            state.input_text.push(character);
            state.status_message = None;
            if state.input_mode == InputMode::Filter {
                state.rebuild_view(true);
            }
        }
        InputIntent::Ignore => {}
    }

    Ok(DashboardLoopAction::Continue)
}

enum DashboardLoopAction {
    Continue,
    Exit,
    RunInteractiveAction(RequestedAction),
}

impl DashboardState {
    fn load(
        service: &SessionService,
        selected_identity: Option<SelectedSession>,
        preferred_action: DashboardAction,
        input_mode: InputMode,
        input_text: String,
        status_message: Option<String>,
        theme: Theme,
    ) -> Result<Self> {
        let current_directory = std::env::current_dir().ok();
        let snapshot = DashboardSnapshot::from_session_entries(service.list_dashboard_sessions()?);

        let mut state = Self {
            snapshot,
            view: DashboardView {
                groups: Vec::new(),
                totals: DashboardSummary {
                    attached: 0,
                    detached: 0,
                    saved: 0,
                    filtered_sessions: 0,
                    filtered_running: 0,
                    filtered_memory_bytes: 0,
                },
            },
            selected_index: 0,
            selected_action: preferred_action,
            input_mode,
            input_text,
            status_message,
            current_directory,
            theme,
            pending_restart: None,
            selected_identity,
        };

        state.rebuild_view(false);
        Ok(state)
    }

    fn refresh(&mut self, service: &SessionService) -> Result<()> {
        let snapshot = DashboardSnapshot::from_session_entries(service.list_dashboard_sessions()?);
        if snapshot != self.snapshot {
            self.snapshot = snapshot;
            self.rebuild_view(false);
        }
        Ok(())
    }

    pub fn selected_row(&self) -> Option<&super::types::DashboardRow> {
        self.view.sessions().nth(self.selected_index)
    }

    pub fn selected_session(&self) -> Option<SelectedSession> {
        self.selected_identity
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.selected_identity = selected_identity_at(&self.view, self.selected_index);
            self.reconcile_selected_action();
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.selected_index + 1 < self.view.sessions().count() {
            self.selected_index += 1;
            self.selected_identity = selected_identity_at(&self.view, self.selected_index);
            self.reconcile_selected_action();
        }
    }

    pub fn cycle_action(&mut self, delta: isize) {
        if let Some(row) = self.selected_row() {
            self.selected_action = cycle_action_for_row(row, self.selected_action, delta);
        }
    }

    pub fn enter_command_mode(&mut self) {
        self.input_mode = InputMode::Command;
        if !self.input_text.ends_with(' ') {
            self.input_text.push(' ');
        }
        self.rebuild_view(true);
    }

    pub fn handle_backspace(&mut self, _service: &SessionService) -> Result<()> {
        if self.input_text.pop().is_none() {
            return Ok(());
        }

        if self.input_mode == InputMode::Command && !self.input_text.contains(' ') {
            self.input_mode = InputMode::Filter;
            self.rebuild_view(true);
        } else if self.input_mode == InputMode::Filter {
            self.rebuild_view(true);
        }

        Ok(())
    }

    fn execute_enter(&mut self, service: &SessionService) -> Result<DashboardLoopAction> {
        if self.input_mode == InputMode::Command {
            return self.execute_command(service);
        }

        if let Some(row) = self.selected_row() {
            return self.execute_action(service, row.name.clone());
        }

        Ok(DashboardLoopAction::Continue)
    }

    pub fn available_actions(&self) -> Vec<DashboardAction> {
        self.selected_row()
            .map(available_actions)
            .unwrap_or_default()
    }

    pub fn summary(&self) -> DashboardSummary {
        summary_for_view(
            &self.snapshot.summary,
            &self.view,
            self.input_mode,
            &self.input_text,
        )
    }

    pub fn totals_scope_label(&self) -> &'static str {
        totals_scope_label(self.input_mode, &self.input_text)
    }

    fn reconcile_selected_action(&mut self) {
        if let Some(row) = self.selected_row() {
            self.selected_action = preferred_action_for_row(row, self.selected_action);
        }
    }

    fn has_active_filter(&self) -> bool {
        self.input_mode == InputMode::Filter && !self.input_text.is_empty()
    }

    fn execute_command(&mut self, service: &SessionService) -> Result<DashboardLoopAction> {
        let selected_identity = self.selected_session();
        match parse_command(&self.input_text) {
            Ok(command) => match command {
                RequestedAction::New { .. } | RequestedAction::Move { .. } => {
                    let action = command;
                    self.input_mode = InputMode::Filter;
                    self.input_text.clear();
                    self.status_message = None;
                    self.rebuild_view(true);
                    return Ok(DashboardLoopAction::RunInteractiveAction(action));
                }
                _ => {
                    commands::run_requested_action(service, command)?;
                    *self = Self::load(
                        service,
                        selected_identity,
                        self.selected_action,
                        InputMode::Filter,
                        String::new(),
                        None,
                        self.theme,
                    )?;
                }
            },
            Err(error) => self.status_message = Some(format_command_error(error)),
        }

        Ok(DashboardLoopAction::Continue)
    }

    fn execute_action(
        &mut self,
        service: &SessionService,
        target: String,
    ) -> Result<DashboardLoopAction> {
        match self.selected_action {
            DashboardAction::Attach => {
                return Ok(DashboardLoopAction::RunInteractiveAction(
                    RequestedAction::AttachTarget { target },
                ));
            }
            DashboardAction::Stop => service.stop_session(&target)?,
            DashboardAction::Remove => service.remove_session(&target)?,
            DashboardAction::Restart => {
                self.begin_restart(service, target);
                return Ok(DashboardLoopAction::Continue);
            }
        }

        self.refresh(service)?;
        Ok(DashboardLoopAction::Continue)
    }

    fn clear_input(&mut self, service: &SessionService) -> Result<()> {
        self.input_text.clear();
        self.input_mode = InputMode::Filter;
        self.status_message = None;
        self.rebuild_view(true);
        self.refresh(service)
    }

    fn handle_quit(&mut self, service: &SessionService) -> Result<DashboardLoopAction> {
        if self.input_text.is_empty() {
            return Ok(DashboardLoopAction::Exit);
        }

        self.input_text.clear();
        self.input_mode = InputMode::Filter;
        self.status_message = None;
        self.rebuild_view(true);
        self.refresh(service)?;
        Ok(DashboardLoopAction::Continue)
    }

    fn rebuild_view(&mut self, filter_text_changed: bool) {
        self.view = build_view(
            &self.snapshot,
            &self.input_text,
            self.input_mode,
            self.current_directory.clone(),
        );
        self.reconcile_selection(filter_text_changed);
        self.reconcile_selected_action();
    }

    fn reconcile_selection(&mut self, filter_text_changed: bool) {
        let visible_count = self.view.sessions().count();
        if visible_count == 0 {
            self.selected_index = 0;
            self.selected_identity = None;
            return;
        }

        if let Some(index) = index_for_selected_identity(&self.view, self.selected_identity) {
            self.selected_index = index;
            return;
        }

        self.selected_identity = if self.has_active_filter() {
            if filter_text_changed {
                selected_identity_at(&self.view, 0)
            } else {
                default_selected_identity(&self.view, self.current_directory.as_deref())
                    .or_else(|| selected_identity_at(&self.view, 0))
            }
        } else {
            default_selected_identity(&self.view, self.current_directory.as_deref())
                .or_else(|| selected_identity_at(&self.view, 0))
        };

        self.selected_index =
            index_for_selected_identity(&self.view, self.selected_identity).unwrap_or(0);
    }

    fn begin_restart(&mut self, service: &SessionService, target: String) {
        if self.pending_restart.is_some() {
            self.status_message = Some(String::from("Restart already in progress"));
            return;
        }

        let (sender, receiver) = mpsc::channel();
        let service = service.clone();
        let target_for_thread = target.clone();
        thread::spawn(move || {
            let result = service.restart_session(&target_for_thread);
            let _ = sender.send(result);
        });

        self.pending_restart = Some(PendingRestart {
            target: target.clone(),
            receiver,
        });
        self.status_message = Some(format!("Restarting session {target}..."));
    }

    fn poll_background_work(&mut self, service: &SessionService) -> Result<()> {
        let Some(pending_restart) = self.pending_restart.take() else {
            return Ok(());
        };

        match pending_restart.receiver.try_recv() {
            Ok(Ok(())) => {
                self.status_message = Some(format!("Restarted session {}", pending_restart.target));
                self.refresh(service)?;
            }
            Ok(Err(error)) => {
                self.status_message = Some(format!(
                    "Restart failed for {}: {error:#}",
                    pending_restart.target
                ));
                self.refresh(service)?;
            }
            Err(TryRecvError::Empty) => {
                self.pending_restart = Some(pending_restart);
            }
            Err(TryRecvError::Disconnected) => {
                self.status_message = Some(format!(
                    "Restart failed for {}: worker disconnected",
                    pending_restart.target
                ));
                self.refresh(service)?;
            }
        }

        Ok(())
    }
}

fn format_command_error(error: CommandParseError) -> String {
    match error {
        CommandParseError::Empty => String::from("Enter a command"),
        CommandParseError::UnknownCommand(command) => format!("Unknown command: {command}"),
        CommandParseError::MissingArgument(command) => {
            format!("Missing argument for command: {command}")
        }
        CommandParseError::TooManyArguments(command) => {
            format!("Too many arguments for command: {command}")
        }
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

    fn suspend(&mut self) -> Result<()> {
        disable_raw_mode().context("failed to disable terminal raw mode")?;
        crossterm::execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
            .context("failed to leave alternate screen")?;
        self.terminal
            .show_cursor()
            .context("failed to show cursor")?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().context("failed to enable terminal raw mode")?;
        crossterm::execute!(self.terminal.backend_mut(), EnterAlternateScreen)
            .context("failed to enter alternate screen")?;
        self.terminal.clear().context("failed to clear terminal")?;
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

fn run_interactive_action(
    service: &SessionService,
    state: &mut DashboardState,
    terminal: &mut TerminalGuard,
    action: RequestedAction,
) -> Result<()> {
    let selected_identity = state.selected_session();
    let selected_action = state.selected_action;
    let theme = state.theme;

    terminal.suspend()?;
    let action_result = commands::run_requested_action(service, action.clone());
    let resume_result = terminal.resume();

    let status_message = match action_result {
        Ok(()) => None,
        Err(error) => commands::interactive_attach_failure_summary(&action)
            .or_else(|| Some(error.to_string())),
    };

    resume_result?;

    *state = DashboardState::load(
        service,
        selected_identity,
        selected_action,
        InputMode::Filter,
        String::new(),
        status_message,
        theme,
    )?;

    Ok(())
}
