import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

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

export function GeneralPane() {
  const [launchAtLogin, setLaunchAtLogin] = useState(true);
  const { theme, setTheme } = useTheme();
  const { enabled: showInFullscreen, setEnabled: setShowInFullscreen } =
    useShowInFullscreen();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("core_version")
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

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
          onCheckedChange={setLaunchAtLogin}
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
