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

    pub state: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct MetricsRequest<T: Serialize> {
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
pub struct UpdateMetricRequest<'b> {
    #[serde(rename = "metricId")]
    pub metric_id: String,

    pub name: Option<&'b str>,

    #[serde(rename = "valueAnnotation")]
    pub value_annotation: Option<&'b str>,
}

#[derive(Debug, Serialize)]
pub struct DeleteMetricRequest {
    #[serde(rename = "metricId")]
    pub metric_id: String,
}

#[derive(Debug, Serialize)]
pub struct PingRequest {
    pub request: String,
}

#[derive(Debug, Deserialize)]
pub struct PingResponse {
    pub answer: String,
}
