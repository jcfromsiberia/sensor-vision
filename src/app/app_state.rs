use crate::app::ui_state::UIState;
use crate::app::widgets::{DialogButton, InputModal, TextModal};
use crate::client::state::{SensorState, SensorStateEvent};
use crate::client::SensorVisionClient;
use crate::model::ToMqttId;
use std::sync::{Arc, Mutex, RwLock, Weak};

use crate::model::protocol::MetricValue;
use crate::model::sensor::{Metric, ValueType};

#[derive(Clone)]
pub(super) struct AppState {
    pub client: Arc<Mutex<SensorVisionClient>>,
    pub state: Arc<RwLock<SensorState>>,

    pub state_event_connection: Option<signals2::Connection>,

    pub ui_state: UIState,
    pub weak_self: Weak<RwLock<Self>>,
}

impl AppState {
    pub fn next_sensor(&mut self) {
        let state = self.state.read().unwrap();
        let ui_state = &mut self.ui_state;
        if state.sensors.is_empty() {
            ui_state.current_sensor = None;
            ui_state.current_metric = None;
            return;
        }
        if let Some((current_index, _)) = ui_state.current_sensor {
            if current_index < state.sensors.len() - 1 {
                let new_index = current_index.wrapping_add(1);
                ui_state.current_sensor = Some((
                    new_index,
                    state.sensors.iter().nth(new_index).unwrap().0.clone(),
                ));
                ui_state.current_metric = None;
                return;
            }
        }
        ui_state.current_sensor = Some((0, state.sensors.iter().nth(0).unwrap().0.clone()));
        ui_state.current_metric = None;
    }

    pub fn next_metric(&mut self) {
        let state = self.state.read().unwrap();
        let ui_state = &mut self.ui_state;

        if let Some((_, current_sensor_id)) = ui_state.current_sensor {
            if let Some(sensor) = state.sensors.get(&current_sensor_id) {
                let metrics = &sensor.metrics;
                if metrics.is_empty() {
                    ui_state.current_metric = None;
                    return;
                }
                let mut new_index = 0;
                if let Some((current_index, _)) = ui_state.current_metric {
                    if current_index < metrics.len() - 1 {
                        new_index = current_index.wrapping_add(1);
                    }
                }
                ui_state.current_metric = Some((new_index, metrics[new_index].metric_id().clone()));
            }
        }
    }

    pub fn handle_state_event(&mut self, event: SensorStateEvent) {
        match event {
            SensorStateEvent::NewLinkedSensorLoaded(..)
            | SensorStateEvent::ExistingLinkedSensorLoaded(..) => {
                if self.ui_state.current_sensor.is_none() {
                    self.next_sensor();
                }
            }

            SensorStateEvent::Livedata {
                sensor_id,
                metric_id,
                value,
                timestamp,
            } => {
                self.ui_state
                    .accept_livedata(sensor_id, metric_id, value, timestamp);
            }

            SensorStateEvent::SensorDeleted { sensor_id } => {
                if let Some((_, current_sensor_id)) = self.ui_state.current_sensor {
                    if sensor_id == current_sensor_id {
                        self.next_sensor();
                        self.next_metric();
                    }
                }
            }

            SensorStateEvent::MetricDeleted {
                sensor_id,
                metric_id,
            } => match (self.ui_state.current_sensor, self.ui_state.current_metric) {
                (Some((_, current_sensor_id)), Some((_, current_metric_id))) => {
                    if current_sensor_id == sensor_id && current_metric_id == metric_id {
                        self.next_metric();
                    }
                }
                _ => {}
            },
            _ => {}
        };
    }

    pub fn delete_sensor(&mut self) {
        if let Some((_, sensor_id)) = self.ui_state.current_sensor {
            let weak_client = Arc::downgrade(&self.client);
            let accept_callback: Box<dyn Fn() + Send + Sync> = Box::new(move || {
                if let Some(client) = weak_client.upgrade() {
                    if let Err(err) = client.lock().unwrap().delete_sensor(&sensor_id) {
                        log::error!("Failed to send SensorDelete for {sensor_id}: {err}");
                    }
                }
            });

            self.ui_state.modal_dialog = Some(Arc::new(RwLock::new(TextModal {
                title: "Delete Sensor".to_owned(),
                text: format!("Delete Sensor #{}?", sensor_id.to_mqtt()),
                accept_callback,
                close_callback: self.close_dialog_callback(),
                focused_button: Some(DialogButton::Cancel),
            })));
        }
    }

    pub fn delete_metric(&mut self) {
        match (self.ui_state.current_sensor, self.ui_state.current_metric) {
            (Some((_, sensor_id)), Some((_, metric_id))) => {
                // FIXME Get rid of copy-paste!
                let weak_client = Arc::downgrade(&self.client);
                let accept_callback: Box<dyn Fn() + Send + Sync> = Box::new(move || {
                    if let Some(client) = weak_client.upgrade() {
                        if let Err(err) =
                            client.lock().unwrap().delete_metric(&sensor_id, &metric_id)
                        {
                            log::error!("Failed to send MetricDelete for {sensor_id}: {err}");
                        }
                    }
                });

                self.ui_state.modal_dialog = Some(Arc::new(RwLock::new(TextModal {
                    title: "Delete Metric".to_owned(),
                    text: format!(
                        "Delete Metric # {} / #{}?",
                        sensor_id.to_mqtt(),
                        metric_id.to_mqtt()
                    ),
                    accept_callback,
                    close_callback: self.close_dialog_callback(),
                    focused_button: Some(DialogButton::Cancel),
                })));
            }
            _ => {}
        }
    }

