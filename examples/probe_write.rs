//! Diagnostic: safely round-trips a DDC/CI write by setting the input to
//! whatever it's already on (a no-op from the monitor's perspective, so it
//! won't disrupt the display), then reads it back. Run with
//! `cargo run --example probe_write`.
use aw_switcher::monitor::Monitor;

fn main() {
    let mut m = match Monitor::find("AW3926") {
        Ok(m) => m,
        Err(e) => {
            println!("Monitor::find failed: {e:#}");
            return;
        }
    };

    let before = match m.current_input() {
        Ok(v) => v,
        Err(e) => {
            println!("current_input() failed: {e:#}");
            return;
        }
    };
    println!("current input before: 0x{before:02x}");

    println!("writing the same value back (self-set, should be a no-op)...");
    if let Err(e) = m.set_input(before) {
        println!("set_input() failed: {e:#}");
        return;
    }

    match m.current_input() {
        Ok(after) => println!("current input after: 0x{after:02x} ({})", if after == before { "matches" } else { "MISMATCH" }),
        Err(e) => println!("current_input() failed: {e:#}"),
    }
}
