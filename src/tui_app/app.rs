use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, StreamHandler, WrapFuture};
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use eyre::Result;

use futures::StreamExt;

use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::Mutex;

use crate::client::client::SensorVisionClient;
use crate::client::client_queries::{
    CreateMetrics, CreateSensor, DeleteMetric, DeleteSensor, LoadSensors, PushValue, UpdateMetric,
    UpdateSensor,
};
use crate::client::state::queries::GetStateSnapshot;
use crate::client::state::{SensorStateEvent, Sensors, SubscribeToStateEvents};
use crate::model::sensor::{Metric, ValueType, ValueUnit};
use crate::tui_app::dialog::{
    ConfirmationDialogActor, ConfirmationDialogState, DialogButton, DialogResult, InputDialogActor,
    InputDialogState, MetricDialogActor, MetricDialogState, ModalDialog,
};
use crate::tui_app::tui::{SharedTui, Tui};
use crate::tui_app::ui_state::queries::*;
use crate::tui_app::ui_state::render::Render;
use crate::tui_app::ui_state::UIState;

use crate::tui_app::theme::THEME_INDEX;

#[derive(Message)]
#[rtype(result = "()")]
pub struct RunLoop {
    pub finished_sender: oneshot::Sender<Result<()>>,
    pub tui: Tui,
}

#[derive(Clone)]
pub struct AppClient {
    sv_client_actor: Addr<SensorVisionClient>,
    ui_state_actor: Addr<UIState>,

    rerun_sender: Option<mpsc::Sender<()>>,
    exit_sender: Option<mpsc::Sender<()>>,
}

impl AppClient {
    pub fn new(sv_client_actor: Addr<SensorVisionClient>) -> Self {
        let ui_state_actor = UIState::default().start();
        Self {
            sv_client_actor,
            ui_state_actor,
            rerun_sender: Option::default(),
            exit_sender: Option::default(),
        }
    }

    async fn run(
        &mut self,
        tui: Tui,
        mut rerun_receiver: mpsc::Receiver<()>,
        mut exit_receiver: mpsc::Receiver<()>,
    ) -> Result<()> {
        self.sv_client_actor.send(LoadSensors).await??;
        let tui: SharedTui = Arc::new(Mutex::new(tui));

        loop {
            self.render(tui.clone()).await?;

            tokio::select! {
                Some(_) = exit_receiver.recv() => {
                    break;
                }
                Some(_) = rerun_receiver.recv() => {
                    continue;
                }
            }
        }
        tui.lock().await.exit()?;
        Ok(())
    }

    async fn render(&self, tui: SharedTui) -> Result<()> {
        let sensors = self.sv_client_actor.send(GetStateSnapshot).await?;
        self.ui_state_actor.send(Render { tui, sensors }).await?;
        Ok(())
    }

    async fn rerender(&self) {
        if let Some(sender) = &self.rerun_sender {
            let _ = sender.send(()).await;
        }
    }

    async fn current_state(&self) -> Result<(Sensors, UIState)> {
        // FIXME potential data race, use in-memory SQLite for storing state
        let sensors = self.sv_client_actor.send(GetStateSnapshot).await?;
        let ui_state = self.ui_state_actor.send(GetUIStateSnapshot).await?;
        Ok((sensors, ui_state))
    }

    async fn next_sensor(&self) -> Result<()> {
        let (sensors, ui_state) = self.current_state().await?;

        let ui_state_actor = self.ui_state_actor.clone();

        if sensors.is_empty() {
            ui_state_actor.send(SelectSensor(None)).await?;
            ui_state_actor.send(SelectMetric(None)).await?;
            return Ok(());
        }

        if let Some((current_index, _)) = ui_state.current_sensor {
            if current_index < sensors.len() - 1 {
                let new_index = current_index.wrapping_add(1);
                ui_state_actor
                    .send(SelectSensor(Some((
                        new_index,
                        sensors.iter().nth(new_index).unwrap().0.clone(),
                    ))))
                    .await?;
                ui_state_actor.send(SelectMetric(None)).await?;
                return Ok(());
            }
        }

        ui_state_actor
            .send(SelectSensor(Some((
                0,
                sensors.iter().nth(0).unwrap().0.clone(),
            ))))
            .await?;

        ui_state_actor.send(SelectMetric(None)).await?;

        Ok(())
    }

