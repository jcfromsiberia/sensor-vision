# Sensor Vision

---

TUI Client for [TeamViewer IoT MQTT API](https://docs-iot.teamviewer.com/mqtt-api/)
for managing IoT Sensors and Metrics on target device.

Requires [TeamViewer IoT Agent](https://www.teamviewer.com/en/global/support/knowledge-base/teamviewer-tensor-classic/teamviewer-embedded/installation/) installed 
and IoT Monitoring setting enabled.

Currently, the only way to get binaries is building the app on the host machine.

> [!NOTE]  
> That's a pet project with the only goal: to gain experience with the modern Rust.
> 
> The app design is too far from perfection, I just wanted to feel the async
> programming spirit.

## Dependencies

---
* [rustc+cargo](https://github.com/rust-lang/cargo/) >= 1.85.0 (for Rust 2024 edition)
* librust-openssl-dev >= 0.10

## Building

---

Just building
```shell
cargo build
```
you find binaries in `./target`

## Installation

For current user
```shell
cargo install --git https://github.com/jcfromsiberia/sensor-vision.git --force
```

System-wide
```shell
sudo cargo install --git https://github.com/jcfromsiberia/sensor-vision.git --force --root=/usr/local
```

## Usage

The app can manage single MQTT client, and it requires exising `clientCert.crt` (along with `privkey.pem`) in
the working directory to start. The instructions how to get `clientCert.crt` can
be found [here](https://www.teamviewer.com/en/global/support/knowledge-base/teamviewer-tensor-classic/teamviewer-embedded/sensors/example-connect-a-sensor-to-the-teamviewer-embedded-agent/).

The app exposes the way to cut this corner by requesting and storing the certificate for you.
However, it requires certificate request file `csr.pem` to be located in the current directory.
Generate a new CSR or use the existing one to get the new certificate:
```shell
openssl req -nodes -new -newkey rsa:2048 -sha256 -out csr.pem
sensor-vision --new
```

If you already have the certificate, start the app in the respective directory with no args.
```shell
sensor-vision
```

## Screenshots

![Screen1](/images/Screenshot1.png)
![Screen2](/images/Screenshot2.png)
