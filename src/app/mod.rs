use crate::client::SensorVisionClient;
use std::sync::{Arc, Mutex, RwLock, Weak};
use widgetui::*;
use app_state::AppState;
use crate::app::ui_state::UIState;

pub mod render;
mod widgets;
mod app_state;
mod ui_state;
mod livedata;

#[derive(State)]
pub struct AppStateWrapper {
    app_state: Arc<RwLock<AppState>>,
}

impl AppStateWrapper {
    pub fn new(client: &Arc<Mutex<SensorVisionClient>>) -> Self {
        let state = client.lock().unwrap().get_state();
        let app_state = Arc::new(RwLock::new(AppState {
            client: client.clone(),
            state: state.clone(),

            state_event_connection: Option::default(),

            ui_state: UIState::default(),
            weak_self: Weak::default(),
        }));

        {
            let app_state_weak = Arc::downgrade(&app_state);
            let mut app_state_unlocked = app_state.write().unwrap();
            app_state_unlocked.state_event_connection = Some(
                client
                    .lock()
                    .unwrap()
                    .subscribe_to_state_events(move |event| {
                        if let Some(ui_state) = app_state_weak.upgrade() {
                            ui_state.write().unwrap().handle_state_event(event);
                        }
                    })
                    .unwrap(),
            );
            app_state_unlocked.weak_self = Arc::downgrade(&app_state);
        }

        Self {
            app_state,
        }
    }
}
