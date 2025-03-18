use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use log::info;
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph, StatefulWidget, Widget, Wrap},
    DefaultTerminal, Frame,
};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::sync::CancellationToken;
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};
use unidiff::PatchSet;

use crate::models::patchpal::{patch_response::Status, Patch, PatchResponse};

#[derive(Debug, Clone)]
pub struct PatchRequest {
    pub patch_set: PatchSet,
    pub metadata: String,
    pub response_chan: Sender<PatchResponse>,
}

impl TryFrom<(Patch, Sender<PatchResponse>)> for PatchRequest {
    type Error = anyhow::Error;

    fn try_from((patch, response_chan): (Patch, Sender<PatchResponse>)) -> anyhow::Result<Self> {
        let metadata = patch.metadata;
        let patch_set = patch.patch.parse::<PatchSet>()?;

        Ok(PatchRequest {
            patch_set,
            metadata,
            response_chan,
        })
    }
}

#[derive(Debug)]
pub struct App {
    submit_rx: Receiver<PatchRequest>,
    active_patch: Option<PatchRequest>,
    exit: bool,
    scroll_state: ScrollViewState,
}

impl App {
    pub fn new(submit_rx: Receiver<PatchRequest>) -> Self {
        App {
            submit_rx,
            active_patch: None,
            exit: false,
            scroll_state: ScrollViewState::new(),
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

    fn draw(&mut self, frame: &mut Frame) {
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
                    info!("Recvd patch w/ metadata: {}", patch.metadata);
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
            KeyEvent {
                code: KeyCode::Char('k'),
                ..
            } => {
                self.scroll_state.scroll_up();
            }
            KeyEvent {
                code: KeyCode::Char('j'),
                ..
            } => {
                self.scroll_state.scroll_down();
            }
            KeyEvent {
                code: KeyCode::Char('g'),
                ..
            } => {
                self.scroll_state.scroll_to_top();
            }
            KeyEvent {
                code: KeyCode::Char('G'),
                ..
            } => {
                self.scroll_state.scroll_to_bottom();
            }
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.scroll_state.scroll_page_up();
            }
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.scroll_state.scroll_page_down();
            }
            _ => {}
        }
    }

    fn handle_new_patch(&mut self, request: PatchRequest) {
        self.active_patch = Some(request);
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

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.active_patch {
            None => Line::from(" Patchpal (waiting..) ".bold()),
            Some(_) => Line::from(" Patchpal ".bold()),
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

        let block = Block::new()
            .title(title.centered())
            .title_bottom(instructions.centered());

        if let Some(patch) = &self.active_patch {
            DiffWidget {
                inner: &patch.patch_set,
                metadata: &patch.metadata,
            }
            .render(block.inner(area), buf, &mut self.scroll_state);
        }

        Paragraph::default().block(block).render(area, buf);
    }
}

struct DiffWidget<'a> {
    inner: &'a PatchSet,
    metadata: &'a str,
}

impl StatefulWidget for DiffWidget<'_> {
    type State = ScrollViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let metadata = Paragraph::new(Line::from(vec!["Metadata: ".blue(), self.metadata.into()]));

        let mut patch_offset_y =
            area.top() + metadata.line_count(metadata.line_width() as u16) as u16;
        let mut hunks_render_info = vec![];
        for patch in self.inner.files() {
            // TODO: print the file name too
            let mut hunk_offset_y = 0u16;
            for hunk in patch.hunks() {
                let mut hunk_text = Text::from(vec![]);
                for line in hunk.lines() {
                    match line {
                        l if l.is_added() => {
                            hunk_text.lines.push(Line::from(l.value.clone().green()))
                        }
                        l if l.is_removed() => {
                            hunk_text.lines.push(Line::from(l.value.clone().red()))
                        }
                        l if l.is_context() => {
                            hunk_text.lines.push(Line::from(l.value.clone().dim()))
                        }
                        _ => unreachable!(),
                    }
                }

                let hunk_area = Rect {
                    x: area.left(),
                    y: patch_offset_y + hunk_offset_y,
                    width: area.width - 1,
                    height: hunk_text.height() as u16,
                };
                hunk_offset_y += hunk_text.height() as u16;

                let hunk_paragraph = Paragraph::new(hunk_text).block(Block::bordered());
                hunks_render_info.push((hunk_area, hunk_paragraph));
            }
            patch_offset_y += hunk_offset_y;
        }
        let mut scroll_view = ScrollView::new(Size::new(area.width, patch_offset_y))
            .scrollbars_visibility(ScrollbarVisibility::Never);

        scroll_view.render_widget(
            metadata
                .clone()
                .wrap(Wrap { trim: true })
                .block(Block::new().padding(Padding::horizontal(1))),
            area,
        );
        for (hunk_area, hunk_paragraph) in hunks_render_info {
            scroll_view.render_widget(hunk_paragraph, hunk_area);
        }
        scroll_view.render(area, buf, state);
    }
}