    async fn next_metric(&self) -> Result<()> {
        let (sensors, ui_state) = self.current_state().await?;

        let Some((_, current_sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let Some(sensor) = sensors.get(&current_sensor_id) else {
            return Ok(());
        };

        let ui_state_actor = self.ui_state_actor.clone();

        let metrics = &sensor.metrics;
        if metrics.is_empty() {
            ui_state_actor.send(SelectMetric(None)).await?;
            return Ok(());
        }
        let mut new_index = 0;
        if let Some((current_index, _)) = ui_state.current_metric {
            if current_index < metrics.len() - 1 {
                new_index = current_index.wrapping_add(1);
            }
        }
        ui_state_actor
            .send(SelectMetric(Some((
                new_index,
                metrics[new_index].metric_id().clone(),
            ))))
            .await?;

        Ok(())
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        if key_event.kind != KeyEventKind::Press {
            return Ok(());
        }

        if self.ui_state_actor.send(HandleKeyEvent(key_event)).await? {
            return Ok(());
        }

        use KeyCode::*;

        match key_event.code {
            Char('q') => {
                if let Some(sender) = &self.exit_sender {
                    sender.send(()).await?;
                }
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

            Char('N') => {
                self.create_metric().await?;
            }

            Char('e') => {
                self.update_sensor().await?;
            }

            Char('E') => {
                self.update_metric().await?;
            }

            Char(' ') => {
                self.push_value().await?;
            }

            Char('t') => {
                let theme_idx = THEME_INDEX.load(Ordering::SeqCst);
                THEME_INDEX.store(if theme_idx != 0 { 0 } else { 1 }, Ordering::SeqCst);
            }

            _ => {
                return Ok(());
            }
        }

        self.rerender().await;

        Ok(())
    }

    async fn create_sensor(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let dialog_actor = InputDialogActor::new(
            InputDialogState {
                title: "Create Sensor".to_owned(),
                text: "Create a new Sensor?".to_owned(),
                label: "Name:".to_owned(),
                text_input: None,
                focused_button: Some(DialogButton::Ok),
            },
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
            if let DialogResult::Accept { result: new_name } = dialog_result {
                if let Err(err) = sv_client_actor
                    .send(CreateSensor {
                        name: new_name.clone(),
                    })
                    .await
                {
                    log::error!("Failed to send SensorUpdate for {new_name}: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Input(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

        Ok(())
    }

    async fn update_sensor(&self) -> Result<()> {
        let (sensors, ui_state) = self.current_state().await?;
        let Some((_, sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let sensor_name = sensors.get(&sensor_id).unwrap().name.clone();

        let (tx, rx) = oneshot::channel();
        let dialog_actor = InputDialogActor::new(
            InputDialogState {
                title: "Update Sensor".to_owned(),
                text: format!("Rename Sensor {}?", sensor_name),
                label: "Name:".to_owned(),
                text_input: Some(sensor_name),
                focused_button: Some(DialogButton::Ok),
            },
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
            if let DialogResult::Accept { result: new_name } = dialog_result {
                if let Err(err) = sv_client_actor
                    .send(UpdateSensor {
                        sensor_id,
                        name: new_name.clone(),
                        state: None,
                    })
                    .await
                {
                    log::error!("Failed to send SensorUpdate for {new_name}: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Input(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

        Ok(())
    }

    async fn delete_sensor(&self) -> Result<()> {
        let ui_state = self.ui_state_actor.send(GetUIStateSnapshot).await?;
        let Some((_, sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let (tx, rx) = oneshot::channel::<DialogResult<()>>();
        let dialog_actor = ConfirmationDialogActor::new(
            ConfirmationDialogState {
                title: "Delete Sensor".to_owned(),
                text: format!("Delete Sensor #{}?", sensor_id),
                focused_button: Some(DialogButton::Cancel),
            },
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
            if matches!(dialog_result, DialogResult::Accept { result: () }) {
                if let Err(err) = sv_client_actor.send(DeleteSensor { sensor_id }).await {
                    log::error!("Failed to send SensorDelete for {sensor_id}: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Confirmation(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

        Ok(())
    }

    async fn create_metric(&self) -> Result<()> {
        let ui_state = self.ui_state_actor.send(GetUIStateSnapshot).await?;
        let Some((_, sensor_id)) = ui_state.current_sensor else {
            return Ok(());
        };

        let (tx, rx) = oneshot::channel();
        let dialog_actor = MetricDialogActor::new(
            MetricDialogState::new(
                "Create Metric".to_owned(),
                "Which Metric to create?".to_owned(),
                vec![
                    Metric::predefined(String::default(), ValueUnit::Percent),
                    Metric::custom(String::default(), ValueType::Integer, String::default()),
                ],
            )?,
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
            if let DialogResult::Accept { result: new_metric } = dialog_result {
                if let Err(err) = sv_client_actor
                    .send(CreateMetrics {
                        sensor_id,
                        metrics: vec![new_metric],
                    })
                    .await
                {
                    log::error!("Failed to send CreateMetrics: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Metric(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

        Ok(())
    }

    async fn update_metric(&self) -> Result<()> {
        let (sensors, ui_state) = self.current_state().await?;
        let (Some((_, sensor_id)), Some((_, metric_id))) =
            (ui_state.current_sensor, ui_state.current_metric)
        else {
            return Ok(());
        };

        let current_metric = sensors
            .get(&sensor_id)
            .unwrap()
            .metrics
            .iter()
            .find(|metric| metric.metric_id() == &metric_id)
            .unwrap();

        let (tx, rx) = oneshot::channel();
        let dialog_actor = MetricDialogActor::new(
            MetricDialogState::new(
                "Update Metric".to_owned(),
                "Change current metric".to_owned(),
                vec![current_metric.clone()],
            )?,
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
            if let DialogResult::Accept { result: metric } = dialog_result {
                if let Err(err) = sv_client_actor
                    .send(UpdateMetric {
                        sensor_id,
                        metric_id,
                        name: Some(metric.name().to_owned()),
                        value_annotation: {
                            match metric {
                                Metric::Custom {
                                    value_annotation, ..
                                } => Some(value_annotation),
                                _ => None,
                            }
                        },
                    })
                    .await
                {
                    log::error!("Failed to send MetricUpdate: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Metric(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

        Ok(())
    }

    async fn delete_metric(&self) -> Result<()> {
        let ui_state = self.ui_state_actor.send(GetUIStateSnapshot).await?;
        let (Some((_, sensor_id)), Some((_, metric_id))) =
            (ui_state.current_sensor, ui_state.current_metric)
        else {
            return Ok(());
        };

        let (tx, rx) = oneshot::channel::<DialogResult<()>>();
        let dialog_actor = ConfirmationDialogActor::new(
            ConfirmationDialogState {
                title: "Delete Metric".to_owned(),
                text: format!("Delete Metric # {} / #{}?", sensor_id, metric_id),
                focused_button: Some(DialogButton::Cancel),
            },
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
            if matches!(dialog_result, DialogResult::Accept { result: () }) {
                if let Err(err) = sv_client_actor
                    .send(DeleteMetric {
                        sensor_id,
                        metric_id,
                    })
                    .await
                {
                    log::error!("Failed to send MetricDelete for {sensor_id}/{metric_id}: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Confirmation(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

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

        let (tx, rx) = oneshot::channel();
        let dialog_actor = InputDialogActor::new(
            InputDialogState {
                title: "Push Value to Metric".to_owned(),
                text: format!("Push value to Metric {metric_name}?"),
                label: "Value:".to_owned(),
                text_input: default_value,
                focused_button: Some(DialogButton::Ok),
            },
            tx,
        )
        .start();

        let ui_state_actor = self.ui_state_actor.clone();
        let sv_client_actor = self.sv_client_actor.clone();

        actix::spawn(async move {
            let dialog_result = rx.await.expect("Receiving failed");
            let _ = ui_state_actor.send(SetModalDialog(None)).await;
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

                if let Err(err) = sv_client_actor
                    .send(PushValue {
                        sensor_id,
                        metric_id,
                        value: metric_value,
                        timestamp: None,
                    })
                    .await
                {
                    log::error!("Failed to Push Metric: {err}");
                }
            }
        });

        let message = SetModalDialog(Some(ModalDialog::Input(dialog_actor.clone())));
        self.ui_state_actor.send(message).await?;

        Ok(())
    }
}

impl Actor for AppClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let sv_client_actor = self.sv_client_actor.clone();
        let weak_this = ctx.address().downgrade().recipient();
        ctx.spawn(
            async move {
                let _ = sv_client_actor
                    .send(SubscribeToStateEvents(weak_this))
                    .await;
            }
            .into_actor(self),
        );

        let term_event_stream = crossterm::event::EventStream::new();

        let event_stream = term_event_stream
            .filter_map(|term_event| async { term_event.ok().map(|event| TermEvent(event)) });

        ctx.add_stream(event_stream);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct TermEvent(CrosstermEvent);

impl StreamHandler<TermEvent> for AppClient {
    fn handle(&mut self, TermEvent(event): TermEvent, ctx: &mut Self::Context) {
        match event {
            CrosstermEvent::Key(key_event) => {
                let mut app = self.clone();
                ctx.spawn(
                    async move {
                        let _ = app.handle_key_event(key_event).await;
                        app.rerender().await;
                    }
                    .into_actor(self),
                );
            }

            CrosstermEvent::Resize(..) => {
                let app = self.clone();
                ctx.spawn(
                    async move {
                        app.rerender().await;
                    }
                    .into_actor(self),
                );
            }

            _ => {}
        }
    }
}

impl Handler<SensorStateEvent> for AppClient {
    type Result = ();

    fn handle(&mut self, event: SensorStateEvent, ctx: &mut Self::Context) -> Self::Result {
        use SensorStateEvent::*;
        let app = self.clone();

        match event {
            NewLinkedSensorLoaded(..)
            | ExistingLinkedSensorLoaded(..)
            | NewMetricLoaded { .. }
            | NewSensorCreated(..)
            | NewMetricCreated { .. } => {
                let app = self.clone();
                ctx.spawn(
                    async move {
                        let ui_state = app.ui_state_actor.send(GetUIStateSnapshot).await;
                        if let Err(err) = ui_state {
                            log::error!("Failed to load UI State: {err}");
                            return;
                        }
                        let ui_state = ui_state.unwrap();
                        if ui_state.current_sensor.is_none() {
                            let _ = app.next_sensor().await;
                        }
                        if ui_state.current_metric.is_none() {
                            let _ = app.next_metric().await;
                        }
                        app.rerender().await;
                    }
                    .into_actor(self),
                );
            }

            Livedata {
                sensor_id,
                metric_id,
                value,
                timestamp,
            } => {
                let ui_state_actor = self.ui_state_actor.clone();
                ctx.spawn(
                    async move {
                        let _ = ui_state_actor
                            .send(AcceptLivedata {
                                sensor_id,
                                metric_id,
                                value,
                                timestamp,
                            })
                            .await;
                        app.rerender().await;
                    }
                    .into_actor(self),
                );
            }

            SensorDeleted { sensor_id } => {
                let app = app.clone();
                ctx.spawn(
                    async move {
                        let _ = app.ui_state_actor.send(DropSensor(sensor_id)).await;
                        let _ = app.next_sensor().await;
                        let _ = app.next_metric().await;
                        app.rerender().await;
                    }
                    .into_actor(self),
                );
            }

            MetricDeleted {
                sensor_id,
                metric_id,
            } => {
                let app = app.clone();
                ctx.spawn(
                    async move {
                        let _ = app
                            .ui_state_actor
                            .send(DropMetric(sensor_id, metric_id))
                            .await;
                        let _ = app.next_metric().await;
                        app.rerender().await;
                    }
                    .into_actor(self),
                );
            }

            _ => {}
        }
    }
}

impl Handler<RunLoop> for AppClient {
    type Result = ();

    fn handle(
        &mut self,
        RunLoop {
            finished_sender,
            tui,
        }: RunLoop,
        _: &mut Self::Context,
    ) -> Self::Result {
        let (rerun_sender, rerun_receiver) = mpsc::channel(1);
        let (exit_sender, exit_receiver) = mpsc::channel(1);
        self.rerun_sender = Some(rerun_sender);
        self.exit_sender = Some(exit_sender);

        let mut app = self.clone();
        actix::spawn(async move {
            if let Err(err) =
                finished_sender.send(app.run(tui, rerun_receiver, exit_receiver).await)
            {
                log::error!("Run loop error {:?}", err);
            }
        });
    }
}
