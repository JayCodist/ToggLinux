#[cfg(target_os = "linux")]
mod autostart;
mod budget;
mod idle;
mod logger;
mod power;
mod secrets;
mod state;
mod store;
mod sync;
mod toggl;
mod tray;
mod update;

use std::sync::Arc;

use tauri::{
    menu::{MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindow, Wry,
};
use tauri_plugin_autostart::ManagerExt;

use state::{broadcast, mark_disconnected, AppState, LastEntry, TimerState, TrayItems};
use store::{EntryRow, PickerProject};

// ---------- commands ----------

/// Gives the UI the current state when it starts.
/// The UI calls this on mount. The first timer-state broadcast can happen
/// before the webview starts listening. This command covers that gap.
/// The snapshot fills the list from the cache, so a returning user sees
/// their data at once, even when no broadcast has run yet (for example
/// during a quota block at startup). Reading the cache uses no requests.
#[tauri::command]
fn get_state(state: tauri::State<'_, AppState>) -> TimerState {
    state::snapshot(&state)
}

/// Gives an idle prompt that is waiting for an answer. The UI calls this on
/// mount, the same guard as get_state: the event can arrive before the
/// listener exists.
#[tauri::command]
fn get_idle_pending(state: tauri::State<'_, AppState>) -> Option<state::PendingIdle> {
    state.idle.lock().unwrap().pending.clone()
}

#[tauri::command]
async fn set_api_token(app: tauri::AppHandle<Wry>, token: String) -> Result<(), String> {
    let token = token.trim().to_string();
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve the app data directory: {}", e))?;
    secrets::set_token(&data_dir, &token)?;
    *app.state::<AppState>().token.lock().unwrap() = Some(token.clone());
    sync::connect(&app, token).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn start_timer(
    app: tauri::AppHandle<Wry>,
    description: String,
    project_id: Option<i64>,
    tags: Vec<String>,
    billable: bool,
) -> Result<(), String> {
    do_start(
        &app,
        StartPayload {
            description,
            project_id,
            tags,
            billable,
        },
    )
}

#[tauri::command]
fn stop_timer(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    do_stop(&app)
}

#[tauri::command]
fn resume_last(app: tauri::AppHandle<Wry>) {
    request_resume(&app);
}

/// Runs one sync step right now, from the Sync-now button in Settings.
/// Returns a user-facing error when there is no session or the hourly
/// quota window is full; the button is disabled in that case, so the
/// error only covers the race between the click and the state update.
#[tauri::command]
async fn sync_now(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    sync::force_sync(&app).await
}

/// Removes the token from the keyring and from memory, then shows the Auth
/// screen.
#[tauri::command]
fn logout(app: tauri::AppHandle<Wry>) {
    if let Ok(data_dir) = app.path().app_data_dir() {
        secrets::delete_token(&data_dir);
    }
    {
        let st = app.state::<AppState>();
        *st.token.lock().unwrap() = None;
        *st.last_entry.lock().unwrap() = None;
    }
    mark_disconnected(&app);
}

/// Gives the cached entries for the history window. This uses no API
/// requests. The UI groups them by day: Today, Yesterday, and dates.
#[tauri::command]
fn get_entries(state: tauri::State<'_, AppState>) -> Vec<EntryRow> {
    let wid = *state.active_workspace.lock().unwrap();
    if wid == 0 {
        return Vec::new();
    }
    state
        .store
        .as_ref()
        .map(|s| s.entries_since(wid, state::start_of_window(state::window_days(Some(s)))))
        .unwrap_or_default()
}

/// Grows the history window for the "Load earlier entries" button. This
/// doubles the window, up to one year. Then it wakes the sync loop to fetch
/// the wider range.
#[tauri::command]
fn extend_window(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let Some(store) = st.store.as_ref() else {
        return Err("cache unavailable".into());
    };
    let next = (state::window_days(Some(store)) * 2).min(366);
    store.set_setting("window_days", &next.to_string());
    // Put the delta cursor back to the start of the new window. The wider
    // fetch re-transfers the older rows. Upsert of a row already in the
    // cache does not change it. The cursor is per organisation.
    let start = state::start_of_window(next);
    let wid = *st.active_workspace.lock().unwrap();
    store.set_meta(&format!("last_sync:{wid}"), start);
    broadcast(&app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

/// Gives the cached workspace projects, with colors and client names. This
/// uses no API requests. Limited to the active organisation.
#[tauri::command]
fn get_picker_projects(state: tauri::State<'_, AppState>) -> Vec<PickerProject> {
    let wid = *state.active_workspace.lock().unwrap();
    if wid == 0 {
        return Vec::new();
    }
    state
        .store
        .as_ref()
        .map(|s| s.picker_projects(wid))
        .unwrap_or_default()
}

#[tauri::command]
fn get_picker_tags(state: tauri::State<'_, AppState>) -> Vec<String> {
    let wid = *state.active_workspace.lock().unwrap();
    if wid == 0 {
        return Vec::new();
    }
    state
        .store
        .as_ref()
        .map(|s| s.picker_tags(wid))
        .unwrap_or_default()
}

/// Gives the cached organisations (workspaces) the user belongs to. The
/// switcher menu calls this. Reading the cache uses no API requests; the
/// list is fetched once per session at connect.
#[tauri::command]
fn get_workspaces(state: tauri::State<'_, AppState>) -> Vec<toggl::Workspace> {
    state
        .store
        .as_ref()
        .map(|s| s.workspaces())
        .unwrap_or_default()
}

/// Switches the active organisation (workspace). The entry list, pickers,
/// and totals are reloaded for it. Queued local changes of another
/// organisation are untouched and keep syncing in the background.
#[tauri::command]
async fn switch_workspace(app: tauri::AppHandle<Wry>, workspace_id: i64) -> Result<(), String> {
    sync::switch_workspace(&app, workspace_id)
        .await
        .map_err(|e| e.to_string())
}

/// Gives the recent log lines for the Diagnostics section in Settings.
/// Include these lines in a bug report. Also gives the log file path.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    log_tail: String,
    log_path: String,
}

#[tauri::command]
fn get_diagnostics() -> Diagnostics {
    Diagnostics {
        log_tail: logger::read_tail(8000),
        log_path: logger::file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

/// Gives the user settings. All values are local. This uses no API requests.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    hourly_cap: usize,
    default_project_id: Option<i64>,
    default_project_name: Option<String>,
    /// One of: system, light, or dark. The webview applies this to the
    /// html element.
    theme: String,
    /// Whether the tray time readout includes seconds.
    tray_show_seconds: bool,
    /// Stop a running timer when the machine sleeps or shuts down. On by
    /// default, like the official desktop clients.
    stop_on_sleep: bool,
    /// Whether the idle prompt is on. On by default.
    idle_enabled: bool,
    /// Idle threshold in minutes. Default 5, to match Toggl.
    idle_threshold_min: i64,
    /// Merge same-day entries with the same description, project and client
    /// into one cluster row. On by default, like the official clients.
    group_entries: bool,
    /// The global shortcut accelerator, such as CommandOrControl+Alt+D.
    hotkey: String,
    /// Whether the global shortcut is on. On by default.
    hotkey_enabled: bool,
    /// True on a Wayland session. Global shortcuts need X11, so the
    /// Settings screen shows setup steps instead of the input.
    wayland: bool,
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle<Wry>) -> AppSettings {
    let st = app.state::<AppState>();
    let wid = *st.active_workspace.lock().unwrap();
    let saved_default = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("default_project"))
        .and_then(|v| v.parse::<i64>().ok());
    // A default project belongs to the organisation where it was chosen.
    // Offer it only when the project exists in the active organisation.
    // The picker list is cached per organisation, so this uses no requests.
    let default_project = saved_default.and_then(|id| {
        st.store
            .as_ref()
            .and_then(|s| s.picker_projects(wid).into_iter().find(|p| p.id == id))
    });
    let default_project_id = default_project.as_ref().map(|p| p.id);
    let default_project_name = default_project.map(|p| p.name);
    let theme = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("theme"))
        .unwrap_or_else(|| "system".into());
    let tray_show_seconds = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("tray_show_seconds"))
        .map(|v| v == "true")
        .unwrap_or(false);
    let stop_on_sleep = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("stop_on_sleep"))
        .map(|v| v != "false")
        .unwrap_or(true);
    let idle_enabled = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("idle_enabled"))
        .map(|v| v != "false")
        .unwrap_or(true);
    let idle_threshold_min = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("idle_threshold_min"))
        .and_then(|v| v.parse().ok())
        .filter(|m: &i64| (1..=240).contains(m))
        .unwrap_or(5);
    let group_entries = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("group_entries"))
        .map(|v| v != "false")
        .unwrap_or(true);
    let hotkey = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("hotkey"))
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
    let hotkey_enabled = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("hotkey_enabled"))
        .map(|v| v != "false")
        .unwrap_or(true);
    AppSettings {
        hourly_cap: st.budget.max(),
        default_project_id,
        default_project_name,
        theme,
        tray_show_seconds,
        stop_on_sleep,
        idle_enabled,
        idle_threshold_min,
        group_entries,
        hotkey,
        hotkey_enabled,
        wayland: is_wayland(),
    }
}

