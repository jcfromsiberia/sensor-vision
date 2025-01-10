use ratatui::layout::Rect;
use ratatui::widgets::{
    Block, BorderType, Borders, Paragraph
    ,
};
use ratatui::Frame;
use crate::tui_app::dialog::DialogButton;
use crate::tui_app::theme::*;
use UIElement::*;

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
            .themed(DialogButton);

        if let Some(focused) = focused {
            if focused == *self {
                button_block = button_block.themed(DialogButtonFocused);
            }
        }

        let button = Paragraph::new(text).centered().block(button_block);

        frame.render_widget(button, area);
    }
}
