use clap::{CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::Shell;
use std::io::{self, Write};
use std::path::PathBuf;

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
    about = "Interactive TUI session manager for OpenCode"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(value_hint = ValueHint::Other)]
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
complete -c oc -n '__fish_oc_needs_command' -k -a '
alias\tCreate an alias for a directory
completion\tGenerate shell completion scripts
d\tRemove a session from the database
db-path\tPrint the database path
delete\tRemove a session from the database
help\tPrint this message or the help of the given subcommand(s)
migrate\tMigrate legacy aliases into the database
mv\tMove a session to a new directory
n\tCreate a new OpenCode session
new\tCreate a new OpenCode session
restart\tRestart a session
rm\tRemove a session from the database
stop\tStop a running session
unalias\tRemove a saved directory alias'

complete -c oc -n '__fish_oc_using_subcommand alias' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand alias; and __fish_is_nth_token 3' -r -a '(__fish_complete_directories)'

complete -c oc -n '__fish_oc_using_subcommand completion' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand completion; and __fish_is_nth_token 2' -a '
bash\tBash
fish\tFish
zsh\tZsh'

complete -c oc -n '__fish_oc_using_subcommand d delete rm' -s h -l help -d 'Print help'
complete -c oc -n '__fish_oc_using_subcommand d delete rm; and __fish_is_nth_token 2' -a "(__fish_oc_session_names)"

complete -c oc -n '__fish_oc_using_subcommand db-path' -s h -l help -d 'Print help'

complete -c oc -n '__fish_oc_using_subcommand help; and __fish_is_nth_token 2' -a '
alias\tCreate an alias for a directory
completion\tGenerate shell completion scripts
d\tRemove a session from the database
db-path\tPrint the database path
delete\tRemove a session from the database
help\tPrint this message or the help of the given subcommand(s)
migrate\tMigrate legacy aliases into the database
mv\tMove a session to a new directory
n\tCreate a new OpenCode session
new\tCreate a new OpenCode session
restart\tRestart a session
rm\tRemove a session from the database
stop\tStop a running session
unalias\tRemove a saved directory alias'

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
    Completion { shell: CompletionShell },
    #[command(visible_alias = "n", about = "Create a new OpenCode session")]
    New {
        #[arg(value_hint = ValueHint::Other)]
        name: String,
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
        #[arg(last = true)]
        launch_args: Vec<String>,
    },
    #[command(about = "Create an alias for a directory")]
    Alias {
        #[arg(value_hint = ValueHint::Other)]
        name: String,
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
    #[command(about = "Remove a saved directory alias")]
    Unalias {
        #[arg(value_hint = ValueHint::Other)]
        name: String,
    },
    #[command(name = "rm", visible_aliases = ["delete", "d"], about = "Remove a session from the database")]
    Rm {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(about = "Stop a running session")]
    Stop {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(about = "Restart a session")]
    Restart {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(name = "mv", about = "Move a session to a new directory")]
    Move {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
        #[arg(value_hint = ValueHint::DirPath)]
        new_dir: PathBuf,
    },
    #[command(about = "Migrate legacy aliases into the database")]
    Migrate,
    #[command(about = "List tracked sessions")]
    List {
        #[arg(long)]
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
