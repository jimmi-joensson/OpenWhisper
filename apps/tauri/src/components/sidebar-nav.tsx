import { Activity, Home, Settings as SettingsIcon } from "lucide-react";

export type Route = "home" | "settings" | "diagnostics";

interface SidebarNavProps {
  active: Route;
  onSelect: (route: Route) => void;
}

const ITEMS: ReadonlyArray<{ id: Route; label: string; Icon: typeof Home }> = [
  { id: "home", label: "Home", Icon: Home },
  { id: "settings", label: "Settings", Icon: SettingsIcon },
  { id: "diagnostics", label: "Diagnostics", Icon: Activity },
];

export function SidebarNav({ active, onSelect }: SidebarNavProps) {
  return (
    <nav className="ow-sidebar" aria-label="Primary">
      {ITEMS.map(({ id, label, Icon }) => (
        <button
          key={id}
          type="button"
          data-testid={`sidebar-item-${id}`}
          className={
            "ow-sidebar__item" +
            (active === id ? " ow-sidebar__item--active" : "")
          }
          aria-current={active === id ? "page" : undefined}
          onClick={() => onSelect(id)}
        >
          <Icon size={16} aria-hidden="true" />
          <span>{label}</span>
        </button>
      ))}
    </nav>
  );
}
