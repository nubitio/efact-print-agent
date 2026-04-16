use std::{path::PathBuf, process::Command, thread};

use tokio::sync::oneshot;
use tracing::{error, warn};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
};

use crate::{config::AgentConfig, run_server, AppState};

#[derive(Debug, Clone)]
enum UserEvent {
    Menu(MenuEvent),
}

pub(crate) fn run(config: AgentConfig, state: AppState) {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let port = config.port;

    let server_thread = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async move {
            let shutdown = async move {
                let _ = shutdown_rx.await;
            };

            if let Err(err) = run_server(state, port, shutdown).await {
                error!("Failed to run printer agent server: {err}");
            }
        });
    });

    if let Err(err) = run_tray_app(config, shutdown_tx) {
        error!("Tray application failed: {err}");
    }

    let _ = server_thread.join();
}

fn run_tray_app(config: AgentConfig, shutdown_tx: oneshot::Sender<()>) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|err| err.to_string())?;

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let mut app = TrayApp::new(config, shutdown_tx, event_loop.create_proxy());
    event_loop.run_app(&mut app).map_err(|err| err.to_string())
}

struct TrayApp {
    config: AgentConfig,
    shutdown_tx: Option<oneshot::Sender<()>>,
    event_proxy: EventLoopProxy<UserEvent>,
    tray_icon: Option<tray_icon::TrayIcon>,
    status_item: Option<MenuItem>,
    open_config_item: Option<MenuItem>,
    open_logs_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
}

impl TrayApp {
    fn new(
        config: AgentConfig,
        shutdown_tx: oneshot::Sender<()>,
        event_proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            config,
            shutdown_tx: Some(shutdown_tx),
            event_proxy,
            tray_icon: None,
            status_item: None,
            open_config_item: None,
            open_logs_item: None,
            quit_item: None,
        }
    }

    fn build_tray(&mut self) -> Result<(), String> {
        let menu = Menu::new();
        let status_text = configured_printer_label(&self.config);
        let status_item = MenuItem::new(status_text, false, None);
        let open_config_item = MenuItem::new("Open config folder", true, None);
        let open_logs_item = MenuItem::new("Open logs folder", true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        menu.append(&status_item).map_err(|err| err.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|err| err.to_string())?;
        menu.append(&open_config_item)
            .map_err(|err| err.to_string())?;
        menu.append(&open_logs_item)
            .map_err(|err| err.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|err| err.to_string())?;
        menu.append(&quit_item).map_err(|err| err.to_string())?;

        let tooltip = configured_printer_label(&self.config);
        let tray_icon = TrayIconBuilder::new()
            .with_tooltip(format!("eFact Printer Agent\n{tooltip}"))
            .with_menu(Box::new(menu))
            .with_icon(build_icon()?)
            .build()
            .map_err(|err| err.to_string())?;

        self.status_item = Some(status_item);
        self.open_config_item = Some(open_config_item);
        self.open_logs_item = Some(open_logs_item);
        self.quit_item = Some(quit_item);
        self.tray_icon = Some(tray_icon);

        Ok(())
    }

    fn handle_menu_event(&mut self, event: MenuEvent, event_loop: &ActiveEventLoop) {
        if self
            .open_config_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            if let Err(err) = open_folder(config_dir()) {
                warn!("Failed to open config folder: {err}");
            }
            return;
        }

        if self
            .open_logs_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            if let Err(err) = open_folder(log_dir()) {
                warn!("Failed to open logs folder: {err}");
            }
            return;
        }

        if self
            .quit_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<UserEvent> for TrayApp {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let _ = &self.event_proxy;
        if self.tray_icon.is_none() {
            if let Err(err) = self.build_tray() {
                error!("Failed to create tray icon: {err}");
                if let Some(shutdown_tx) = self.shutdown_tx.take() {
                    let _ = shutdown_tx.send(());
                }
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(event) => self.handle_menu_event(event, event_loop),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}

fn build_icon() -> Result<Icon, String> {
    let width = 16usize;
    let height = 16usize;
    let mut rgba = vec![0u8; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let is_border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
            let is_paper = (3..=12).contains(&x) && (2..=13).contains(&y);
            let is_header = is_paper && y <= 4;
            let is_line = is_paper && matches!(y, 7 | 9 | 11) && (5..=10).contains(&x);

            let (r, g, b, a) = if is_border {
                (26, 43, 60, 255)
            } else if is_header {
                (37, 99, 235, 255)
            } else if is_line {
                (148, 163, 184, 255)
            } else if is_paper {
                (255, 255, 255, 255)
            } else {
                (15, 23, 42, 255)
            };

            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    }

    Icon::from_rgba(rgba, width as u32, height as u32).map_err(|err| err.to_string())
}

fn configured_printer_label(config: &AgentConfig) -> String {
    if let Some(printer_name) = &config.system_printer_name {
        return format!("Printer: {printer_name}");
    }

    if config.prefer_system_backend {
        return "Printer: system default".to_string();
    }

    "Printer: auto (HID or system default)".to_string()
}

// ── Platform-specific paths ───────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("efact-printer-agent")
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(".config")
            .join("efact-printer-agent")
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(".config")
            .join("efact-printer-agent")
    }
}

pub(crate) fn log_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("efact-printer-agent")
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("Library")
            .join("Logs")
            .join("efact-printer-agent")
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(".local")
            .join("share")
            .join("efact-printer-agent")
    }
}

fn open_folder(path: PathBuf) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(&path)?;

    #[cfg(target_os = "windows")]
    Command::new("explorer").arg(path).spawn()?;

    #[cfg(target_os = "macos")]
    Command::new("open").arg(path).spawn()?;

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    Command::new("xdg-open").arg(path).spawn()?;

    Ok(())
}
