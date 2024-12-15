use crate::client::state::{SensorState, SensorStateEvent};
use crate::client::SensorVisionClient;
use crate::model::protocol::MetricValue;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock, Weak};
use uuid::Uuid;
use widgetui::*;

pub mod render;
mod widgets;

#[derive(Default, Clone)]
struct AppSharedUIState {
    current_sensor_index: Option<usize>,
    state_event_connection: Option<signals2::Connection>,

    livedata_double: HashMap<(Uuid, Uuid), VecDeque<f64>>,
    livedata_integer: HashMap<(Uuid, Uuid), VecDeque<i64>>,
    livedata_boolean: HashMap<(Uuid, Uuid), VecDeque<bool>>,
    livedata_string: HashMap<(Uuid, Uuid), VecDeque<String>>,

    weak_self: Weak<Self>,
}

impl AppSharedUIState {
    fn handle_state_event(&mut self, event: SensorStateEvent) {
        match event {
            SensorStateEvent::Livedata {
                sensor_id,
                metric_id,
                value,
            } => {
                let key = (sensor_id, metric_id);
                match value {
                    MetricValue::Double(value) => {
                        let values = self.livedata_double.entry(key).or_default();
                        if values.len() == 15 {
                            values.pop_front();
                        }
                        values.push_back(value);
                    }
                    MetricValue::Integer(value) => {
                        let values = self.livedata_integer.entry(key).or_default();
                        if values.len() == 15 {
                            values.pop_front();
                        }
                        values.push_back(value);
                    }
                    MetricValue::Boolean(value) => {
                        let values = self.livedata_boolean.entry(key).or_default();
                        if values.len() == 15 {
                            values.pop_front();
                        }
                        values.push_back(value);
                    }
                    MetricValue::String(value) => {
                        let values = self.livedata_string.entry(key).or_default();
                        if values.len() == 15 {
                            values.pop_front();
                        }
                        values.push_back(value);
                    }
                };
            }
            _ => {}
        }
    }
}

#[derive(State)]
pub struct AppState {
    client: Arc<Mutex<SensorVisionClient>>,
    state: Arc<RwLock<SensorState>>,

    ui_state: Arc<RwLock<AppSharedUIState>>,
}

impl AppState {
    pub fn new(client: &Arc<Mutex<SensorVisionClient>>) -> Self {
        let state = client.lock().unwrap().get_state();
        let ui_state = Arc::new(RwLock::new(AppSharedUIState::default()));
        let ui_state_weak = Arc::downgrade(&ui_state);
        ui_state.write().unwrap().state_event_connection = Some(
            state
                .write()
                .unwrap()
                .subscribe_to_state_events(move |event| {
                    if let Some(ui_state) = ui_state_weak.upgrade() {
                        ui_state.write().unwrap().handle_state_event(event);
                    }
                })
                .unwrap(),
        );
        Self {
            state,
            client: client.clone(),
            ui_state,
        }
    }
}
