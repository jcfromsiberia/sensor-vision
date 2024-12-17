use std::sync::{Arc, Mutex, RwLock, Weak};
use crate::app::ui_state::UIState;
use crate::client::SensorVisionClient;
use crate::client::state::{SensorState, SensorStateEvent};

#[derive(Clone)]
pub(super) struct AppState {
    pub client: Arc<Mutex<SensorVisionClient>>,
    pub state: Arc<RwLock<SensorState>>,

    pub state_event_connection: Option<signals2::Connection>,

    pub ui_state: UIState,
    pub weak_self: Weak<Self>,
}

impl AppState {
    pub fn next_sensor(&mut self) {
        let state = self.state.read().unwrap();
        if state.sensors.is_empty() {
            self.ui_state.current_sensor_index = None;
            return;
        }
        if let Some(current_index) = self.ui_state.current_sensor_index {
            if current_index < state.sensors.len() - 1 {
                self.ui_state.current_sensor_index = Some(current_index.wrapping_add(1));
                return;
            }
        }
        self.ui_state.current_sensor_index = Some(0)
    }

    pub(super) fn handle_state_event(&mut self, event: SensorStateEvent) {
        match event {
            SensorStateEvent::Livedata {
                sensor_id,
                metric_id,
                value,
                timestamp,
            } => {
                self.ui_state.accept_livedata(sensor_id, metric_id, value, timestamp);
            },
            // SensorStateEvent::NewSensorCreated(..) | SensorStateEvent::NewLinkedSensorLoaded(..) |
            //     SensorStateEvent::ExistingLinkedSensorLoaded(..) => {
            //     if ()
            // },
            _ => {}
        }
    }
}
