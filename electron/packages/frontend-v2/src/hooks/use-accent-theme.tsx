"use client";

import * as React from "react";

type Accent = "burgundy" | "sage";

interface AccentThemeContextValue {
  accent: Accent;
  setAccent: (accent: Accent) => void;
}

const AccentThemeContext = React.createContext<AccentThemeContextValue | undefined>(
  undefined
);

interface AccentThemeProviderProps {
  children: React.ReactNode;
  defaultAccent?: Accent;
}

export function AccentThemeProvider({
  children,
  defaultAccent = "burgundy",
}: AccentThemeProviderProps) {
  const [accent, setAccentState] = React.useState<Accent>(defaultAccent);

  // On mount, read from localStorage and apply
  React.useEffect(() => {
    const stored = localStorage.getItem("accent-theme") as Accent | null;
    if (stored === "sage" || stored === "burgundy") {
      setAccentState(stored);
      applyAccent(stored);
    } else {
      applyAccent(defaultAccent);
    }
  }, [defaultAccent]);

  function applyAccent(value: Accent) {
    if (value === "sage") {
      document.documentElement.setAttribute("data-accent", "sage");
    } else {
      document.documentElement.removeAttribute("data-accent");
    }
  }

  function setAccent(value: Accent) {
    setAccentState(value);
    applyAccent(value);
    localStorage.setItem("accent-theme", value);
  }

  return (
    <AccentThemeContext.Provider value={{ accent, setAccent }}>
      {children}
    </AccentThemeContext.Provider>
  );
}

export function useAccentTheme(): AccentThemeContextValue {
  const context = React.useContext(AccentThemeContext);
  if (context === undefined) {
    throw new Error("useAccentTheme must be used within an AccentThemeProvider");
  }
  return context;
}
