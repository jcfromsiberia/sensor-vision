use actix::Actor;

use clap::{arg, command, ArgAction};

use eyre::{OptionExt, Result};

use ratatui::{backend::CrosstermBackend, Terminal};

use sensor_vision::client::client::*;
use sensor_vision::client::mqtt::setup_new_certificate;

use sensor_vision::model::ConnectorId;

use sensor_vision::tui_app::app::{AppClient, RunLoop};
use sensor_vision::tui_app::tui::Tui;

use std::fs;
use std::io;

use tokio::sync::oneshot;
use x509_certificate::X509Certificate;

#[actix::main]
async fn main() -> Result<()> {
    let matches = command!()
        .arg(arg!(-n --new "Quick setup a new connector").action(ArgAction::SetTrue))
        .get_matches();
    if matches.get_flag("new") {
        setup_new_certificate().await?;
    }

    let cert_contents = fs::read("clientCert.crt")?;

    let cert = X509Certificate::from_pem(&cert_contents).expect("Failed to parse clientCert.crt");

    let connector_id = cert.subject_common_name().ok_or_eyre("Certificate has no CN")?;
    let connector_id: ConnectorId = connector_id.into();

    let client_actor = SensorVisionClient::new(connector_id).await?.start();

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend)?;

    let mut tui = Tui::new(terminal);
    tui.init()?;

    let app_actor = AppClient::new(client_actor).start();

    let (finished_sender, rx) = oneshot::channel();

    app_actor.send(RunLoop{finished_sender, tui}).await?;

    Ok(rx.await??)
}
