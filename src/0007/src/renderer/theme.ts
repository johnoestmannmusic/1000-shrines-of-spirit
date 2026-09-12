export type ThemeName = "light" | "dark";

/**
 * Light is the project's default brand theme (green #6cd73c on white panels).
 * The System option from the Rust port is intentionally dropped.
 */
export const DEFAULT_THEME: ThemeName = "light";

const STORAGE_KEY = "lantern-theme";

export function loadTheme(): ThemeName {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark") return stored;
  } catch {
    /* storage unavailable */
  }
  return DEFAULT_THEME;
}

export function applyTheme(theme: ThemeName): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    /* storage unavailable */
  }
}

/** Maps the legacy Project JSON theme strings onto the two supported themes. */
export function normalizeProjectTheme(theme: string | undefined): ThemeName {
  return theme === "dark" ? "dark" : "light";
}
