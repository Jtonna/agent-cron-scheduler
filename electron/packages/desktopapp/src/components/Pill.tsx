import { ToggleButton } from "react-aria-components";

interface PillProps {
  label: string;
  active?: boolean;
  onChange?: (isSelected: boolean) => void;
}

export function Pill({ label, active, onChange }: PillProps) {
  return (
    <ToggleButton
      isSelected={active}
      onChange={onChange}
      className={`px-3 py-1 text-xs rounded-pill border cursor-pointer transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring ${
        active
          ? "text-fg border-border-active bg-surface-secondary"
          : "text-fg-muted bg-transparent border-border-strong hover:border-fg-subtle hover:text-fg-secondary"
      }`}
    >
      {label}
    </ToggleButton>
  );
}
