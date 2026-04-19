import { render, screen, act } from "@testing-library/react";
import { renderHook } from "@testing-library/react";
import { AccentThemeProvider, useAccentTheme } from "../use-accent-theme";

describe("useAccentTheme", () => {
  let setAttributeSpy: ReturnType<typeof vi.spyOn>;
  let removeAttributeSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-accent");
    setAttributeSpy = vi.spyOn(document.documentElement, "setAttribute");
    removeAttributeSpy = vi.spyOn(document.documentElement, "removeAttribute");
  });

  afterEach(() => {
    setAttributeSpy.mockRestore();
    removeAttributeSpy.mockRestore();
  });

  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <AccentThemeProvider>{children}</AccentThemeProvider>
  );

  it("defaults to burgundy when no localStorage value", () => {
    const { result } = renderHook(() => useAccentTheme(), { wrapper });
    expect(result.current.accent).toBe("burgundy");
  });

  it("useAccentTheme returns current accent", () => {
    const { result } = renderHook(() => useAccentTheme(), { wrapper });
    expect(result.current.accent).toBe("burgundy");
    expect(typeof result.current.setAccent).toBe("function");
  });

  it("setAccent('sage') updates accent and sets data-accent='sage' on html element", () => {
    const { result } = renderHook(() => useAccentTheme(), { wrapper });
    act(() => {
      result.current.setAccent("sage");
    });
    expect(result.current.accent).toBe("sage");
    expect(document.documentElement.getAttribute("data-accent")).toBe("sage");
  });

  it("setAccent('burgundy') removes data-accent attribute", () => {
    const { result } = renderHook(() => useAccentTheme(), { wrapper });
    // First set to sage
    act(() => {
      result.current.setAccent("sage");
    });
    // Then back to burgundy
    act(() => {
      result.current.setAccent("burgundy");
    });
    expect(result.current.accent).toBe("burgundy");
    expect(document.documentElement.hasAttribute("data-accent")).toBe(false);
  });

  it("reads from localStorage on mount", () => {
    localStorage.setItem("accent-theme", "sage");
    const { result } = renderHook(() => useAccentTheme(), { wrapper });
    // After effect runs, accent should be sage
    expect(result.current.accent).toBe("sage");
  });

  it("throws when used outside AccentThemeProvider", () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => {
      renderHook(() => useAccentTheme());
    }).toThrow("useAccentTheme must be used within an AccentThemeProvider");
    consoleSpy.mockRestore();
  });
});
