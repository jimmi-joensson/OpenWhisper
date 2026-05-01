import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  disable as autostartDisable,
  enable as autostartEnable,
  isEnabled as autostartIsEnabled,
} from "@tauri-apps/plugin-autostart";

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
import { usePauseAudio } from "@/lib/use-pause-audio";
import { useShowInFullscreen } from "@/lib/use-show-in-fullscreen";
import { useTheme } from "@/lib/use-theme";

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
  const { enabled: pauseAudio, setEnabled: setPauseAudio } = usePauseAudio();
  const [version, setVersion] = useState<string | null>(null);
  const [followActiveScreen, setFollowActiveScreen] = useState(true);

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

      <SectionHeader>Audio</SectionHeader>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="pause-audio">
            Pause audio during dictation
          </FieldLabel>
          <FieldDescription>
            Pauses Spotify, browser playback, and other media when you
            start recording, then resumes when recording ends. Falls back
            to muting system output for apps that don't support media
            controls.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="pause-audio"
          checked={pauseAudio}
          onCheckedChange={(next) => {
            void setPauseAudio(next);
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
