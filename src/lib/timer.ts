// Shared timer types and helpers. These mirror the Rust TimerState in
// src-tauri/src/state.rs.

export type ConnStatus = "verifying" | "disconnected" | "connected";

/** A cached time entry. Mirrors the Rust EntryRow. */
export type EntryRow = {
  id: number;
  workspaceId: number;
  description: string | null;
  /** Unix seconds. */
  start: number;
  /** Unix seconds. Null while running. */
  stop: number | null;
  duration: number;
  projectId: number | null;
  /** From the cached project list, when the project is known. */
  projectName: string | null;
  /** The color Toggl assigned to the project (hex, no `#`), when known. */
  projectColor: string | null;
  /** The client that owns the project, when the project has one. */
  clientName: string | null;
  tags: string[];
  billable: boolean;
  /** A local change not yet pushed to the server. */
  dirty: boolean;
};

/** A picker option. Mirrors the Rust PickerProject, from the workspace
 * project list cached with the client list. */
export type ProjectOption = {
  id: number;
  name: string;
  billable: boolean;
  /** The color Toggl assigned to the project (hex, no `#`), when known. */
  color: string | null;
  /** The name of the owning client, when the project has a client. */
  clientName: string | null;
};

/** The payload of the resume-requested event. Mirrors the Rust LastEntry. */
export type LastEntry = {
  description: string | null;
  projectId: number | null;
  tags: string[];
  billable: boolean;
};

/** One organisation (workspace) the user belongs to. Mirrors the Rust
 * Workspace. The switcher lists these. */
export type Workspace = {
  id: number;
  name: string;
};

/** Local settings snapshot. Mirrors the Rust AppSettings. */
export type AppSettings = {
  hourlyCap: number;
  defaultProjectId: number | null;
  defaultProjectName: string | null;
  /** "system", "light", or "dark". */
  theme: string;
  /** The tray icon shows seconds while a timer runs. */
  trayShowSeconds: boolean;
  /** Stop a running timer when the machine sleeps or shuts down. */
  stopOnSleep: boolean;
  /** Ask what to do with idle time when a timer runs. */
  idleEnabled: boolean;
  /** Idle threshold in minutes. Default 5. */
  idleThresholdMin: number;
  /** Merge same-day entries with the same title, project and client. */
  groupEntries: boolean;
  /** The global shortcut accelerator, such as "CommandOrControl+Alt+D". */
  hotkey: string;
  /** Whether the global shortcut is on. */
  hotkeyEnabled: boolean;
  /** True on Wayland, where global shortcuts cannot be registered. */
  wayland: boolean;
};

/** An idle period that ended while the timer ran. Mirrors the Rust
 * PendingIdle, the payload of the idle-dialog event. Unix seconds. */
export type IdlePending = {
  idleStart: number;
  idleEnd: number;
  idleSeconds: number;
  runningDescription: string | null;
};

export type TimerState = {
  running: boolean;
  description: string | null;
  /** Unix seconds, as a string. */
  startedAt: string | null;
  /** Seconds tracked today, not including the running entry. */
  todaySeconds: number;
  status: ConnStatus;
  /** Recent entries, newest first. */
  entries: EntryRow[];
  /** Requests left in the rolling hour. The free plan allows about 30. */
  requestsLeft: number;
  /** True while a server quota block (402 or 429) is active. */
  blocked: boolean;
  /** Seconds until the next request can go out. Zero when nothing waits. */
  nextSyncIn: number;
  /** Idle backend in use: "gnome", "kde", or "none". */
  idleBackend: string;
  /** Active organisation (workspace) id. 0 before the first connect. */
  workspaceId: number;
  /** Cached name of the active organisation, when known. */
  workspaceName: string | null;
};

export const EMPTY: TimerState = {
  running: false,
  description: null,
  startedAt: null,
  todaySeconds: 0,
  status: "verifying",
  entries: [],
  requestsLeft: 0,
  blocked: false,
  nextSyncIn: 0,
  idleBackend: "",
  workspaceId: 0,
  workspaceName: null,
};

/**
 * Applies the saved appearance choice. "system" removes the attribute, so the
 * CSS media query follows the OS. "light" and "dark" pin it. See styles.css.
 */
export function applyTheme(theme: string) {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") root.dataset.theme = theme;
  else delete root.dataset.theme;
}

export function fmtDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/** Format a wait time for a message: "28m", "1h 05m". Rounds minutes up.
 * Mirrors budget::fmt_wait on the Rust side. */
export function fmtWait(secs: number): string {
  const mins = Math.ceil(secs / 60);
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, "0")}m`;
}

export function fmtShort(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? `${h}h ${m}m` : `${m}m`;
}

/** The Toggl desktop day-total style: "4 h 15 min". */
export function fmtDayTotal(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h} h ${String(m).padStart(2, "0")} min`;
}

/** HH:MM in local time, from Unix seconds. */
export function fmtClock(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** A 24h HH:MM value for `<input type="time">`. Does not depend on locale. */
export function toTimeValue(ts: number): string {
  const d = new Date(ts * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(
    d.getMinutes(),
  ).padStart(2, "0")}`;
}

/**
 * Unix seconds at HH:MM today (local). Returns `fallback` when the value is
 * blank.
 */
export function todayAt(hhmm: string, fallback: number): number {
  if (!hhmm) return fallback;
  const [h, m] = hhmm.split(":").map(Number);
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/** Local midnight of the day that contains `ts`, as Unix seconds. */
export function startOfDay(ts: number): number {
  const d = new Date(ts * 1000);
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/**
 * The Toggl desktop section header: "Today", "Yesterday", or a date such as
 * "Mon, 08 Aug". `today` is the caller's current local midnight. Passing it
 * (instead of reading the clock here) lets the list re-label the headers
 * when the day changes while the app sits open across midnight.
 */
export function dayLabel(startTs: number, today?: number): string {
  const now = today ?? startOfDay(Math.floor(Date.now() / 1000));
  const day = startOfDay(startTs);
  if (day === now) return "Today";
  if (day === now - 86400) return "Yesterday";
  return new Date(startTs * 1000).toLocaleDateString([], {
    weekday: "short",
    day: "2-digit",
    month: "short",
  });
}

/** A deterministic project color from the project name, like Toggl's. */
const PROJECT_COLORS = [
  "#d12351",
  "#e56363",
  "#d1675f",
  "#d98016",
  "#d9a616",
  "#a3b745",
  "#69b065",
  "#37a4a6",
  "#2d8bba",
  "#5f7ed1",
  "#7c60d5",
  "#b15edf",
  "#a05a2c",
  "#45a7c4",
  "#42a872",
  "#df3966",
];

export function projectColor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) | 0;
  }
  return PROJECT_COLORS[Math.abs(h) % PROJECT_COLORS.length];
}

/**
 * The dot color for a project: the color Toggl assigned when we have it (the
 * API returns hex with or without a leading `#`), else the deterministic
 * fallback from the name.
 */
export function dotColor(
  serverColor: string | null | undefined,
  name: string | null | undefined,
): string {
  if (serverColor) {
    return serverColor.startsWith("#") ? serverColor : `#${serverColor}`;
  }
  return projectColor(name ?? "");
}

/**
 * A very faint version of a hex color, for chip backgrounds. This puts the
 * color of the selected project as a light wash behind its own chip, like the
 * Toggl web app.
 */
export function faint(hex: string, alpha = 0.13): string {
  const m = hex.replace("#", "");
  const full =
    m.length === 3
      ? m
          .split("")
          .map((c) => c + c)
          .join("")
      : m;
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
