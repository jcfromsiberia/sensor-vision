use actix::{AsyncContext, Handler, Message, MessageResult, WrapFuture};

use crossterm::event::KeyEvent;

use crate::model::protocol::MetricValue;
use crate::model::{MetricId, SensorId};
use crate::tui_app::dialog::ModalDialog;
use crate::tui_app::ui_state::UIState;

#[derive(Message)]
#[rtype(result = "UIState")]
pub struct GetUIStateSnapshot;

#[derive(Message)]
#[rtype(result = "()")]
pub struct SelectSensor(pub Option<(usize, SensorId)>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct SelectMetric(pub Option<(usize, MetricId)>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct AcceptLivedata {
    pub sensor_id: SensorId,
    pub metric_id: MetricId,
    pub value: MetricValue,
    pub timestamp: u64,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetModalDialog(pub Option<ModalDialog>);

#[derive(Message)]
#[rtype(result = "bool")]
pub struct HandleKeyEvent(pub KeyEvent);

#[derive(Message)]
#[rtype(result = "()")]
pub struct DropSensor(pub SensorId);

#[derive(Message)]
#[rtype(result = "()")]
pub struct DropMetric(pub SensorId, pub MetricId);

impl Handler<GetUIStateSnapshot> for UIState {
    type Result = MessageResult<GetUIStateSnapshot>;

    fn handle(&mut self, _: GetUIStateSnapshot, _: &mut Self::Context) -> Self::Result {
        MessageResult(self.clone())
    }
}

impl Handler<SelectSensor> for UIState {
    type Result = ();

    fn handle(
        &mut self,
        SelectSensor(sensor): SelectSensor,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.current_sensor = sensor;
    }
}

impl Handler<SelectMetric> for UIState {
    type Result = ();

    fn handle(
        &mut self,
        SelectMetric(metric): SelectMetric,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.current_metric = metric;
    }
}

impl Handler<AcceptLivedata> for UIState {
    type Result = ();

    fn handle(
        &mut self,
        AcceptLivedata {
            sensor_id,
            metric_id,
            value,
            timestamp,
        }: AcceptLivedata,
        _: &mut Self::Context,
    ) -> Self::Result {
        let key = (sensor_id, metric_id);
        let value = match value {
            MetricValue::Double(value) => value,
            MetricValue::Integer(value) => value as f64,
            MetricValue::Boolean(value) => value as u8 as f64,
            MetricValue::String(_) => {
                return;
            }
        };

        let metric_livedata_window = self.livedata.entry(key).or_default();
        metric_livedata_window.push_data(timestamp, value);
    }
}

impl Handler<DropSensor> for UIState {
    type Result = ();

    fn handle(&mut self, DropSensor(sensor_id): DropSensor, _: &mut Self::Context) -> Self::Result {
        self.livedata
            .retain(|(sens_id, _), _| sens_id != &sensor_id);
        if self
            .current_sensor
            .is_some_and(|(_, sens_id)| sens_id == sensor_id)
        {
            self.current_sensor = None;
            self.current_metric = None;
        }
    }
}

impl Handler<DropMetric> for UIState {
    type Result = ();

    fn handle(
        &mut self,
        DropMetric(sensor_id, metric_id): DropMetric,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.livedata
            .retain(|(sens_id, metr_id), _| sensor_id.ne(sens_id) && metric_id.ne(metr_id));
        if self
            .current_sensor
            .is_some_and(|(_, sens_id)| sens_id == sensor_id)
            && self
                .current_metric
                .is_some_and(|(_, metr_id)| metr_id == metric_id)
        {
            self.current_metric = None;
        }
    }
}

impl Handler<SetModalDialog> for UIState {
    type Result = ();

    fn handle(
        &mut self,
        SetModalDialog(dialog): SetModalDialog,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.modal_dialog = dialog;
    }
}

impl Handler<HandleKeyEvent> for UIState {
    type Result = bool;

    fn handle(
        &mut self,
        key_event_message: HandleKeyEvent,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Some(dialog) = &self.modal_dialog {
            use ModalDialog::*;
            match dialog {
                Confirmation(dialog_actor) => {
                    let dialog_actor = dialog_actor.clone();
                    ctx.spawn(async move {
                        let _ = dialog_actor.send(key_event_message).await;
                    }.into_actor(self));
                },
                Input(dialog_actor) => {
                    let dialog_actor = dialog_actor.clone();
                    ctx.spawn(async move {
                        let _ = dialog_actor.send(key_event_message).await;
                    }.into_actor(self));
                },
            }
            true
        } else {
            false
        }
    }
}
