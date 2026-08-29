//! Direct macOS DDC/CI for VCP feature 0x60 (input source), bypassing
//! `ddc-hi`'s built-in macOS backend (the `ddc-macos` crate).
//!
//! On this project's test hardware (Apple Silicon, macOS 27, a Dell
//! Alienware AW3926QW), `ddc-macos` finds the display fine but its DDC/CI
//! reads and writes are silently wrong: it asks `IOAVServiceReadI2C` for 11
//! bytes and passes a hardcoded I2C offset of `0x6E` for both read and
//! write. [`m1ddc`](https://github.com/waydabber/m1ddc) (MIT, same author
//! as BetterDisplay) instead requests 12 bytes and uses offset `0x51` for
//! both operations, and reads/writes reliably on the same machine — this
//! module ports that exact recipe. The AV-service discovery walk below is
//! unchanged from `ddc-macos`'s `arm.rs` (that part already worked); only
//! the I2C parameters differ.
use std::{ffi::c_void, os::raw::c_char, thread, time::Duration};

use anyhow::{bail, Context, Result};
use core_foundation::{
    base::{CFType, TCFType},
    dictionary::CFDictionary,
    string::CFString,
};
use core_foundation_sys::base::{kCFAllocatorDefault, CFAllocatorRef, CFTypeRef};
use core_graphics::display::CGDisplay;
use io_kit_sys::{
    kIOMasterPortDefault, kIORegistryIterateRecursively,
    keys::kIOServicePlane,
    types::{io_object_t, io_registry_entry_t},
    IOIteratorNext, IOObjectConformsTo, IOObjectRelease, IORegistryEntryCreateCFProperty,
    IORegistryEntryCreateIterator, IORegistryEntryGetName, IORegistryEntryGetParentEntry,
    IORegistryEntryGetPath, IORegistryGetRootEntry,
};
use mach2::kern_return::KERN_SUCCESS;

type IOAVServiceRef = CFTypeRef;

const CHIP_ADDRESS_DEFAULT: u32 = 0x37;
const CHIP_ADDRESS_MCDP29XX: u32 = 0xB7;
/// I2C offset m1ddc passes to both `IOAVServiceReadI2C` and
/// `IOAVServiceWriteI2C`; `ddc-macos` uses `0x00`/`0x6E` here instead, which
/// is the actual bug this module works around.
const INPUT_ADDR: u32 = 0x51;
/// VESA DDC/CI destination sub-address, embedded in the payload itself
/// (distinct from the `offset` parameter above).
const DDC_DEST_SUB_ADDRESS: u8 = 0x6e;
const WRITE_ATTEMPTS: u32 = 2;
const WRITE_WAIT: Duration = Duration::from_millis(10);
const READ_WAIT: Duration = Duration::from_millis(10);
const READ_BUF_LEN: usize = 12;

#[link(name = "CoreDisplay", kind = "framework")]
unsafe extern "C" {
    fn IOAVServiceCreateWithService(allocator: CFAllocatorRef, service: io_object_t) -> IOAVServiceRef;
    fn IOAVServiceReadI2C(
        service: IOAVServiceRef,
        chip_address: u32,
        offset: u32,
        output_buffer: *mut c_void,
        output_buffer_size: u32,
    ) -> i32;
    fn IOAVServiceWriteI2C(
        service: IOAVServiceRef,
        chip_address: u32,
        data_address: u32,
        input_buffer: *const c_void,
        input_buffer_size: u32,
    ) -> i32;
    fn CoreDisplay_DisplayCreateInfoDictionary(display_id: u32) -> core_foundation_sys::dictionary::CFDictionaryRef;
}

struct Transport {
    service: IOAVServiceRef,
    chip_address: u32,
}

fn cf_string_property(entry: io_registry_entry_t, key: &str) -> Option<String> {
    unsafe {
        let key = CFString::new(key);
        let value = IORegistryEntryCreateCFProperty(entry, key.as_concrete_TypeRef(), kCFAllocatorDefault, 0);
        if value.is_null() {
            return None;
        }
        CFType::wrap_under_create_rule(value).downcast::<CFString>().map(|s| s.to_string())
    }
}

