use aw_switcher::{config::Config, icon, monitor::Monitor};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
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
    let _ = menu.append(&MenuItem::with_id(
        "rescan",
        "Reload Config && Rescan Monitor",
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id("open_config", "Open Config File", true, None));
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

fn main() {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

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

    let mut monitor = match Monitor::find(&config.monitor_match) {
        Ok(m) => Some(m),
        Err(err) => {
            eprintln!("{err:#}");
            None
        }
    };
    let mut monitor_error = monitor.is_none().then(|| "No monitor found".to_string());

    let mut tray: Option<TrayIcon> = None;

    event_loop.run(move |event, _target, control_flow| {
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

        let Event::UserEvent(user_event) = event else { return };
        let Some(tray) = tray.as_ref() else { return };

        match user_event {
            UserEvent::HotKey(event) => {
                if event.state != global_hotkey::HotKeyState::Pressed {
                    return;
                }
                if let Some(m) = monitor.as_mut() {
                    let current = m.current_input().ok();
                    let next = match current.and_then(|c| config.inputs.iter().position(|i| i.code == c)) {
                        Some(pos) => config.inputs.get((pos + 1) % config.inputs.len()),
                        None => config.inputs.first(),
                    };
                    if let Some(next) = next {
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
