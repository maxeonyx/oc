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
    TooManyArguments(String),
}

pub fn parse_command(input: &str) -> Result<ParsedCommand, CommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CommandParseError::Empty);
    }

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or_default();

    match command {
        "new" | "n" => parse_single_arg(command, parts).map(|name| ParsedCommand::New { name }),
        "rm" | "delete" | "d" => {
            parse_single_arg(command, parts).map(|target| ParsedCommand::Remove { target })
        }
        "stop" => parse_single_arg(command, parts).map(|target| ParsedCommand::Stop { target }),
        "restart" => {
            parse_single_arg(command, parts).map(|target| ParsedCommand::Restart { target })
        }
        "mv" => {
            let target = parts
                .next()
                .ok_or_else(|| CommandParseError::MissingArgument(String::from(command)))?;
            let remaining = parts.collect::<Vec<_>>();
            if remaining.is_empty() {
                return Err(CommandParseError::MissingArgument(String::from(command)));
            }

            Ok(ParsedCommand::Move {
                target: String::from(target),
                new_dir: PathBuf::from(remaining.join(" ")),
            })
        }
        _ => Err(CommandParseError::UnknownCommand(String::from(command))),
    }
}

fn parse_single_arg<'a>(
    command: &str,
    mut parts: impl Iterator<Item = &'a str>,
) -> Result<String, CommandParseError> {
    let argument = parts
        .next()
        .ok_or_else(|| CommandParseError::MissingArgument(String::from(command)))?;
    if parts.next().is_some() {
        return Err(CommandParseError::TooManyArguments(String::from(command)));
    }

    Ok(String::from(argument))
}
