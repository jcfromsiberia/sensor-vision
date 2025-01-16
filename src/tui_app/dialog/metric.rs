use crossterm::event::{KeyCode, KeyEvent};

use eyre::{eyre, Result};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Line, Stylize};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::sensor::{Metric, ValueType, ValueUnit};
use crate::tui_app::dialog::generic::{DialogButton, DialogResult};
use crate::tui_app::dialog::render::*;
use crate::tui_app::dialog::{DialogActor, KeyEventHandler};

use crate::tui_app::theme::*;
use UIElement::*;

use crate::tui_app::utils::centered_rect_abs;
use crate::utils::CircularEnum;

pub type MetricDialogActor = DialogActor<MetricDialogState, Metric>;

#[derive(Default, Clone)]
pub struct MetricDialogState {
    title: String,
    text: String,

    forms: Vec<MetricForm>,
    focused_form: usize,
}

#[derive(Default, Clone)]
struct MetricForm {
    metric: Metric,
    focused_field: usize,
}

impl MetricDialogState {
    pub fn new(title: String, text: String, metrics: Vec<Metric>) -> Result<Self> {
        if metrics.len() == 0 {
            Err(eyre!("No metrics"))
        } else {
            Ok(Self {
                title,
                text,
                forms: metrics
                    .into_iter()
                    .map(|metric| MetricForm {
                        metric,
                        focused_field: 0,
                    })
                    .collect(),
                focused_form: 0,
            })
        }
    }
}

impl MetricForm {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        let mut metric = self.metric.clone();
        match &mut metric {
            Metric::Predefined {
                name, value_unit, ..
            } => self.handle_key_event_predefined(key_event, name, value_unit),
            Metric::Custom {
                name,
                value_type,
                value_annotation,
                ..
            } => self.handle_key_event_custom(key_event, name, value_type, value_annotation),
        }
        self.metric = metric;
    }

    fn handle_key_event_predefined(
        &mut self,
        key_event: KeyEvent,
        name: &mut String,
        value_unit: &mut ValueUnit,
    ) {
        match key_event.code {
            KeyCode::Char(char) => {
                if self.focused_field == 0 {
                    name.push(char);
                }
            }

            KeyCode::Backspace => {
                if self.focused_field == 0 {
                    name.pop();
                }
            }

            KeyCode::Down => {
                self.focused_field = self.focused_field.wrapping_add(1);
                if self.focused_field > 1 {
                    self.focused_field = 0;
                }
            }

            KeyCode::Up => {
                self.focused_field = self.focused_field.wrapping_sub(1);
                if self.focused_field > 1 {
                    self.focused_field = 1;
                }
            }

            KeyCode::Left => {
                if self.focused_field == 1 {
                    *value_unit = value_unit.prev();
                }
            }

            KeyCode::Right => {
                if self.focused_field == 1 {
                    *value_unit = value_unit.next();
                }
            }

            _ => {}
        }
    }

    fn handle_key_event_custom(
        &mut self,
        key_event: KeyEvent,
        name: &mut String,
        value_type: &mut ValueType,
        value_annotation: &mut String,
    ) {
        match key_event.code {
            KeyCode::Char(char) => match self.focused_field {
                0 => name.push(char),
                2 => value_annotation.push(char),
                _ => {}
            },

            KeyCode::Backspace => match self.focused_field {
                0 => {
                    name.pop();
                }
                2 => {
                    value_annotation.pop();
                }
                _ => {}
            },

            KeyCode::Down => {
                self.focused_field = self.focused_field.wrapping_add(1);
                if self.focused_field > 2 {
                    self.focused_field = 0;
                }
            }

            KeyCode::Up => {
                self.focused_field = self.focused_field.wrapping_sub(1);
                if self.focused_field > 2 {
                    self.focused_field = 2;
                }
            }

            KeyCode::Left => {
                if self.focused_field == 1 {
                    *value_type = value_type.prev();
                }
            }

            KeyCode::Right => {
                if self.focused_field == 1 {
                    *value_type = value_type.next();
                }
            }

            _ => {}
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        match &self.metric {
            Metric::Predefined {
                name, value_unit, ..
            } => self.render_predefined(frame, area, name, value_unit, focused),

            Metric::Custom {
                name,
                value_type,
                value_annotation,
                ..
            } => self.render_custom(frame, area, name, value_type, value_annotation, focused),
        }
    }

    fn render_predefined(
        &self,
        frame: &mut Frame,
        area: Rect,
        name: &str,
        value_unit: &ValueUnit,
        focused: bool,
    ) {
        let pad = Block::bordered()
            .border_type(BorderType::Rounded)
            .themed(if focused {
                OptionCardSelected
            } else {
                OptionCard
            });
        let content_area = centered_rect_abs(area.width - 2, area.height - 2, area);

        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // 0 Name input area
                Constraint::Length(1),
                // 1 Unit input area
                Constraint::Length(1),
                // 2 Buttons area { [   OK   ] }
                Constraint::Length(1),
            ])
            .split(content_area);

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);

        let name_input = Line::from(if name.is_empty() { "<name>" } else { name }).themed(
            if focused && self.focused_field == 0 {
                DialogTextInputFocused
            } else {
                DialogTextInput
            },
        );

        let value_unit_input =
            Line::from(value_unit.emojified()).themed(if focused && self.focused_field == 1 {
                DialogTextInputFocused
            } else {
                DialogTextInput
            });

        frame.render_widget(name_input, content_layout[0]);
        frame.render_widget(value_unit_input, content_layout[1]);
        DialogButton::Ok.render(frame, content_layout[2], None);
    }

    fn render_custom(
        &self,
        frame: &mut Frame,
        area: Rect,
        name: &str,
        value_type: &ValueType,
        value_annotation: &str,
        focused: bool,
    ) {
        let pad = Block::bordered()
            .border_type(BorderType::Rounded)
            .themed(if focused {
                OptionCardSelected
            } else {
                OptionCard
            });
        let content_area = centered_rect_abs(area.width - 2, area.height - 2, area);

        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // 0 Name input area
                Constraint::Length(1),
                // 1 Type input area
                Constraint::Length(1),
                // 2 Annotation input area
                Constraint::Length(1),
                // 3 Buttons area { [   OK   ] }
                Constraint::Length(1),
            ])
            .split(content_area);

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);

        let name_input = Line::from(if name.is_empty() { "<name>" } else { name }).themed(
            if focused && self.focused_field == 0 {
                DialogTextInputFocused
            } else {
                DialogTextInput
            },
        );

        let value_type_input =
            Line::from(value_type.emojified()).themed(if focused && self.focused_field == 1 {
                DialogTextInputFocused
            } else {
                DialogTextInput
            });

        let value_annotation_input = Line::from(if value_annotation.is_empty() {
            "<annotation>"
        } else {
            value_annotation
        })
        .themed(if focused && self.focused_field == 2 {
            DialogTextInputFocused
        } else {
            DialogTextInput
        });

        frame.render_widget(name_input, content_layout[0]);
        frame.render_widget(value_type_input, content_layout[1]);
        frame.render_widget(value_annotation_input, content_layout[2]);

        DialogButton::Ok.render(frame, content_layout[3], None);
    }
}

