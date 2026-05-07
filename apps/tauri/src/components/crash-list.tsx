import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import { Button } from "./ui/button";
import { CrashRow } from "./crash-row";
import { CrashDetailSheet } from "./crash-detail-sheet";
import { useCrashes } from "../lib/use-crashes";

const CRASHES_DIR_HINT_MAC = "~/Library/Logs/OpenWhisper/crashes/";

export interface CrashListProps {
  /// Back to the Diagnostics overview pane. Owned by `DiagnosticsPane`
  /// so the breadcrumb advances local view-state, not a router-level
  /// route — per the design pivot, no nested rail.
  onBack: () => void;
}

/// Full-pane crash list — pane header (breadcrumb + counts +
/// Delete-all) + scrollable list. Empty state replaces the entire
/// pane. Polls `crashes_list` + `crashes_unread_count` at 2 Hz while
/// mounted.
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
      <div
        className="ow-crashes ow-crashes--empty"
        data-testid="crash-list-empty"
      >
        <CrashEmpty onBack={onBack} />
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
            <ArrowLeft size={14} aria-hidden="true" />
            Diagnostics
          </button>
          <span className="ow-crashes__breadcrumb-sep">/</span>
          <span className="ow-crashes__breadcrumb-current">Crashes</span>
          <span className="ow-crashes__breadcrumb-counts" data-testid="crash-list-counts">
            {unreadCount} unread · {total} total
          </span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="ow-crashes__delete-all"
          data-testid="crash-list-delete-all"
          onClick={() => setConfirmDeleteAll(true)}
        >
          Delete all
        </Button>
      </header>

      <ul className="ow-crashes__list">
        {list.map((crash) => (
          <li key={crash.id} className="ow-crashes__list-item">
            <CrashRow
              crash={crash}
              onOpen={(id) => setOpenId(id)}
              onMarkRead={(id) => void markRead(id)}
              onDelete={(id) => void deleteOne(id)}
            />
          </li>
        ))}
      </ul>

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
      />

      <AlertDialog
        open={confirmDeleteAll}
        onOpenChange={setConfirmDeleteAll}
      >
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

/// Empty-state composition — replaces the entire pane (no header).
function CrashEmpty({ onBack }: { onBack: () => void }) {
  const onOpenFolder = () => {
    // The Tauri opener plugin needs a real path. We don't have the
    // resolved app log dir on the React side — invoke goes through
    // the plugin's `open_path` and lets the shell's command resolve
    // it from app_log_dir(). The error path is intentionally swallowed
    // (clicking the button on an unresolvable path shouldn't blow up).
    void invoke("plugin:opener|open_path", {
      path: CRASHES_DIR_HINT_MAC,
    }).catch(() => {});
  };

  return (
    <div className="ow-crashes-empty" data-testid="crash-list-empty-body">
      <div className="ow-crashes-empty__tile" aria-hidden="true">
        <span className="ow-crashes-empty__glyph">!</span>
      </div>
      <h2 className="ow-crashes-empty__title">No crashes recorded</h2>
      <p className="ow-crashes-empty__caption">
        We log crashes to <code>{CRASHES_DIR_HINT_MAC}</code> so you can read
        or delete them yourself.
      </p>
      <div className="ow-crashes-empty__actions">
        <Button variant="ghost" size="sm" onClick={onBack}>
          ← Diagnostics
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onOpenFolder}
          data-testid="crash-list-open-folder"
        >
          Open crash folder
        </Button>
      </div>
    </div>
  );
}
