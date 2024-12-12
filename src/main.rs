use eyre::{OptionExt, Result};

use std::fs;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use x509_certificate::X509Certificate;

use clap::{arg, command, ArgAction};

use uuid::Uuid;

mod model;
mod client;

use client::SensorVisionClient;
use client::mqtt::{setup_new_certificate, MqttClientWrapper};

use model::sensor::{Metric, ValueUnit, ValueType};

fn main() -> Result<()> {
    let matches = command!()
        .arg(arg!(-n --new "Quick setup a new connector").action(ArgAction::SetTrue))
        .arg(arg!(-t --test "Generate test sensor and metrics").action(ArgAction::SetTrue))
        .arg(arg!(-p --ping "Ping test").action(ArgAction::SetTrue))
        .get_matches();

    if matches.get_flag("new") {
        return setup_new_certificate();
    }

    let cert_contents = fs::read("clientCert.crt")?;

    let cert = X509Certificate::from_pem(&cert_contents).expect("Failed to parse clientCert.crt");

    let connector_id = cert.subject_common_name().ok_or_eyre("Certificate has no CN")?;
    let connector_id = Uuid::parse_str(&connector_id)?;
    let client = Arc::new(Mutex::new(MqttClientWrapper::new()?));

    let sensor_state_rc = SensorVisionClient::new(connector_id, client)?;
    {
        let mut sensor_state = sensor_state_rc.lock().unwrap();

        if matches.get_flag("ping") {
            return sensor_state.ping_test();
        }

        sensor_state.load_sensors()?;
        sleep(Duration::from_secs(1));
    }

    if matches.get_flag("test") {
        let mut sensor_state = sensor_state_rc.lock().unwrap();
        // sensor_state.create_sensor("Sensor07122024")?;

        let sensor_id = sensor_state.sensor_id_by_name("Sensor07122024").unwrap();

        let metrics = vec![
            Metric::predefined(
                String::from("CPUUsage"),
                ValueUnit::Percent,
            ),
            Metric::predefined(
                String::from("CPUIdle"),
                ValueUnit::Percent,
            ),
            Metric::custom(
                String::from("ShopFloor1"),
                ValueType::Integer,
                String::from("goods per minute"),
            ),
        ];
        sensor_state.create_metrics(&sensor_id, &metrics)?;

        //
        // let sensor_ref =
        //     {
        //         let mut sensor_state = sensor_state_rc.borrow();
        //         sensor_state.sensors[&sensor_state.sensors.keys().next().unwrap()].clone()
        //     };
        // sensor_ref.borrow_mut().create_metrics(metrics)?;
    }

    sleep(Duration::from_secs(1));

    println!("{}", sensor_state_rc.lock().unwrap().dump_sensors()?);

    Ok(())
}
