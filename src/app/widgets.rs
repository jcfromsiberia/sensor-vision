use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use widgetui::{constraint, layout, Events, WidgetFrame};

pub(super) trait ModalDialog {
    fn render(&self, frame: &mut WidgetFrame);

    fn handle_events(&mut self, events: &Events);
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum DialogButton {
    Ok,
    Cancel,
}

impl DialogButton {
    fn render(&self, frame: &mut WidgetFrame, area: Rect, focused: Option<DialogButton>) {
        let text = match self {
            Self::Ok => "OK",
            Self::Cancel => "CANCEL",
        };

        let mut button_block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_type(BorderType::Rounded)
            .style(Style::new().bg(Color::Gray));

        if let Some(focused) = focused {
            if focused == *self {
                button_block = button_block.style(Style::new().bg(Color::LightBlue));
            }
        }

        let button = Paragraph::new(text).block(button_block);

        frame.render_widget(button, area);
    }
}

pub(super) struct TextModal {
    pub title: String,
    pub text: String,

    pub accept_callback: Box<dyn Fn() + Send + Sync>,
    pub close_callback: Box<dyn Fn() + Send + Sync>,

    pub focused_button: Option<DialogButton>,
}
impl ModalDialog for TextModal {
    fn render(&self, frame: &mut WidgetFrame) {
        let area = frame.size();
        let area = centered_area(30, 20, area);

        let instructions = Line::from(vec![
            " Select Button ".into(),
            "<Tab>".blue().bold(),
            " Press ".into(),
            "<Enter>".blue().bold(),
            " Close ".into(),
            "<Esc> ".blue().bold(),
        ]);
        let pad = Block::bordered()
            .title(Line::from(self.title.as_str()).centered())
            .title_bottom(instructions.centered())
            .bg(Color::Indexed(172));

        let content_layout = &layout! {
            area,
            // Header
            (#1),
            // Text area
            (%100),
            // Buttons Area { [   OK   ]_[  CANCEL  ] }
            (#1) => { >1, #10, #1, #10, >1 },
            // Footer
            (#1)
        };
        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[1][0]);

        DialogButton::Ok.render(frame, content_layout[2][1], self.focused_button);
        DialogButton::Cancel.render(frame, content_layout[2][3], self.focused_button);
    }

    fn handle_events(&mut self, events: &Events) {
        if let Some(event) = &events.event {
            match event {
                Event::Key(
                    key_event @ KeyEvent {
                        kind: KeyEventKind::Press,
                        ..
                    },
                ) => {
                    match key_event.code {
                        KeyCode::Esc => {
                            (self.close_callback)();
                        }

                        KeyCode::Enter => {
                            if self.focused_button.is_none() {
                                return;
                            }
                            match self.focused_button.unwrap() {
                                DialogButton::Ok => {
                                    (self.accept_callback)();
                                    (self.close_callback)();
                                }
                                DialogButton::Cancel => {
                                    (self.close_callback)();
                                }
                            }
                        }

                        KeyCode::Tab => {
                            if let Some(_button @ DialogButton::Ok) = self.focused_button {
                                self.focused_button = Some(DialogButton::Cancel);
                            } else {
                                self.focused_button = Some(DialogButton::Ok);
                            }
                        }

                        _ => {}
                    };
                }
                _ => {}
            }
        }
    }
}

pub struct InputModal {
    pub title: String,
    pub text: String,
    pub label: String,

    pub text_input: Option<String>,

    pub accept_callback: Box<dyn Fn(String) + Send + Sync>,
    pub close_callback: Box<dyn Fn() + Send + Sync>,

    pub focused_button: Option<DialogButton>,
}

impl ModalDialog for InputModal {
    fn render(&self, frame: &mut WidgetFrame) {
        let area = frame.size();
        let area = centered_area(30, 20, area);

        let instructions = Line::from(vec![
            " Select Button ".into(),
            "<Tab>".blue().bold(),
            " Press ".into(),
            "<Enter>".blue().bold(),
            " Close ".into(),
            "<Esc> ".blue().bold(),
        ]);

        let pad = Block::bordered()
            .title(Line::from(self.title.clone()).centered())
            .title_bottom(instructions.centered())
            .bg(Color::Indexed(172));

        let content_layout = &layout! {
            area,
            // #0 Header
            (#1),
            // #1 Text area
            (%100),
            // #2 Text input { Label [<input>          ] }
            (#1) => { #1, #10, %80, >1 },
            // #3 Buttons Area { [   OK   ]_[  CANCEL  ] }
            (#1) => { >1, #10, #1, #10, >1 },
            // #4 Footer
            (#1)
        };
        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        let label = Line::from(self.label.as_str());
        let text_input_pad = Block::new().bg(Color::Indexed(75));
        let text_input = Line::from(
            self.text_input
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or_else(|| "<input>"),
        );

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[1][0]);
        frame.render_widget(label, content_layout[2][1]);
        frame.render_widget(text_input_pad, content_layout[2][2]);
        frame.render_widget(text_input, content_layout[2][2]);

        DialogButton::Ok.render(frame, content_layout[3][1], self.focused_button);
        DialogButton::Cancel.render(frame, content_layout[3][3], self.focused_button);
    }

    fn handle_events(&mut self, events: &Events) {
        if let Some(event) = &events.event {
            match event {
                Event::Key(
                    key_event @ KeyEvent {
                        kind: KeyEventKind::Press,
                        ..
                    },
                ) => {
                    match key_event.code {
                        KeyCode::Esc => {
                            (self.close_callback)();
                        }

                        KeyCode::Enter => {
                            if self.focused_button.is_none() {
                                return;
                            }
                            match self.focused_button.unwrap() {
                                DialogButton::Ok => {
                                    (self.accept_callback)(
                                        self.text_input.as_ref().unwrap().clone(),
                                    );
                                    (self.close_callback)();
                                }
                                DialogButton::Cancel => {
                                    (self.close_callback)();
                                }
                            }
                        }

                        KeyCode::Tab => {
                            if let Some(_button @ DialogButton::Ok) = self.focused_button {
                                self.focused_button = Some(DialogButton::Cancel);
                            } else {
                                self.focused_button = Some(DialogButton::Ok);
                            }
                        }

                        KeyCode::Char(char) => {
                            self.text_input
                                .get_or_insert_with(|| String::new())
                                .push(char);
                        }

                        KeyCode::Backspace => {
                            if let Some(ref mut text_input) = self.text_input {
                                text_input.pop();
                            }
                        }

                        _ => {}
                    };
                }
                _ => {}
            }
        }
    }
}

pub(super) fn centered_area(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let x_crop_percent = (100 - percent_x) / 2;
    let y_crop_percent = (100 - percent_y) / 2;
    (layout! {
        area,
        (%y_crop_percent),
        (%percent_y) => { %x_crop_percent, %percent_x, %x_crop_percent },
        (%y_crop_percent)
    })[1][1]
}
