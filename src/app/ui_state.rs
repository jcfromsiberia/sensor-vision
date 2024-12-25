use eyre::Result;

use chrono::{DateTime, Utc};
use crossterm::event::{KeyEvent, KeyEventKind};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::app::dialog::{DialogCommand, ModalDialog};
use crate::model::protocol::MetricValue;
use crate::model::{MetricId, SensorId};

#[derive(Debug)]
pub enum UIActorCommand {
    Snapshot {
        respond_to: oneshot::Sender<UIState>,
    },

    SelectSensor(Option<(usize, SensorId)>),

    SelectMetric(Option<(usize, MetricId)>),

    SetModalDialog(Option<ModalDialog>),

    AcceptLivedata {
        sensor_id: SensorId,
        metric_id: MetricId,
        value: MetricValue,
        timestamp: u64,
    },

    HandleKeyEvent{
        event: KeyEvent,
        respond_to: oneshot::Sender<bool>,
    },

    DropSensor(SensorId),
    DropMetric(SensorId, MetricId),
}

#[derive(Debug, Clone)]
pub struct UIStateActorHandle {
    sender: mpsc::UnboundedSender<UIActorCommand>,
}

#[derive(Debug, Clone, Default)]
pub struct UIState {
    pub current_sensor: Option<(usize, SensorId)>,
    pub current_metric: Option<(usize, MetricId)>,

    pub modal_dialog: Option<ModalDialog>,

    pub livedata: HashMap<(SensorId, MetricId), MetricLivedataWindow>,
}

impl UIStateActorHandle {
    pub fn new() -> Result<Self> {
        let (actor_command_sender, actor_command_receiver) =
            mpsc::unbounded_channel::<UIActorCommand>();
        let actor = UIStateActor::new(actor_command_receiver);
        tokio::task::Builder::new()
            .name("ui state actor loop")
            .spawn(ui_state_actor_loop(actor))?;
        Ok(Self {
            sender: actor_command_sender,
        })
    }

    pub async fn snapshot(&self) -> Result<UIState> {
        let (tx, rx) = oneshot::channel::<UIState>();
        let command = UIActorCommand::Snapshot { respond_to: tx };
        self.actor_command(command);
        Ok(rx.await?)
    }

    pub fn actor_commands(&self, commands: Vec<UIActorCommand>) {
        for command in commands {
            self.actor_command(command);
        }
    }

    pub fn actor_command(&self, command: UIActorCommand) {
        self.sender.send(command).expect("Sending failed");
    }
}

async fn ui_state_actor_loop(mut actor: UIStateActor) {
    while let Some(command) = actor.receiver.recv().await {
        actor.handle_user_command(command);
    }
}

#[derive(Debug)]
struct UIStateActor {
    receiver: mpsc::UnboundedReceiver<UIActorCommand>,
    ui_state: UIState,
}

impl UIStateActor {
    fn new(receiver: mpsc::UnboundedReceiver<UIActorCommand>) -> Self {
        Self {
            receiver,
            ui_state: UIState::default(),
        }
    }

