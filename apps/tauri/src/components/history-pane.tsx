import { useEffect, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogOverlay,
  AlertDialogPortal,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { HistoryDays } from "@/components/history-row";
import {
  useDebounced,
  useHistoryActions,
  useHistoryPage,
  useHistorySettings,
} from "@/lib/use-history";

// Entries per page on the History screen.
export const HISTORY_PAGE_SIZE = 25;

// How long typing in the search field may pause before the list
// follows.
const SEARCH_DEBOUNCE_MS = 200;

interface HistoryPaneProps {
  onOpenSettings?: () => void;
}

// The History screen (TASK-95): every stored dictation, newest first,
// grouped by day, with search, pages, copy, delete, show original and
// clear all.
export function HistoryPane({ onOpenSettings }: HistoryPaneProps) {
  const { settings } = useHistorySettings();
  const [search, setSearch] = useState("");
  const query = useDebounced(search.trim(), SEARCH_DEBOUNCE_MS);
  const [pageIndex, setPageIndex] = useState(0);
  const [clearOpen, setClearOpen] = useState(false);
  const { page, stale, error } = useHistoryPage(query, pageIndex, HISTORY_PAGE_SIZE);
  const actions = useHistoryActions();

  // A new search starts at its first page.
  useEffect(() => {
    setPageIndex(0);
  }, [query]);

  const pageCount = page ? Math.max(1, Math.ceil(page.matched / HISTORY_PAGE_SIZE)) : 1;
  // Deleting the last entry of the last page leaves that page empty;
  // step back to the one that still has entries.
  useEffect(() => {
    if (page && !stale && pageIndex > pageCount - 1) {
      setPageIndex(pageCount - 1);
    }
  }, [page, stale, pageIndex, pageCount]);

  const enabled = settings?.enabled ?? false;
  const failure = error ?? actions.error;

  return (
    <div className="ow-history-pane" data-testid="history-pane">
      <header className="ow-history-pane__header">
        <div>
          <h1 className="ow-history-pane__title">History</h1>
          <p className="ow-history-pane__sub">
            The text of your dictations, kept on this computer only.
          </p>
        </div>
        {page && page.stored > 0 && (
          <Input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.currentTarget.value)}
            placeholder="Search"
            aria-label="Search history"
            data-testid="history-search"
            className="ow-history-pane__search"
          />
        )}
      </header>

      {settings && !enabled && (
        <p className="ow-history__note" data-testid="history-off-note">
          History is off. New dictations are not saved.{" "}
          {onOpenSettings && (
            <button
              type="button"
              className="ow-transcript-row__refine-toggle"
              onClick={onOpenSettings}
              data-testid="history-open-settings"
            >
              History settings
            </button>
          )}
        </p>
      )}
      {failure && (
        <p className="ow-history__note" role="alert">
          Could not read the history: {failure}
        </p>
      )}

      {!page ? null : page.stored === 0 ? (
        <p className="ow-history__note" data-testid="history-empty">
          {enabled
            ? "Nothing saved yet. Your next dictation shows up here."
            : "Nothing is saved. Switch history on to keep your dictations."}
        </p>
      ) : page.matched === 0 ? (
        <p className="ow-history__note" data-testid="history-no-match">
          No dictation matches “{query}”.
        </p>
      ) : (
        <HistoryDays entries={page.entries} onDelete={actions.remove} />
      )}

      {page && page.stored > 0 && (
        <div className="ow-history__foot">
          <span className="ow-history__count" data-testid="history-count">
            {query ? `${page.matched} of ${page.stored} saved` : `${page.stored} saved`}
          </span>
          {pageCount > 1 && (
            <nav className="ow-history__pager" aria-label="History pages">
              <Button
                variant="outline"
                size="sm"
                disabled={pageIndex === 0}
                onClick={() => setPageIndex((i) => Math.max(0, i - 1))}
                data-testid="history-prev"
              >
                <ChevronLeft data-icon="inline-start" />
                Newer
              </Button>
              <span data-testid="history-page-label">
                Page {pageIndex + 1} of {pageCount}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={pageIndex >= pageCount - 1}
                onClick={() => setPageIndex((i) => Math.min(pageCount - 1, i + 1))}
                data-testid="history-next"
              >
                Older
                <ChevronRight data-icon="inline-end" />
              </Button>
            </nav>
          )}
          <AlertDialog open={clearOpen} onOpenChange={setClearOpen}>
            <AlertDialogTrigger
              render={
                <Button variant="ghost" size="sm" data-testid="history-clear-trigger">
                  Clear history
                </Button>
              }
            />
            <AlertDialogPortal>
              <AlertDialogOverlay />
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Clear the history?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This removes the text of all {page.stored} saved dictations
                    from this computer. Your word counts and Time Saved stay.
                    This cannot be undone.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction
                    variant="destructive"
                    data-testid="history-clear-confirm"
                    onClick={() => {
                      setClearOpen(false);
                      actions.clear();
                    }}
                  >
                    Clear history
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialogPortal>
          </AlertDialog>
        </div>
      )}
    </div>
  );
}
