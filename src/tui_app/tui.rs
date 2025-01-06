use eyre::Result;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};

use ratatui::{backend::CrosstermBackend, Terminal};

use std::io;
use std::io::Stdout;
use std::panic;

pub type CrosstermTerminal = Terminal<CrosstermBackend<Stdout>>;
pub type SharedTui = std::sync::Arc<tokio::sync::Mutex<Tui>>;

#[derive(Debug)]
pub struct Tui {
    pub terminal: CrosstermTerminal,
}

impl Tui {
    pub fn new(terminal: CrosstermTerminal) -> Self {
        Self { terminal }
    }

    pub fn init(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        let panic_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic| {
            Self::reset().expect("failed to reset the terminal");
            panic_hook(panic);
        }));

        self.terminal.hide_cursor()?;
        self.terminal.clear()?;
        Ok(())
    }

    fn reset() -> Result<()> {
        terminal::disable_raw_mode()?;
        crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        Self::reset()?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}