fn registry_entry_path(entry: io_registry_entry_t) -> Option<String> {
    let mut buf = [0 as c_char; 1024];
    unsafe {
        if IORegistryEntryGetPath(entry, kIOServicePlane, buf.as_mut_ptr()) != KERN_SUCCESS {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    }
}

fn registry_entry_name(entry: io_registry_entry_t) -> Option<String> {
    let mut buf = [0 as c_char; 128];
    unsafe {
        if IORegistryEntryGetName(entry, buf.as_mut_ptr()) != KERN_SUCCESS {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    }
}

fn is_mcdp29xx_proxy(entry: io_registry_entry_t) -> bool {
    unsafe {
        let mut parent: io_registry_entry_t = 0;
        if IORegistryEntryGetParentEntry(entry, kIOServicePlane, &mut parent) != KERN_SUCCESS {
            return false;
        }
        let is_it = cf_string_property(parent, "EPICProviderClass").as_deref() == Some("AppleDCPMCDP29XX");
        IOObjectRelease(parent);
        is_it
    }
}

fn display_location(display: CGDisplay) -> Result<String> {
    unsafe {
        let dict_ref = CoreDisplay_DisplayCreateInfoDictionary(display.id);
        if dict_ref.is_null() {
            bail!("CoreDisplay_DisplayCreateInfoDictionary returned null for this display");
        }
        let info: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(dict_ref);
        let key = CFString::from_static_string("IODisplayLocation");
        info.find(&key)
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .context("display has no IODisplayLocation property")
    }
}

/// Walks the IOKit service registry once, looking for the "DCPAVServiceProxy"
/// entry that follows the entry whose path matches the display's location,
/// and that itself has an "External" Location property.
fn find_transport(display: CGDisplay) -> Result<Transport> {
    if display.is_builtin() {
        bail!("built-in displays don't support DDC/CI");
    }
    let location = display_location(display)?;

    unsafe {
        let root = IORegistryGetRootEntry(kIOMasterPortDefault);
        let mut iter: io_object_t = 0;
        if IORegistryEntryCreateIterator(root, kIOServicePlane, kIORegistryIterateRecursively, &mut iter) != KERN_SUCCESS {
            bail!("failed to create an IOKit registry iterator");
        }

        let mut framebuffer_matches = false;
        loop {
            let entry = IOIteratorNext(iter);
            if entry == 0 {
                break;
            }

            if IOObjectConformsTo(entry, c"IOMobileFramebuffer".as_ptr() as *mut c_char) != 0 {
                framebuffer_matches = registry_entry_path(entry).as_deref() == Some(location.as_str());
                IOObjectRelease(entry);
                continue;
            }

            if !framebuffer_matches || registry_entry_name(entry).as_deref() != Some("DCPAVServiceProxy") {
                IOObjectRelease(entry);
                continue;
            }

            let av_service = IOAVServiceCreateWithService(kCFAllocatorDefault, entry);
            let is_external = cf_string_property(entry, "Location").as_deref() == Some("External");
            let chip_address = if is_mcdp29xx_proxy(entry) { CHIP_ADDRESS_MCDP29XX } else { CHIP_ADDRESS_DEFAULT };
            IOObjectRelease(entry);

            if av_service.is_null() || !is_external {
                continue;
            }

            IOObjectRelease(iter);
            return Ok(Transport { service: av_service, chip_address });
        }

        IOObjectRelease(iter);
    }

    bail!("could not find an IOAVService for this display")
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(DDC_DEST_SUB_ADDRESS, |acc, &b| acc ^ b)
}

fn write_ddc(transport: &Transport, payload: &[u8]) -> Result<()> {
    let mut last_err = 0;
    for _ in 0..WRITE_ATTEMPTS {
        thread::sleep(WRITE_WAIT);
        let ret = unsafe {
            IOAVServiceWriteI2C(
                transport.service,
                transport.chip_address,
                INPUT_ADDR,
                payload.as_ptr() as *const c_void,
                payload.len() as u32,
            )
        };
        if ret == 0 {
            return Ok(());
        }
        last_err = ret;
    }
    bail!("IOAVServiceWriteI2C failed (status {last_err})");
}

fn read_ddc(transport: &Transport) -> Result<[u8; READ_BUF_LEN]> {
    thread::sleep(READ_WAIT);
    let mut buf = [0u8; READ_BUF_LEN];
    let ret = unsafe {
        IOAVServiceReadI2C(
            transport.service,
            transport.chip_address,
            INPUT_ADDR,
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32,
        )
    };
    if ret != 0 {
        bail!("IOAVServiceReadI2C failed (status {ret})");
    }
    Ok(buf)
}

/// Reads the current value of a VCP feature. Byte offsets below (current
/// value at buffer index 9) match m1ddc's `convertI2CtoDDC`, which this
/// mirrors rather than re-deriving from the DDC/CI spec, since it's what's
/// empirically proven to work on this hardware.
pub fn get_vcp_feature(display: CGDisplay, code: u8) -> Result<u8> {
    let transport = find_transport(display)?;
    let request = [0x82, 0x01, code, 0];
    let mut request = request;
    request[3] = checksum(&request[..3]);
    write_ddc(&transport, &request)?;
    let reply = read_ddc(&transport)?;
    Ok(reply[9])
}

pub fn set_vcp_feature(display: CGDisplay, code: u8, value: u8) -> Result<()> {
    let transport = find_transport(display)?;
    let mut request = [0x84, 0x03, code, 0, value, 0];
    request[3] = 0;
    request[5] = checksum(&request[..5]);
    write_ddc(&transport, &request)
}