impl KeyEventHandler<Metric> for MetricDialogState {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<DialogResult<Metric>> {
        match key_event.code {
            KeyCode::Esc => Some(DialogResult::Cancel),

            KeyCode::Enter => Some(DialogResult::Accept {
                result: self.forms[self.focused_form].metric.clone(),
            }),

            KeyCode::Tab => {
                self.focused_form = self.focused_form.wrapping_add(1);
                if self.focused_form == self.forms.len() {
                    self.focused_form = 0;
                }
                None
            }

            _ => {
                self.forms[self.focused_form].handle_key_event(key_event);
                None
            }
        }
    }
}

impl Renderable for MetricDialogState {
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let area = centered_rect_abs(76, 8, area);

        let instructions = Line::from(vec![
            " Select Card ".themed(DialogInstructionsText),
            "↹ ".themed(DialogInstructionsActionText).bold(),
            " Change Field ".themed(DialogInstructionsText),
            "↑/↓".themed(DialogInstructionsActionText).bold(),
            " Change Value ".themed(DialogInstructionsText),
            "←/→".themed(DialogInstructionsActionText).bold(),
            " Accept ".themed(DialogInstructionsText),
            "↵".themed(DialogInstructionsActionText).bold(),
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
                // 1 Option Cards Area
                Constraint::Length(6),
            ])
            .split(content_area);

        let option_cards_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Ratio(1, self.forms.len() as u32);
                self.forms.len()
            ])
            .split(content_layout[1]);

        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[0]);
        for (i, form) in self.forms.iter().enumerate() {
            form.render(frame, option_cards_layout[i], i == self.focused_form);
        }
    }
}
