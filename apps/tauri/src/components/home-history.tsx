import { ArrowRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { HistoryDays, type LatestNotes } from "@/components/history-row";
import type { HistoryPage } from "@/lib/history";
import { useHistoryActions } from "@/lib/use-history";

// How many of the newest dictations Home shows. The rest, with search
// and pages, is on the History screen.
export const HOME_HISTORY_COUNT = 5;

interface HomeHistoryProps {
  // The history switch. With it off Home still shows what is stored,
  // and says that new dictations are not saved.
  enabled: boolean;
  // The newest entries; owned by HomePane, which also needs the first.
  page: HistoryPage | null;
  error?: string | null;
  // The dictation that just finished, when it is the first entry.
  latest?: LatestNotes | null;
  onSeeAll?: () => void;
}

// The recent dictations on Home (TASK-95): the newest few, newest
// first, with copy and delete, and a way to the History screen.
// Renders nothing while history is off and nothing is stored.
export function HomeHistory({ enabled, page, error, latest = null, onSeeAll }: HomeHistoryProps) {
  const actions = useHistoryActions();

  if (!page) return null;
  if (page.stored === 0 && !enabled) return null;

  const failure = error ?? actions.error;

  return (
    <section className="ow-history" data-testid="home-history" aria-label="History">
      <div className="ow-history__head">
        <h2 className="ow-history__title">History</h2>
        {page.stored > 0 && onSeeAll && (
          <Button variant="ghost" size="sm" onClick={onSeeAll} data-testid="history-see-all">
            {page.stored > page.entries.length ? `See all ${page.stored}` : "Open history"}
            <ArrowRight data-icon="inline-end" />
          </Button>
        )}
      </div>

      {!enabled && (
        <p className="ow-history__note" data-testid="history-off-note">
          History is off. New dictations are not saved.
        </p>
      )}
      {failure && (
        <p className="ow-history__note" role="alert">
          Could not read the history: {failure}
        </p>
      )}

      {page.stored === 0 ? (
        <p className="ow-history__note" data-testid="history-empty">
          Nothing saved yet. Your next dictation shows up here.
        </p>
      ) : (
        <HistoryDays entries={page.entries} latest={latest} onDelete={actions.remove} />
      )}
    </section>
  );
}
