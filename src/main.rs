#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use auto_launch::{AutoLaunch, AutoLaunchBuilder};
#[cfg(target_os = "macos")]
use auto_launch::MacOSLaunchMode;
#[cfg(target_os = "windows")]
use auto_launch::WindowsEnableMode;
use aw_switcher::{config::Config, icon, monitor::Monitor};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use tao::dpi::LogicalSize;
use tao::event::{ElementState, Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::keyboard::{KeyCode, ModifiersState};
#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tao::window::{Window, WindowBuilder};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

enum UserEvent {
    Tray,
    Menu(MenuEvent),
    HotKey(GlobalHotKeyEvent),
}

/// Builds the tray dropdown: a status line, one entry per configured input
/// (checked if it's the active one), then reload/quit actions.
fn build_menu(config: &Config, current: Option<u16>, monitor_error: Option<&str>) -> Menu {
    let menu = Menu::new();

    let status_text = match (monitor_error, current) {
        (Some(err), _) => format!("⚠ {err}"),
        (None, Some(code)) => {
            let name = config
                .inputs
                .iter()
                .find(|i| i.code == code)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| format!("Unknown (0x{code:02x})"));
            format!("Current: {name}")
        }
        (None, None) => "Current: unknown".to_string(),
    };
    let _ = menu.append(&MenuItem::with_id("status", status_text, false, None));
    let _ = menu.append(&PredefinedMenuItem::separator());

    for (index, input) in config.inputs.iter().enumerate() {
        let checked = current == Some(input.code);
        let item = CheckMenuItem::with_id(format!("input:{index}"), &input.name, true, checked, None);
        let _ = menu.append(&item);
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let cycle_menu = Submenu::new("Cycle Inputs", true);
    for (index, input) in config.inputs.iter().enumerate() {
        let item = CheckMenuItem::with_id(format!("toggle:{index}"), &input.name, true, input.enabled, None);
        let _ = cycle_menu.append(&item);
    }
    let _ = menu.append(&cycle_menu);

    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        "hotkey_status",
        format!("Hotkey: {}", config.hotkey),
        false,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id("set_hotkey", "Set Hotkey…", true, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        "rescan",
        "Reload Config && Rescan Monitor",
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id("open_config", "Open Config File", true, None));
    let _ = menu.append(&CheckMenuItem::with_id(
        "open_at_login",
        "Open at Login",
        true,
        config.open_at_login,
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id("quit", "Quit", true, None));

    menu
}

/// Re-reads the monitor's current input and rebuilds/replaces the tray menu.
fn refresh(tray: &TrayIcon, config: &Config, monitor: &mut Option<Monitor>, monitor_error: &mut Option<String>) {
    let current = monitor.as_mut().and_then(|m| match m.current_input() {
        Ok(code) => Some(code),
        Err(err) => {
            *monitor_error = Some(err.to_string());
            None
        }
    });
    let menu = build_menu(config, current, monitor_error.as_deref());
    tray.set_menu(Some(Box::new(menu)));

    let tooltip = match monitor {
        Some(m) => format!("AW Switcher — {}", m.name()),
        None => "AW Switcher — no monitor found".to_string(),
    };
    let _ = tray.set_tooltip(Some(tooltip));
}

fn parse_input_id(id: &str) -> Option<usize> {
    id.strip_prefix("input:")?.parse().ok()
}

fn parse_toggle_id(id: &str) -> Option<usize> {
    id.strip_prefix("toggle:")?.parse().ok()
}

/// Builds the auto-launch handle for the current platform: a LaunchAgent
/// plist on macOS, a per-user registry Run entry on Windows. Both point
/// directly at whatever executable is currently running, so this also
/// works (harmlessly) from a `cargo run` dev build.
fn build_auto_launch() -> Option<AutoLaunch> {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("Failed to determine executable path for auto-launch: {err:#}");
            return None;
        }
    };
    let exe = exe.to_string_lossy().into_owned();

    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name("AW Switcher");
    builder.set_app_path(&exe);
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(MacOSLaunchMode::LaunchAgent);
    #[cfg(target_os = "windows")]
    builder.set_windows_enable_mode(WindowsEnableMode::CurrentUser);

    match builder.build() {
        Ok(auto) => Some(auto),
        Err(err) => {
            eprintln!("Failed to configure auto-launch: {err:#}");
            None
        }
    }
}

/// Makes the OS-level auto-launch registration match `enabled`. Called on
/// every startup (so it self-heals if the LaunchAgent/registry entry was
/// removed out-of-band) and whenever the tray checkbox is toggled.
fn sync_auto_launch(auto: &AutoLaunch, enabled: bool) {
    let result = if enabled { auto.enable() } else { auto.disable() };
    if let Err(err) = result {
        let verb = if enabled { "enable" } else { "disable" };
        eprintln!("Failed to {verb} open-at-login: {err:#}");
    }
}

