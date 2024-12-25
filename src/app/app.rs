use crate::app::dialog::{
    DialogButton, DialogResult, InputDialogState, InputModalDialogHandle, MessageDialogState,
    MessageModalDialogHandle, ModalDialog, RespondingDialogState, RespondingInputDialogState,
    RespondingMessageDialogState,
};
use crate::app::events::{Event, EventHandler};
use crate::app::render::render_state;
use crate::app::tui::Tui;
use crate::app::ui_state::{UIActorCommand, UIState, UIStateActorHandle};
use crate::client::client::SensorVisionClient;
use crate::client::state::{SensorStateEvent, Sensors};
use crate::model::sensor::{Metric, ValueType};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEventKind};
use futures::executor::block_on;
use ratatui::backend::Backend;
use ratatui::Frame;
use std::fmt::Debug;
use tokio::sync::{broadcast, oneshot};

#[derive(Debug, Clone)]
pub struct AppClient {
    sv_client: SensorVisionClient,

    ui_state_handle: UIStateActorHandle,

    pub running: bool,
}

impl AppClient {
    pub fn new(sv_client: SensorVisionClient) -> Result<Self> {
        let ui_state_handle = UIStateActorHandle::new()?;
        Ok(Self {
            sv_client,
            ui_state_handle,
            running: false,
        })
    }

    #[tracing::instrument]
    pub async fn run<B: Backend + Debug>(
        &mut self,
        tui: &mut Tui<B>,
        mut event_stream: EventHandler,
    ) -> Result<()> {
        let mut sensor_state_event_receiver = self.sv_client.state_event_receiver();

        self.sv_client.load_sensors()?;

        self.running = true;
        while self.running {
            tokio::select! {
                Ok(event) = event_stream.next() => {
                    block_on(self.handle_terminal_event(event))?;
                },

                Ok(event) = sensor_state_event_receiver.recv() => {
                    block_on(self.handle_state_event(event))?;
                },
            }

            tui.terminal.draw(|frame| self.render(frame))?;
        }

        Ok(())
    }

    async fn current_state(&self) -> Result<(Sensors, UIState)> {
        let sensors = self.sv_client.get_sensors().await;
        let ui_state = self.ui_state_handle.snapshot().await?;
        Ok((sensors, ui_state))
    }

    pub async fn next_sensor(&self) -> Result<()> {
        use UIActorCommand::{SelectMetric, SelectSensor};
        let (sensors, ui_state) = self.current_state().await?;

        let ui_state_handle = self.ui_state_handle.clone();

        if sensors.is_empty() {
            ui_state_handle.actor_commands(vec![SelectSensor(None), SelectMetric(None)]);
            return Ok(());
        }

        if let Some((current_index, _)) = ui_state.current_sensor {
            if current_index < sensors.len() - 1 {
                let new_index = current_index.wrapping_add(1);
                ui_state_handle.actor_commands(vec![
                    SelectSensor(Some((
                        new_index,
                        sensors.iter().nth(new_index).unwrap().0.clone(),
                    ))),
                    SelectMetric(None),
                ]);
                return Ok(());
            }
        }

        ui_state_handle.actor_commands(vec![
            SelectSensor(Some((0, sensors.iter().nth(0).unwrap().0.clone()))),
            SelectMetric(None),
        ]);

        Ok(())
    }

