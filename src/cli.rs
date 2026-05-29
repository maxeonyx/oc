use clap::{CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::Shell;
use std::io::{self, Write};
use std::path::PathBuf;

const ROOT_LONG_ABOUT: &str = "Interactive TUI session manager for OpenCode\n\n[TARGET] is a session name or alias to attach to.\n\nWorkflow:\n  Use bare `oc` to open the TUI session manager.\n  Use `oc <name>` to attach to a tracked session.";

const ROOT_EXAMPLES: &str = "Examples:\n  $ oc                           # Open the TUI session manager\n  $ oc my-project                # Attach to session 'my-project'\n  $ oc new my-project ~/src/foo  # Create a new session\n  $ oc list                      # List all tracked sessions\n  $ oc list --json               # List as JSON (for scripting)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestedAction {
    Completion {
        shell: CompletionShell,
    },
    New {
        name: String,
        dir: Option<PathBuf>,
        launch_args: Vec<String>,
    },
    Alias {
        name: String,
        dir: Option<PathBuf>,
    },
    Unalias {
        name: String,
    },
    Rm {
        target: String,
    },
    Stop {
        target: String,
    },
    Restart {
        target: String,
    },
    Move {
        target: String,
        new_dir: PathBuf,
    },
    Migrate,
    AttachTarget {
        target: String,
    },
    Default,
    List {
        json: bool,
    },
    DbPath,
    DumpSessionList,
    DumpSessionDebug,
    DumpRuntimeConfig,
    ParseMemoryStatus {
        path: PathBuf,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = "oc",
    version,
    about = "Interactive TUI session manager for OpenCode",
    long_about = ROOT_LONG_ABOUT,
    after_help = ROOT_EXAMPLES
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(help = "Session name or alias to attach to", value_hint = ValueHint::Other)]
    pub target: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

impl From<CompletionShell> for Shell {
    fn from(value: CompletionShell) -> Self {
        match value {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Fish => Shell::Fish,
            CompletionShell::Zsh => Shell::Zsh,
        }
    }
}

impl Cli {
    pub fn requested_action(self) -> RequestedAction {
        match (self.command, self.target) {
            (Some(Command::Completion { shell }), None) => RequestedAction::Completion { shell },
            (
                Some(Command::New {
                    name,
                    dir,
                    launch_args,
                }),
                None,
            ) => RequestedAction::New {
                name,
                dir,
                launch_args,
            },
            (Some(Command::Alias { name, dir }), None) => RequestedAction::Alias { name, dir },
            (Some(Command::Unalias { name }), None) => RequestedAction::Unalias { name },
            (Some(Command::Rm { target }), None) => RequestedAction::Rm { target },
            (Some(Command::Stop { target }), None) => RequestedAction::Stop { target },
            (Some(Command::Restart { target }), None) => RequestedAction::Restart { target },
            (Some(Command::Move { target, new_dir }), None) => {
                RequestedAction::Move { target, new_dir }
            }
            (Some(Command::Migrate), None) => RequestedAction::Migrate,
            (Some(Command::List { json }), None) => RequestedAction::List { json },
            (Some(Command::DbPath), None) => RequestedAction::DbPath,
            (Some(Command::DumpSessionList), None) => RequestedAction::DumpSessionList,
            (Some(Command::DumpSessionDebug), None) => RequestedAction::DumpSessionDebug,
            (Some(Command::DumpRuntimeConfig), None) => RequestedAction::DumpRuntimeConfig,
            (Some(Command::ParseMemoryStatus { path }), None) => {
                RequestedAction::ParseMemoryStatus { path }
            }
            (None, Some(target)) => RequestedAction::AttachTarget { target },
            (None, None) => RequestedAction::Default,
            (Some(_), Some(target)) => {
                panic!("clap should not accept both a subcommand and bare target: {target}")
            }
        }
    }
}

pub fn print_completion(shell: CompletionShell) {
    if shell == CompletionShell::Fish {
        io::stdout()
            .write_all(fish_completion_script().as_bytes())
            .expect("failed to write fish completion");
    } else {
        let mut command = Cli::command();
        let bin_name = command.get_name().to_string();
        let generator: Shell = shell.into();
        let mut output = Vec::new();
        clap_complete::generate(generator, &mut command, bin_name, &mut output);

        io::stdout()
            .write_all(&output)
            .expect("failed to write completion");
    }
}

fn fish_completion_script() -> &'static str {
    r#"complete -c oc -f

function __fish_oc_global_optspecs
    string join \n h/help V/version
end

function __fish_oc_needs_command
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_oc_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_oc_using_subcommand
    set -l cmd (__fish_oc_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

function __fish_oc_session_names
    command oc __dump-session-list 2>/dev/null
end

complete -c oc -n '__fish_oc_needs_command' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_needs_command' -s V -l version -d 'Print version'
complete -c oc -n '__fish_oc_needs_command' -k -a "(__fish_oc_session_names)"
complete -c oc -n '__fish_oc_needs_command' -k -a alias -d 'Create an alias for a directory'
complete -c oc -n '__fish_oc_needs_command' -k -a completion -d 'Generate shell completion scripts'
complete -c oc -n '__fish_oc_needs_command' -k -a d -d 'Remove a session from the database'
complete -c oc -n '__fish_oc_needs_command' -k -a db-path -d 'Print the database path'
complete -c oc -n '__fish_oc_needs_command' -k -a delete -d 'Remove a session from the database'
complete -c oc -n '__fish_oc_needs_command' -k -a help -d 'Print this message or the help of the given subcommand(s)'
complete -c oc -n '__fish_oc_needs_command' -k -a list -d 'List sessions'
complete -c oc -n '__fish_oc_needs_command' -k -a migrate -d 'Migrate legacy aliases into the database'
complete -c oc -n '__fish_oc_needs_command' -k -a mv -d 'Move a session to a new directory'
complete -c oc -n '__fish_oc_needs_command' -k -a n -d 'Create a new OpenCode session'
complete -c oc -n '__fish_oc_needs_command' -k -a new -d 'Create a new OpenCode session'
complete -c oc -n '__fish_oc_needs_command' -k -a restart -d 'Restart a session'
complete -c oc -n '__fish_oc_needs_command' -k -a rm -d 'Remove a session from the database'
complete -c oc -n '__fish_oc_needs_command' -k -a stop -d 'Stop a running session'
complete -c oc -n '__fish_oc_needs_command' -k -a unalias -d 'Remove a saved directory alias'

complete -c oc -n '__fish_oc_using_subcommand alias' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand alias; and __fish_is_nth_token 3' -r -a '(__fish_complete_directories)'

complete -c oc -n '__fish_oc_using_subcommand completion' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand completion; and __fish_is_nth_token 2' -a bash -d 'Bash'
complete -c oc -n '__fish_oc_using_subcommand completion; and __fish_is_nth_token 2' -a fish -d 'Fish'
complete -c oc -n '__fish_oc_using_subcommand completion; and __fish_is_nth_token 2' -a zsh -d 'Zsh'

complete -c oc -n '__fish_oc_using_subcommand d delete rm' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand d delete rm; and __fish_is_nth_token 2' -a "(__fish_oc_session_names)"

complete -c oc -n '__fish_oc_using_subcommand db-path' -s h -l help -d 'Print help'

complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a alias -d 'Create an alias for a directory'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a completion -d 'Generate shell completion scripts'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a d -d 'Remove a session from the database'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a db-path -d 'Print the database path'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a delete -d 'Remove a session from the database'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a help -d 'Print this message or the help of the given subcommand(s)'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a list -d 'List sessions'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a migrate -d 'Migrate legacy aliases into the database'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a mv -d 'Move a session to a new directory'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a n -d 'Create a new OpenCode session'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a new -d 'Create a new OpenCode session'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a restart -d 'Restart a session'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a rm -d 'Remove a session from the database'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a stop -d 'Stop a running session'
complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a unalias -d 'Remove a saved directory alias'

complete -c oc -n '__fish_oc_using_subcommand list' -l json -d 'Render tracked sessions as JSON'
complete -c oc -n '__fish_oc_using_subcommand list' -s h -l help -d 'Print help'

complete -c oc -n '__fish_oc_using_subcommand migrate' -s h -l help -d 'Print help'

complete -c oc -n '__fish_oc_using_subcommand mv' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand mv; and __fish_is_nth_token 2' -a "(__fish_oc_session_names)"
complete -c oc -n '__fish_oc_using_subcommand mv; and __fish_is_nth_token 3' -r -a '(__fish_complete_directories)'

complete -c oc -n '__fish_oc_using_subcommand n new' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand n new; and __fish_is_nth_token 3' -r -a '(__fish_complete_directories)'

complete -c oc -n '__fish_oc_using_subcommand restart' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand restart; and __fish_is_nth_token 2' -a "(__fish_oc_session_names)"

complete -c oc -n '__fish_oc_using_subcommand stop' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand stop; and __fish_is_nth_token 2' -a "(__fish_oc_session_names)"

complete -c oc -n '__fish_oc_using_subcommand unalias' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand unalias; and __fish_is_nth_token 2' -a "(__fish_oc_session_names)"
"#
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Generate shell completion scripts")]
    Completion {
        #[arg(help = "Shell to generate completions for")]
        shell: CompletionShell,
    },
    #[command(visible_alias = "n", about = "Create a new OpenCode session")]
    New {
        #[arg(help = "Session name", value_hint = ValueHint::Other)]
        name: String,
        #[arg(
            help = "Working directory (defaults to current)",
            value_hint = ValueHint::DirPath
        )]
        dir: Option<PathBuf>,
        #[arg(help = "Additional arguments passed to OpenCode", last = true)]
        launch_args: Vec<String>,
    },
    #[command(about = "Create an alias for a directory")]
    Alias {
        #[arg(help = "Alias name", value_hint = ValueHint::Other)]
        name: String,
        #[arg(
            help = "Working directory (defaults to current)",
            value_hint = ValueHint::DirPath
        )]
        dir: Option<PathBuf>,
    },
    #[command(about = "Remove a saved directory alias")]
    Unalias {
        #[arg(help = "Alias name to remove", value_hint = ValueHint::Other)]
        name: String,
    },
    #[command(name = "rm", visible_aliases = ["delete", "d"], about = "Remove a session from the database")]
    Rm {
        #[arg(help = "Session name or alias to remove", value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(about = "Stop a running session")]
    Stop {
        #[arg(help = "Session name or alias to stop", value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(about = "Restart a session")]
    Restart {
        #[arg(help = "Session name or alias to restart", value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(name = "mv", about = "Move a session to a new directory")]
    Move {
        #[arg(help = "Session to move", value_hint = ValueHint::Other)]
        target: String,
        #[arg(help = "New working directory", value_hint = ValueHint::DirPath)]
        new_dir: PathBuf,
    },
    #[command(about = "Migrate legacy aliases into the database")]
    Migrate,
    #[command(about = "List tracked sessions")]
    List {
        #[arg(help = "Output as JSON for scripting", long)]
        json: bool,
    },
    #[command(name = "db-path", about = "Print the database path")]
    DbPath,
    #[command(name = "__dump-session-list", hide = true)]
    DumpSessionList,
    #[command(name = "__dump-session-debug", hide = true)]
    DumpSessionDebug,
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
    #[command(name = "__parse-memory-status", hide = true)]
    ParseMemoryStatus {
        #[arg(value_hint = ValueHint::FilePath)]
        path: PathBuf,
    },
}
