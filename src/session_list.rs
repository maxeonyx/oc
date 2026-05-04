use anyhow::Result;
use std::collections::HashMap;

use crate::session::{ManagedSessionRuntime, SavedSession, SessionListEntry, SessionStatus};

pub fn merge_saved_and_runtime_sessions_with_prefix(
    saved_sessions: Vec<SavedSession>,
    runtimes: Vec<ManagedSessionRuntime>,
    tmux_prefix: &str,
) -> Result<Vec<SessionListEntry>> {
    let runtime_by_name = runtimes
        .into_iter()
        .map(|runtime| (runtime.tmux_session_name.clone(), runtime))
        .collect::<HashMap<_, _>>();

    let mut entries = saved_sessions
        .into_iter()
        .map(|saved_session| {
            let tmux_session_name = saved_session.managed_tmux_session_name(tmux_prefix);
            let runtime = runtime_by_name.get(&tmux_session_name);
            SessionListEntry::from_saved_session(saved_session, runtime)
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        status_rank(left.status)
            .cmp(&status_rank(right.status))
            .then_with(|| {
                right
                    .saved_session
                    .last_used_at
                    .cmp(&left.saved_session.last_used_at)
            })
            .then_with(|| right.saved_session.id.cmp(&left.saved_session.id))
    });

    Ok(entries)
}

fn status_rank(status: SessionStatus) -> u8 {
    match status {
        SessionStatus::RunningAttached => 0,
        SessionStatus::RunningDetached => 1,
        SessionStatus::Saved => 2,
    }
}
