interface PillProps {
  label: string;
  active?: boolean;
  onClick?: () => void;
}

export function Pill({ label, active, onClick }: PillProps) {
  return (
    <span
      onClick={onClick}
      className={`px-3 py-1 text-xs rounded-full border cursor-pointer transition-colors ${
        active
          ? "text-gray-900 border-gray-900 bg-gray-50"
          : "text-gray-500 bg-transparent border-gray-300 hover:border-gray-400 hover:text-gray-700"
      }`}
    >
      {label}
    </span>
  );
}