    pub async fn next_metric(&self) -> Result<()> {
        use UIActorCommand::SelectMetric;
        let (sensors, ui_state) = self.current_state().await?;

        let Some((_, current_sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let Some(sensor) = sensors.get(&current_sensor_id) else {
            return Ok(());
        };

        let ui_state_handle = self.ui_state_handle.clone();

        let metrics = &sensor.metrics;
        if metrics.is_empty() {
            ui_state_handle.actor_command(SelectMetric(None));
            return Ok(());
        }
        let mut new_index = 0;
        if let Some((current_index, _)) = ui_state.current_metric {
            if current_index < metrics.len() - 1 {
                new_index = current_index.wrapping_add(1);
            }
        }
        ui_state_handle.actor_command(SelectMetric(Some((
            new_index,
            metrics[new_index].metric_id().clone(),
        ))));

        Ok(())
    }

    async fn handle_terminal_event(&mut self, event: Event) -> Result<()> {
        let Event::Key(key_event) = event else {
            return Ok(());
        };

        if key_event.kind != KeyEventKind::Press {
            return Ok(());
        }

        let (tx, rx) = oneshot::channel::<bool>();
        let command = UIActorCommand::HandleKeyEvent {
            event: key_event.clone(),
            respond_to: tx,
        };
        self.ui_state_handle.actor_command(command);

        if rx.await? {
            return Ok(());
        }

        use KeyCode::*;

        match key_event.code {
            Char('q') => {
                self.running = false;
            }

            Tab => {
                self.next_sensor().await?;
                self.next_metric().await?;
            }

            BackTab => {
                self.next_metric().await?;
            }

            Char('d') => {
                self.delete_sensor().await?;
            }

            Char('D') => {
                self.delete_metric().await?;
            }

            Char('n') => {
                self.create_sensor().await?;
            }

            Char('e') => {
                self.update_sensor().await?;
            }

            Char(' ') => {
                self.push_value().await?;
            }

            _ => {}
        }

        Ok(())
    }

    async fn handle_state_event(&self, event: SensorStateEvent) -> Result<()> {
        use SensorStateEvent::*;
        use UIActorCommand::*;
        match event {
            Livedata {
                sensor_id,
                metric_id,
                value,
                timestamp,
            } => {
                self.ui_state_handle.actor_command(AcceptLivedata {
                    sensor_id,
                    metric_id,
                    value,
                    timestamp,
                });
            }

            SensorDeleted { sensor_id } => {
                self.ui_state_handle.actor_command(DropSensor(sensor_id));
                self.next_sensor().await?;
                self.next_metric().await?;
            }

            MetricDeleted {
                sensor_id,
                metric_id,
            } => {
                self.ui_state_handle
                    .actor_command(DropMetric(sensor_id, metric_id));
                self.next_metric().await?;
            }

            _ => {}
        }

        Ok(())
    }

    async fn delete_sensor(&self) -> Result<()> {
        // TODO Reduce boilerplate in favor of macros
        let ui_state = self.ui_state_handle.snapshot().await?;

        let Some((_, sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let initial_dialog_state = MessageDialogState {
            title: "Delete Sensor".to_owned(),
            text: format!("Delete Sensor #{}?", sensor_id),
            focused_button: Some(DialogButton::Cancel),
        };

        let (tx, rx) = oneshot::channel();

        let responding_state = RespondingMessageDialogState::new(initial_dialog_state, tx);

        let ui_state_handle = self.ui_state_handle.clone();
        let sv_client = self.sv_client.clone();

        tokio::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            ui_state_handle.actor_command(UIActorCommand::SetModalDialog(None));
            if matches!(dialog_result, DialogResult::Accept { result: () }) {
                if let Err(err) = sv_client.delete_sensor(sensor_id) {
                    log::error!("Failed to send SensorDelete for {sensor_id}: {err}");
                }
            }
        });

        let handle = MessageModalDialogHandle::new(responding_state)?;
        let dialog = ModalDialog::Confirmation(handle);

        let command = UIActorCommand::SetModalDialog(Some(dialog));
        self.ui_state_handle.actor_command(command);

        Ok(())
    }

    async fn delete_metric(&self) -> Result<()> {
        let ui_state = self.ui_state_handle.snapshot().await?;
        let (Some((_, sensor_id)), Some((_, metric_id))) =
            (ui_state.current_sensor, ui_state.current_metric)
        else {
            return Ok(());
        };

        let initial_dialog_state = MessageDialogState {
            title: "Delete Metric".to_owned(),
            text: format!("Delete Metric # {} / #{}?", sensor_id, metric_id),
            focused_button: Some(DialogButton::Cancel),
        };

        let (tx, rx) = oneshot::channel();

        let responding_state = RespondingMessageDialogState::new(initial_dialog_state, tx);

        let ui_state_handle = self.ui_state_handle.clone();
        let sv_client = self.sv_client.clone();

        tokio::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            ui_state_handle.actor_command(UIActorCommand::SetModalDialog(None));
            if matches!(dialog_result, DialogResult::Accept { result: () }) {
                if let Err(err) = sv_client.delete_metric(sensor_id, metric_id) {
                    log::error!("Failed to send MetricDelete for {sensor_id}: {err}");
                }
            }
        });

        let handle = MessageModalDialogHandle::new(responding_state)?;
        let dialog = ModalDialog::Confirmation(handle);

        let command = UIActorCommand::SetModalDialog(Some(dialog));
        self.ui_state_handle.actor_command(command);

        Ok(())
    }

