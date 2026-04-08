use anyhow::Result;
use std::collections::HashMap;

use crate::session::{ManagedSessionRuntime, SavedSession, SessionListEntry};

pub fn merge_saved_and_runtime_sessions_with_prefix(
    saved_sessions: Vec<SavedSession>,
    runtimes: Vec<ManagedSessionRuntime>,
    tmux_prefix: &str,
) -> Result<Vec<SessionListEntry>> {
    let runtime_by_name = runtimes
        .into_iter()
        .map(|runtime| (runtime.tmux_session_name.clone(), runtime))
        .collect::<HashMap<_, _>>();

    Ok(saved_sessions
        .into_iter()
        .map(|saved_session| {
            let tmux_session_name = saved_session.managed_tmux_session_name(tmux_prefix);
            let runtime = runtime_by_name.get(&tmux_session_name);
            SessionListEntry::from_saved_session(saved_session, runtime)
        })
        .collect())
}
