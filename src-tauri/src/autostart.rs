//! Autostart (launch at login) for the Flatpak build.
//!
//! Why a separate path: `tauri-plugin-autostart` writes a `.desktop`
//! file into `~/.config/autostart`. Inside the sandbox that directory
//! maps to `~/.var/app/<id>/config/autostart`, which the host session
//! never reads, and the `Exec` line points at `/app/bin/trackfecta`,
//! which the host cannot run. The toggle silently does nothing.
//!
//! The sandbox-native way is the XDG Background portal
//! (`org.freedesktop.portal.Background.RequestBackground`). The portal
//! runs on the host, so it writes the real autostart entry with the
//! correct `flatpak run` command line. It asks the user for permission
//! once per request, and needs no extra finish-args: portal access is
//! granted to every flatpak by default.
//!
//! The portal has no getter, so the last requested state lives in the
//! settings cache under `autostart`. The host entry itself is the
//! source of truth for whether login launch happens; this value only
//! drives the Settings switch. The portal answers asynchronously, so a
//! denied request leaves the host entry unwritten while the switch
//! shows the requested state.

use std::collections::HashMap;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{Array, Signature, Value};

/// The portal service, object path, and interface.
const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const BACKGROUND: &str = "org.freedesktop.portal.Background";

/// True when running inside a Flatpak sandbox. Every sandbox has this
/// file mounted read-only.
pub fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Registers (or unregisters) login launch through the Background
/// portal. `commandline` carries the app binary plus `--hidden`, so a
/// login start opens minimized to the tray, matching the native
/// plugin on other platforms.
pub fn request(enable: bool) -> Result<(), String> {
    let conn = Connection::session().map_err(|e| e.to_string())?;
    let proxy = Proxy::new(&conn, PORTAL, PORTAL_PATH, BACKGROUND).map_err(|e| e.to_string())?;

    // RequestBackground takes the command as a string array when the
    // app is not desktop-activatable. The first token is the sandbox
    // binary name; the portal wraps it in `flatpak run --command=`.
    let sig = Signature::try_from("s").expect("string signature");
    let mut array = Array::new(&sig);
    array
        .append(Value::from("trackfecta"))
        .map_err(|e| e.to_string())?;
    array
        .append(Value::from("--hidden"))
        .map_err(|e| e.to_string())?;

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("autostart", Value::from(enable));
    options.insert("dbus-activatable", Value::from(false));
    options.insert(
        "reason",
        Value::from(
            "Trackfecta starts minimized so the tray timer is available right after login.",
        ),
    );
    options.insert("commandline", Value::from(array));

    // Empty parent-window: the request comes from Settings, and the
    // portal dialog does not need a transient window.
    let _: zbus::zvariant::OwnedObjectPath = proxy
        .call("RequestBackground", &("", &options))
        .map_err(|e| e.to_string())?;
    Ok(())
}