/// Saves a setting. The hourly_cap setting takes effect at once. The
/// default_project setting prefills the project picker of the timer. A null
/// value clears the setting.
/// The value is serde_json::Value because a Svelte number input sends a JSON
/// number. An Option<String> parameter rejected those values, and the whole
/// save failed.
#[tauri::command]
fn set_setting(
    app: tauri::AppHandle<Wry>,
    key: String,
    value: Option<serde_json::Value>,
) -> Result<(), String> {
    let st = app.state::<AppState>();
    let value = match value {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => Some(other.to_string()),
    };
    if key == "hourly_cap" {
        let cap: usize = value
            .as_deref()
            .unwrap_or("")
            .parse()
            .map_err(|_| "hourly_cap must be a number".to_string())?;
        st.budget.set_max(cap);
    }
    if let Some(store) = st.store.as_ref() {
        match &value {
            Some(v) => store.set_setting(&key, v),
            None => store.clear_setting(&key),
        }
    }
    if key == "tray_show_seconds" {
        // Draw the tray icon again with the new format.
        tray::refresh(&app);
    }
    if key == "hotkey" || key == "hotkey_enabled" {
        // Apply the new shortcut right away. A failure, such as any
        // registration on Wayland, becomes a toast instead of an error: the
        // setting is still saved for when the user returns to an X11
        // session.
        if let Err(e) = apply_hotkey(&app) {
            state::emit_toast(&app, &e);
        }
    }
    // The requests-left number can change with the new cap.
    broadcast(&app);
    Ok(())
}

