use anyhow::Result;

use crate::cli::RequestedAction;
use crate::service::SessionService;
use crate::tui;

pub fn run(service: &SessionService, action: RequestedAction) -> Result<()> {
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
        RequestedAction::AttachTarget { target } => service.activate_target(&target),
        RequestedAction::Default => {
            if service.auto_attach_directory_match()? {
                return Ok(());
            }

            tui::run_dashboard(service)
        }
        RequestedAction::DumpSessionList => run_dump_session_list(service),
        RequestedAction::DumpRuntimeConfig => {
            service.config().write_debug_dump();
            Ok(())
        }
    }
}

fn run_dump_session_list(service: &SessionService) -> Result<()> {
    for session in service.list_dashboard_sessions()? {
        println!("{}", session.debug_dump_line());
    }

    Ok(())
}
