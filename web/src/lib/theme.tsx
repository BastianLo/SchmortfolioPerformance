import * as React from "react";
import { setStorage, storage } from "./utils";

export type ThemeMode = "light" | "dark" | "system";

type Ctx = { mode: ThemeMode; dark: boolean; setMode: (m: ThemeMode) => void };
const ThemeContext = React.createContext<Ctx>({ mode: "system", dark: false, setMode: () => {} });

function systemDark() {
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [mode, setModeState] = React.useState<ThemeMode>(() => storage<ThemeMode>("theme", "system"));
  const [sys, setSys] = React.useState(systemDark);
  React.useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const on = () => setSys(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  const dark = mode === "dark" || (mode === "system" && sys);
  React.useLayoutEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", dark ? "#0b0f17" : "#f6f7f9");
  }, [dark]);
  const setMode = (m: ThemeMode) => {
    setModeState(m);
    setStorage("theme", m);
  };
  return <ThemeContext.Provider value={{ mode, dark, setMode }}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  return React.useContext(ThemeContext);
}

/** Reads a CSS custom property (theme token). */
export function cssVar(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}
