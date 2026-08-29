//! Diagnostic: dumps every DDC/CI display's raw capabilities string so the
//! real supported VCP 0x60 (input source) values can be read off, instead
//! of guessed. Run with `cargo run --example probe`.
//!
//! Read-only: this does not send any SET command, so it won't switch your
//! monitor's input.
use std::{thread, time::Duration};

use ddc_hi::{Ddc, Display};

fn main() {
    let mut displays = Display::enumerate();
    if displays.is_empty() {
        println!("No DDC/CI-capable displays found.");
        return;
    }

    for display in &mut displays {
        println!("=== {:?} {} ===", display.info.backend, display.info.id);
        println!("model_name: {:?}", display.info.model_name);
        println!("manufacturer_id: {:?}", display.info.manufacturer_id);

        let mut last_err = None;
        let mut got = None;
        for attempt in 0..5 {
            match display.handle.get_vcp_feature(0x60) {
                Ok(v) => {
                    got = Some(v);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    thread::sleep(Duration::from_millis(60 * (attempt + 1)));
                }
            }
        }
        match got {
            Some(v) => println!(
                "current VCP 0x60 raw: ty={:#04x} mh={:#04x} ml={:#04x} sh={:#04x} sl={:#04x}",
                v.ty, v.mh, v.ml, v.sh, v.sl
            ),
            None => println!("get_vcp_feature(0x60) failed after retries: {:#}", last_err.unwrap()),
        }

        thread::sleep(Duration::from_millis(250));

        let mut last_err = None;
        let mut got = None;
        for attempt in 0..5 {
            match display.handle.capabilities_string() {
                Ok(bytes) => {
                    got = Some(bytes);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    thread::sleep(Duration::from_millis(60 * (attempt + 1)));
                }
            }
        }
        match got {
            Some(bytes) => println!("capabilities string:\n{}", String::from_utf8_lossy(&bytes)),
            None => println!("capabilities_string() failed after retries: {:#}", last_err.unwrap()),
        }
        println!();
    }
}
