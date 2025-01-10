use crossterm::event::{KeyCode, KeyEvent};

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::{Line, Stylize};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use strum::IntoEnumIterator;

use crate::tui_app::dialog::generic::{DialogButton, DialogResult};
use crate::tui_app::dialog::render::*;
use crate::tui_app::dialog::{DialogActor, KeyEventHandler};

use crate::tui_app::utils::centered_rect_abs;
use crate::tui_app::theme::*;
use UIElement::*;

use crate::utils::CircularEnum;

pub type ConfirmationDialogActor = DialogActor<ConfirmationDialogState, ()>;

#[derive(Default, Clone)]
pub struct ConfirmationDialogState {
    pub(crate) title: String,
    pub(crate) text: String,
    pub(crate) focused_button: Option<DialogButton>,
}

impl KeyEventHandler<()> for ConfirmationDialogState {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<DialogResult<()>> {
        match key_event.code {
            KeyCode::Esc => Some(DialogResult::Cancel),

            KeyCode::Enter => {
                let Some(focused_button) = &self.focused_button else {
                    return None;
                };
                match focused_button {
                    DialogButton::Ok => Some(DialogResult::Accept { result: () }),
                    DialogButton::Cancel => Some(DialogResult::Cancel),
                }
            }

            KeyCode::Tab => {
                self.focused_button = Some(
                    self.focused_button
                        .map_or(DialogButton::iter().next().unwrap(), |btn| btn.next()),
                );
                None
            }

            _ => None,
        }
    }
}

impl Renderable for ConfirmationDialogState {
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let area = centered_rect_abs(50, 5, area);

        let instructions = Line::from(vec![
            " Select Button ".themed(DialogInstructionsText),
            "<Tab>".themed(DialogInstructionsActionText).bold(),
            " Press ".themed(DialogInstructionsText),
            "<Enter>".themed(DialogInstructionsActionText).bold(),
            " Close ".themed(DialogInstructionsText),
            "<Esc> ".themed(DialogInstructionsActionText).bold(),
        ]);
        let pad = Block::bordered()
            .title(Line::from(self.title.as_str()).centered())
            .title_bottom(instructions.centered())
            .themed(DialogPad);

        let content_area = centered_rect_abs(area.width - 2, area.height - 2, area);

        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // Text area
                Constraint::Fill(1),
                // Buttons area { [   OK   ]_[  CANCEL  ] }
                Constraint::Length(1),
            ])
            .split(content_area);

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
            .split(content_layout[1]);

        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[0]);

        DialogButton::Ok.render(frame, buttons_layout[1], self.focused_button);
        DialogButton::Cancel.render(frame, buttons_layout[3], self.focused_button);
    }
}
