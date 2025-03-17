use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use log::{info, warn};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
    DefaultTerminal,
    Frame,
};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::sync::CancellationToken;

use crate::models::patchpal::{patch_response::Status, Patch, PatchResponse};

#[derive(Debug, Clone)]
pub struct PatchRequest {
    pub patch: Patch,
    pub response_chan: Sender<PatchResponse>,
}

#[derive(Debug)]
pub struct App {
    submit_rx: Receiver<PatchRequest>,
    active_patch: Option<PatchRequest>,
    exit: bool,
}

impl App {
    pub fn new(submit_rx: Receiver<PatchRequest>) -> Self {
        App {
            submit_rx,
            active_patch: None,
            exit: false,
        }
    }

    /// runs the application's main loop until the user quits
    pub async fn run(
        &mut self,
        token: &CancellationToken,
        terminal: &mut DefaultTerminal,
    ) -> anyhow::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events(token).await?;
        }
        ratatui::restore();
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    async fn handle_events(&mut self, token: &CancellationToken) -> anyhow::Result<()> {
        let mut reader = EventStream::new();

        // TODO: switch on event::read + rx
        select! {
            event = reader.next() => {
                if let Some(event) = event {
                    match event? {
                        // it's important to check that the event is a key press event as
                        // crossterm also emits key release and repeat events on Windows.
                        Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                            self.handle_key_event(key_event).await;
                        }
                        _ => {}
                    }
                }
            },
            patch = self.submit_rx.recv() => {
                if let Some(patch) = patch {
                    info!("Recvd patch w/ metadata: {}", patch.patch.metadata);
                    self.handle_new_patch(patch);
                }
            },
            _ = token.cancelled() => {
                info!("Shutting down from signal");
                self.exit = true;
            }
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) {
        info!("handling keys for {:?}", key_event);
        match key_event {
            // must support <C-q> as well, since we run in raw mode
            KeyEvent {
                code: KeyCode::Char('q'),
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.exit(),
            KeyEvent {
                code: KeyCode::Char('y'),
                ..
            } => {
                info!("got `yes` reponse");
                self.handle_patch_response(PatchResponse {
                    status: Status::Accepted.into(),
                })
                .await;
            }
            KeyEvent {
                code: KeyCode::Char('n'),
                ..
            } => {
                info!("got `no` reponse");
                self.handle_patch_response(PatchResponse {
                    status: Status::Rejected.into(),
                })
                .await;
            }
            _ => {}
        }
    }

    fn handle_new_patch(&mut self, patch: PatchRequest) {
        self.active_patch = Some(patch);
    }

    async fn handle_patch_response(&mut self, response: PatchResponse) {
        // TODO: should we check the queue first?
        info!("handling patch reponse: {:?}", response);
        let _ = self
            .active_patch
            .clone()
            .expect("can't handle response w/o patch")
            .response_chan
            .send(response)
            .await;
        self.active_patch = None;
        info!("active_patch is now None")
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // PERF: would prefer not to recreate this each render
        let patches = self
            .active_patch
            .as_ref()
            .map(|p| patch::Patch::from_multiple(&p.patch.patch));

        // TODO: should handle them all
        if let Some(Err(e)) = &patches {
            warn!("Err: {:?}", e);
        }

        let title = match patches {
            None => Line::from(" Patchpal (waiting..) ".bold()),
            Some(patch) => {
                let patch = patch.unwrap()[0].clone();
                Line::from(vec![
                    " Src:".into(),
                    format!(" {} ", patch.old.path.clone()).red().bold(),
                    "Dst:".into(),
                    format!(" {} ", patch.new.path.clone()).green().bold(),
                ])
            }
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
            // TODO: not implemented
            //// all
            //"a".green().bold(),
            //"ll,".into(),
            //// done
            //"d".red().bold(),
            //"one,".into(),
            // quit
            "q".blue().bold(),
            "uit".into(),
            "] ".into(),
        ]);

        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        if let Some(patch) = &self.active_patch {
            DiffWidget::new(&patch).render(block.inner(area), buf);
        }

        Paragraph::default().block(block).render(area, buf);
    }
}

struct DiffWidget<'a> {
    inner: patch::Patch<'a>,
    metadata: &'a str,
}

impl<'a> DiffWidget<'a> {
    fn new(req: &'a PatchRequest) -> DiffWidget<'a> {
        let patches = patch::Patch::from_multiple(&req.patch.patch).unwrap();
        DiffWidget {
            inner: patches[0].clone(),
            metadata: &req.patch.metadata,
        }
    }
}

impl Widget for DiffWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(2), Constraint::Fill(4)])
            .split(area);

        Paragraph::new(Line::from(vec!["Metadata: ".blue(), self.metadata.into()]))
            .wrap(Wrap { trim: true })
            .block(Block::new().borders(Borders::BOTTOM))
            .render(chunks[0], buf);

        let mut diff_text = Text::from(vec![]);
        for hunk in self.inner.hunks {
            for line in hunk.lines {
                match line {
                    patch::Line::Add(l) => diff_text.lines.push(Line::from(l.green())),
                    patch::Line::Remove(l) => diff_text.lines.push(Line::from(l.red())),
                    patch::Line::Context(_) => {}
                }
            }
        }
        Paragraph::new(diff_text)
            .block(Block::new())
            .render(chunks[1], buf);
    }
}
