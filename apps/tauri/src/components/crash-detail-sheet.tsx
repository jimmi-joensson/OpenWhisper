import { useEffect, useState } from "react";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "./ui/sheet";
import { type CrashFile, type UseCrashesResult } from "../lib/use-crashes";

export interface CrashDetailSheetProps {
  openId: string | null;
  onOpenChange: (open: boolean) => void;
  read: UseCrashesResult["read"];
  markRead: UseCrashesResult["markRead"];
}

/// Right-side detail sheet, ~580 px wide. Owns:
/// - Mark-read on open (per AC #3 — opening the sheet IS the read action).
/// - The crash-file fetch keyed off `openId`.
///
/// v1 (TASK-78.3) renders a placeholder body that proves the open/close
/// contract end-to-end; TASK-78.4 fills in the identity / backtrace /
/// events / footer regions. Keeping the placeholder explicit (rather
/// than rendering nothing) lets manual / Playwright verification of
/// the open + mark-read flow happen before 78.4 ships.
export function CrashDetailSheet({
  openId,
  onOpenChange,
  read,
  markRead,
}: CrashDetailSheetProps) {
  const [crash, setCrash] = useState<CrashFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const open = openId !== null;

  useEffect(() => {
    if (openId === null) {
      setCrash(null);
      setError(null);
      return;
    }
    let cancelled = false;
    // Mark-read fires once on open per AC #3. Closing does not un-read.
    void markRead(openId).catch(() => {});
    void read(openId)
      .then((file) => {
        if (cancelled) return;
        setCrash(file);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [openId, read, markRead]);

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="right"
        className="ow-crashes__sheet"
        data-testid="crash-detail-sheet"
      >
        <SheetHeader>
          <SheetTitle>Crash report</SheetTitle>
          <SheetDescription>
            Detail body lands in TASK-78.4 — opening the sheet has already
            marked this crash as read.
          </SheetDescription>
        </SheetHeader>
        {error && (
          <p className="ow-crashes__sheet-error" data-testid="crash-detail-error">
            {error}
          </p>
        )}
        {crash && (
          <div className="ow-crashes__sheet-placeholder">
            <p className="ow-crashes__sheet-message">
              {crash.rust_panic.message}
            </p>
            <p className="ow-crashes__sheet-meta">
              {crash.app_version} · {crash.os} · {crash.rust_panic.location}
            </p>
          </div>
        )}
      </SheetContent>
    </Sheet>
  );
}
