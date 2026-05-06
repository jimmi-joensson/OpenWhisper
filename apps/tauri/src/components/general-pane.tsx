import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  disable as autostartDisable,
  enable as autostartEnable,
  isEnabled as autostartIsEnabled,
} from "@tauri-apps/plugin-autostart";

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
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Separator } from "@/components/ui/separator";
import { useShowInFullscreen } from "@/lib/use-show-in-fullscreen";
import { useTheme } from "@/lib/use-theme";
import {
  USER_WPM_MAX,
  USER_WPM_MIN,
  useUserWpm,
} from "@/lib/use-user-wpm";

type PillSettings = { follow_active_screen: boolean };

export function GeneralPane() {
  // Hydrated from the platform autostart registration on mount, NOT from
  // local state — a user who enabled autostart in a previous run sees the
  // Switch reflect reality after a fresh boot. False is the safe default
  // before isEnabled() resolves: we'd rather render a momentary unchecked
  // Switch and flip it on than render checked and silently flip it off.
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const { theme, setTheme } = useTheme();
  const { enabled: showInFullscreen, setEnabled: setShowInFullscreen } =
    useShowInFullscreen();
  const [version, setVersion] = useState<string | null>(null);
  const [followActiveScreen, setFollowActiveScreen] = useState(true);
  const { wpm, setWpm } = useUserWpm();
  const [wpmDraft, setWpmDraft] = useState<string>(String(wpm));
  const [wpmFocused, setWpmFocused] = useState(false);
  const [resetOpen, setResetOpen] = useState(false);

  // Sync the input draft from the persisted value whenever the user
  // isn't actively editing — covers initial load + the
  // settings_stats_changed event firing after a successful save (the
  // Rust side may have clamped the value, in which case the input
  // snaps to the clamped number).
  useEffect(() => {
    if (!wpmFocused) {
      setWpmDraft(String(wpm));
    }
  }, [wpm, wpmFocused]);

  const commitWpm = () => {
    setWpmFocused(false);
    const parsed = Number.parseInt(wpmDraft, 10);
    if (!Number.isFinite(parsed)) {
      setWpmDraft(String(wpm));
      return;
    }
    void setWpm(parsed).catch((e) => {
      // eslint-disable-next-line no-console
      console.warn("settings_set_user_wpm failed", e);
      setWpmDraft(String(wpm));
    });
  };

  const handleResetStats = () => {
    setResetOpen(false);
    void invoke("stats_reset").catch((e) => {
      // eslint-disable-next-line no-console
      console.warn("stats_reset failed", e);
    });
  };

  useEffect(() => {
    invoke<string>("core_version")
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  useEffect(() => {
    autostartIsEnabled()
      .then(setLaunchAtLogin)
      .catch((e) => {
        // eslint-disable-next-line no-console
        console.warn("autostart isEnabled failed", e);
        setLaunchAtLogin(false);
      });
  }, []);

  useEffect(() => {
    invoke<PillSettings>("settings_get_pill")
      .then((s) => setFollowActiveScreen(s.follow_active_screen))
      .catch(() => setFollowActiveScreen(true));
  }, []);

  const onLaunchAtLoginChange = (next: boolean) => {
    setLaunchAtLogin(next);
    const apply = next ? autostartEnable() : autostartDisable();
    apply.catch((e) => {
      // eslint-disable-next-line no-console
      console.warn(`autostart ${next ? "enable" : "disable"} failed`, e);
      setLaunchAtLogin(!next);
    });
  };

  const onFollowChange = (next: boolean) => {
    setFollowActiveScreen(next);
    invoke("settings_set_pill_follow", { follow: next }).catch((e) => {
      // eslint-disable-next-line no-console
      console.warn("settings_set_pill_follow failed", e);
      setFollowActiveScreen(!next);
    });
  };

  return (
    <FieldGroup className="px-1 py-2">
      <SectionHeader>Startup</SectionHeader>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="launch-at-login">Launch at login</FieldLabel>
          <FieldDescription>
            OpenWhisper runs in the background, ready for your hotkey.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="launch-at-login"
          checked={launchAtLogin}
          onCheckedChange={onLaunchAtLoginChange}
        />
      </Field>

      <Separator />

      <SectionHeader>Appearance</SectionHeader>
      <Field orientation="horizontal">
        <FieldLabel>Theme</FieldLabel>
        <ToggleGroup
          value={[theme]}
          onValueChange={(v) => {
            const next = v[0];
            if (next === "system" || next === "light" || next === "dark") {
              setTheme(next);
            }
          }}
          variant="outline"
        >
          <ToggleGroupItem value="system">System</ToggleGroupItem>
          <ToggleGroupItem value="light">Light</ToggleGroupItem>
          <ToggleGroupItem value="dark">Dark</ToggleGroupItem>
        </ToggleGroup>
      </Field>

      <Separator />

      <SectionHeader>Behavior</SectionHeader>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="show-in-fullscreen">
            Show in fullscreen apps
          </FieldLabel>
          <FieldDescription>
            Keeps the pill visible and the hotkey active even when another
            app is in fullscreen. Off by default — most users want
            OpenWhisper to step aside for games and video.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="show-in-fullscreen"
          checked={showInFullscreen}
          onCheckedChange={(next) => {
            void setShowInFullscreen(next);
          }}
        />
      </Field>

      <Separator />

      <SectionHeader>Pill</SectionHeader>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="follow-active-screen">
            Follow active screen
          </FieldLabel>
          <FieldDescription>
            Pill jumps to whichever screen has the focused app.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="follow-active-screen"
          checked={followActiveScreen}
          onCheckedChange={onFollowChange}
        />
      </Field>

      <Separator />

      <SectionHeader>Stats</SectionHeader>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="user-wpm">Typing speed</FieldLabel>
          <FieldDescription>
            Used for the Time Saved estimate on Home. {USER_WPM_MIN}–
            {USER_WPM_MAX} wpm; default 40 is an average adult baseline.
          </FieldDescription>
        </FieldContent>
        <Input
          id="user-wpm"
          type="number"
          min={USER_WPM_MIN}
          max={USER_WPM_MAX}
          step={1}
          value={wpmDraft}
          onFocus={() => setWpmFocused(true)}
          onChange={(e) => setWpmDraft(e.currentTarget.value)}
          onBlur={commitWpm}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.currentTarget.blur();
            }
          }}
          className="w-20 text-right tabular-nums"
          inputMode="numeric"
          aria-label="Typing speed in words per minute"
        />
      </Field>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Reset all stats</FieldLabel>
          <FieldDescription>
            Permanently clears word counts and Time Saved. Cannot be undone.
          </FieldDescription>
        </FieldContent>
        <AlertDialog open={resetOpen} onOpenChange={setResetOpen}>
          <AlertDialogTrigger
            render={
              <Button
                variant="destructive"
                size="sm"
                data-testid="stats-reset-trigger"
              >
                Reset stats…
              </Button>
            }
          />
          <AlertDialogPortal>
            <AlertDialogOverlay />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Reset all stats?</AlertDialogTitle>
                <AlertDialogDescription>
                  This permanently deletes every recorded dictation row.
                  Your Today / Week / All-time counters and Time Saved
                  total all return to zero. This cannot be undone.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Cancel</AlertDialogCancel>
                <AlertDialogAction
                  variant="destructive"
                  onClick={handleResetStats}
                  data-testid="stats-reset-confirm"
                >
                  Reset stats
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialogPortal>
        </AlertDialog>
      </Field>

      <Separator />

      <SectionHeader>Updates</SectionHeader>
      <Field orientation="horizontal">
        <FieldLabel>Current version</FieldLabel>
        <span className="font-mono text-sm">{version ?? "—"}</span>
      </Field>
    </FieldGroup>
  );
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
      {children}
    </h3>
  );
}

