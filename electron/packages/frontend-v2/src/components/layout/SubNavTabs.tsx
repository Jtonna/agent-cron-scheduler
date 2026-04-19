interface Tab {
  id: string;
  label: string;
  closable?: boolean;
}

interface SubNavTabsProps {
  tabs: Tab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
  onTabClose?: (tabId: string) => void;
}

export function SubNavTabs({
  tabs,
  activeTab,
  onTabChange,
  onTabClose,
}: SubNavTabsProps) {
  return (
    <div className="sticky top-[49px] z-40 bg-background border-b border-border">
      <div className="flex items-end overflow-x-auto">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTab;
          return (
            <div
              key={tab.id}
              className={`relative flex items-center gap-1 px-4 py-2.5 text-sm cursor-pointer select-none transition-colors whitespace-nowrap ${
                isActive
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
              onClick={() => onTabChange(tab.id)}
              role="tab"
              aria-selected={isActive}
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onTabChange(tab.id);
                }
              }}
            >
              <span>{tab.label}</span>

              {tab.closable && onTabClose && (
                <button
                  type="button"
                  className="ml-1 flex items-center justify-center size-4 rounded text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                  aria-label={`Close ${tab.label}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onTabClose(tab.id);
                  }}
                >
                  ×
                </button>
              )}

              {/* Active indicator */}
              {isActive && (
                <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary" />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