    async fn create_sensor(&self) -> Result<()> {
        let initial_dialog_state = InputDialogState {
            title: "Create Sensor".to_owned(),
            text: "Create a new Sensor?".to_owned(),
            label: "Name:".to_owned(),
            text_input: None,
            focused_button: Some(DialogButton::Ok),
        };

        let (tx, rx) = oneshot::channel();

        let responding_state = RespondingInputDialogState::new(initial_dialog_state, tx);

        let ui_state_handle = self.ui_state_handle.clone();
        let sv_client = self.sv_client.clone();

        tokio::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            ui_state_handle.actor_command(UIActorCommand::SetModalDialog(None));
            if let DialogResult::Accept { result: name } = dialog_result {
                if let Err(err) = sv_client.create_sensor(&name) {
                    log::error!("Failed to send SensorCreate for {name}: {err}");
                }
            }
        });

        let handle = InputModalDialogHandle::new(responding_state)?;

        let dialog = ModalDialog::Input(handle);

        let command = UIActorCommand::SetModalDialog(Some(dialog));
        self.ui_state_handle.actor_command(command);

        Ok(())
    }

    async fn update_sensor(&self) -> Result<()> {
        let (sensors, ui_state) = self.current_state().await?;
        let Some((_, sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let sensor_name = sensors.get(&sensor_id).unwrap().name.clone();

        let initial_dialog_state = InputDialogState {
            title: "Update Sensor".to_owned(),
            text: format!("Rename Sensor {}?", sensor_name),
            label: "Name:".to_owned(),
            text_input: Some(sensor_name),
            focused_button: Some(DialogButton::Ok),
        };

        let (tx, rx) = oneshot::channel();

        let responding_state = RespondingInputDialogState::new(initial_dialog_state, tx);

        let ui_state_handle = self.ui_state_handle.clone();
        let sv_client = self.sv_client.clone();

        tokio::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            ui_state_handle.actor_command(UIActorCommand::SetModalDialog(None));

            if let DialogResult::Accept { result: new_name } = dialog_result {
                if let Err(err) = sv_client.update_sensor(sensor_id, &new_name, None) {
                    log::error!("Failed to send SensorUpdate for {new_name}: {err}");
                }
            }
        });

        let handle = InputModalDialogHandle::new(responding_state)?;

        let dialog = ModalDialog::Input(handle);

        let command = UIActorCommand::SetModalDialog(Some(dialog));
        self.ui_state_handle.actor_command(command);

        Ok(())
    }

    async fn push_value(&self) -> Result<()> {
        let (sensors, ui_state) = self.current_state().await?;

        let (Some((_, sensor_id)), Some((metric_index, metric_id))) =
            (ui_state.current_sensor, ui_state.current_metric)
        else {
            return Ok(());
        };

        let metric = sensors
            .get(&sensor_id)
            .unwrap()
            .metrics
            .get(metric_index)
            .unwrap()
            .clone();

        let metric_name = metric.name().clone();

        let default_value = ui_state
            .livedata
            .get(&(sensor_id, metric_id))
            .map(|window| window.data.last().map(|(_, val)| val.to_string()))
            .flatten();

        let initial_dialog_state = InputDialogState {
            title: "Push Value to Metric".to_owned(),
            text: format!("value to Metric {metric_name}?"),
            label: "Value".to_owned(),
            text_input: default_value,
            focused_button: Some(DialogButton::Ok),
        };

        let (tx, rx) = oneshot::channel();

        let responding_state = RespondingInputDialogState::new(initial_dialog_state, tx);

        let ui_state_handle = self.ui_state_handle.clone();
        let sv_client = self.sv_client.clone();

        // Dialog result "callback"
        tokio::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            ui_state_handle.actor_command(UIActorCommand::SetModalDialog(None));
            if let DialogResult::Accept { result: new_value } = dialog_result {
                let metric_value = match &metric {
                    Metric::Predefined { .. } => ValueType::Double.to_value(&new_value),
                    Metric::Custom { value_type, .. } => value_type.to_value(&new_value),
                };

                if let Err(err) = &metric_value {
                    log::error!("Failed to parse \"{new_value}\": {err}");
                    return;
                }

                let metric_value = metric_value.unwrap();

                if let Err(err) = sv_client.push_value(sensor_id, metric_id, metric_value, None) {
                    log::error!("Failed to Push Metric: {err}");
                }
            }
        });

        let handle = InputModalDialogHandle::new(responding_state)?;

        let dialog = ModalDialog::Input(handle);

        let command = UIActorCommand::SetModalDialog(Some(dialog));
        self.ui_state_handle.actor_command(command);

        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        let (sensors, ui_state) = block_on(self.current_state()).expect("Failed to fetch state");
        let _ = render_state(frame, &sensors, &ui_state);
    }
}
