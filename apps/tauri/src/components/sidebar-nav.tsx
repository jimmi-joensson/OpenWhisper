import { useCallback, useRef } from "react";
import {
  Activity,
  Boxes,
  Home,
  Keyboard,
  Mic,
  Settings as SettingsIcon,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { SETTINGS_PANES, type SettingsPaneId } from "../lib/settings-panes";
import { useCrashesUnreadCount } from "../lib/use-crashes";

const SETTINGS_PANE_ICONS: Record<SettingsPaneId, LucideIcon> = {
  general: SlidersHorizontal,
  audio: Mic,
  models: Boxes,
  shortcuts: Keyboard,
};

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
  // Persistent rail badge: the Diagnostics item carries a small
  // recording-orange dot whenever an unread crash exists, even if
  // the user is on a different route. Per spec the dot is never
  // auto-dismissed by visiting Diagnostics — only by opening each
  // unread crash (which calls `crashes_mark_read`). Polling lives
  // here in the sidebar so it runs everywhere the rail does.
  const unreadCrashes = useCrashesUnreadCount();

  return (
    <nav className="ow-sidebar" aria-label="Primary">
      {ROUTE_ITEMS.map(({ id, label, Icon }) => {
        const showCrashDot = id === "diagnostics" && unreadCrashes > 0;
        return (
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
            {showCrashDot && (
              <span
                className="ow-sidebar__badge-dot"
                data-testid="sidebar-diagnostics-unread-dot"
                aria-label={`${unreadCrashes} unread crash report${unreadCrashes === 1 ? "" : "s"}`}
              />
            )}
          </button>
        );
      })}
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
      {SETTINGS_PANES.map((p) => {
        const Icon = SETTINGS_PANE_ICONS[p.id];
        return (
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
            <Icon size={16} aria-hidden="true" />
            <span>{p.label}</span>
          </button>
        );
      })}
    </div>
  );
}
