use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    symbols::border,
    text::Line,
    widgets::Block,
    DefaultTerminal, Frame,
};

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(render)?;
        if matches!(event::read()?, Event::Key(_)) {
            break Ok(());
        }
    }
}

fn render(frame: &mut Frame) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    let diff = {
        let title = Line::from(" Welcome to client (diff) ");
        Block::bordered().title(title).border_set(border::PLAIN)
    };
    let metadata = {
        let title = Line::from(" Welcome to client (metadata) ");
        Block::bordered().title(title).border_set(border::PLAIN)
    };
    frame.render_widget(diff, layout[0]);
    frame.render_widget(metadata, layout[1]);
}
