use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};

#[derive(Default)]
struct State {
    pending_chars: Vec<char>,
}

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let mut state = State::default();
    let result = run(terminal, &mut state);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal, state: &mut State) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, state))?;
        match event::read()? {
            Event::Key(e)
                if e.code == KeyCode::Char('q')
                    || (e.code == KeyCode::Char('c') && e.modifiers == KeyModifiers::CONTROL) =>
            {
                break Ok(())
            }
            Event::Key(event::KeyEvent {
                code: KeyCode::Char(c),
                ..
            }) => state.pending_chars.push(c),
            _ => continue,
        }
    }
}

fn render(frame: &mut Frame, state: &mut State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    {
        let title = Line::from(" Welcome to diff-client (press q to quit) ");
        let block = Block::bordered().title(title).border_set(border::PLAIN);
        let text = Text::from(vec![Line::from(format!("{:?}", state.pending_chars))]);
        let paragraph = Paragraph::new(text).centered().block(block);

        frame.render_widget(paragraph, layout[0])
    };
    {
        let title = Line::from(" Welcome to metadata-client (press q to quit) ");
        let block = Block::bordered().title(title).border_set(border::PLAIN);

        frame.render_widget(block, layout[1]);
    };
}
