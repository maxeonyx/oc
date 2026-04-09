use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIntent {
    ClearInput,
    Quit,
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
    if key_event.modifiers == KeyModifiers::CONTROL {
        return match key_event.code {
            KeyCode::Char('c') => InputIntent::ClearInput,
            KeyCode::Char('d') => InputIntent::Quit,
            _ => InputIntent::Ignore,
        };
    }

    match key_event.code {
        KeyCode::Esc => InputIntent::Quit,
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
