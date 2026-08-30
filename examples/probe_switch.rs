//! Diagnostic: performs a REAL input switch (away from the current input,
//! then back) to verify set_input actually changes what the monitor is
//! displaying, not just what it reports over DDC. This WILL visibly
//! interrupt the display for a moment. Run with `cargo run --example
//! probe_switch`.
use std::{thread, time::Duration};

use aw_switcher::monitor::Monitor;

fn main() {
    let mut m = match Monitor::find("AW3926") {
        Ok(m) => m,
        Err(e) => {
            println!("Monitor::find failed: {e:#}");
            return;
        }
    };

    let original = match m.current_input() {
        Ok(v) => v,
        Err(e) => {
            println!("current_input() failed: {e:#}");
            return;
        }
    };
    println!("original input: 0x{original:02x}");

    // Pick a different input to switch to: HDMI-1 (0x11) unless we're
    // already on it, in which case HDMI-2 (0x12).
    let target = if original == 0x11 { 0x12 } else { 0x11 };
    println!("switching to 0x{target:02x} ...");
    if let Err(e) = m.set_input(target) {
        println!("set_input(0x{target:02x}) failed: {e:#}");
        return;
    }

    thread::sleep(Duration::from_millis(500));
    match m.current_input() {
        Ok(v) => println!("input after switch: 0x{v:02x} ({})", if v == target { "SWITCH CONFIRMED" } else { "did not change" }),
        Err(e) => println!("current_input() after switch failed: {e:#}"),
    }

    println!("restoring original input 0x{original:02x} ...");
    if let Err(e) = m.set_input(original) {
        println!("set_input(0x{original:02x}) (restore) failed: {e:#}");
        return;
    }

    thread::sleep(Duration::from_millis(500));
    match m.current_input() {
        Ok(v) => println!("input after restore: 0x{v:02x} ({})", if v == original { "RESTORED" } else { "did not restore!" }),
        Err(e) => println!("current_input() after restore failed: {e:#}"),
    }
}
