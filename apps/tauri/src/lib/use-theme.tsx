import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type Theme = "system" | "light" | "dark";

// localStorage key — namespaced so it doesn't collide with anything the
// Tauri WebView might pick up from a co-hosted Vite dev origin.
const STORAGE_KEY = "openwhisper-theme";

interface ThemeContextValue {
  theme: Theme;
  setTheme: (t: Theme) => void;
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

function readStoredTheme(): Theme {
  if (typeof window === "undefined") return "system";
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark" || stored === "system") {
    return stored;
  }
  return "system";
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  const effective =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme;
  root.classList.add(effective);
}

// For a window that has no ThemeProvider of its own (the answer
// window of Ask): follow the theme chosen in the main window. The
// choice lives in localStorage, which the windows share; index.html
// applies it when the page loads, and this keeps it right while the
// window stays open: when the main window stores another choice (the
// `storage` event) and when the OS switches between light and dark.
// Returns a function that applies the stored choice again, for a
// window that wants to be sure before it shows something new.
export function useStoredTheme(): () => void {
  useEffect(() => {
    applyStoredTheme();
    const onStorage = (event: StorageEvent) => {
      // `key` is null when the whole storage was cleared.
      if (event.key === null || event.key === STORAGE_KEY) applyStoredTheme();
    };
    window.addEventListener("storage", onStorage);
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    mql.addEventListener("change", applyStoredTheme);
    return () => {
      window.removeEventListener("storage", onStorage);
      mql.removeEventListener("change", applyStoredTheme);
    };
  }, []);
  return applyStoredTheme;
}

function applyStoredTheme() {
  try {
    applyTheme(readStoredTheme());
  } catch {
    // localStorage unavailable: keep the theme the page loaded with.
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(readStoredTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  // System mode follows OS-level prefers-color-scheme changes live.
  useEffect(() => {
    if (theme !== "system") return;
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme("system");
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [theme]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme,
      setTheme: (t) => {
        window.localStorage.setItem(STORAGE_KEY, t);
        setThemeState(t);
      },
    }),
    [theme],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return ctx;
}