/// Enables or disables launch at login. Inside a Flatpak sandbox the
/// autostart plugin cannot work: its `.desktop` file lands in the sandbox
/// copy of the autostart directory, which the host session never reads.
/// Those builds go through the XDG Background portal instead, which writes
/// the real host entry. See `autostart.rs`.
#[tauri::command]
fn set_autostart(app: tauri::AppHandle<Wry>, enable: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if autostart::in_flatpak() {
        autostart::request(enable)?;
        // The portal offers no getter. The stored choice is what the
        // Settings switch shows after the next start.
        if let Some(store) = app.state::<AppState>().store.as_ref() {
            store.set_setting("autostart", if enable { "true" } else { "false" });
        }
        return Ok(());
    }
    if enable {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

/// The current launch-at-login state for the Settings switch.
#[tauri::command]
fn get_autostart(app: tauri::AppHandle<Wry>) -> bool {
    #[cfg(target_os = "linux")]
    if autostart::in_flatpak() {
        return app
            .state::<AppState>()
            .store
            .as_ref()
            .and_then(|s| s.get_setting("autostart"))
            .map(|v| v == "true")
            .unwrap_or(false);
    }
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Edits an entry. Writes the change to the cache at once and puts the push
/// in the queue.
// The parameters are the Tauri IPC contract with the UI. A struct would
// change the wire format, so the flat form stays.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn update_entry(
    app: tauri::AppHandle<Wry>,
    id: i64,
    description: String,
    start: i64,
    stop: Option<i64>,
    tags: Vec<String>,
    project_id: Option<i64>,
    billable: bool,
) -> Result<(), String> {
    let st = app.state::<AppState>();
    if let Some(store) = st.store.as_ref() {
        let desc = if description.trim().is_empty() {
            None
        } else {
            Some(description.as_str())
        };
        store.update_local(id, desc, &tags, start, stop, project_id, billable);
        // The tray tooltip, the timer bar, and the idle logic read
        // timer.description and timer.started_at, not the cached row. Keep
        // both current when the running entry is edited, such as a corrected
        // start time.
        if stop.is_none() {
            let mut t = st.timer.lock().unwrap();
            if t.running {
                t.description = desc.map(str::to_string);
                t.started_at = Some(start.to_string());
            }
        }
    }
    broadcast(&app);
    st.wakeup.notify_one();
    Ok(())
}

/// Deletes an entry. Marks the row deleted in the cache at once and puts
/// the push in the queue.
#[tauri::command]
fn delete_entry(app: tauri::AppHandle<Wry>, id: i64) -> Result<(), String> {
    let st = app.state::<AppState>();
    if let Some(store) = st.store.as_ref() {
        store.mark_deleted(id);
    }
    broadcast(&app);
    st.wakeup.notify_one();
    Ok(())
}

/// Creates a manual entry. Inserts it in the cache with a negative temporary
/// id and puts the push in the queue. When the push succeeds, the real
/// server id replaces the temporary id.
#[tauri::command]
fn create_entry(
    app: tauri::AppHandle<Wry>,
    description: String,
    start: i64,
    stop: Option<i64>,
    tags: Vec<String>,
    project_id: Option<i64>,
    billable: bool,
) -> Result<(), String> {
    let st = app.state::<AppState>();
    let workspace_id = require_workspace(&app)?;
    let temp_id = -chrono::Utc::now().timestamp_micros();
    if let Some(store) = st.store.as_ref() {
        let entry = toggl::TimeEntry {
            id: temp_id,
            workspace_id,
            description: (!description.trim().is_empty()).then(|| description.clone()),
            start: chrono::DateTime::<chrono::Utc>::from_timestamp(start, 0)
                .unwrap_or_else(chrono::Utc::now),
            stop: stop.and_then(|s| chrono::DateTime::<chrono::Utc>::from_timestamp(s, 0)),
            duration: stop.map(|s| (s - start) as f64).unwrap_or(0.0),
            project_id,
            tags: Some(tags),
            billable: Some(billable),
            updated_at: None,
            deleted: None,
            server_deleted_at: None,
        };
        store.insert_manual(&entry);
    }
    broadcast(&app);
    st.wakeup.notify_one();
    Ok(())
}

/// Answers the idle dialog. The three actions match Toggl desktop:
///   * keep: do nothing. The idle time stays inside the running entry.
///   * discard: stop the entry at the start of the idle period. The idle
///     time and the time after it are not tracked, and the timer stops.
///   * split: stop the entry at the start of the idle period, and start a
///     new entry with the same fields at the moment activity returned.
///     The idle time is excluded and tracking continues.
#[tauri::command]
fn resolve_idle(app: tauri::AppHandle<Wry>, action: String) -> Result<(), String> {
    let st = app.state::<AppState>();
    let Some(pending) = st.idle.lock().unwrap().pending.take() else {
        // Nothing to answer. The dialog can be open only with a pending
        // prompt, so this is a double-click guard.
        return Ok(());
    };
    if action == "keep" {
        broadcast(&app);
        return Ok(());
    }
    if action != "discard" && action != "split" {
        return Err("unknown idle action".into());
    }
    let Some(store) = st.store.as_ref() else {
        return Err("cache unavailable".into());
    };
    // The running row is global (any organisation). The idle prompt always
    // targets the entry this user is running.
    let Some(row) = store.open_entry() else {
        // The timer stopped while the dialog was open. Nothing to fix.
        return Ok(());
    };
    // The idle boundary must fall inside the entry. A clock that disagrees
    // (a bad backend value) is answered like "keep" rather than corrupting
    // the row.
    if pending.idle_start <= row.start {
        return Ok(());
    }
    // Stop the running row at the start of the idle period. dirty=1 puts
    // the PUT in the queue like any other local edit.
    store.stop_local(row.id, pending.idle_start);
    if action == "discard" {
        let mut t = st.timer.lock().unwrap();
        t.running = false;
        t.description = None;
        t.started_at = None;
        *st.entry_id.lock().unwrap() = None;
    } else {
        // Split: a new running entry from the moment of activity, with the
        // same description, project, tags, and billable flag.
        let now = chrono::Utc::now();
        let temp_id = -now.timestamp_micros();
        let entry = toggl::TimeEntry {
            id: temp_id,
            workspace_id: row.workspace_id,
            description: row.description.clone(),
            start: chrono::DateTime::<chrono::Utc>::from_timestamp(pending.idle_end, 0)
                .unwrap_or_else(chrono::Utc::now),
            stop: None,
            duration: 0.0,
            project_id: row.project_id,
            tags: Some(row.tags.clone()),
            billable: Some(row.billable),
            updated_at: None,
            deleted: None,
            server_deleted_at: None,
        };
        store.insert_manual(&entry);
        // The timer keeps running, counted from the return of activity.
        let mut t = st.timer.lock().unwrap();
        t.started_at = Some(pending.idle_end.to_string());
        *st.entry_id.lock().unwrap() = None;
    }
    broadcast(&app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

// ---------- hotkeys ----------

/// The default global shortcut. Alt keeps it clear of browser and editor
/// bindings, and of the app-local Ctrl+D.
pub const DEFAULT_HOTKEY: &str = "CommandOrControl+Alt+D";

// ---------- app self-update ----------

/// Gives the UI the last update-check result. None before the first check
/// finishes. The Settings About section fetches this on mount.
#[tauri::command]
fn get_update_info(state: tauri::State<'_, update::UpdateState>) -> Option<update::Info> {
    state.last.lock().unwrap().clone()
}

/// Checks for an app update now. Returns the result for the Settings row.
#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle<Wry>) -> update::Info {
    update::check_now(&app).await
}

/// Installs the newest update and restarts the app.
#[tauri::command]
async fn install_update(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    update::install(&app).await
}

/// Opens the release page for the newest version in the browser.
#[tauri::command]
fn open_release_page(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    update::open_release(&app)
}

/// True on a Wayland session. The global-hotkey backend needs X11; there
/// is no Wayland protocol for grabbing keys.
fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok()
}

/// Re-registers the global shortcut from the current settings. Unregisters
/// everything first, so a changed accelerator cannot leave the old one
/// live. Returns Err with a user-facing note when registration fails.
fn apply_hotkey(app: &AppHandle<Wry>) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let st = app.state::<AppState>();
    let enabled = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("hotkey_enabled"))
        .map(|v| v != "false")
        .unwrap_or(true);
    let accel = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("hotkey"))
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
    let gs = app.global_shortcut();
    // Always clear first. unregister_all is safe with nothing registered.
    gs.unregister_all().map_err(|e| e.to_string())?;
    if !enabled {
        return Ok(());
    }
    gs.on_shortcut(accel.as_str(), |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            toggle_timer(_app);
        }
    })
    .map_err(|e| {
        crate::logger::log(
            "warn",
            &format!("hotkey: register '{accel}' failed: {e}"),
        );
        if is_wayland() {
            // Expected on Wayland. Point at the shell-level alternative.
            format!("Global shortcut needs X11. On Wayland, add a custom keybinding for {accel} in the shell, or use Ctrl+D in the window.")
        } else {
            format!("Could not register {accel}: {e}")
        }
    })?;
    crate::logger::log("info", &format!("hotkey: registered {accel}"));
    Ok(())
}

