use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommand {
    New { name: String },
    Remove { target: String },
    Stop { target: String },
    Restart { target: String },
    Move { target: String, new_dir: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandParseError {
    Empty,
    UnknownCommand(String),
    MissingArgument(String),
}

pub fn parse_command(input: &str) -> Result<ParsedCommand, CommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CommandParseError::Empty);
    }

    let command = trimmed.split_whitespace().next().unwrap_or_default();
    Err(CommandParseError::UnknownCommand(String::from(command)))
}
