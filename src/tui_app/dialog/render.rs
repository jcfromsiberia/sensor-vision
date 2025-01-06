use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{
    Block, BorderType, Borders, Paragraph
    ,
};
use ratatui::Frame;
use crate::tui_app::dialog::DialogButton;

pub trait Renderable {
    fn render(&self, frame: &mut Frame);
}

impl DialogButton {
    pub fn render(&self, frame: &mut Frame, area: Rect, focused: Option<DialogButton>) {
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
