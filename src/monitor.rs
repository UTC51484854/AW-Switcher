use std::{thread, time::Duration};

use anyhow::{bail, Context, Result};
use ddc_hi::{Ddc, Display};

/// VCP feature code for "Input Source" (MCCS 0x60).
const INPUT_SOURCE: u8 = 0x60;

/// DDC/CI over a real display link is flaky: a single command dropping a
/// byte or getting a truncated reply is normal and expected, not exceptional.
const RETRY_ATTEMPTS: u32 = 5;
const RETRY_BASE_DELAY_MS: u64 = 60;

/// After a SET command, give the monitor's control board time to act on it
/// before issuing another DDC/CI command; firing one immediately after
/// another is a common cause of the next command failing or being ignored.
const POST_SET_SETTLE_MS: u64 = 250;

fn with_retries<T>(mut f: impl FnMut() -> Result<T>) -> Result<T> {
    let mut last_err = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(RETRY_BASE_DELAY_MS * (attempt as u64 + 1)));
            }
        }
    }
    Err(last_err.unwrap())
}

pub struct Monitor {
    display: Display,
}

impl Monitor {
    /// Finds the target monitor among all DDC/CI-capable displays.
    ///
    /// If exactly one display is found, it's used regardless of its name.
    /// If more than one is found, `model_match` (case-insensitive substring)
    /// is used to disambiguate.
    pub fn find(model_match: &str) -> Result<Self> {
        let mut displays = Display::enumerate();
        if displays.is_empty() {
            bail!(
                "No DDC/CI-capable displays found. Make sure DDC/CI is enabled in the \
                 monitor's on-screen menu, and that the cable carries DDC (most DisplayPort \
                 and HDMI cables do)."
            );
        }

        let index = if displays.len() == 1 {
            0
        } else {
            let needle = model_match.to_lowercase();
            displays
                .iter()
                .position(|d| {
                    d.info
                        .model_name
                        .as_deref()
                        .map(|m| m.to_lowercase().contains(&needle))
                        .unwrap_or(false)
                })
                .with_context(|| {
                    let found: Vec<String> = displays
                        .iter()
                        .map(|d| d.info.model_name.clone().unwrap_or_else(|| d.info.id.clone()))
                        .collect();
                    format!(
                        "Multiple displays found but none matched \"{model_match}\": {}. \
                         Set `monitor_match` in the config to a substring of the target \
                         monitor's model name.",
                        found.join(", ")
                    )
                })?
        };

        Ok(Self { display: displays.remove(index) })
    }

    pub fn name(&self) -> String {
        self.display
            .info
            .model_name
            .clone()
            .unwrap_or_else(|| self.display.info.id.clone())
    }

    /// Reads the current input source (VCP 0x60) value.
    ///
    /// MCCS defines Input Source as a one-byte value carried in the low byte
    /// (`sl`) of the reply; some monitors put garbage (often a copy of `sl`)
    /// in the high byte (`sh`), so it must be ignored rather than combined in.
    pub fn current_input(&mut self) -> Result<u16> {
        let display = &mut self.display;
        with_retries(|| {
            display
                .handle
                .get_vcp_feature(INPUT_SOURCE)
                .map(|v| v.sl as u16)
                .context("Failed to read the current input source over DDC/CI")
        })
    }

    /// Sets the input source (VCP 0x60) to the given value.
    pub fn set_input(&mut self, code: u16) -> Result<()> {
        let display = &mut self.display;
        with_retries(|| {
            display
                .handle
                .set_vcp_feature(INPUT_SOURCE, code)
                .context("Failed to set the input source over DDC/CI")
        })?;
        // Give the monitor time to act on the switch before any follow-up
        // DDC/CI command (e.g. the status read right after this call).
        thread::sleep(Duration::from_millis(POST_SET_SETTLE_MS));
        Ok(())
    }
}
