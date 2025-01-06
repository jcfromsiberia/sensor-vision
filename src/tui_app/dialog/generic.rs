use actix::{ActorContext, Addr, Message, Handler, Actor, Context, MessageResult};

use crossterm::event::KeyEvent;

use std::fmt::Debug;
use std::marker::PhantomData;

use tokio::sync::oneshot;

use crate::tui_app::dialog::{ConfirmationDialogActor, InputDialogActor};
use crate::tui_app::ui_state::queries::HandleKeyEvent;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DialogButton {
    Ok,
    Cancel,
}

#[derive(Debug, Clone)]
pub enum DialogResult<T> {
    Accept { result: T },
    Cancel,
}

#[derive(Debug, Clone)]
pub enum ModalDialog {
    Confirmation(Addr<ConfirmationDialogActor>),
    Input(Addr<InputDialogActor>),
}

/// `S` stands for State
///
/// `R` stands for Response

#[derive(Default)]
pub struct StateSnapshot<S>(PhantomData<S>);
impl<S: 'static> Message for StateSnapshot<S> {
    type Result = S;
}

pub struct DialogActor<S, R: Debug> {
    pub state: S,
    respond_to: Option<oneshot::Sender<DialogResult<R>>>,
}

impl<S, R: Debug> DialogActor<S, R> {
    pub fn new(
        state: S,
        respond_to: oneshot::Sender<DialogResult<R>>,
    ) -> Self {
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

pub trait KeyEventHandler<R> {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<DialogResult<R>>;
}

impl<S: Sized + Unpin + 'static, R: Debug + 'static> Actor for DialogActor<S, R> {
    type Context = Context<Self>;
}

impl<S: KeyEventHandler<R> + Sized + Unpin + 'static, R: Debug + 'static> Handler<HandleKeyEvent> for DialogActor<S, R> {
    type Result = bool;

    fn handle(
        &mut self,
        HandleKeyEvent(key_event): HandleKeyEvent,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Some(result) = self.state.handle_key_event(key_event) {
            self.respond_once(result);
            ctx.terminate();
            true
        } else {
            false
        }
    }
}

impl<S: Clone + Sized + Unpin + 'static, R: Debug + 'static> Handler<StateSnapshot<S>> for DialogActor<S, R> {
    type Result = MessageResult<StateSnapshot<S>>;

    fn handle(&mut self, _: StateSnapshot<S>, _: &mut Self::Context) -> Self::Result {
        MessageResult(self.state.clone())
    }
}
