use crate::model::protocol::MetricValue;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use crate::app::livedata::MetricLivedataWindow;
use crate::app::widgets::ModalDialog;
use crate::model::ToMqttId;

#[derive(Clone, Default)]
pub(super) struct UIState {
    pub current_sensor: Option<(usize, Uuid)>,
    pub current_metric: Option<(usize, Uuid)>,
    pub modal_dialog: Option<Arc<RwLock<dyn ModalDialog + Send + Sync + 'static>>>,

    pub livedata: HashMap<(Uuid, Uuid), MetricLivedataWindow>,
}

impl UIState {
    pub fn accept_livedata(
        &mut self,
        sensor_id: Uuid,
        metric_id: Uuid,
        value: MetricValue,
        timestamp: u64,
    ) {
        let key = (sensor_id, metric_id);
        let value = match value {
            MetricValue::Double(value) => value,
            MetricValue::Integer(value) => value as f64,
            MetricValue::Boolean(value) => value as u8 as f64,
            MetricValue::String(value) => {
                log::debug!("String livedata {} for metric {}/{} is ignored", value, key.0.to_mqtt(), key.1.to_mqtt());
                return;
            }
        };

        let metric_livedata_window = self.livedata.entry(key).or_default();
        metric_livedata_window.push_data(timestamp, value);
    }
}