    pub fn create_sensor(&mut self) {
        let weak_client = Arc::downgrade(&self.client);
        let accept_callback: Box<dyn Fn(String) + Send + Sync> = Box::new(move |new_name| {
            if let Some(client) = weak_client.upgrade() {
                if let Err(err) = client.lock().unwrap().create_sensor(&new_name) {
                    log::error!("Failed to send SensorCreate for {new_name}: {err}");
                }
            }
        });

        self.ui_state.modal_dialog = Some(Arc::new(RwLock::new(InputModal {
            title: "Create Sensor".to_owned(),
            text: "Create a new Sensor?".to_owned(),
            label: "Name:".to_owned(),
            text_input: None,
            accept_callback,
            close_callback: self.close_dialog_callback(),
            focused_button: Some(DialogButton::Ok),
        })));
    }

    pub fn update_sensor(&mut self) {
        if let Some((_, sensor_id)) = self.ui_state.current_sensor {
            let weak_client = Arc::downgrade(&self.client);
            let accept_callback: Box<dyn Fn(String) + Send + Sync> = Box::new(move |new_name| {
                if let Some(client) = weak_client.upgrade() {
                    if let Err(err) = client
                        .lock()
                        .unwrap()
                        .update_sensor(&sensor_id, &new_name, None)
                    {
                        log::error!("Failed to send SensorUpdate for {new_name}: {err}");
                    }
                }
            });

            let sensor_name = self
                .state
                .read()
                .unwrap()
                .sensors
                .get(&sensor_id)
                .unwrap()
                .name
                .clone();

            self.ui_state.modal_dialog = Some(Arc::new(RwLock::new(InputModal {
                title: "Update Sensor".to_owned(),
                text: format!("Rename Sensor {}?", sensor_name),
                label: "Name:".to_owned(),
                text_input: Some(sensor_name),
                accept_callback,
                close_callback: self.close_dialog_callback(),
                focused_button: Some(DialogButton::Ok),
            })));
        }
    }

    pub fn push_value(&mut self) {
        match (self.ui_state.current_sensor, self.ui_state.current_metric) {
            (Some((_, sensor_id)), Some((metric_index, metric_id))) => {
                let metric = self
                    .state
                    .read()
                    .unwrap()
                    .sensors
                    .get(&sensor_id)
                    .unwrap()
                    .metrics
                    .get(metric_index)
                    .unwrap()
                    .clone();

                let metric_name = metric.name().clone();
                let weak_client = Arc::downgrade(&self.client);
                let accept_callback: Box<dyn Fn(String) + Send + Sync> =
                    Box::new(move |new_value| {
                        if let Some(client) = weak_client.upgrade() {
                            // FIXME Get rid of copy-pasta
                            let metric_value = match &metric {
                                Metric::Predefined { .. } => {
                                    let parsed = new_value.parse::<f64>();
                                    if let Err(err) = &parsed {
                                        log::error!("{err}");
                                        return;
                                    }
                                    MetricValue::Double(parsed.unwrap())
                                }
                                Metric::Custom { value_type, .. } => match value_type {
                                    ValueType::Double => {
                                        let parsed = new_value.parse::<f64>();
                                        if let Err(err) = &parsed {
                                            log::error!("{err}");
                                            return;
                                        }
                                        MetricValue::Double(parsed.unwrap())
                                    }
                                    ValueType::Integer => {
                                        let parsed = new_value.parse::<i64>();
                                        if let Err(err) = &parsed {
                                            log::error!("{err}");
                                            return;
                                        }
                                        MetricValue::Integer(parsed.unwrap())
                                    }
                                    ValueType::Boolean => {
                                        let parsed = new_value.parse::<bool>();
                                        if let Err(err) = &parsed {
                                            log::error!("{err}");
                                            return;
                                        }
                                        MetricValue::Boolean(parsed.unwrap())
                                    }

                                    ValueType::String => MetricValue::String(new_value),
                                },
                            };

                            if let Err(err) = client.lock().unwrap().push_value(
                                &sensor_id,
                                &metric_id,
                                &metric_value,
                                None,
                            ) {
                                log::error!("Failed to Push Metric: {err}");
                            }
                        }
                    });

                let default_value = self
                    .ui_state
                    .livedata
                    .get(&(sensor_id, metric_id))
                    .map(|window| window.data.last().map(|(_, val)| val.to_string()))
                    .flatten();

                self.ui_state.modal_dialog = Some(Arc::new(RwLock::new(InputModal {
                    title: "Push Value to Metric".to_owned(),
                    text: format!("value to Metric {metric_name}?",),
                    label: "Value".to_owned(),
                    text_input: default_value,
                    accept_callback,
                    close_callback: self.close_dialog_callback(),
                    focused_button: Some(DialogButton::Ok),
                })));
            }
            _ => {}
        }
    }

    fn close_dialog_callback(&mut self) -> Box<dyn Fn() + Send + Sync> {
        let weak_this = self.weak_self.clone();
        Box::new(move || {
            if let Some(this) = weak_this.upgrade() {
                this.write().unwrap().ui_state.modal_dialog = None;
            }
        })
    }
}
