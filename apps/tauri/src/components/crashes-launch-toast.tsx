import { useEffect, useState } from "react";

import { Alert, AlertAction, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { useCrashes } from "../lib/use-crashes";

/// Delta-driven launch notice for unread crash reports (TASK-78.5).
///
/// Renders when more unread crashes exist now than at the last "see"
/// event (entering the inspector or dismissing this notice). Strict
/// inequality — equal-or-lower restarts get only the persistent rail
/// dot. Both buttons acknowledge the delta:
///
/// - **View** routes to the Diagnostics overview (NOT the inspector;
///   entering the inspector is the explicit per-crash read action) +
///   marks seen. Mounting `DiagnosticsPane` itself doesn't auto-mark.
/// - **Dismiss** marks seen without navigating.
///
/// Hidden once dismissed within the session even if a fresh crash
/// lands, so a panic that fires while the app is open doesn't keep
/// re-popping the same notice. New crashes pump the rail dot in
/// real time; the launch-notice is a *boot-time* surface only.
export interface CrashesLaunchToastProps {
  /// Switch to the Diagnostics route. Provided by the App shell so
  /// this component doesn't need to know about routing internals.
  onView: () => void;
}

export function CrashesLaunchToast({ onView }: CrashesLaunchToastProps) {
  const { unreadCount, lastSeenUnreadCount, loading, markSeen } = useCrashes();
  const [dismissed, setDismissed] = useState(false);

  // Snapshot the boot-time delta the first time the store finishes
  // loading. Without this latch, the moment `markSeen()` lands and
  // the snapshot updates, the toast would unmount mid-animation —
  // and a subsequent new crash arriving during the same session
  // would re-mount the notice (which AC #2 calls out as wrong:
  // restart-time only).
  const [shouldShow, setShouldShow] = useState<boolean | null>(null);
  useEffect(() => {
    if (loading || shouldShow !== null) return;
    setShouldShow(unreadCount > lastSeenUnreadCount);
  }, [loading, unreadCount, lastSeenUnreadCount, shouldShow]);

  if (loading || shouldShow !== true || dismissed) return null;

  const handleView = () => {
    void markSeen().catch(() => {});
    setDismissed(true);
    onView();
  };

  const handleDismiss = () => {
    void markSeen().catch(() => {});
    setDismissed(true);
  };

  const label = unreadCount === 1 ? "1 new crash report" : `${unreadCount} new crash reports`;

  return (
    <Alert
      data-testid="crashes-launch-toast"
      className="ow-crashes-launch-toast"
    >
      <AlertTitle>{label}</AlertTitle>
      <AlertAction>
        <Button
          size="sm"
          variant="ghost"
          onClick={handleDismiss}
          data-testid="crashes-launch-toast-dismiss"
        >
          Dismiss
        </Button>
        <Button
          size="sm"
          variant="default"
          onClick={handleView}
          data-testid="crashes-launch-toast-view"
        >
          View
        </Button>
      </AlertAction>
    </Alert>
  );
}
