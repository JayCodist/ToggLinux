//! Power-event handling: stop a running timer when the machine sleeps or
//! shuts down. This matches the official desktop clients. Without it, a
//! timer keeps counting through the sleep, and a shutdown leaves the entry
//! running on the server.
//!
//! The resume side of `PrepareForSleep` also triggers a refresh. Sleep
//! pauses the monotonic clock that the request budget uses, so the loop
//! can wake thinking the whole hour is spent and never poll. Without a
//! refresh, the UI keeps showing the pre-sleep day: an app that slept
//! past midnight labels yesterday's entries "Today".
//!
//! Backend: systemd logind on the system bus. It sends `PrepareForSleep`
//! and `PrepareForShutdown` with a boolean that is true just before the
//! action. Both signals are present on any systemd distro (Ubuntu, Debian).
//! On a system without logind, the connection fails, the thread returns,
//! and the feature stays off without an error.
//!
//! Why a plain OS thread, parked on the signal: `zbus::blocking` runs its
//! own mini runtime per call, so it must not run on the Tauri async runtime
//! (the "async sandwich" panic). The iterator blocks on the socket until a
//! signal arrives. It uses no CPU and no API budget while waiting.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Wry};
use zbus::blocking::Proxy;

/// True once the watcher threads have started. Guards against a second spawn.
static STARTED: AtomicBool = AtomicBool::new(false);

const LOGIN1: &str = "org.freedesktop.login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const CORE_PATH: &str = "/org/freedesktop/login1";

/// Starts the sleep and shutdown watchers once. Safe to call from setup.
pub fn spawn(app: AppHandle<Wry>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    // One parked thread per signal. Both block on the socket and use no CPU
    // until logind sends the event.
    for (signal, name) in [
        ("PrepareForSleep", "power-sleep"),
        ("PrepareForShutdown", "power-shutdown"),
    ] {
        let app = app.clone();
        if std::thread::Builder::new()
            .name(name.into())
            .spawn(move || watch(app, signal))
            .is_err()
        {
            STARTED.store(false, Ordering::SeqCst);
            return;
        }
    }
}

/// Connects to logind and waits for one signal. Returns when the service is
/// gone or the connection drops.
fn watch(app: AppHandle<Wry>, signal: &'static str) {
    let Ok(conn) = zbus::blocking::Connection::system() else {
        crate::logger::log("info", "power: no system bus, sleep stop disabled");
        return;
    };
    let Ok(proxy) = Proxy::new(&conn, LOGIN1, CORE_PATH, MANAGER) else {
        crate::logger::log("info", "power: no logind, sleep stop disabled");
        return;
    };
    let Ok(iter) = proxy.receive_signal(signal) else {
        return;
    };
    crate::logger::log("info", &format!("power: watching {signal}"));
    for msg in iter {
        // Both signals carry one boolean: true means the action is about to
        // happen. PrepareForSleep with false means the machine just resumed.
        if let Ok((active,)) = msg.body().deserialize::<(bool,)>() {
            if active {
                handle(&app, signal);
            } else if signal == "PrepareForSleep" {
                on_resume(&app);
            }
        }
    }
}

/// The machine just resumed from sleep. Refresh once now, and once more
/// shortly if the network was not back yet. The refresh pulls any entries
/// tracked elsewhere overnight, recomputes the day total against the new
/// local date, and broadcasts, so the list and the tray show the right
/// day. It also nudges the sync loop so queued pushes go out.
fn on_resume(app: &AppHandle<Wry>) {
    crate::logger::log("info", "power: resumed from sleep");
    app.state::<crate::state::AppState>().wakeup.notify_one();
    let a = app.clone();
    tauri::async_runtime::spawn(async move {
        if crate::sync::refresh(&a).await.is_err() {
            // Wi-Fi and VPN commonly need a few seconds to re-associate.
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let _ = crate::sync::refresh(&a).await;
        }
    });
}

/// Stops the running timer if the feature is on. Reads the setting fresh, so
/// a toggle in Settings takes effect on the next event.
fn handle(app: &AppHandle<Wry>, signal: &str) {
    let (enabled, running) = {
        let st = app.state::<crate::state::AppState>();
        let enabled = st
            .store
            .as_ref()
            .and_then(|s| s.get_setting("stop_on_sleep"))
            .map(|v| v != "false")
            .unwrap_or(true);
        let running = st.timer.lock().unwrap().running;
        (enabled, running)
    };
    if enabled && running {
        crate::logger::log("info", &format!("power: {signal} stops the timer"));
        // do_stop writes the stop time to the cache at once and queues the
        // PUT. The push lands when the machine is back online.
        let _ = crate::do_stop(app);
    }
}
