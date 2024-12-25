use eyre::{OptionExt, Result};

use std::{io, fs};

use ratatui::{backend::CrosstermBackend, Terminal};

use x509_certificate::X509Certificate;

use sensor_vision::app::{
    app::AppClient,
    events::{Event, EventHandler},
    tui::Tui,
};
use sensor_vision::client::client::SensorVisionClient;

use sensor_vision::model::ConnectorId;

// #[tokio::main]
#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<()> {
    console_subscriber::init();

    let cert_contents = fs::read("clientCert.crt")?;

    let cert = X509Certificate::from_pem(&cert_contents).expect("Failed to parse clientCert.crt");

    let connector_id = cert.subject_common_name().ok_or_eyre("Certificate has no CN")?;
    let connector_id: ConnectorId = connector_id.into();

    let sv_client = SensorVisionClient::new(connector_id)?;

    let events = EventHandler::new();
    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend)?;

    let mut tui = Tui::new(terminal);
    tui.init()?;

    let mut app = AppClient::new(sv_client)?;

    app.run(&mut tui, events).await?;
    tui.exit()?;

    Ok(())
}
