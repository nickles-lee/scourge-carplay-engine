// SPDX-FileCopyrightText: 2026 The SCOURGE Project contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Additional terms: see the repository NOTICE and the LIVI attribution.

//! Offline Gen5W protocol checks. This is not the finished projection daemon:
//! it decodes real CSM route vectors and exercises the local bridge contract.
//! It does not open USB, AirPlay, audio or video and cannot move the vehicle.

use std::process::ExitCode;

use livi_core_proto::helper_events::HelperKind;
use scourge_gen5w::{bridge, route::RouteUpdate};

fn hex(text: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if bytes.len() % 2 != 0 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    bytes
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        })
        .collect()
}

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  scourge-gen5w --selftest SOCKET\n  scourge-gen5w --decode-route HEX [SOCKET]\n\n\
         --decode-route prints the normalized update; with SOCKET it also posts\n\
         the update to the local bridge. No vehicle output is enabled."
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, socket] if flag == "--selftest" => match bridge::status(socket) {
            Ok(value) => {
                println!("{value}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("selftest failed: {e}");
                ExitCode::FAILURE
            }
        },
        [flag, frame] if flag == "--decode-route" => decode(frame, None),
        [flag, frame, socket] if flag == "--decode-route" => decode(frame, Some(socket)),
        _ => usage(),
    }
}

fn decode(frame: &str, socket: Option<&String>) -> ExitCode {
    let Some(bytes) = hex(frame) else {
        eprintln!("frame is not even-length hex");
        return ExitCode::FAILURE;
    };
    let mut tag = livi_helper_stack::events::EventTag::default();
    let Some(HelperKind::Navigation(patch)) = tag.navigation_event(&bytes) else {
        eprintln!("frame is not a route guidance message");
        return ExitCode::FAILURE;
    };
    let Some(update) = RouteUpdate::from_patch(&patch) else {
        eprintln!("route guidance carried no forwardable change");
        return ExitCode::FAILURE;
    };
    let text = match serde_json::to_string(&update) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("encode failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("{text}");
    if let Some(socket) = socket {
        match bridge::route(socket, &update) {
            Ok(value) => println!("{value}"),
            Err(e) => {
                eprintln!("bridge rejected the update: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn hex_rejects_non_ascii_and_odd_input_without_panicking() {
        assert_eq!(hex("0a 0B"), Some(vec![0x0a, 0x0b]));
        assert_eq!(hex("aéa"), None);
        assert_eq!(hex("0a0"), None);
        assert_eq!(hex("0g"), None);
    }
}
