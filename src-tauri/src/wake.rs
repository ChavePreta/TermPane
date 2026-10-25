//! Recover the WKWebView after the machine sleeps or hibernates.
//!
//! On macOS the OS can terminate a WKWebView's WebContent (renderer) process
//! during hibernation. When that happens the frontend's JS heap is gone, so no
//! amount of in-page repainting (`visibilitychange` handlers, `term.refresh()`)
//! can recover it — the window is left painted white with only the title bar.
//! The only escape is a native `-[WKWebView reload]`, which relaunches the
//! renderer. The shells survive regardless (the PTYs live in this Rust process,
//! not the webview), so a reload brings the terminals back attached to the same
//! live shells; only the on-screen scrollback is lost.
//!
//! Reloading always would needlessly discard scrollback on the milder wake
//! failures (GPU context loss, a stalled compositor) where the JS heap is still
//! alive and a repaint suffices. So this is layered: on a system wake we emit
//! `system:woke` and give the frontend a short window to prove — from inside a
//! `requestAnimationFrame`, which only fires once the compositor paints again —
//! that it is alive, by calling the `ack_woke` command. If that ack never
//! arrives, the WebContent process is dead and the watchdog reloads.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::AppHandle;

// How long to wait after a wake for the frontend to paint a frame and ack
// before concluding the webview is dead and reloading it. Long enough that a
// busy-but-alive page under post-wake load won't be reloaded out from under a
// live scrollback; short enough that a genuinely dead window recovers quickly.
const ACK_TIMEOUT: Duration = Duration::from_millis(2500);

#[derive(Default)]
struct WakeInner {
    // Bumped on every wake so a stale ack from an earlier wake can't cancel the
    // current one.
    generation: u64,
    // Set on wake, cleared on ack or when the watchdog fires. `Some` means a
    // reload is pending unless the frontend acks in time.
    armed_at: Option<Instant>,
}

pub struct WakeWatch {
    inner: Mutex<WakeInner>,
}

impl WakeWatch {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(WakeInner::default()),
        })
    }

    // A system wake just happened: bump the generation and arm the watchdog.
    // Returns the new generation to hand the frontend.
    fn arm(&self) -> u64 {
        let mut g = self.inner.lock();
        g.generation += 1;
        g.armed_at = Some(Instant::now());
        g.generation
    }

    // The frontend painted a frame after waking — cancel the pending reload.
    pub fn ack(&self, generation: u64) {
        let mut g = self.inner.lock();
        if g.generation == generation {
            g.armed_at = None;
        }
    }

    // Watchdog poll: if armed and the ack window has elapsed, disarm and report
    // that a reload is due.
    fn reload_due(&self) -> bool {
        let mut g = self.inner.lock();
        match g.armed_at {
            Some(t) if t.elapsed() >= ACK_TIMEOUT => {
                g.armed_at = None;
                true
            }
            _ => false,
        }
    }
}

pub fn install(app: &AppHandle, watch: Arc<WakeWatch>) {
    #[cfg(target_os = "macos")]
    {
        spawn_watchdog(app.clone(), watch.clone());
        install_wake_observer(app.clone(), watch);
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Wake-induced white screens are a macOS WKWebView behavior; nothing to
        // arm elsewhere. `ack_woke` stays a harmless no-op.
        let _ = (app, watch);
    }
}

#[cfg(target_os = "macos")]
fn spawn_watchdog(app: AppHandle, watch: Arc<WakeWatch>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        if watch.reload_due() {
            tracing::warn!("no frame after wake within {ACK_TIMEOUT:?}; reloading webview");
            reload_all_webviews(&app);
        }
    });
}

#[cfg(target_os = "macos")]
fn reload_all_webviews(app: &AppHandle) {
    use tauri::Manager;
    for window in app.webview_windows().into_values() {
        // `with_webview` runs the closure on the main thread, which is where
        // `-[WKWebView reload]` must be called from.
        let _ = window.with_webview(|platform| unsafe {
            use objc2_web_kit::WKWebView;
            let view: &WKWebView = &*platform.inner().cast::<WKWebView>();
            view.reload();
        });
    }
}

#[cfg(target_os = "macos")]
fn install_wake_observer(app: AppHandle, watch: Arc<WakeWatch>) {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;
    use tauri::Emitter;

    let handler = RcBlock::new(move |_notif: NonNull<NSNotification>| {
        let generation = watch.arm();
        let _ = app.emit("system:woke", generation);
    });

    // NSWorkspace notifications post on the main thread, and `install` is called
    // from Tauri's `.setup()` which also runs on the main thread — so this
    // registration and the block's later invocation are both main-thread.
    unsafe {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let observer = center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            None,
            &handler,
        );
        // The observation is removed when this observer is dropped; it must live
        // for the whole process, so leak it deliberately.
        std::mem::forget(observer);
    }
}
