use serde_with::{serde_as, DisplayFromStr};
use serde::{Deserialize, Serialize};

use uuid::Uuid;
use crate::model::sensor::Metric;

// TODO Incorporate into MqttRequest and MqtResponse types

#[derive(Debug, Serialize)]
pub struct CreateSensorRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateSensorRequest {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct MetricsArrayRequest<T> {
    pub metrics: Vec<T>,
}

impl<T> MetricsArrayRequest<T> {
    pub fn one(metric: T) -> Self {
        Self { metrics: vec![metric] }
    }
    pub fn many(metrics: Vec<T>) -> Self {
        Self { metrics }
    }
}

#[derive(Debug, Deserialize)]
pub struct MetricsArrayResponse<T> {
    pub metrics: Vec<T>,
}

#[serde_as]
#[derive(Deserialize)]
pub struct CreateMetricResponsePayload {
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "matchingId")]
    pub matching_id: usize,

    #[serde(rename = "metricId")]
    #[serde(with = "uuid::serde::simple")]
    pub metric_id: Uuid,
}

#[serde_as]
#[derive(Debug, Serialize)]
pub struct CreateMetricPayload {
    #[serde(flatten)]
    pub metric: Metric,

    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "matchingId")]
    pub matching_id: usize,
}

#[derive(Debug, Serialize)]
pub struct UpdateMetricRequest<'a> {
    #[serde(rename = "metricId")]
    #[serde(with = "uuid::serde::simple")]
    pub metric_id: Uuid,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,

    #[serde(rename = "valueAnnotation")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_annotation: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct DeleteMetricRequest {
    #[serde(rename = "metricId")]
    #[serde(with = "uuid::serde::simple")]
    pub metric_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Double(f64),
    String(String),
    Boolean(bool),
}

#[derive(Debug, Serialize)]
pub struct PushMetricValueRequest {
    #[serde(rename = "metricId")]
    #[serde(with = "uuid::serde::simple")]
    pub metric_id: Uuid,

    pub value: MetricValue,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u128>,
}

#[derive(Debug, Deserialize)]
pub struct PushMetricValueResponse {
    #[serde(rename = "metricId")]
    #[serde(with = "uuid::serde::simple")]
    pub metric_id: Uuid,

    pub value: MetricValue,
}

#[derive(Debug, Serialize)]
pub struct PingRequest {
    pub request: String,
}

#[derive(Debug, Deserialize)]
pub struct PingResponse {
    pub answer: String,
}
