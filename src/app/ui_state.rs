use crate::model::protocol::MetricValue;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, UNIX_EPOCH};
use uuid::Uuid;
use crate::model::ToMqttId;

const LIVEDATA_WINDOW_LIMIT: usize = 50;

#[derive(Clone, Default)]
pub(super) struct MetricLivedataWindow {
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

#[derive(Default, Clone)]
pub(super) struct UIState {
    pub current_sensor_index: Option<usize>,

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
