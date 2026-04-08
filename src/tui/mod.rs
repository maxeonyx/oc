mod app;
mod model;
mod render;

use anyhow::Result;

use crate::service::SessionService;

pub fn run_dashboard(service: &SessionService) -> Result<()> {
    app::run(service)
}