/// Starts or stops the timer. Used by the global shortcut. A start reuses
/// the last entry's fields, or starts an empty entry.
fn toggle_timer(app: &AppHandle<Wry>) {
    let running = app.state::<AppState>().timer.lock().unwrap().running;
    if running {
        let _ = do_stop(app);
        return;
    }
    let last = app.state::<AppState>().last_entry.lock().unwrap().clone();
    let payload = match last {
        Some(l) => StartPayload {
            description: l.description.unwrap_or_default(),
            project_id: l.project_id,
            tags: l.tags,
            billable: l.billable,
        },
        None => StartPayload {
            description: String::new(),
            project_id: None,
            tags: Vec::new(),
            billable: false,
        },
    };
    let _ = do_start(app, payload);
}

// ---------- core operations (shared by the tray and the commands) ----------

/// The active organisation id. Start and the manual create need it. When
/// there is no live session, fall back to the id saved at the last
/// successful connect. A quota block or a network failure must not stop
/// local tracking: the entry goes into the cache with the dirty flag, and
/// the sync loop pushes it when the window opens. The error only covers a
/// fresh install that never connected.
fn require_workspace(app: &AppHandle<Wry>) -> Result<i64, String> {
    let st = app.state::<AppState>();
    let active = *st.active_workspace.lock().unwrap();
    if active != 0 {
        return Ok(active);
    }
    let saved = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("workspace_id"))
        .and_then(|v| v.parse::<i64>().ok());
    match saved {
        Some(ws) => Ok(ws),
        None => Err("not connected".into()),
    }
}

