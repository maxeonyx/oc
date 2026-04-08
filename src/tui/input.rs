use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIntent {
    QuitOrClear,
    MoveUp,
    MoveDown,
    CycleLeft,
    CycleRight,
    Submit,
    Backspace,
    EnterCommandMode,
    InsertChar(char),
    Ignore,
}

pub fn map_key_event(key_event: KeyEvent, command_mode: bool) -> InputIntent {
    match key_event.code {
        KeyCode::Esc => InputIntent::QuitOrClear,
        KeyCode::Up => InputIntent::MoveUp,
        KeyCode::Down => InputIntent::MoveDown,
        KeyCode::Left => InputIntent::CycleLeft,
        KeyCode::Right => InputIntent::CycleRight,
        KeyCode::Enter => InputIntent::Submit,
        KeyCode::Backspace => InputIntent::Backspace,
        KeyCode::Char(' ') if !command_mode => InputIntent::EnterCommandMode,
        KeyCode::Char(character)
            if key_event.modifiers.is_empty() || key_event.modifiers == KeyModifiers::SHIFT =>
        {
            InputIntent::InsertChar(character)
        }
        _ => InputIntent::Ignore,
    }
}
