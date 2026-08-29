//! Diagnostic: reads a connected monitor's current VCP 0x60 (input source)
//! value through this project's real code path, and dumps
//! `ddc-hi`'s raw capabilities string as a secondary reference. Run with
//! `cargo run --example probe`. Read-only: doesn't switch your input.
use aw_switcher::monitor::Monitor;
use ddc_hi::{Ddc, Display};

fn main() {
    match Monitor::find("AW3926") {
        Ok(mut m) => match m.current_input() {
            Ok(code) => println!("{}: current input = 0x{code:02x}", m.name()),
            Err(e) => println!("{}: current_input() failed: {e:#}", m.name()),
        },
        Err(e) => println!("Monitor::find failed: {e:#}"),
    }

    println!();
    println!("--- raw ddc-hi capabilities (secondary reference) ---");
    for display in &mut Display::enumerate() {
        println!("=== {:?} {} ===", display.info.backend, display.info.id);
        match display.handle.capabilities_string() {
            Ok(bytes) => println!("capabilities string:\n{}", String::from_utf8_lossy(&bytes)),
            Err(e) => println!("capabilities_string() failed: {e:#}"),
        }
    }
}
