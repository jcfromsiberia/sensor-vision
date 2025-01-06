use actix::Actor;

use eyre::{OptionExt, Result};

use std::fs;

use sensor_vision::client::client::*;
use sensor_vision::client::client_queries::{DumpSensors, LoadSensors, PingTest};
use sensor_vision::model::ConnectorId;

use tokio::time::{sleep, Duration};

use x509_certificate::X509Certificate;

#[actix::main]
async fn main() -> Result<()> {
    let cert_contents = fs::read("clientCert.crt")?;

    let cert = X509Certificate::from_pem(&cert_contents).expect("Failed to parse clientCert.crt");

    let connector_id = cert.subject_common_name().ok_or_eyre("Certificate has no CN")?;
    let connector_id: ConnectorId = connector_id.into();

    let client_actor = SensorVisionClient::new(connector_id).await?.start();

    client_actor.send(LoadSensors).await??;
    client_actor.send(PingTest).await??;
    sleep(Duration::from_secs(1)).await;
    let dump = client_actor.send(DumpSensors).await??;
    println!("{}", dump);

    Ok(())
}
