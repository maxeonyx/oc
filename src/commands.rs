use anyhow::Result;

use crate::cli::RequestedAction;
use crate::service::SessionService;

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
        RequestedAction::AttachTarget { target } => service.activate_target(&target),
        RequestedAction::Default => {
            service.auto_attach_directory_match()?;
            Ok(())
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
