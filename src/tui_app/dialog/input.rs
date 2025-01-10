use crossterm::event::{KeyCode, KeyEvent};

use ratatui::Frame;
use ratatui::prelude::{Line, Stylize};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::layout::{Constraint, Direction, Layout};

use strum::IntoEnumIterator;

use crate::tui_app::dialog::{DialogActor, KeyEventHandler};
use crate::tui_app::dialog::generic::{DialogButton, DialogResult};
use crate::tui_app::dialog::render::*;

use crate::tui_app::theme::*;
use UIElement::*;

use crate::tui_app::utils::centered_rect_abs;
use crate::utils::CircularEnum;

pub type InputDialogActor = DialogActor<InputDialogState, String>;

#[derive(Default, Clone)]
pub struct InputDialogState {
    pub title: String,
    pub text: String,
    pub label: String,

    pub text_input: Option<String>,
    pub focused_button: Option<DialogButton>,
}

impl KeyEventHandler<String> for InputDialogState {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<DialogResult<String>> {
        match key_event.code {
            KeyCode::Esc => Some(DialogResult::Cancel),

            KeyCode::Enter => {
                let Some(focused_button) = &self.focused_button else {
                    return None;
                };
                match focused_button {
                    DialogButton::Ok => {
                        let result = self
                            .text_input
                            .take()
                            .unwrap_or_default();

                        Some(DialogResult::Accept{result})
                    },
                    DialogButton::Cancel => Some(DialogResult::Cancel),
                }
            },

            KeyCode::Tab => {
                self.focused_button = Some(
                    self.focused_button
                        .map_or(DialogButton::iter().next().unwrap(), |btn| btn.next()),
                );
                None
            }

            KeyCode::Char(char) => {
                self.text_input
                    .get_or_insert_with(|| String::new())
                    .push(char);
                None
            }

            KeyCode::Backspace => {
                if let Some(ref mut text_input) = self.text_input {
                    text_input.pop();
                }
                None
            }

            _ => None
        }
    }
}

impl Renderable for InputDialogState {
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let area = centered_rect_abs(50, 6, area);

        let instructions = Line::from(vec![
            " Select Button ".themed(DialogInstructionsText),
            "<Tab>".themed(DialogInstructionsActionText).bold(),
            " Press ".themed(DialogInstructionsText),
            "<Enter>".themed(DialogInstructionsActionText).bold(),
            " Close ".themed(DialogInstructionsText),
            "<Esc> ".themed(DialogInstructionsActionText).bold(),
        ]);

        let pad = Block::bordered()
            .title(Line::from(self.title.clone()).centered())
            .title_bottom(instructions.centered())
            .themed(DialogPad);
        let content_area = centered_rect_abs(area.width - 2, area.height - 2, area);

        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // 0 Text area
                Constraint::Fill(1),
                // 1 Text input { Label [<input>          ] }
                Constraint::Length(1),
                // 2 Buttons area { [   OK   ]_[  CANCEL  ] }
                Constraint::Length(1),
            ])
            .split(content_area);

        let input_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                // { Label [<input>          ] }
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Percentage(80),
                Constraint::Min(1),
            ])
            .split(content_layout[1]);

        let buttons_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                // { [   OK   ]_[  CANCEL  ] }
                Constraint::Min(1),
                Constraint::Length(10),
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Min(1),
            ])
            .split(content_layout[2]);

        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        let label = Line::from(self.label.as_str());
        let text_input_pad = Block::new().themed(DialogTextInputFocused);
        let text_input = Line::from(
            self.text_input
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or_else(|| "<input>"),
        ).themed(DialogTextInputFocused);

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[0]);
        frame.render_widget(label, input_layout[1]);
        frame.render_widget(text_input_pad, input_layout[2]);
        frame.render_widget(text_input, input_layout[2]);

        DialogButton::Ok.render(frame, buttons_layout[1], self.focused_button);
        DialogButton::Cancel.render(frame, buttons_layout[3], self.focused_button);
    }
}
