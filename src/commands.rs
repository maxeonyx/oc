use anyhow::Result;
use std::path::PathBuf;

use crate::cli::RequestedAction;
use crate::service::SessionService;
use crate::tmux;
use crate::tui;

pub fn run(service: &SessionService, action: RequestedAction) -> Result<()> {
    if matches!(action, RequestedAction::Default) {
        match auto_attach_result(service)? {
            AutoAttachResult::Attached => return Ok(()),
            AutoAttachResult::FallbackToDashboard(status_message) => {
                return tui::run_dashboard_with_status(service, status_message);
            }
            AutoAttachResult::NoMatch => {}
        }
    }

    let fallback_action = action.clone();
    match run_requested_action(service, action) {
        Ok(()) => Ok(()),
        Err(error) => match interactive_attach_failure_status(&fallback_action, &error) {
            Some(status_message) => tui::run_dashboard_with_status(service, Some(status_message)),
            None => Err(error),
        },
    }
}

enum AutoAttachResult {
    Attached,
    FallbackToDashboard(Option<String>),
    NoMatch,
}

fn auto_attach_result(service: &SessionService) -> Result<AutoAttachResult> {
    let matches = service.current_directory_matches()?;
    let [saved_session] = matches.as_slice() else {
        return Ok(AutoAttachResult::NoMatch);
    };

    match service.activate_session(saved_session) {
        Ok(()) => Ok(AutoAttachResult::Attached),
        Err(error) if is_attach_failure(&error) => Ok(AutoAttachResult::FallbackToDashboard(Some(
            format!("Auto-attach failed for {}: {error:#}", saved_session.name),
        ))),
        Err(error) => Err(error),
    }
}

pub fn interactive_attach_failure_status(
    action: &RequestedAction,
    error: &anyhow::Error,
) -> Option<String> {
    if !is_attach_failure(error) {
        return None;
    }

    match action {
        RequestedAction::AttachTarget { target } => {
            Some(format!("Attach failed for {target}: {error:#}"))
        }
        RequestedAction::New { name, .. } => Some(format!("Attach failed for {name}: {error:#}")),
        RequestedAction::Move { target, .. } => {
            Some(format!("Attach failed for {target}: {error:#}"))
        }
        _ => None,
    }
}

fn is_attach_failure(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string().contains("failed to attach"))
}

pub fn run_requested_action(service: &SessionService, action: RequestedAction) -> Result<()> {
    match action {
        RequestedAction::New {
            name,
            dir,
            opencode_args,
        } => service.create_session(name, dir, opencode_args),
        RequestedAction::Alias {
            name,
            dir,
            opencode_args,
        } => service.save_alias(name, dir, opencode_args),
        RequestedAction::Unalias { name } => service.remove_alias(&name),
        RequestedAction::Rm { target } => service.remove_session(&target),
        RequestedAction::Stop { target } => service.stop_session(&target),
        RequestedAction::Restart { target } => service.restart_session(&target),
        RequestedAction::Move { target, new_dir } => service.move_session(&target, new_dir),
        RequestedAction::Migrate => run_migrate(service),
        RequestedAction::AttachTarget { target } => service.activate_target(&target),
        RequestedAction::Default => tui::run_dashboard(service),
        RequestedAction::DumpSessionList => run_dump_session_list(service),
        RequestedAction::DumpRuntimeConfig => {
            service.config().write_debug_dump();
            Ok(())
        }
        RequestedAction::ParseMemoryStatus { path } => run_parse_memory_status(path),
    }
}

fn run_dump_session_list(service: &SessionService) -> Result<()> {
    for session in service.list_dashboard_sessions()? {
        println!("{}", session.debug_dump_line());
    }

    Ok(())
}

fn run_migrate(service: &SessionService) -> Result<()> {
    let report = service.migrate_legacy_aliases()?;
    println!(
        "imported {} skipped {} conflicts {}",
        report.imported,
        report.skipped,
        report.conflicts.len()
    );
    for conflict in report.conflicts {
        println!("conflict {conflict}");
    }

    Ok(())
}

fn run_parse_memory_status(path: PathBuf) -> Result<()> {
    let bytes = tmux::read_process_memory_bytes(&path)?.unwrap_or(0);
    println!("{bytes}");
    Ok(())
}
