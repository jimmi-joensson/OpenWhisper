import { useState } from "react";
import { ArrowLeft } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "./ui/alert-dialog";
import { CrashRow } from "./crash-row";
import { CrashEmpty } from "./crash-empty";
import { CrashDetailSheet } from "./crash-detail-sheet";
import { useCrashes } from "../lib/use-crashes";

export interface CrashListProps {
  /// Back to the Diagnostics overview pane. Owned by `DiagnosticsPane`
  /// — local view-state, no router-level change.
  onBack: () => void;
}

/// Full-pane crash list: pane header (back link + two-line counts +
/// Delete-all destructive-ghost) followed by a borderless list of
/// rows on the surface-sunken backdrop. Empty state replaces the
/// entire pane (no header).
export function CrashList({ onBack }: CrashListProps) {
  const { list, unreadCount, loading, error, deleteOne, deleteAll, markRead, read } =
    useCrashes(true);
  const [openId, setOpenId] = useState<string | null>(null);
  const [confirmDeleteAll, setConfirmDeleteAll] = useState(false);

  const total = list.length;
  const isEmpty = !loading && total === 0;

  const onDeleteAll = () => {
    void deleteAll().finally(() => setConfirmDeleteAll(false));
  };

  if (isEmpty) {
    return (
      <div className="ow-crashes ow-crashes--empty" data-testid="crash-list-empty">
        <CrashEmpty />
      </div>
    );
  }

  return (
    <div className="ow-crashes" data-testid="crash-list">
      <header className="ow-crashes__header">
        <div className="ow-crashes__breadcrumb">
          <button
            type="button"
            className="ow-crashes__back"
            data-testid="crash-list-back"
            aria-label="Back to Diagnostics overview"
            onClick={onBack}
          >
            <ArrowLeft size={12} aria-hidden="true" />
            Diagnostics
          </button>
          <span className="ow-crashes__breadcrumb-sep">/</span>
          <div className="ow-crashes__breadcrumb-block">
            <div className="ow-crashes__breadcrumb-kicker">Crashes</div>
            <div
              className="ow-crashes__breadcrumb-counts"
              data-testid="crash-list-counts"
            >
              {unreadCount > 0 && (
                <>
                  <span className="ow-crashes__breadcrumb-counts-strong">
                    {unreadCount} unread
                  </span>
                  <span className="ow-crashes__breadcrumb-counts-sep"> · </span>
                </>
              )}
              <span className="ow-crashes__breadcrumb-counts-mute">
                {total} total
              </span>
            </div>
          </div>
        </div>
        <button
          type="button"
          className="ow-crashes__delete-all"
          data-testid="crash-list-delete-all"
          onClick={() => setConfirmDeleteAll(true)}
        >
          Delete all
        </button>
      </header>

      <div className="ow-crashes__list">
        {list.map((crash, i) => (
          <CrashRow
            key={crash.id}
            crash={crash}
            first={i === 0}
            onOpen={(id) => setOpenId(id)}
            onMarkRead={(id) => void markRead(id)}
            onDelete={(id) => void deleteOne(id)}
          />
        ))}
      </div>

      {error && (
        <p className="ow-crashes__error" data-testid="crash-list-error">
          {error}
        </p>
      )}

      <CrashDetailSheet
        openId={openId}
        onOpenChange={(o) => {
          if (!o) setOpenId(null);
        }}
        read={read}
        markRead={markRead}
        deleteOne={deleteOne}
      />

      <AlertDialog open={confirmDeleteAll} onOpenChange={setConfirmDeleteAll}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete all crash reports?</AlertDialogTitle>
            <AlertDialogDescription>
              {unreadCount} unread will be removed too. This can't be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              data-testid="crash-list-delete-all-confirm"
              onClick={onDeleteAll}
            >
              Delete {total} reports
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
