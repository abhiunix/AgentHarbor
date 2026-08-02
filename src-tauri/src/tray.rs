use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewWindowBuilder, WindowEvent, Wry,
};
use std::sync::atomic::{AtomicBool, Ordering};

static TRAY_CREATED: AtomicBool = AtomicBool::new(false);

const POPOVER_LABEL: &str = "tray-popover";
const POPOVER_WIDTH: f64 = 440.0;
const POPOVER_HEIGHT: f64 = 680.0;

/// Toggle the tray popover: reposition + show, or hide if already visible.
/// The popover is pre-created at setup time (hidden), so this is always fast.
fn toggle_tray_popover(app: &AppHandle<Wry>, tray: &TrayIcon<Wry>) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            position_popover_near_tray(&window, tray);
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit("tray-popover-opened", ());
        }
    }
}

/// Extract physical x,y from a tauri::Position enum.
fn position_to_physical(pos: &tauri::Position) -> (f64, f64) {
    match pos {
        tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
        tauri::Position::Logical(l) => (l.x, l.y),
    }
}

/// Extract physical width,height from a tauri::Size enum.
fn size_to_physical(size: &tauri::Size) -> (f64, f64) {
    match size {
        tauri::Size::Physical(p) => (p.width as f64, p.height as f64),
        tauri::Size::Logical(l) => (l.width, l.height),
    }
}

/// Compute popover (x, y) anchored to the tray icon and clamped to the monitor.
/// macOS menubar is at the top, so anchor below; Windows/Linux taskbar tray is at
/// the bottom-right, so anchor above.
fn popover_position(window: &tauri::WebviewWindow<Wry>, tray: &TrayIcon<Wry>) -> (f64, f64) {
    if let Some(rect) = tray.rect().ok().flatten() {
        let (tray_x, tray_y) = position_to_physical(&rect.position);
        let (tray_w, tray_h) = size_to_physical(&rect.size);
        let mut x = tray_x + (tray_w / 2.0) - (POPOVER_WIDTH / 2.0);
        let mut y = if cfg!(target_os = "macos") {
            tray_y + tray_h + 4.0
        } else {
            tray_y - POPOVER_HEIGHT - 4.0
        };

        if let Ok(Some(monitor)) = window.current_monitor() {
            let mon_pos = monitor.position();
            let mon_size = monitor.size();
            let mon_x = mon_pos.x as f64;
            let mon_y = mon_pos.y as f64;
            let mon_w = mon_size.width as f64;
            let mon_h = mon_size.height as f64;
            let margin = 8.0;
            let min_x = mon_x + margin;
            let max_x = mon_x + mon_w - POPOVER_WIDTH - margin;
            let min_y = mon_y + margin;
            let max_y = mon_y + mon_h - POPOVER_HEIGHT - margin;
            if max_x >= min_x {
                x = x.clamp(min_x, max_x);
            }
            if max_y >= min_y {
                y = y.clamp(min_y, max_y);
            }
        }
        (x, y)
    } else {
        // Fallback: top-right of screen
        let fallback_x = 1440.0 - 400.0;
        let fallback_y = 25.0;
        (fallback_x, fallback_y)
    }
}

/// Reposition an existing popover window near the tray icon.
fn position_popover_near_tray(window: &tauri::WebviewWindow<Wry>, tray: &TrayIcon<Wry>) {
    let (x, y) = popover_position(window, tray);
    let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
}

/// Restore the main window: back into the Dock/Cmd-Tab on macOS, then show + focus.
/// Every reopen path (tray menu, popover, frontend navigate-to) must go through
/// here so a window detached by ✕ comes back fully.
pub fn restore_main_window(app: &AppHandle<Wry>) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Tauri command: show and focus the main application window.
/// Called from the tray popover's "Show AgentHarbor" button.
#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    restore_main_window(&app);
}

/// Tauri command: quit the app from tray popover actions.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

pub fn setup_tray(app: &AppHandle<Wry>, show: bool) -> tauri::Result<()> {
    if show && !TRAY_CREATED.load(Ordering::SeqCst) {
        let quit_item = MenuItem::with_id(app, "quit", "Quit AgentHarbor", true, None::<&str>)?;
        let open_item = MenuItem::with_id(app, "open", "Open AgentHarbor", true, None::<&str>)?;
        let deploy_item = MenuItem::with_id(app, "deploy", "Quick Deploy...", true, None::<&str>)?;
        let sync_item = MenuItem::with_id(app, "sync", "Sync Registry", true, None::<&str>)?;
        let recs_item = MenuItem::with_id(app, "recommendations", "AI Recommendations", true, None::<&str>)?;
        let separator = PredefinedMenuItem::separator(app)?;

        let menu = Menu::with_items(
            app,
            &[
                &open_item,
                &recs_item,
                &deploy_item,
                &sync_item,
                &separator,
                &quit_item,
            ],
        )?;

        // Pre-create the popover window (hidden, offscreen) so tray clicks are instant.
        // Without this, the first click creates a new WebviewWindow which loads the
        // entire React app — causing a visible lag and spinning cursor.
        let popover = WebviewWindowBuilder::new(
            app,
            POPOVER_LABEL,
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("AgentHarbor")
        .inner_size(POPOVER_WIDTH, POPOVER_HEIGHT)
        .position(-9999.0, -9999.0) // offscreen until first show
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible(false)
        .skip_taskbar(true)
        .resizable(false)
        .build()?;

        // Auto-hide popover when it loses focus (user clicks outside).
        // This gives native menu-bar-app behavior: click tray to open,
        // click anywhere else to dismiss.
        let popover_for_blur = popover.clone();
        popover.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = popover_for_blur.hide();
            }
        });

        let _tray = TrayIconBuilder::with_id("main-tray")
            .icon(app.default_window_icon().unwrap().clone())
            .menu(&menu)
            .menu_on_left_click(false)
            .on_menu_event(|app: &AppHandle<Wry>, event| match event.id.as_ref() {
                "quit" => {
                    app.exit(0);
                }
                "open" => {
                    restore_main_window(app);
                }
                "deploy" => {
                    restore_main_window(app);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("open-deploy-wizard", ());
                    }
                }
                "sync" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("sync-registry", ());
                    }
                }
                "recommendations" => {
                    restore_main_window(app);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("navigate-to", "/recommendations");
                    }
                }
                _ => {}
            })
            .on_tray_icon_event(|tray: &TrayIcon<Wry>, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let app: &AppHandle<Wry> = tray.app_handle();
                    toggle_tray_popover(app, tray);
                }
            })
            .build(app)?;

        TRAY_CREATED.store(true, Ordering::SeqCst);

        // Start background analytics refresh for instant tray data
        crate::analytics::commands::start_tray_background_refresh(app.clone());
    }
    Ok(())
}