/// The data the user submitted in the timer bar, or the data reused by
/// "Resume last entry".
#[derive(Clone)]
struct StartPayload {
    description: String,
    project_id: Option<i64>,
    tags: Vec<String>,
    billable: bool,
}

/// Starts a timer. Like every other action, this is optimistic. The running
/// entry goes into the cache at once with a negative temporary id and the
/// dirty flag. The UI updates at once. The sync loop POSTs the entry and
/// replaces the temporary id with the server id. If the push fails, the
/// cache removes the row and the timer stops.
fn do_start(app: &AppHandle<Wry>, payload: StartPayload) -> Result<(), String> {
    let (workspace_id, store) = {
        let st = app.state::<AppState>();
        if st.timer.lock().unwrap().running {
            return Err("a timer is already running".into());
        }
        (require_workspace(app)?, st.store.clone())
    };

    let now = chrono::Utc::now();
    let desc = (!payload.description.trim().is_empty()).then(|| payload.description.clone());
    let temp_id = -now.timestamp_micros();
    let entry = toggl::TimeEntry {
        id: temp_id,
        workspace_id,
        description: desc.clone(),
        start: now,
        stop: None,
        duration: 0.0,
        project_id: payload.project_id,
        tags: Some(payload.tags.clone()),
        billable: Some(payload.billable),
        updated_at: None,
        deleted: None,
        server_deleted_at: None,
    };
    if let Some(store) = &store {
        // dirty=1 makes the push loop POST the row. stop:null means running.
        store.insert_manual(&entry);
    }
    {
        let st = app.state::<AppState>();
        let mut t = st.timer.lock().unwrap();
        t.running = true;
        t.description = desc.clone();
        t.started_at = Some(now.timestamp().to_string());
        *st.last_entry.lock().unwrap() = Some(LastEntry {
            description: desc,
            project_id: payload.project_id,
            tags: payload.tags,
            billable: payload.billable,
        });
        // The server id is unknown until the POST succeeds. do_stop finds
        // the running row with the open-entry lookup in the cache.
        *st.entry_id.lock().unwrap() = None;
    }
    broadcast(app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

/// Stops the running timer. The cached row gets its stop time and duration
/// at once, with the dirty flag. The UI clears at once. The sync loop PUTs
/// the change. If the push fails, the timer goes back to running.
pub(crate) fn do_stop(app: &AppHandle<Wry>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let entry_id = match *st.entry_id.lock().unwrap() {
        Some(id) => id,
        // The entry is not on the server yet. The running row is in the
        // cache with its temporary id. The running row is global, so it can
        // live in another organisation after a workspace switch.
        None => st
            .store
            .as_ref()
            .and_then(|s| s.open_entry())
            .map(|r| r.id)
            .ok_or("no running entry")?,
    };
    let now = chrono::Utc::now().timestamp();
    if let Some(store) = st.store.as_ref() {
        // stop_local computes the duration from the stored start time.
        store.stop_local(entry_id, now);
    }
    {
        let mut t = st.timer.lock().unwrap();
        t.running = false;
        t.description = None;
        t.started_at = None;
        *st.entry_id.lock().unwrap() = None;
    }
    broadcast(app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

/// Resumes the last entry: starts a timer with its description, project,
/// tags, and billable flag. Same data the hotkey reuses in `toggle_timer`.
/// The UI is also asked to prefill the bar, which is what the user sees when
/// the start cannot happen: a blocked quota, or no entry to resume.
fn request_resume(app: &AppHandle<Wry>) {
    let last = app.state::<AppState>().last_entry.lock().unwrap().clone();
    if let Some(w) = app.get_webview_window("main") {
        show_window(&w);
    }
    let _ = app.emit("resume-requested", last.clone());
    let Some(l) = last else {
        return;
    };
    if app.state::<AppState>().timer.lock().unwrap().running {
        return;
    }
    if let Err(e) = do_start(
        app,
        StartPayload {
            description: l.description.unwrap_or_default(),
            project_id: l.project_id,
            tags: l.tags,
            billable: l.billable,
        },
    ) {
        state::emit_toast(app, &e);
    }
}

fn show_window(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch shows the existing window instead.
            if let Some(w) = app.get_webview_window("main") {
                show_window(&w);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // These arguments go into the autostart .desktop entry. The app
            // starts minimized.
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Opens external links (target=_blank anchors) in the OS default
        // browser. The webview cannot open a new window on its own.
        .plugin(tauri_plugin_opener::init())
        // App self-update. The plugin reads the signed release manifest and
        // installs updates. The webview does not call it directly; the Rust
        // commands in this file drive it. So no updater capability entry is
        // needed.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Remember the window size and position across restarts. The
        // plugin restores them when the window is ready and saves them on
        // exit. State lives in the app data dir; no frontend API is used,
        // so no capability entries are needed. VISIBLE is excluded on
        // purpose: this app hides instead of closing, so a saved
        // "hidden" state would make a normal launch look dead. The
        // --hidden flag still covers autostart.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .setup(|app| {
            // ---- Start file logging first. Everything after this is logged. ----
            logger::install_panic_hook();
            if let Ok(dir) = app.path().app_data_dir() {
                logger::init(&dir);
            }
            // ---- Open the SQLite cache. If it fails, the app uses live data. ----
            let store = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| store::Store::open(&dir))
                .map(Arc::new);
            app.manage(AppState::new(store));
            app.manage(update::UpdateState::new());
            // Apply the hourly cap from settings. Paid plans allow more than 30.
            if let Some(cap) = app
                .state::<AppState>()
                .store
                .as_ref()
                .and_then(|s| s.get_setting("hourly_cap"))
                .and_then(|v| v.parse::<usize>().ok())
            {
                app.state::<AppState>().budget.set_max(cap);
            }
            // Restore the organisation the user last chose. The connect
            // step checks it against the live membership list, so a stale
            // id (an organisation the user left) falls back to the account
            // default. Without this, the first cached reads would run with
            // no scope at all.
            if let Some(wid) = app
                .state::<AppState>()
                .store
                .as_ref()
                .and_then(|s| s.get_setting("active_workspace"))
                .and_then(|v| v.parse::<i64>().ok())
            {
                *app.state::<AppState>().active_workspace.lock().unwrap() = wid;
            }
            // A restart clears the in-memory window, so restore a quota
            // block the previous run recorded. Without this, a restart
            // during a block immediately spends requests and re-hits the
            // limit. The stored value is a wall-clock instant.
            if let Some(until) = app
                .state::<AppState>()
                .store
                .as_ref()
                .and_then(|s| s.get_meta("quota_blocked_until"))
            {
                let now = chrono::Utc::now().timestamp();
                if until > now {
                    app.state::<AppState>()
                        .budget
                        .block_for(std::time::Duration::from_secs((until - now) as u64));
                }
            }

            // ---- Tray menu: status item with changing text, and actions ----
            // Linux trays do not get mouse events. All operations are in the menu.
            let status = MenuItem::with_id(app, "status", "Today: 0:00", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show Trackfecta", true, None::<&str>)?;
            let resume =
                MenuItem::with_id(app, "resume", "Resume last entry", false, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop timer", false, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = MenuBuilder::new(app)
                .item(&status)
                .separator()
                .item(&show)
                .item(&resume)
                .item(&stop)
                .separator()
                .item(&quit)
                .build()?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .tooltip("Trackfecta: unofficial Toggl Track client")
                .on_menu_event(|app, event| match event.id().0.as_str() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            show_window(&w);
                        }
                    }
                    // The top status item is the primary action. While a
                    // timer runs its text reads "Stop: <desc>", so clicking
                    // it stops. When idle it shows the day total, so clicking
                    // it opens the window.
                    "status" => {
                        let running = app.state::<AppState>().timer.lock().unwrap().running;
                        if running {
                            let _ = do_stop(app);
                        } else if let Some(w) = app.get_webview_window("main") {
                            show_window(&w);
                        }
                    }
                    "stop" => {
                        let _ = do_stop(app);
                    }
                    "resume" => request_resume(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            app.manage(TrayItems {
                status,
                show,
                resume,
                stop,
                quit,
            });

            // The tray icon shows the time tracked today and the running
            // state. Draw it once now, before the first broadcast. Then start
            // the ticker that keeps the running time counting. The ticker
            // uses no API requests.
            tray::refresh(app.handle());
            tray::spawn(app.handle().clone());

            // ---- idle detection (local D-Bus reads, no API budget) ----
            idle::spawn(app.handle().clone());

            // ---- stop the timer on sleep or shutdown (logind signals) ----
            power::spawn(app.handle().clone());

            // ---- global shortcut (X11 only; Wayland reports a note) ----
            if let Err(e) = apply_hotkey(app.handle()) {
                crate::logger::log("warn", &format!("startup hotkey: {e}"));
            }

            // ---- Startup: restore the session if a token is in the keyring ----
            // A temporary failure, such as being offline at startup, leaves the
            // status as verifying. The sync loop retries connect with the
            // cached token by itself. While a quota block is active, skip the
            // attempt: GET /me is itself a request and would only re-hit the
            // limit. The loop retries on its own after the block lifts.
            let data_dir = app.path().app_data_dir().ok();
            let token = data_dir.as_ref().and_then(|dir| secrets::get_token(dir));
            if let Some(token) = token {
                *app.state::<AppState>().token.lock().unwrap() = Some(token.clone());
                let blocked = !app.state::<AppState>().budget.until_blocked().is_zero();
                if !blocked {
                    let app_clone = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = sync::connect(&app_clone, token).await;
                    });
                }
            } else {
                mark_disconnected(app.handle());
            }

            // Autostart passes --hidden: start minimized to the tray. The
            // window still opens on a second launch (single-instance) or
            // from the tray menu.
            if std::env::args().any(|a| a == "--hidden") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // ---- background polling loop ----
            sync::spawn(app.handle().clone());

            // ---- app self-update: a daily check against GitHub Releases ----
            update::spawn(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it. The app keeps running in the tray.
            // The Quit item in the tray menu is the only way to exit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            // Wake the sync loop when the window gets focus. This keeps the
            // data fresh.
            if let tauri::WindowEvent::Focused(true) = event {
                window.state::<AppState>().wakeup.notify_one();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_idle_pending,
            set_api_token,
            logout,
            start_timer,
            stop_timer,
            resume_last,
            sync_now,
            get_entries,
            extend_window,
            get_picker_projects,
            get_picker_tags,
            get_settings,
            set_setting,
            set_autostart,
            get_autostart,
            get_workspaces,
            switch_workspace,
            update_entry,
            delete_entry,
            create_entry,
            resolve_idle,
            get_diagnostics,
            get_update_info,
            check_for_updates,
            install_update,
            open_release_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running Trackfecta");
}
