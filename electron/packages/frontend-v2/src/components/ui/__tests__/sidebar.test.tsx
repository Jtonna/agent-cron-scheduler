import { render, screen } from "@testing-library/react";
import { renderHook } from "@testing-library/react";
import { SidebarProvider, useSidebar } from "../sidebar";

describe("SidebarProvider", () => {
  it("provides context and renders children", () => {
    render(
      <SidebarProvider>
        <div>Child content</div>
      </SidebarProvider>
    );
    expect(screen.getByText("Child content")).toBeInTheDocument();
  });

  it("renders the sidebar wrapper element", () => {
    render(
      <SidebarProvider>
        <div>Content</div>
      </SidebarProvider>
    );
    const wrapper = document.querySelector('[data-slot="sidebar-wrapper"]');
    expect(wrapper).toBeInTheDocument();
  });
});

describe("useSidebar", () => {
  it("returns expected shape", () => {
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <SidebarProvider>{children}</SidebarProvider>
    );
    const { result } = renderHook(() => useSidebar(), { wrapper });
    expect(result.current).toHaveProperty("state");
    expect(result.current).toHaveProperty("open");
    expect(result.current).toHaveProperty("setOpen");
    expect(result.current).toHaveProperty("toggleSidebar");
    expect(result.current.state).toBe("expanded");
    expect(result.current.open).toBe(true);
    expect(typeof result.current.setOpen).toBe("function");
    expect(typeof result.current.toggleSidebar).toBe("function");
  });

  it("throws when used outside SidebarProvider", () => {
    // Suppress React error boundary console output
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => {
      renderHook(() => useSidebar());
    }).toThrow("useSidebar must be used within a SidebarProvider.");
    consoleSpy.mockRestore();
  });
});
