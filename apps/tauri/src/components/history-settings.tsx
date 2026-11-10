import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { RETENTION_OPTIONS, retentionLabel } from "@/lib/history";
import { useHistorySettings } from "@/lib/use-history";

// Settings → General → History (TASK-95): the switch, how long text
// is kept, and a plain statement of what is stored and where, with
// the folder one click away. The list itself, with delete and clear,
// lives on Home and the History screen.
export function HistorySettingsSection() {
  const { settings, folder, error, update } = useHistorySettings();
  const custom = Boolean(settings?.folder);

  const chooseFolder = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Folder for your dictation history",
      });
      if (typeof picked === "string" && picked) update({ folder: picked });
    } catch {
      // Picker unavailable or dismissed: nothing changes.
    }
  };
  const days = settings?.retention_days ?? 30;
  // A retention set from the CLI may not be one of the four choices.
  const items = RETENTION_OPTIONS.some((o) => o.value === String(days))
    ? RETENTION_OPTIONS
    : [...RETENTION_OPTIONS, { value: String(days), label: retentionLabel(days) }];

  return (
    <>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="history-enabled">Keep transcript history</FieldLabel>
          <FieldDescription>
            Saves the text of each dictation, the text before Refine changed
            it, and which app it went into, as plain Markdown files, one per
            day. They stay on this computer, and other programs (a coding
            agent, a notes app) can read them. Off by default; switching it
            off stops saving and leaves what is already stored.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="history-enabled"
          data-testid="history-enabled-switch"
          checked={settings?.enabled ?? false}
          disabled={settings === null}
          onCheckedChange={(enabled) => update({ enabled })}
        />
      </Field>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Keep text for</FieldLabel>
          <FieldDescription>
            Older text is removed automatically, also while history is off.
            Word counts and Time Saved stay. Single dictations can be deleted
            on Home.
          </FieldDescription>
        </FieldContent>
        <Select
          items={items}
          value={String(days)}
          disabled={settings === null}
          onValueChange={(v) => {
            const next = Number.parseInt(String(v), 10);
            if (Number.isFinite(next) && next >= 0) {
              update({ retention_days: next });
            }
          }}
        >
          <SelectTrigger
            className="w-[140px]"
            aria-label="Keep text for"
            data-testid="history-retention"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="end">
            <SelectGroup>
              {items.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Folder</FieldLabel>
          <FieldDescription>
            <span className="ow-selectable font-mono break-all" data-testid="history-location">
              {folder}
            </span>
            {custom && (
              <span className="mt-1 block" data-testid="history-folder-note">
                If this folder is synced (iCloud, Dropbox, a notes app), your
                dictations are synced with it. Files already written are not
                moved.
              </span>
            )}
          </FieldDescription>
        </FieldContent>
        <div className="flex shrink-0 flex-wrap justify-end gap-2">
          <Button
            variant="outline"
            size="sm"
            data-testid="history-reveal"
            disabled={!folder}
            onClick={() => void invoke("history_reveal").catch(() => {})}
          >
            Open
          </Button>
          <Button
            variant="outline"
            size="sm"
            data-testid="history-choose-folder"
            disabled={settings === null}
            onClick={() => void chooseFolder()}
          >
            Change…
          </Button>
          {custom && (
            <Button
              variant="ghost"
              size="sm"
              data-testid="history-default-folder"
              onClick={() => update({ folder: null })}
            >
              Use default
            </Button>
          )}
        </div>
      </Field>

      {error && (
        <p className="text-xs text-destructive" role="alert">
          Could not save the history setting: {error}
        </p>
      )}
    </>
  );
}
