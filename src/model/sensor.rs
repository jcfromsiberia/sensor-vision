use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ValueUnit {
    #[serde(rename = "SI.ElectricCurrent.AMPERE")]
    Ampere,

    #[serde(rename = "SI.DataAmount.BIT")]
    Bit,

    #[serde(rename = "SI.LuminousIntensity.CANDELA")]
    Candela,

    #[serde(rename = "SI.Temperature.CELSIUS")]
    Celsius,

    #[serde(rename = "NoSI.Dimensionless.DECIBEL")]
    Decibel,

    #[serde(rename = "SI.ElectricCapacitance.FARAD")]
    Farad,

    #[serde(rename = "SI.Frequency.HERTZ")]
    Hertz,

    #[serde(rename = "SI.Energy.JOULE")]
    Joule,

    #[serde(rename = "SI.Mass.KILOGRAM")]
    Kilogram,

    #[serde(rename = "NoSI.Location.LATITUDE")]
    Latitude,

    #[serde(rename = "NoSI.Location.LONGITUDE")]
    Longitude,

    #[serde(rename = "SI.Length.METER")]
    Meter,

    #[serde(rename = "SI.Velocity.METERS_PER_SECOND")]
    MetersPerSecond,

    #[serde(rename = "SI.Acceleration.METERS_PER_SQUARE_SECOND")]
    MetersPerSquareSecond,

    #[serde(rename = "SI.AmountOfSubstance.MOLE")]
    Mole,

    #[serde(rename = "SI.Force.NEWTON")]
    Newton,

    #[serde(rename = "SI.ElectricResistance.OHM")]
    Ohm,

    #[serde(rename = "SI.Pressure.PASCAL")]
    Pascal,

    #[serde(rename = "NoSI.Dimensionless.PERCENT")]
    Percent,

    #[serde(rename = "SI.Angle.RADIAN")]
    Radian,

    #[serde(rename = "SI.Duration.SECOND")]
    Second,

    #[serde(rename = "SI.Area.SQUARE_METRE")]
    SquareMetre,

    #[serde(rename = "SI.ElectricPotential.VOLT")]
    Volt,

    #[serde(rename = "SI.Power.WATT")]
    Watt,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ValueType {
    #[serde(rename = "boolean")]
    Boolean,

    #[serde(rename = "double")]
    Double,

    #[serde(rename = "integer")]
    Integer,

    #[serde(rename = "string")]
    String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Validate)]
pub struct Sensor<T> {
    #[validate(min_length = 2)]
    #[validate(max_length = 64)]
    pub name: String,

    #[serde(rename = "sensorId")]
    #[serde(with = "uuid::serde::simple")]
    pub sensor_id: Uuid,

    // TODO Spike how to get HashMap here instead of Vec
    #[serde(default)]
    pub metrics: Vec<T>,

    #[serde(skip)]
    pub connector_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize, Validate)]
#[serde(untagged)]
pub enum Metric {
    Predefined {
        #[validate(min_length = 2)]
        #[validate(max_length = 64)]
        name: String,

        #[serde(rename = "metricId")]
        #[serde(with = "uuid::serde::simple")]
        #[serde(skip_serializing_if = "Uuid::is_nil")]
        metric_id: Uuid,

        #[serde(rename = "valueUnit")]
        value_unit: ValueUnit,
    },
    Custom {
        #[validate(min_length = 2)]
        #[validate(max_length = 64)]
        name: String,

        #[serde(rename = "metricId")]
        #[serde(with = "uuid::serde::simple")]
        #[serde(skip_serializing_if = "Uuid::is_nil")]
        metric_id: Uuid,

        // TODO Add validation
        #[serde(rename = "valueAnnotation")]
        value_annotation: String,

        #[serde(rename = "valueType")]
        value_type: ValueType,
    },
}

impl Metric {
    pub fn predefined(name: String, value_unit: ValueUnit) -> Self {
        Metric::Predefined {
            name,
            value_unit,
            metric_id: Uuid::nil(),
        }
    }

    pub fn custom(name: String, value_type: ValueType, value_annotation: String) -> Self {
        Metric::Custom {
            name,
            value_type,
            value_annotation,
            metric_id: Uuid::nil(),
        }
    }
    pub fn name(&self) -> &String {
        match self {
            Metric::Predefined { name, .. } => name,
            Metric::Custom { name, .. } => name,
        }
    }