    fn handle_user_command(&mut self, command: UIActorCommand) {
        use UIActorCommand::*;
        match command {

            HandleKeyEvent {event, respond_to} => {
                let Some(dialog) =  &self.ui_state.modal_dialog else {
                    respond_to.send(false).expect("Responding failed");
                    return;
                };

                match dialog {
                    ModalDialog::Confirmation(handle) => {
                        handle.send_command(DialogCommand::HandleKeyEvent(event));
                    }
                    ModalDialog::Input(handle) => {
                        handle.send_command(DialogCommand::HandleKeyEvent(event));
                    }
                }

                respond_to.send(true).expect("Responding failed");
            }

            Snapshot { respond_to } => {
                let _ = respond_to.send(self.ui_state.clone());
            }

            SelectSensor(sensor) => {
                self.ui_state.current_sensor = sensor;
            }

            SelectMetric(metric) => {
                self.ui_state.current_metric = metric;
            }

            AcceptLivedata {
                sensor_id,
                metric_id,
                value,
                timestamp,
            } => self.accept_livedata(sensor_id, metric_id, value, timestamp),

            SetModalDialog(dialog) => {
                self.ui_state.modal_dialog = dialog;
            }

            DropSensor(sensor_id) => {
                self.ui_state
                    .livedata
                    .retain(|(sens_id, _), _| sens_id != &sensor_id);
                if self.ui_state.current_sensor.is_some_and(|(_, sens_id)| sens_id == sensor_id) {
                    self.ui_state.current_sensor = None;
                }
            }

            DropMetric(sensor_id, metric_id) => {
                self.ui_state
                    .livedata
                    .retain(|(sens_id, metr_id), _| sensor_id.ne(sens_id) && metric_id.ne(metr_id));
                if self.ui_state.current_sensor.is_some_and(|(_, sens_id)| sens_id == sensor_id) &&
                    self.ui_state.current_metric.is_some_and(|(_, metr_id)| metr_id == metric_id) {
                    self.ui_state.current_metric = None;
                }
            }
        }
    }

    fn accept_livedata(
        &mut self,
        sensor_id: SensorId,
        metric_id: MetricId,
        value: MetricValue,
        timestamp: u64,
    ) {
        let key = (sensor_id, metric_id);
        let value = match value {
            MetricValue::Double(value) => value,
            MetricValue::Integer(value) => value as f64,
            MetricValue::Boolean(value) => value as u8 as f64,
            MetricValue::String(_) => {
                return;
            }
        };

        let metric_livedata_window = self.ui_state.livedata.entry(key).or_default();
        metric_livedata_window.push_data(timestamp, value);
    }
}

const LIVEDATA_WINDOW_LIMIT: usize = 50;

#[derive(Debug, Clone, Default)]
pub struct MetricLivedataWindow {
    pub data: Vec<(f64, f64)>,
    pub min_value: f64,
    pub max_value: f64,
    pub min_value_str: String,
    pub max_value_str: String,

    pub min_timestamp: f64,
    pub max_timestamp: f64,
    pub min_timestamp_str: String,
    pub max_timestamp_str: String,

    data_sorted: BTreeMap<u64, f64>,
}

impl MetricLivedataWindow {
    pub(super) fn push_data(&mut self, timestamp: u64, value: f64) {
        if self.data_sorted.len() == LIVEDATA_WINDOW_LIMIT {
            self.data_sorted
                .remove(&self.data_sorted.keys().next().unwrap().clone());
        }
        self.data_sorted.insert(timestamp, value);

        let min_timestamp = self.data_sorted.first_key_value().unwrap().0;
        let max_timestamp = self.data_sorted.last_key_value().unwrap().0;
        self.min_timestamp = *min_timestamp as f64;
        self.max_timestamp = *max_timestamp as f64;
        let min_datetime =
            DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_millis(*min_timestamp));
        let max_datetime =
            DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_millis(*max_timestamp));
        let ts_format = if min_datetime.date_naive() == max_datetime.date_naive() {
            "%H:%M:%S"
        } else {
            "%H:%M:%S %d-%m-%y"
        };
        self.min_timestamp_str = min_datetime.format(ts_format).to_string();
        self.max_timestamp_str = max_datetime.format(ts_format).to_string();
        self.data = self
            .data_sorted
            .iter()
            .map(|(ts, val)| (*ts as f64, *val))
            .collect();
        self.min_value = self
            .data
            .iter()
            .map(|(_, val)| *val)
            .reduce(f64::min)
            .unwrap();
        self.max_value = self
            .data
            .iter()
            .map(|(_, val)| *val)
            .reduce(f64::max)
            .unwrap();
        self.min_value_str = format!("{:.2}", self.min_value);
        self.max_value_str = format!("{:.2}", self.max_value);
    }
}
