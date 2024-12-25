use std::fmt::Debug;
use color_eyre::Result;

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub enum DialogResult<T> {
    Accept { result: T },
    Cancel,
}

pub type MessageModalDialogHandle = DialogActorHandle<MessageDialogState>;
pub type InputModalDialogHandle = DialogActorHandle<InputDialogState>;

#[derive(Debug, Clone)]
pub enum ModalDialog {
    Confirmation(MessageModalDialogHandle),
    Input(InputModalDialogHandle),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DialogButton {
    Ok,
    Cancel,
}

/// `S` stands for State
/// `R` stands for Response
/// `RS` stands for RespondingState `State + oneshot::Sender<DialogResult<Response>>`

#[derive(Debug)]
pub enum DialogCommand<S> {
    Snapshot { respond_to: oneshot::Sender<S> },
    HandleKeyEvent(KeyEvent),
}

#[derive(Debug)]
pub struct RespondingDialogState<S, R: Debug> {
    pub state: S,

    pub respond_to: Option<oneshot::Sender<DialogResult<R>>>,
}

#[derive(Debug, Clone)]
pub struct MessageDialogState {
    pub title: String,
    pub text: String,
    pub focused_button: Option<DialogButton>,
}

#[derive(Debug, Clone)]
pub struct InputDialogState {
    pub title: String,
    pub text: String,
    pub label: String,

    pub text_input: Option<String>,
    pub focused_button: Option<DialogButton>,
}

pub type RespondingMessageDialogState = RespondingDialogState<MessageDialogState, ()>;
pub type RespondingInputDialogState = RespondingDialogState<InputDialogState, String>;

#[derive(Debug, Clone)]
pub struct DialogActorHandle<S: Debug + Send + 'static> {
    sender: mpsc::UnboundedSender<DialogCommand<S>>,
}

impl<S: Debug + Send + 'static> DialogActorHandle<S> {
    pub fn new<RS: StateCommandHandler<S> + Debug + Send + 'static>(initial_state: RS) -> Result<Self> {
        let (sender, receiver) = mpsc::unbounded_channel::<DialogCommand<S>>();
        let actor = DialogActor::<S, RS> {
            state: initial_state,
            receiver,
        };

        tokio::task::Builder::new()
            .name("dialog actor loop")
            .spawn(dialog_actor_loop(actor))?;

        Ok(Self { sender })
    }

    pub fn send_command(&self, command: DialogCommand<S>) {
        let _ = self.sender.send(command);
    }
}

async fn dialog_actor_loop<S: Debug + Send, RS: StateCommandHandler<S> + Debug + Send + 'static>(
    mut actor: DialogActor<S, RS>,
) {
    while let Some(command) = actor.receiver.recv().await {
        actor.handle_command(command);
    }
}

trait StateCommandHandler<S> {
    fn handle_command(&mut self, command: DialogCommand<S>);
}

#[derive(Debug)]
struct DialogActor<S: Send, RS: StateCommandHandler<S> + Send> {
    state: RS,
    receiver: mpsc::UnboundedReceiver<DialogCommand<S>>,
}

impl<S: Send, RS: StateCommandHandler<S> + Send> DialogActor<S, RS> {
    fn handle_command(&mut self, command: DialogCommand<S>) {
        self.state.handle_command(command);
    }
}

impl<S, R: Debug> RespondingDialogState<S, R> {

    pub fn new(state: S, respond_to: oneshot::Sender<DialogResult<R>>) -> Self {
        Self {
            state,
            respond_to: Some(respond_to),
        }
    }
    fn respond_once(&mut self, response: DialogResult<R>) {
        let Some(sender) = self.respond_to.take() else {
            log::error!("The dialog has already responded");
            return;
        };

        sender.send(response).expect("Responding failed");
    }
}

impl StateCommandHandler<MessageDialogState> for RespondingMessageDialogState {
    fn handle_command(&mut self, command: DialogCommand<MessageDialogState>) {
        match command {

            DialogCommand::Snapshot {respond_to} => {
                respond_to.send(self.state.clone()).expect("Responding failed");
            },

            DialogCommand::HandleKeyEvent(key_event) => {
                match key_event.code {

                    KeyCode::Esc => {
                        self.respond_once(DialogResult::Cancel);
                    }

                    KeyCode::Enter => {
                        let Some(focused_button) = &self.state.focused_button else {
                            return;
                        };
                        match focused_button {
                            DialogButton::Ok => {
                                self.respond_once(DialogResult::Accept{result: ()});
                            }
                            DialogButton::Cancel => {
                                self.respond_once(DialogResult::Cancel);
                            }
                        }
                    }

                    KeyCode::Tab => {
                        if let Some(_button @ DialogButton::Ok) = self.state.focused_button {
                            self.state.focused_button = Some(DialogButton::Cancel);
                        } else {
                            self.state.focused_button = Some(DialogButton::Ok);
                        }
                    }

                    _ => {}
                };
            }
        }
    }
}

impl StateCommandHandler<InputDialogState> for RespondingInputDialogState {
    fn handle_command(&mut self, command: DialogCommand<InputDialogState>) {
        match command {

            DialogCommand::Snapshot {respond_to} => {
                respond_to.send(self.state.clone()).expect("Responding failed");
            },

            DialogCommand::HandleKeyEvent(key_event) => {
                match key_event.code {

                    KeyCode::Esc => {
                        self.respond_once(DialogResult::Cancel);
                    }

                    KeyCode::Enter => {
                        let Some(focused_button) = &self.state.focused_button else {
                            return;
                        };
                        match focused_button {
                            DialogButton::Ok => {
                                let result = self
                                    .state
                                    .text_input
                                    .take()
                                    .unwrap_or_default();

                                self.respond_once(DialogResult::Accept{result});
                            }
                            DialogButton::Cancel => {
                                self.respond_once(DialogResult::Cancel);
                            }
                        }
                    }

                    KeyCode::Tab => {
                        if let Some(_button @ DialogButton::Ok) = self.state.focused_button {
                            self.state.focused_button = Some(DialogButton::Cancel);
                        } else {
                            self.state.focused_button = Some(DialogButton::Ok);
                        }
                    }

                    KeyCode::Char(char) => {
                        self.state.text_input
                            .get_or_insert_with(|| String::new())
                            .push(char);
                    }

                    KeyCode::Backspace => {
                        if let Some(ref mut text_input) = self.state.text_input {
                            text_input.pop();
                        }
                    }

                    _ => {}
                };
            }

        }
    }
}