    pub fn metric_id(&self) -> &Uuid {
        match self {
            Metric::Predefined { metric_id, .. } => metric_id,
            Metric::Custom { metric_id, .. } => metric_id,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LinkedMetric {
    pub link: String,

    #[serde(rename = "metricId")]
    #[serde(with = "uuid::serde::simple")]
    pub metric_id: Uuid,
}

// TODO get rid of Default impl for Metric
impl Default for Metric {
    fn default() -> Self {
        Self::Custom {
            name: "Unknown".to_string(),
            value_type: ValueType::Integer,
            value_annotation: "Unit".to_string(),
            metric_id: Uuid::nil(),
        }
    }
}

impl Sensor<Metric> {
    // pub fn create_metrics(&mut self, metrics: Vec<Metric>) -> Result<Vec<Uuid>> {
    //     // https://docs-iot.teamviewer.com/mqtt-api/#541-create
    //     #[serde_as]
    //     #[derive(Debug, Serialize)]
    //     struct CreateMetricPayload {
    //         #[serde(flatten)]
    //         metric: Metric,
    //
    //         #[serde_as(as = "DisplayFromStr")]
    //         #[serde(rename = "matchingId")]
    //         matching_id: usize,
    //     }
    //
    //     #[serde_as]
    //     #[derive(Deserialize)]
    //     struct CreateMetricResponsePayload {
    //         #[serde_as(as = "DisplayFromStr")]
    //         #[serde(rename = "matchingId")]
    //         matching_id: usize,
    //
    //         #[serde(rename = "metricId")]
    //         #[serde(with = "uuid::serde::simple")]
    //         metric_id: Uuid,
    //     }
    //
    //     let request = MetricsRequest::<CreateMetricPayload> {
    //         metrics: metrics
    //             .iter()
    //             .enumerate()
    //             .map(|(i, &ref metric)| CreateMetricPayload {
    //                 metric: metric.clone(),
    //                 matching_id: i + 1,
    //             })
    //             .collect(),
    //     };
    //
    //     let request_serialized = serde_json::to_string(&request)?;
    //
    //     // According to https://docs-iot.teamviewer.com/mqtt-api/#541-create
    //     let response_serialized = {
    //         let state_shared_ptr = self.parent.upgrade().ok_or_eyre("State is null")?;
    //         let mut state = state_shared_ptr.borrow_mut();
    //         state.sync_action(
    //             Action::MetricCreate(&self.sensor_id),
    //             Some(request_serialized),
    //         )?
    //     };
    //
    //     // println!("{response_serialized}");
    //
    //     let metrics_created =
    //         serde_json::from_str::<Vec<CreateMetricResponsePayload>>(&response_serialized)
    //             .wrap_err_with(|| format!("Failed to deserialize: {}", &response_serialized))?;
    //
    //     if metrics_created.len() == metrics.len() {
    //         let mut metrics = metrics;
    //
    //         let metric_ids = metrics_created
    //             .iter()
    //             .map(|metric| metric.metric_id.clone())
    //             .collect();
    //
    //         for metric_created in metrics_created {
    //             match &mut metrics[metric_created.matching_id - 1] {
    //                 Metric::Predefined {
    //                     metric_id,
    //                     name: _,
    //                     value_unit: _,
    //                 } => *metric_id = metric_created.metric_id.clone(),
    //                 Metric::Custom {
    //                     metric_id,
    //                     name: _,
    //                     value_type: _,
    //                     value_annotation: _,
    //                 } => *metric_id = metric_created.metric_id.clone(),
    //             }
    //         }
    //
    //         self.metrics.extend(metrics);
    //         Ok(metric_ids)
    //     } else {
    //         Err(eyre!("Size mismatch"))
    //     }
    // }
    //
    // pub fn update_metric<'a>(
    //     &mut self,
    //     metric_id: &Uuid,
    //     name: Option<&'a str>,
    //     value_annotation: Option<&'a str>,
    // ) -> Result<String> {
    //     // According to https://docs-iot.teamviewer.com/mqtt-api/#543-update
    //     #[derive(Debug, Serialize)]
    //     struct UpdateMetricRequest<'b> {
    //         #[serde(rename = "metricId")]
    //         metric_id: String,
    //
    //         name: Option<&'b str>,
    //
    //         #[serde(rename = "valueAnnotation")]
    //         value_annotation: Option<&'b str>,
    //     }
    //
    //     let request = MetricsRequest::<UpdateMetricRequest> {
    //         metrics: vec![UpdateMetricRequest {
    //             metric_id: metric_id.as_simple().to_string(),
    //             name,
    //             value_annotation,
    //         }],
    //     };
    //
    //     let request_serialized = serde_json::to_string(&request)?;
    //
    //     let state_shared_ptr = self.parent.upgrade().ok_or_eyre("State is null")?;
    //     let mut state = state_shared_ptr.borrow_mut();
    //     let result = state.sync_action(
    //         Action::MetricUpdate(&self.sensor_id),
    //         Some(request_serialized),
    //     )?;
    //
    //     // TODO Async Subscribe to metric metadata topic(s) to avoid checking the result
    //     // and modifying data here
    //
    //     // TODO Update local metric!!
    //
    //     Ok(result)
    // }
    //
    // pub fn delete_metric(&mut self, metric_id: &Uuid) -> Result<String> {
    //     // According to https://docs-iot.teamviewer.com/mqtt-api/#544-delete
    //     #[derive(Debug, Serialize)]
    //     struct DeleteMetricRequest {
    //         #[serde(rename = "metricId")]
    //         metric_id: String,
    //     }
    //
    //     let request = MetricsRequest::<DeleteMetricRequest> {
    //         metrics: vec![DeleteMetricRequest {
    //             metric_id: metric_id.as_simple().to_string(),
    //         }],
    //     };
    //
    //     let request_serialized = serde_json::to_string(&request)?;
    //
    //     let state_shared_ptr = self.parent.upgrade().ok_or_eyre("State is null")?;
    //     let mut state = state_shared_ptr.borrow_mut();
    //     let result = state.sync_action(Action::MetricDelete(&self.sensor_id), None)?;
    //
    //     // TODO Async Subscribe to metric metadata topic(s) to avoid checking the result
    //     // and modifying data here
    //
    //     // TODO Delete local metric!!
    //
    //     Ok(result)
    // }
}