fn register_hotkey(manager: &GlobalHotKeyManager, previous: &mut Option<HotKey>, hotkey_str: &str) {
    if let Some(prev) = previous.take() {
        let _ = manager.unregister(prev);
    }
    match hotkey_str.parse::<HotKey>() {
        Ok(hotkey) => match manager.register(hotkey) {
            Ok(()) => *previous = Some(hotkey),
            Err(err) => eprintln!("Failed to register hotkey \"{hotkey_str}\": {err}"),
        },
        Err(err) => eprintln!("Invalid hotkey \"{hotkey_str}\" in config: {err}"),
    }
}

fn is_modifier_keycode(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
            | KeyCode::CapsLock
            | KeyCode::NumLock
            | KeyCode::ScrollLock
            | KeyCode::Fn
            | KeyCode::FnLock
    )
}

/// tao's `KeyCode` and global-hotkey's `Code` are separate enums that both mirror the
/// UI Events `KeyboardEvent.code` spec with identical variant names, so a plain Debug
/// round-trip (e.g. "KeyI", "Digit1") maps one to the other without a manual table.
fn physical_key_to_hotkey_code(key: KeyCode) -> Option<Code> {
    format!("{key:?}").parse().ok()
}

fn modifiers_from_tao(state: ModifiersState) -> Modifiers {
    let mut mods = Modifiers::empty();
    if state.shift_key() {
        mods |= Modifiers::SHIFT;
    }
    if state.control_key() {
        mods |= Modifiers::CONTROL;
    }
    if state.alt_key() {
        mods |= Modifiers::ALT;
    }
    if state.super_key() {
        mods |= Modifiers::SUPER;
    }
    mods
}

fn open_hotkey_capture_window<T: 'static>(target: &tao::event_loop::EventLoopWindowTarget<T>) -> Option<Window> {
    match WindowBuilder::new()
        .with_title("Press the new hotkey… (Esc to cancel)")
        .with_inner_size(LogicalSize::new(420.0, 90.0))
        .with_resizable(false)
        .with_always_on_top(true)
        .build(target)
    {
        Ok(window) => {
            window.set_focus();
            Some(window)
        }
        Err(err) => {
            eprintln!("Failed to open hotkey capture window: {err:#}");
            None
        }
    }
}

