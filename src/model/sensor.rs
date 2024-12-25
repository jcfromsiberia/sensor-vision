use eyre::Result;

use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use crate::model::protocol::MetricValue;
use crate::model::*;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
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

impl ValueType {
    pub fn to_value(&self, value: &str) -> Result<MetricValue> {
        match self {
            Self::Double => Ok(MetricValue::Double(value.parse()?)),
            Self::Integer => Ok(MetricValue::Integer(value.parse()?)),
            Self::Boolean => Ok(MetricValue::Boolean(value.parse()?)),
            Self::String => Ok(MetricValue::String(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Validate)]
pub struct Sensor<T> {
    #[validate(min_length = 2)]
    #[validate(max_length = 64)]
    pub name: String,

    #[serde(rename = "sensorId")]
    pub sensor_id: SensorId,

    // TODO Spike how to get HashMap here instead of Vec
    #[serde(default)]
    pub metrics: Vec<T>,

    #[serde(skip)]
    pub connector_id: ConnectorId,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Validate)]
#[serde(untagged)]
pub enum Metric {
    Predefined {
        #[validate(min_length = 2)]
        #[validate(max_length = 64)]
        name: String,

        #[serde(rename = "metricId")]
        #[serde(skip_serializing_if = "MetricId::is_nil")]
        metric_id: MetricId,

        #[serde(rename = "valueUnit")]
        value_unit: ValueUnit,
    },
    Custom {
        #[validate(min_length = 2)]
        #[validate(max_length = 64)]
        name: String,

        #[serde(rename = "metricId")]
        #[serde(skip_serializing_if = "MetricId::is_nil")]
        metric_id: MetricId,

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
            metric_id: MetricId::default(),
        }
    }

    pub fn custom(name: String, value_type: ValueType, value_annotation: String) -> Self {
        Metric::Custom {
            name,
            value_type,
            value_annotation,
            metric_id: MetricId::default(),
        }
    }
    pub fn name(&self) -> &String {
        match self {
            Metric::Predefined { name, .. } => name,
            Metric::Custom { name, .. } => name,
        }
    }

    pub fn metric_id(&self) -> &MetricId {
        match self {
            Metric::Predefined { metric_id, .. } => metric_id,
            Metric::Custom { metric_id, .. } => metric_id,
        }
    }

    pub fn rename(&mut self, new_name: String) {
        match self {
            Metric::Predefined { name, .. } => {*name = new_name;},
            Metric::Custom { name, .. } => {*name = new_name;},
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default, Deserialize)]
pub struct LinkedMetric {
    pub link: String,

    #[serde(rename = "metricId")]
    pub metric_id: MetricId,
}

// TODO get rid of Default impl for Metric
impl Default for Metric {
    fn default() -> Self {
        Self::Custom {
            name: "Unknown".to_string(),
            value_type: ValueType::Integer,
            value_annotation: "Unit".to_string(),
            metric_id: MetricId::default(),
        }
    }
}
