// SPDX-FileCopyrightText: 2026 The SCOURGE Project contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Additional terms: see the repository NOTICE and the LIVI attribution.

//! Bounded client for SCOURGE's local bridge protocol v1.
//!
//! One request and one response per connection, both newline-terminated JSON.
//! The bridge socket is private to the service account; this client does not
//! open a network listener, retry indefinitely or send vehicle commands.

use std::io::{self, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{Value, json};

pub const MAX_RESPONSE: usize = 16 * 1024;
const DEADLINE: Duration = Duration::from_millis(750);

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Protocol(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "bridge I/O: {e}"),
            Self::Protocol(e) => write!(f, "bridge protocol: {e}"),
        }
    }
}

impl std::error::Error for Error {}

fn call(socket: &str, request: &Value) -> Result<Value, Error> {
    let mut stream = UnixStream::connect(socket).map_err(Error::Io)?;
    stream.set_read_timeout(Some(DEADLINE)).map_err(Error::Io)?;
    stream
        .set_write_timeout(Some(DEADLINE))
        .map_err(Error::Io)?;
    let body = serde_json::to_vec(request).map_err(|e| Error::Protocol(e.to_string()))?;
    stream.write_all(&body).map_err(Error::Io)?;
    stream.write_all(b"\n").map_err(Error::Io)?;
    let mut reader = BufReader::new(stream);
    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    while response.len() < MAX_RESPONSE {
        if reader.read(&mut byte).map_err(Error::Io)? == 0 || byte[0] == b'\n' {
            break;
        }
        response.push(byte[0]);
    }
    if response.len() == MAX_RESPONSE {
        return Err(Error::Protocol("response too large".into()));
    }
    let value: Value =
        serde_json::from_slice(&response).map_err(|e| Error::Protocol(e.to_string()))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(Error::Protocol(
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
        ));
    }
    Ok(value)
}

pub fn status(socket: &str) -> Result<Value, Error> {
    call(socket, &json!({"version": 1, "op": "status"}))
}

pub fn telemetry(socket: &str) -> Result<Value, Error> {
    call(socket, &json!({"version": 1, "op": "telemetry"}))
}

/// Publish one normalized route update. The bridge validates and stores it;
/// `forwarded` stays false until the vehicle-output feature is enabled and
/// car-validated.
pub fn route(socket: &str, route: &crate::route::RouteUpdate) -> Result<Value, Error> {
    let mut request = json!({"version": 1, "op": "route"});
    request["route"] = serde_json::to_value(route).map_err(|e| Error::Protocol(e.to_string()))?;
    call(socket, &request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    #[test]
    fn one_request_one_answer_and_no_forwarding() {
        let dir = std::env::temp_dir().join(format!("scourge-gen5w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("bridge.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let socket = path.to_str().unwrap().to_string();
        let server = std::thread::spawn(move || {
            for reply in [
                r#"{"version":1,"ok":true,"vehicle_writes":false}"#,
                r#"{"version":1,"ok":true,"forwarded":false}"#,
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["version"], 1);
                assert!(stream.write_all(reply.as_bytes()).is_ok());
                stream.write_all(b"\n").unwrap();
            }
        });
        assert_eq!(status(&socket).unwrap()["vehicle_writes"], false);
        let update = crate::route::RouteUpdate {
            state: crate::route::RouteState::Active,
            maneuver: None,
            next_m: Some(12),
            remaining_m: None,
            road: None,
            next_road: None,
            destination: None,
            eta_epoch: None,
            time_remaining_s: None,
        };
        assert_eq!(route(&socket, &update).unwrap()["forwarded"], false);
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_error_answer_is_an_error() {
        let dir = std::env::temp_dir().join(format!("scourge-gen5w-err-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("bridge.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let socket = path.to_str().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = [0u8; 512];
            use std::io::Read;
            let n = stream.read(&mut line).unwrap();
            assert!(n > 0);
            stream
                .write_all(b"{\"version\":1,\"ok\":false,\"error\":\"invalid_payload\"}\n")
                .unwrap();
        });
        let error = status(&socket).unwrap_err();
        assert!(error.to_string().contains("invalid_payload"));
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
