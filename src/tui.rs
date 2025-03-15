use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind};
use futures_util::StreamExt;
use log::info;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal,
    Frame,
};
use tokio::{select, sync::mpsc::Receiver};

use crate::models::patchpal::Patch;

#[derive(Debug, Default)]
pub struct App {
    counter: u8,
    active_patch: Option<Patch>,
    exit: bool,
}

impl App {
    /// runs the application's main loop until the user quits
    pub async fn run(
        &mut self,
        terminal: &mut DefaultTerminal,
        rx: &mut Receiver<Patch>,
    ) -> anyhow::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events(rx).await?;
        }
        ratatui::restore();
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    async fn handle_events(&mut self, rx: &mut Receiver<Patch>) -> anyhow::Result<()> {
        let mut reader = EventStream::new();

        // TODO: switch on event::read + rx
        select! {
            event = reader.next() => {
                if let Some(event) = event {
                    match event? {
                        // it's important to check that the event is a key press event as
                        // crossterm also emits key release and repeat events on Windows.
                        Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                            self.handle_key_event(key_event)
                        }
                        _ => {}
                    }
                }
            },
            patch = rx.recv() => {
                if let Some(patch) = patch {
                    info!("Recvd patch w/ metadata: {}", patch.metadata);
                    self.handle_patch_event(patch);
                }
            },
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Left => self.decrement_counter(),
            KeyCode::Right => self.increment_counter(),
            _ => {}
        }
    }

    fn handle_patch_event(&mut self, patch: Patch) {
        self.active_patch = Some(patch);
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn increment_counter(&mut self) {
        self.counter += 1;
    }

    fn decrement_counter(&mut self) {
        self.counter -= 1;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // PERF: would prefer not to recreate this each render
        let patch = self
            .active_patch
            .as_ref()
            .map(|p| patch::Patch::from_single(&p.patch).unwrap());

        let title = match &patch {
            None => Line::from(" Patchpal (waiting..) ".bold()),
            Some(patch) => Line::from(vec![
                " From:".into(),
                format!(" {} ", patch.old.path.clone()).red().bold(),
                "To:".into(),
                format!(" {} ", patch.new.path.clone()).green().bold(),
            ]),
        };

        // (1/1) Stage this hunk [y,n,q,a,d,e,?]?
        let instructions = Line::from(vec![
            " Stage this patch ".into(),
            "[".into(),
            // yes
            "y".light_green().bold(),
            "es,".into(),
            // no
            "n".light_red().bold(),
            "o,".into(),
            // all
            "a".green().bold(),
            "ll,".into(),
            // done
            "d".red().bold(),
            "one,".into(),
            // quit
            "q".blue().bold(),
            "uit".into(),
            "] ".into(),
        ]);

        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        let mut text = Text::from(vec![]);
        if let Some(patch) = patch {
            let (old_path, new_path) = (patch.old.path.into_owned(), patch.new.path.into_owned());
            let (mut old_content, mut new_content) = (
                vec![Line::from(vec!["Old: ".into(), old_path.red()])],
                vec![Line::from(vec!["New: ".into(), new_path.green()])],
            );
            for hunk in patch.hunks {
                for line in hunk.lines {
                    match line {
                        patch::Line::Add(l) => new_content.push(Line::from(l.green())),
                        patch::Line::Remove(l) => old_content.push(Line::from(l.red())),
                        patch::Line::Context(_) => {}
                    }
                }
            }

            text.lines.append(&mut old_content);
            text.lines.append(&mut new_content);
        }

        Paragraph::new(text).block(block).render(area, buf);
    }
}
