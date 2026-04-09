pub mod command;
pub mod filter;
pub mod format;
pub mod input;
pub mod render;
pub mod selection;
pub mod state;
pub mod types;

use anyhow::Result;

use crate::service::SessionService;

pub fn run_dashboard(service: &SessionService) -> Result<()> {
    run_dashboard_with_status(service, None)
}

pub fn run_dashboard_with_status(
    service: &SessionService,
    status_message: Option<String>,
) -> Result<()> {
    state::run(service, status_message)
}
