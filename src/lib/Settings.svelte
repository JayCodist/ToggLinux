<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { listen } from "@tauri-apps/api/event";
  import {
    projectColor,
    applyTheme,
    dotColor,
    fmtWait,
    type ProjectOption,
    type TimerState,
    type Workspace,
  } from "./timer";
  import Dropdown, { type DropdownOption } from "./Dropdown.svelte";
  import Icons from "./Icons.svelte";

  let { projects, timer }: { projects: ProjectOption[]; timer: TimerState } =
    $props();

  let capDraft = $state("30");
  let savedCap = $state(30);
  let defaultProjectId = $state<number | null>(null);
  let autostart = $state(false);
  let autostartBusy = $state(false);

  let theme = $state("system");
  let traySeconds = $state(false);
  let groupEntries = $state(true);

  let idleEnabled = $state(true);
  let idleMin = $state("5");

  let stopOnSleep = $state(true);

  let hotkey = $state("CommandOrControl+Alt+D");
  let hotkeyEnabled = $state(true);
  let wayland = $state(false);
  // What the recorder box shows. The load effect sets it from the saved
  // value. Empty means "listening": the next key combination replaces it.
  // Escape puts the saved value back.
  let recHotkey = $state("CommandOrControl+Alt+D");
  // The detected idle backend, from the last timer-state broadcast. gnome,
  // kde, or none. Passed in by the parent through the timer prop.
  const idleBackend = $derived(
    timer.idleBackend === "gnome"
      ? "GNOME"
      : timer.idleBackend === "kde"
        ? "KDE"
        : "none",
  );

  let tokenDraft = $state("");
  let tokenBusy = $state(false);
  let note = $state<string | null>(null);
  let error = $state<string | null>(null);

  // Organisations (workspaces) the user belongs to. The Account section
  // lists them in a dropdown. Reading the cache uses no requests; the Rust
  // side fetches the list once per session at connect. The row appears only
  // when there is more than one organisation to choose from.
  let workspaces = $state<Workspace[]>([]);
  let switching = $state(false);

  async function loadWorkspaces() {
    try {
      workspaces = await invoke<Workspace[]>("get_workspaces");
    } catch {
      /* An unavailable cache keeps the list empty. */
    }
  }
  $effect(() => {
    void loadWorkspaces();
  });

  /** Switches the active organisation. The Rust side reloads the entries,
   * pickers, and totals for the new scope, and the timer-state broadcast
   * lands in the parent. A running timer keeps running: it belongs to the
   * user, not to the view. */
  async function switchOrg(v: string) {
    const id = Number(v);
    if (id === timer.workspaceId || switching) return;
    switching = true;
    error = null;
    note = null;
    try {
      await invoke("switch_workspace", { workspaceId: id });
      loadWorkspaces();
      // The default project and the picker lists are scoped per
      // organisation. Reload them so the Account row matches the new scope.
      void loadSettings();
      note = "Organisation switched.";
    } catch (e) {
      error = String(e);
    } finally {
      switching = false;
    }
  }

  // Diagnostics: the recent log lines, for bug reports and crash analysis.
  let diag = $state<{ logTail: string; logPath: string } | null>(null);
  let showLog = $state(false);

  async function loadDiagnostics() {
    showLog = !showLog;
    if (showLog && !diag) {
      try {
        diag = await invoke<{ logTail: string; logPath: string }>(
          "get_diagnostics",
        );
      } catch (e) {
        error = String(e);
      }
    }
  }

  function copyLog() {
    if (!diag) return;
    navigator.clipboard.writeText(`${diag.logPath}\n\n${diag.logTail}`).then(
      () => (note = "Log copied to clipboard."),
      () => (error = "Clipboard unavailable."),
    );
  }

  // App self-update. `appVersion` comes from the bundle metadata, so it is
  // never hard-coded. `updateInfo` is the last check result. The background
  // task and a manual check both fill it.
  let appVersion = $state("");
  getVersion()
    .then((v) => (appVersion = v))
    .catch(() => {});
  type UpdateInfo = {
    available: boolean;
    currentVersion: string;
    latestVersion: string | null;
    notes: string | null;
    releaseUrl: string;
    storeManaged: boolean;
    canInstall: boolean;
    checked: boolean;
  };
  let updateInfo = $state<UpdateInfo | null>(null);
  let updateBusy = $state(false);
  let installing = $state(false);

  async function checkUpdates() {
    updateBusy = true;
    error = null;
    note = null;
    try {
      updateInfo = await invoke<UpdateInfo>("check_for_updates");
      if (updateInfo.available) {
        note = null;
      } else if (updateInfo.checked) {
        note = "You are up to date.";
      } else {
        note = "Could not reach the update server. Check your connection.";
      }
    } catch (e) {
      error = String(e);
    } finally {
      updateBusy = false;
    }
  }

  async function installUpdate() {
    installing = true;
    error = null;
    try {
      // A success restarts the app, so this call does not return.
      await invoke("install_update");
    } catch (e) {
      error = String(e);
      installing = false;
    }
  }

  async function openRelease() {
    try {
      await invoke("open_release_page");
    } catch (e) {
      error = String(e);
    }
  }

  // Fetch the last check on mount, and keep the banner fresh when the
  // background task finds an update while Settings is open.
  $effect(() => {
    invoke<UpdateInfo | null>("get_update_info")
      .then((i) => {
        if (i) updateInfo = i;
      })
      .catch(() => {});
    const un = listen<UpdateInfo>("update-available", (e) => {
      updateInfo = e.payload;
    });
    return () => {
      un.then((f) => f());
    };
  });

  $effect(() => {
    loadSettings();
    invoke<boolean>("get_autostart")
      .then((v) => (autostart = v))
      .catch(() => {});
  });

  async function loadSettings() {
    try {
      const s = await invoke<{
        hourlyCap: number;
        defaultProjectId: number | null;
        theme: string;
        trayShowSeconds: boolean;
        stopOnSleep: boolean;
        idleEnabled: boolean;
        idleThresholdMin: number;
        groupEntries: boolean;
        hotkey: string;
        hotkeyEnabled: boolean;
        wayland: boolean;
      }>("get_settings");
      capDraft = String(s.hourlyCap);
      savedCap = s.hourlyCap;
      defaultProjectId = s.defaultProjectId;
      theme = s.theme ?? "system";
      traySeconds = s.trayShowSeconds ?? false;
      groupEntries = s.groupEntries ?? true;
      stopOnSleep = s.stopOnSleep ?? true;
      idleEnabled = s.idleEnabled ?? true;
      idleMin = String(s.idleThresholdMin ?? 5);
      hotkey = s.hotkey ?? "CommandOrControl+Alt+D";
      recHotkey = hotkey;
      hotkeyEnabled = s.hotkeyEnabled ?? true;
      wayland = s.wayland ?? false;
    } catch {
      /* Keep the current values if the read fails. */
    }
  }

  async function saveCap() {
    const n = Number(capDraft);
    if (!Number.isFinite(n) || n < 5) {
      error = "Cap must be at least 5 requests/hour.";
      return;
    }
    error = null;
    try {
      await invoke("set_setting", { key: "hourly_cap", value: capDraft });
      savedCap = n;
      note = "Quota cap saved.";
    } catch (e) {
      error = String(e);
    }
  }

  // Sync now: one manual sync step through the sync_now command. A manual
  // sync may spend the last interactive slots of the window, so it works
  // even when background polling is paused. Disabled when the quota is
  // exhausted or a server block is active: sending a request then only
  // earns another 429.
  let syncing = $state(false);
  const canSync = $derived(
    timer.status === "connected" && !timer.blocked && timer.requestsLeft > 0,
  );

  async function syncNow() {
    syncing = true;
    error = null;
    note = null;
    try {
      await invoke("sync_now");
      note = "Synced.";
    } catch (e) {
      error = String(e);
    } finally {
      syncing = false;
    }
  }

  const projectOptions = $derived<DropdownOption[]>([
    { value: "", label: "No project" },
    ...projects.map((p) => ({
      value: String(p.id),
      label: p.clientName ? `${p.name}, ${p.clientName}` : p.name,
      color: dotColor(p.color, p.name),
    })),
  ]);

  async function saveDefaultProject(id: number | null) {
    defaultProjectId = id;
    try {
      await invoke("set_setting", {
        key: "default_project",
        value: id === null ? null : String(id),
      });
      note = "Default project saved.";
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleAutostart() {
    autostartBusy = true;
    try {
      // The backend picks the path: the Background portal inside a
      // Flatpak sandbox, the plugin's .desktop entry everywhere else.
      await invoke("set_autostart", { enable: !autostart });
      autostart = !autostart;
    } catch (e) {
      error = String(e);
    } finally {
      autostartBusy = false;
    }
  }

  async function saveTheme(t: string) {
    theme = t;
    applyTheme(t); // live, before the round-trip
    try {
      await invoke("set_setting", { key: "theme", value: t });
      note = "Appearance saved.";
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleTraySeconds() {
    traySeconds = !traySeconds;
    try {
      await invoke("set_setting", {
        key: "tray_show_seconds",
        value: traySeconds ? "true" : "false",
      });
      note = traySeconds
        ? "Tray icon now shows seconds."
        : "Tray icon shows hours and minutes.";
    } catch (e) {
      traySeconds = !traySeconds;
      error = String(e);
    }
  }

  async function toggleIdle() {
    idleEnabled = !idleEnabled;
    try {
      await invoke("set_setting", {
        key: "idle_enabled",
        value: idleEnabled ? "true" : "false",
      });
    } catch (e) {
      idleEnabled = !idleEnabled;
      error = String(e);
    }
  }

  async function toggleGroupEntries() {
    groupEntries = !groupEntries;
    try {
      await invoke("set_setting", {
        key: "group_entries",
        value: groupEntries ? "true" : "false",
      });
    } catch (e) {
      groupEntries = !groupEntries;
      error = String(e);
    }
  }

  async function toggleStopOnSleep() {
    stopOnSleep = !stopOnSleep;
    try {
      await invoke("set_setting", {
        key: "stop_on_sleep",
        value: stopOnSleep ? "true" : "false",
      });
    } catch (e) {
      stopOnSleep = !stopOnSleep;
      error = String(e);
    }
  }

  async function saveIdleMin() {
    const n = Number(idleMin);
    if (!Number.isFinite(n) || n < 1 || n > 240) {
      error = "Threshold must be 1 to 240 minutes.";
      return;
    }
    error = null;
    try {
      await invoke("set_setting", {
        key: "idle_threshold_min",
        value: idleMin,
      });
      note = "Idle threshold saved.";
    } catch (e) {
      error = String(e);
    }
  }

  async function saveHotkey() {
    const accel = recHotkey.trim();
    if (!accel) {
      error = "Press a key combination first.";
      return;
    }
    error = null;
    try {
      await invoke("set_setting", { key: "hotkey", value: accel });
      hotkey = accel;
      note = "Shortcut saved.";
    } catch (e) {
      error = String(e);
    }
  }

  /** Turns a KeyboardEvent into the accelerator grammar the Rust side
   * parses (global-hotkey crate): CommandOrControl+Alt+Shift+Super+Key.
   * e.code values (KeyD, Digit3, F5) match the parser directly. */
  function accelFromEvent(e: KeyboardEvent): string | null {
    const key = e.code;
    // Modifier-only presses are not a complete shortcut. Keep listening.
    if (
      key === "ShiftLeft" ||
      key === "ShiftRight" ||
      key === "ControlLeft" ||
      key === "ControlRight" ||
      key === "AltLeft" ||
      key === "AltRight" ||
      key === "MetaLeft" ||
      key === "MetaRight" ||
      key === "OS"
    ) {
      return null;
    }
    const parts: string[] = [];
    if (e.ctrlKey || e.metaKey) parts.push("CommandOrControl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey && !e.ctrlKey) parts.push("Super");
    // A shortcut needs a modifier. A bare key grabs every keypress.
    if (parts.length === 0) return null;
    parts.push(key);
    return parts.join("+");
  }

  function recordHotkey(e: KeyboardEvent) {
    e.preventDefault();
    if (e.key === "Escape") {
      recHotkey = hotkey;
      (e.target as HTMLElement).blur();
      return;
    }
    const accel = accelFromEvent(e);
    if (accel) recHotkey = accel;
  }

  // Platform for display only. The stored accelerator keeps the portable
  // CommandOrControl token; global-hotkey resolves it to Cmd on macOS and
  // Ctrl everywhere else, so the box shows the key this machine uses.
  const platform = $derived.by(() => {
    const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
    if (/Mac|iPhone|iPad/i.test(ua)) return "mac";
    if (/Windows/i.test(ua)) return "windows";
    return "linux";
  });

  const specialKeys: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Escape: "Esc",
    Backspace: "Backspace",
    Delete: "Del",
    Insert: "Ins",
    ArrowUp: "\u2191",
    ArrowDown: "\u2193",
    ArrowLeft: "\u2190",
    ArrowRight: "\u2192",
    Home: "Home",
    End: "End",
    PageUp: "PgUp",
    PageDown: "PgDn",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Semicolon: ";",
    Quote: "\u2019",
    Backquote: "`",
    Minus: "-",
    Equal: "=",
    Plus: "+",
    BracketLeft: "[",
    BracketRight: "]",
    Backslash: "\\",
    CapsLock: "Caps",
    PrintScreen: "PrtSc",
    ScrollLock: "Scroll",
    NumLock: "Num",
    ContextMenu: "Menu",
  };

  /** One accelerator token (CommandOrControl, KeyD, F5…) as a readable label. */
  function keyLabel(part: string): string {
    switch (part) {
      case "CommandOrControl":
        return platform === "mac" ? "\u2318 Cmd" : "Ctrl";
      case "Command":
      case "Cmd":
        return "\u2318 Cmd";
      case "Control":
        return platform === "mac" ? "\u2303 Control" : "Ctrl";
      case "Alt":
        return platform === "mac" ? "\u2325 Option" : "Alt";
      case "Shift":
        return platform === "mac" ? "\u21e7 Shift" : "Shift";
      case "Super":
      case "Meta":
      case "OS":
        return platform === "mac"
          ? "\u2318 Cmd"
          : platform === "windows"
            ? "\u229e Win"
            : "Super";
    }
    if (/^Key[A-Z]$/.test(part)) return part.slice(3);
    if (/^Digit\d$/.test(part)) return part.slice(5);
    if (specialKeys[part]) return specialKeys[part];
    return part; // F5, Numpad5, etc. are already readable
  }

  // The accelerator shown in the recorder box, one label per key chip.
  const hotkeyParts = $derived(
    recHotkey ? recHotkey.split("+").filter(Boolean).map(keyLabel) : [],
  );

  async function toggleHotkey() {
    hotkeyEnabled = !hotkeyEnabled;
    try {
      await invoke("set_setting", {
        key: "hotkey_enabled",
        value: hotkeyEnabled ? "true" : "false",
      });
    } catch (e) {
      hotkeyEnabled = !hotkeyEnabled;
      error = String(e);
    }
  }

  async function replaceToken() {
    tokenBusy = true;
    error = null;
    note = null;
    try {
      await invoke("set_api_token", { token: tokenDraft });
      tokenDraft = "";
      note = "Token updated. Reconnected.";
    } catch (e) {
      error = String(e);
    } finally {
      tokenBusy = false;
    }
  }

  async function logout() {
    try {
      await invoke("logout");
      // App.svelte swaps to the Auth screen when status flips to disconnected.
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="settings">
  {#if note}
    <p class="muted" style="margin:0;font-size:13px">{note}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div>
    <h3 class="set-title">Account</h3>
    <div class="set-group">
      {#if workspaces.length > 1}
        <div class="set-row">
          <div class="set-main">
            <div class="set-label">Organisation</div>
            <div class="set-hint">
              The workspace your entries and projects belong to. A running timer
              keeps running when you switch.
            </div>
          </div>
          <div class="set-ctl">
            <Dropdown
              options={workspaces.map((w) => ({
                value: String(w.id),
                label: w.name,
              }))}
              value={String(timer.workspaceId)}
              placeholder="Organisation"
              icon="briefcase"
              onChange={(v) => switchOrg(v)}
            />
          </div>
        </div>
      {/if}
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">API token</div>
          <div class="set-hint">
            Stored securely on this device (system keyring when available). Find
            it on your
            <a
              class="link"
              href="https://track.toggl.com/profile"
              target="_blank"
              rel="noreferrer">profile page</a
            >.
          </div>
        </div>
        <div class="set-ctl">
          <input
            class="input"
            type="password"
            placeholder="New token…"
            bind:value={tokenDraft}
            style="width:150px"
          />
          <button
            class="btn ghost"
            disabled={!tokenDraft.trim() || tokenBusy}
            onclick={replaceToken}
          >
            {tokenBusy ? "Saving…" : "Replace"}
          </button>
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Sign out</div>
          <div class="set-hint">Forget the token on this device.</div>
        </div>
        <div class="set-ctl">
          <button class="btn ghost" onclick={logout}>Log out</button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Web</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Toggl Track</div>
          <div class="set-hint">Reports and timesheet live in the web app.</div>
        </div>
        <div class="set-ctl">
          <a
            class="link"
            href="https://track.toggl.com/timer"
            target="_blank"
            rel="noreferrer">Open Track</a
          >
          <a
            class="link"
            href="https://track.toggl.com/reports"
            target="_blank"
            rel="noreferrer">Reports</a
          >
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Appearance</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Dark Mode</div>
        </div>
        <div class="set-ctl">
          <Dropdown
            options={[
              { value: "system", label: "System" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            value={theme}
            placeholder="Dark mode"
            icon=""
            onChange={(v) => saveTheme(v)}
          />
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Tray icon seconds</div>
          <div class="set-hint">
            The tray shows today's tracked time. Include seconds for a
            second-by-second readout while a timer runs.
          </div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={traySeconds}
            role="switch"
            aria-checked={traySeconds}
            aria-label="Show seconds in tray icon"
            onclick={toggleTraySeconds}
          ></button>
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Group entries</div>
          <div class="set-hint">
            Merge entries with the same description, project and client on the
            same day into one row, like the official Toggl clients.
          </div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={groupEntries}
            role="switch"
            aria-checked={groupEntries}
            aria-label="Group entries"
            onclick={toggleGroupEntries}
          ></button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Tracking</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Default project</div>
          <div class="set-hint">Prefills the project picker when idle.</div>
        </div>
        <div class="set-ctl">
          <Dropdown
            options={projectOptions}
            value={defaultProjectId === null ? "" : String(defaultProjectId)}
            placeholder="No project"
            onChange={(v) => saveDefaultProject(v ? Number(v) : null)}
          />
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Idle detection</div>
          <div class="set-hint">
            When a timer runs and you step away, ask what to do with the idle
            time when you return
            {idleBackend}.
          </div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={idleEnabled}
            role="switch"
            aria-checked={idleEnabled}
            aria-label="Idle detection"
            onclick={toggleIdle}
          ></button>
        </div>
      </div>
      {#if idleEnabled}
        <div class="set-row">
          <div class="set-main">
            <div class="set-label">Idle threshold</div>
            <div class="set-hint">
              How many minutes away start an idle period?
            </div>
          </div>
          <div class="set-ctl">
            <input
              class="input"
              type="number"
              min="1"
              max="240"
              bind:value={idleMin}
              style="width:76px"
            />
            <button class="btn ghost" onclick={saveIdleMin}>Save</button>
          </div>
        </div>
      {/if}
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Stop on sleep</div>
          <div class="set-hint">
            Stop a running timer when the machine sleeps or shuts down
          </div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={stopOnSleep}
            role="switch"
            aria-checked={stopOnSleep}
            aria-label="Stop on sleep"
            onclick={toggleStopOnSleep}
          ></button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Keyboard</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Global shortcut</div>
          <div class="set-hint">
            Starts or stops the timer from anywhere. Press the keys in the box
            to record a new shortcut.
            {#if wayland}
              Global shortcuts need an X11 session. On Wayland, GNOME can bind
              the same keys for you: open Settings app, Keyboard, View and
              Customize Shortcuts, Custom Shortcuts, and add a command for <code
                >Trackfecta</code
              > with your chosen shortcut. In the window, Ctrl+D always works.
            {/if}
          </div>
        </div>
        <div class="set-ctl">
          <div
            class="hotkey-box"
            tabindex="0"
            role="textbox"
            aria-label="Global shortcut"
            onkeydown={recordHotkey}
            onfocus={() => (recHotkey = "")}
            onblur={() => {
              if (!recHotkey) recHotkey = hotkey;
            }}
          >
            {#if !recHotkey}
              <span class="hotkey-hint">Press a shortcut…</span>
            {:else}
              {#each hotkeyParts as label, i}
                {#if i > 0}<span class="hotkey-sep">,</span>{/if}
                <kbd>{label}</kbd>
              {/each}
            {/if}
          </div>
          <button
            class="btn ghost"
            disabled={!recHotkey || recHotkey === hotkey}
            onclick={saveHotkey}
          >
            Save
          </button>
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Global shortcut enabled</div>
          <div class="set-hint">Turns the shortcut on or off.</div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={hotkeyEnabled}
            role="switch"
            aria-checked={hotkeyEnabled}
            aria-label="Global shortcut enabled"
            onclick={toggleHotkey}
          ></button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">API budget</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Requests per hour</div>
          <div class="set-hint">
            Toggl limits API calls per rolling hour. Free plans get about 30,
            paid plans more. Background sync and every edit spend from this
            pool; raise the cap to match your plan and sync more aggressively.
          </div>
        </div>
        <div class="set-ctl">
          <input
            class="input"
            type="number"
            min="5"
            max="2000"
            bind:value={capDraft}
            style="width:76px"
          />
          <button class="btn ghost" onclick={saveCap}>Save</button>
        </div>
      </div>
      <div class="set-row">
        <div class="quota-line">
          <span class="quota-status">
            <span
              class="quota-dot"
              class:low={timer.blocked || timer.requestsLeft <= 3}
            ></span>
            {#if timer.blocked}
              API limit reached. Sync resumes in {fmtWait(timer.nextSyncIn)}.
            {:else if timer.requestsLeft <= 0}
              All {savedCap} requests used this hour. Next slot frees in{" "}
              {fmtWait(timer.nextSyncIn)}.
            {:else}
              {timer.requestsLeft} of {savedCap} requests left this hour.
            {/if}
          </span>
          <button
            class="btn sync"
            title={canSync
              ? "Sync now (uses 1 request)"
              : "No API requests left this hour"}
            disabled={!canSync || syncing}
            onclick={syncNow}
          >
            <span class="sync-ico" class:spin={syncing}
              ><Icons name="sync" size={12} /></span
            >
            {syncing ? "Syncing…" : "Sync now"}
          </button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Startup</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Launch at login</div>
          <div class="set-hint">Starts minimized to the tray.</div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={autostart}
            role="switch"
            aria-checked={autostart}
            aria-label="Launch at login"
            disabled={autostartBusy}
            onclick={toggleAutostart}
          ></button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Diagnostics</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Recent activity log</div>
          <div class="set-hint">
            Sync attempts, API errors and crashes are recorded locally. Include
            the tail when you report a bug.
          </div>
        </div>
        <div class="set-ctl">
          <button class="btn ghost" onclick={loadDiagnostics}>
            {showLog ? "Hide" : "Show"}
          </button>
        </div>
      </div>
      {#if showLog}
        <div class="diag">
          {#if diag?.logPath}
            <div class="diag-path" title={diag.logPath}>{diag.logPath}</div>
          {/if}
          <pre class="diag-log">{diag?.logTail?.trim() ||
              "(no log lines yet)"}</pre>
          <button class="btn ghost" onclick={copyLog}>Copy log</button>
        </div>
      {/if}
    </div>
  </div>

  <div>
    <h3 class="set-title">About</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">
            Trackfecta <span class="muted" style="font-weight:400"
              >v{appVersion}</span
            >
            {#if projects.length > 0}
              <span
                class="dot"
                title="Recent projects loaded"
                style="background:{projectColor(projects[0].name)}"
              ></span>
            {/if}
          </div>
          <div class="set-hint">
            Unofficial Toggl Track desktop client for Linux. Not affiliated
            with, endorsed by, or sponsored by Toggl™.
          </div>
        </div>
        <div class="set-ctl">
          <button
            class="btn ghost"
            disabled={updateBusy}
            onclick={checkUpdates}
          >
            {updateBusy ? "Checking…" : "Check for updates"}
          </button>
        </div>
      </div>
      {#if updateInfo?.available}
        <div class="update-banner">
          <div class="update-main">
            <div class="update-title">
              Version {updateInfo.latestVersion} is available
            </div>
            {#if updateInfo.storeManaged}
              <div class="update-notes">
                Your app store delivers updates for this package. Update it with
                your software center or the command line.
              </div>
            {:else if !updateInfo.canInstall}
              <div class="update-notes">
                Download the new package from the release page and install it.
                This build updates through your package manager.
              </div>
            {:else if updateInfo.notes}
              <div class="update-notes">{updateInfo.notes}</div>
            {/if}
          </div>
          <div class="update-ctl">
            <button class="btn ghost" onclick={openRelease}>
              Release page
            </button>
            {#if updateInfo.canInstall}
              <button
                class="btn primary"
                disabled={installing}
                onclick={installUpdate}
              >
                {installing ? "Installing…" : "Update now"}
              </button>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