fn main() {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // A menu-bar-only app: no Dock icon, no Cmd+Tab entry. Info.plist's
    // LSUIElement has no effect on tao's own activation policy, which
    // defaults to Regular regardless — this call is what actually matters.
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let proxy = event_loop.create_proxy();
    let tray_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |_event| {
        let _ = tray_proxy.send_event(UserEvent::Tray);
    }));
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));
    let hotkey_proxy = proxy.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event| {
        let _ = hotkey_proxy.send_event(UserEvent::HotKey(event));
    }));

    let hotkey_manager = GlobalHotKeyManager::new().expect("failed to initialize global hotkey manager");
    let mut registered_hotkey: Option<HotKey> = None;

    let mut config = match Config::load_or_create() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Failed to load config: {err:#}");
            Config::default()
        }
    };
    register_hotkey(&hotkey_manager, &mut registered_hotkey, &config.hotkey);

    let auto_launch = build_auto_launch();
    if let Some(auto) = &auto_launch {
        sync_auto_launch(auto, config.open_at_login);
    }

    let mut monitor = match Monitor::find(&config.monitor_match) {
        Ok(m) => Some(m),
        Err(err) => {
            eprintln!("{err:#}");
            None
        }
    };
    let mut monitor_error = monitor.is_none().then(|| "No monitor found".to_string());

    let mut tray: Option<TrayIcon> = None;
    let mut hotkey_capture: Option<Window> = None;
    let mut capture_mods = Modifiers::empty();

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::NewEvents(tao::event::StartCause::Init) = event {
            let built = TrayIconBuilder::new()
                .with_icon(icon::tray_icon())
                .with_icon_as_template(true)
                .with_tooltip("AW Switcher")
                .with_menu(Box::new(build_menu(&config, None, monitor_error.as_deref())))
                .build()
                .expect("failed to create tray icon");
            refresh(&built, &config, &mut monitor, &mut monitor_error);
            tray = Some(built);
            return;
        }

        if let Event::WindowEvent { window_id, event: win_event, .. } = &event {
            let is_capture_window = hotkey_capture.as_ref().is_some_and(|w| w.id() == *window_id);
            if is_capture_window {
                match win_event {
                    WindowEvent::ModifiersChanged(state) => {
                        capture_mods = modifiers_from_tao(*state);
                    }
                    WindowEvent::KeyboardInput { event: key_event, is_synthetic: false, .. }
                        if key_event.state == ElementState::Pressed =>
                    {
                        if key_event.physical_key == KeyCode::Escape {
                            register_hotkey(&hotkey_manager, &mut registered_hotkey, &config.hotkey);
                            hotkey_capture = None;
                        } else if !is_modifier_keycode(key_event.physical_key) {
                            if let Some(code) = physical_key_to_hotkey_code(key_event.physical_key) {
                                if capture_mods.is_empty() {
                                    eprintln!("Hotkey needs at least one modifier key (Ctrl/Alt/Shift/Cmd)");
                                } else {
                                    let new_hotkey = HotKey::new(Some(capture_mods), code);
                                    config.hotkey = new_hotkey.to_string();
                                    if let Err(err) = config.save() {
                                        eprintln!("Failed to save config: {err:#}");
                                    }
                                    register_hotkey(&hotkey_manager, &mut registered_hotkey, &config.hotkey);
                                    if let Some(t) = tray.as_ref() {
                                        refresh(t, &config, &mut monitor, &mut monitor_error);
                                    }
                                    hotkey_capture = None;
                                }
                            }
                        }
                    }
                    WindowEvent::CloseRequested => {
                        register_hotkey(&hotkey_manager, &mut registered_hotkey, &config.hotkey);
                        hotkey_capture = None;
                    }
                    _ => {}
                }
            }
            return;
        }

        let Event::UserEvent(user_event) = event else { return };
        let Some(tray) = tray.as_ref() else { return };

        match user_event {
            UserEvent::HotKey(event) => {
                if event.state != global_hotkey::HotKeyState::Pressed {
                    return;
                }
                if let Some(m) = monitor.as_mut() {
                    let cycle: Vec<_> = config.inputs.iter().filter(|i| i.enabled).collect();
                    if let Some(next) = if cycle.is_empty() {
                        None
                    } else {
                        let current = m.current_input().ok();
                        let pos = current.and_then(|c| cycle.iter().position(|i| i.code == c));
                        Some(match pos {
                            Some(pos) => cycle[(pos + 1) % cycle.len()],
                            None => cycle[0],
                        })
                    } {
                        if let Err(err) = m.set_input(next.code) {
                            eprintln!("{err:#}");
                            monitor_error = Some(err.to_string());
                        } else {
                            monitor_error = None;
                        }
                    }
                }
                refresh(tray, &config, &mut monitor, &mut monitor_error);
            }
            UserEvent::Menu(event) => {
                let id = event.id().0.as_str();
                match id {
                    "quit" => *control_flow = ControlFlow::Exit,
                    "rescan" => {
                        config = match Config::load_or_create() {
                            Ok(c) => c,
                            Err(err) => {
                                eprintln!("Failed to load config: {err:#}");
                                config.clone()
                            }
                        };
                        register_hotkey(&hotkey_manager, &mut registered_hotkey, &config.hotkey);
                        if let Some(auto) = &auto_launch {
                            sync_auto_launch(auto, config.open_at_login);
                        }
                        monitor = match Monitor::find(&config.monitor_match) {
                            Ok(m) => {
                                monitor_error = None;
                                Some(m)
                            }
                            Err(err) => {
                                monitor_error = Some(err.to_string());
                                None
                            }
                        };
                        refresh(tray, &config, &mut monitor, &mut monitor_error);
                    }
                    "open_config" => {
                        if let Ok(path) = Config::path() {
                            let _ = open_in_default_app(&path);
                        }
                    }
                    "open_at_login" => {
                        config.open_at_login = !config.open_at_login;
                        if let Err(err) = config.save() {
                            eprintln!("Failed to save config: {err:#}");
                        }
                        if let Some(auto) = &auto_launch {
                            sync_auto_launch(auto, config.open_at_login);
                        }
                        refresh(tray, &config, &mut monitor, &mut monitor_error);
                    }
                    "set_hotkey" => {
                        if hotkey_capture.is_none() {
                            // The old hotkey is still globally registered at this point, so
                            // pressing it while the capture window is focused would otherwise
                            // deliver a WM_HOTKEY to the OS instead of a normal key event here.
                            if let Some(prev) = registered_hotkey.take() {
                                let _ = hotkey_manager.unregister(prev);
                            }
                            capture_mods = Modifiers::empty();
                            hotkey_capture = open_hotkey_capture_window(target);
                        }
                    }
                    other => {
                        if let Some(index) = parse_input_id(other) {
                            if let (Some(input), Some(m)) = (config.inputs.get(index), monitor.as_mut()) {
                                if let Err(err) = m.set_input(input.code) {
                                    eprintln!("{err:#}");
                                    monitor_error = Some(err.to_string());
                                } else {
                                    monitor_error = None;
                                }
                                refresh(tray, &config, &mut monitor, &mut monitor_error);
                            }
                        } else if let Some(index) = parse_toggle_id(other) {
                            if let Some(input) = config.inputs.get_mut(index) {
                                input.enabled = !input.enabled;
                                if let Err(err) = config.save() {
                                    eprintln!("Failed to save config: {err:#}");
                                }
                                refresh(tray, &config, &mut monitor, &mut monitor_error);
                            }
                        }
                    }
                }
            }
            UserEvent::Tray => {}
        }
    });
}

fn open_in_default_app(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}
