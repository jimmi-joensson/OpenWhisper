import { useCallback, useRef } from "react";
import { Activity, Home, Settings as SettingsIcon } from "lucide-react";
import { SETTINGS_PANES, type SettingsPaneId } from "../lib/settings-panes";

export type Route = "home" | "settings" | "diagnostics";

interface SidebarNavProps {
  route: Route;
  onRouteSelect: (route: Route) => void;
  settingsPane: SettingsPaneId;
  onSettingsPaneSelect: (pane: SettingsPaneId) => void;
}

const ROUTE_ITEMS: ReadonlyArray<{ id: Route; label: string; Icon: typeof Home }> = [
  { id: "home", label: "Home", Icon: Home },
  { id: "settings", label: "Settings", Icon: SettingsIcon },
  { id: "diagnostics", label: "Diagnostics", Icon: Activity },
];

export function SidebarNav(props: SidebarNavProps) {
  if (props.route === "settings") {
    return (
      <SettingsPaneSidebar
        active={props.settingsPane}
        onSelect={props.onSettingsPaneSelect}
      />
    );
  }
  return <RouteSidebar active={props.route} onSelect={props.onRouteSelect} />;
}

function RouteSidebar({
  active,
  onSelect,
}: {
  active: Route;
  onSelect: (route: Route) => void;
}) {
  return (
    <nav className="ow-sidebar" aria-label="Primary">
      {ROUTE_ITEMS.map(({ id, label, Icon }) => (
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

// Settings mode: tablist semantics so the existing settings sub-sidebar
// tests (role=tab + aria-selected + ArrowDown/Up cycle) keep working.
function SettingsPaneSidebar({
  active,
  onSelect,
}: {
  active: SettingsPaneId;
  onSelect: (pane: SettingsPaneId) => void;
}) {
  const sidebarRef = useRef<HTMLDivElement>(null);

  const onKey = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const idx = SETTINGS_PANES.findIndex((p) => p.id === active);
      const next =
        e.key === "ArrowDown"
          ? SETTINGS_PANES[(idx + 1) % SETTINGS_PANES.length]
          : SETTINGS_PANES[(idx - 1 + SETTINGS_PANES.length) % SETTINGS_PANES.length];
      onSelect(next.id);
      requestAnimationFrame(() => {
        const node = sidebarRef.current?.querySelector<HTMLButtonElement>(
          `[data-pane="${next.id}"]`,
        );
        node?.focus();
      });
    },
    [active, onSelect],
  );

  return (
    <div
      ref={sidebarRef}
      className="ow-sidebar"
      role="tablist"
      aria-orientation="vertical"
      onKeyDown={onKey}
    >
      {SETTINGS_PANES.map((p) => (
        <button
          key={p.id}
          type="button"
          data-pane={p.id}
          data-testid={`settings-pane-${p.id}`}
          role="tab"
          aria-selected={active === p.id}
          tabIndex={active === p.id ? 0 : -1}
          className={
            "ow-sidebar__item" +
            (active === p.id ? " ow-sidebar__item--active" : "")
          }
          onClick={() => onSelect(p.id)}
        >
          <span className="ow-sidebar__icon-text" aria-hidden="true">
            {p.icon}
          </span>
          <span>{p.label}</span>
        </button>
      ))}
    </div>
  );
}
