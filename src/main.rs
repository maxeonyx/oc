use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "oc",
    version,
    about = "Interactive TUI session manager for OpenCode"
)]
struct Cli {}

fn main() {
    let _ = Cli::parse();
}
