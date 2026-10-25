use tauri::menu::{AboutMetadataBuilder, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

/// Menu id for "New Window" — matched in `lib.rs`'s `on_menu_event`.
pub const NEW_WINDOW_ID: &str = "new_window";
/// Menu id for "Redraw Terminals" — reloads the webview (lossless thanks to
/// the per-pane backlog replay). Escape hatch for a blank/black window.
pub const REDRAW_ID: &str = "redraw_terminals";
/// Menu id for "Keyboard Shortcuts" — the frontend opens its shortcuts dialog.
pub const SHORTCUTS_ID: &str = "keyboard_shortcuts";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let pkg = app.package_info();
    let about = AboutMetadataBuilder::new()
        .name(Some(pkg.name.clone()))
        .version(Some(pkg.version.to_string()))
        .website(Some("https://termpane.com".to_string()))
        .website_label(Some("termpane.com".to_string()))
        .license(Some("PolyForm Internal Use 1.0.0".to_string()))
        .credits(Some(
            "termpane.com\nPolyForm Internal Use 1.0.0".to_string(),
        ))
        .build();

    let app_submenu = Submenu::with_items(
        app,
        pkg.name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::services(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::hide_others(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // On Linux, plain Ctrl+letter is reserved for PTY control bytes, so the
    // app-mod is Ctrl+Shift+letter. On macOS, Cmd+N never reaches the PTY.
    #[cfg(target_os = "macos")]
    let new_window_accel = Some("Cmd+N");
    #[cfg(not(target_os = "macos"))]
    let new_window_accel = Some("Ctrl+Shift+N");
    let new_window =
        MenuItem::with_id(app, NEW_WINDOW_ID, "New Window", true, new_window_accel)?;
    let file_submenu = Submenu::with_items(app, "File", true, &[&new_window])?;

    let edit_submenu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let window_submenu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    let redraw_accel = Some("Cmd+Shift+R");
    #[cfg(not(target_os = "macos"))]
    let redraw_accel = Some("Ctrl+Shift+Alt+R");
    let redraw = MenuItem::with_id(app, REDRAW_ID, "Redraw Terminals", true, redraw_accel)?;

    #[cfg(target_os = "macos")]
    let shortcuts_accel = Some("Cmd+Shift+Slash");
    #[cfg(not(target_os = "macos"))]
    let shortcuts_accel = Some("Ctrl+Shift+Alt+Slash");
    let shortcuts_item =
        MenuItem::with_id(app, SHORTCUTS_ID, "Keyboard Shortcuts", true, shortcuts_accel)?;
    let help_submenu = Submenu::with_items(app, "Help", true, &[&shortcuts_item])?;

    #[cfg(target_os = "macos")]
    {
        let view_submenu = Submenu::with_items(
            app,
            "View",
            true,
            &[&redraw, &PredefinedMenuItem::fullscreen(app, None)?],
        )?;
        return Menu::with_items(
            app,
            &[
                &app_submenu,
                &file_submenu,
                &edit_submenu,
                &view_submenu,
                &window_submenu,
                &help_submenu,
            ],
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        let view_submenu = Submenu::with_items(app, "View", true, &[&redraw])?;
        Menu::with_items(
            app,
            &[
                &app_submenu,
                &file_submenu,
                &edit_submenu,
                &view_submenu,
                &window_submenu,
                &help_submenu,
            ],
        )
    }
}

/// Launch a second, independent TermPane process. On a bundled macOS app we go
/// through LaunchServices (`open -n`) so the new instance is registered as a
/// proper separate app; in dev / non-bundled builds we re-exec the binary
/// directly. Each instance gets its own in-memory workspace.
pub fn spawn_new_instance() {
    let Ok(exe) = std::env::current_exe() else {
        tracing::warn!("new window: current_exe() unavailable");
        return;
    };

    #[cfg(target_os = "macos")]
    {
        // exe = <App>.app/Contents/MacOS/<bin>; the .app bundle is 3 levels up.
        if let Some(bundle) = exe.ancestors().nth(3) {
            if bundle.extension().is_some_and(|e| e == "app") {
                if let Err(e) = std::process::Command::new("open")
                    .arg("-n")
                    .arg(bundle)
                    .spawn()
                {
                    tracing::warn!("new window: `open -n` failed: {e}");
                }
                return;
            }
        }
    }

    if let Err(e) = std::process::Command::new(&exe).spawn() {
        tracing::warn!("new window: re-exec failed: {e}");
    }
}
