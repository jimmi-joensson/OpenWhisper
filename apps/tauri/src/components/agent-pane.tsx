import { useId, useMemo, useState } from "react";
import { ChevronRight, CircleAlert, Plus, RefreshCw, X } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  AGENT_STATE_LABELS,
  AGENT_TOOL_KINDS,
  agentAppLabel,
  toolLabel,
  type AgentAppConnection,
  type AgentChoices,
  type AgentConnection,
  type AgentCustomConnection,
  type AgentState,
  type AgentTool,
  type AgentToolKind,
} from "@/lib/agent";
import { formatClock } from "@/lib/history";
import type { SettingsPaneId } from "@/lib/settings-panes";
import { useAgent, useAgentRunningApps, useAgentStatus } from "@/lib/use-agent";
import { cn } from "@/lib/utils";

// Longest name core keeps (`AGENT_NAME_MAX_CHARS`), and the longest
// command, argument or URL (`AGENT_CUSTOM_FIELD_MAX_CHARS`).
const NAME_MAX_CHARS = 120;
const CUSTOM_FIELD_MAX_CHARS = 2000;

interface AgentPaneProps {
  // Opens another settings pane: Ask, which Agent works inside, and
  // Refine, where the backend is set up.
  onOpenPane?: (pane: SettingsPaneId) => void;
}

// Agent pane (TASK-99): the switch, drafts only, the connections an
// ask may use with their tools, and which app belongs to which
// connection.
//
// A thin view over Rust (`agent_get`, `agent_set`, `agent_refresh`,
// `agent_status`). Which tool is of which kind, what an ask may use
// and whether an ask uses tools at all are core's decisions; this
// pane shows them and sends back the person's choices. How a
// connection is reached never comes here.
export function AgentPane({ onOpenPane }: AgentPaneProps) {
  const { state, error, update, refresh, addConnection, removeConnection } = useAgent();
  const status = useAgentStatus();
  const [adding, setAdding] = useState(false);

  const enabled = state?.enabled ?? false;
  // The two things Agent needs besides its own switch. Said whether
  // or not Agent is on, so nobody switches it on and wonders.
  const askOff = status !== null && !status.ask_enabled;
  const otherBackend = status !== null && status.backend === "openai-compatible";

  return (
    <FieldGroup className="px-1 py-2">
      <SectionHeader>Agent</SectionHeader>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="agent-enabled">Agent</FieldLabel>
          <FieldDescription data-testid="agent-can">
            Lets an ask use connections to your other apps: the ones
            Claude Code on this computer already has, and only those
            you switch on below. It can look things up there and
            prepare drafts, so you can ask for a reply to the message
            in front of you.
          </FieldDescription>
          <FieldDescription data-testid="agent-cannot">
            It cannot use a connection or a tool you have not ticked.
            It cannot send, post, delete or change anything while
            Drafts only is on. It cannot open your files or run
            commands on this computer.
          </FieldDescription>
          <FieldDescription data-testid="agent-privacy">
            What a tool returns is sent to Claude together with your
            question. Every tool call is shown in the answer panel.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="agent-enabled"
          checked={enabled}
          disabled={!state}
          onCheckedChange={(on) => update((c) => ({ ...c, enabled: on }))}
        />
      </Field>

      {askOff && (
        <Alert data-testid="agent-needs-ask">
          <CircleAlert />
          <AlertTitle>Ask is switched off</AlertTitle>
          <AlertDescription>
            <p>Agent only works inside an ask. Until Ask is on, it does nothing.</p>
            {onOpenPane && (
              <Button
                variant="outline"
                size="sm"
                className="mt-2"
                onClick={() => onOpenPane("ask")}
              >
                Open Ask settings
              </Button>
            )}
          </AlertDescription>
        </Alert>
      )}
      {otherBackend && (
        <Alert data-testid="agent-needs-claude">
          <CircleAlert />
          <AlertTitle>Agent needs the Claude backend</AlertTitle>
          <AlertDescription>
            <p>
              Refine is set up with an OpenAI-compatible server, which
              cannot use connections. Until it is set up with Claude,
              Agent does nothing.
            </p>
            {onOpenPane && (
              <Button
                variant="outline"
                size="sm"
                className="mt-2"
                onClick={() => onOpenPane("refine")}
              >
                Open Refine settings
              </Button>
            )}
          </AlertDescription>
        </Alert>
      )}
      {/* What an ask would do right now, in core's words. Only while
          Agent is on and the two notes above do not already say why
          it does nothing. */}
      {enabled && status && !askOff && !otherBackend && (
        <FieldDescription data-testid="agent-status" data-ready={status.ready ? "true" : "false"}>
          {status.message}
        </FieldDescription>
      )}

      {error && (
        <FieldDescription className="text-destructive" role="alert">
          {error}
        </FieldDescription>
      )}

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="agent-drafts-only">Drafts only</FieldLabel>
          <FieldDescription>
            Nothing that sends, posts, deletes or changes anything can
            run. Looking things up and preparing drafts still work.
          </FieldDescription>
          <FieldDescription data-testid="agent-drafts-only-note">
            {state && !state.drafts_only
              ? `Off: a ticked tool that acts can run, but only after you allow it in the answer panel, every time, with everything it is about to do in front of you. No answer within ${state.confirm_timeout_s} seconds counts as no.`
              : "Switched off, a ticked tool that acts can run, but only after you allow it in the answer panel, every time."}
          </FieldDescription>
        </FieldContent>
        <Switch
          id="agent-drafts-only"
          checked={state?.drafts_only ?? true}
          disabled={!state}
          onCheckedChange={(on) => update((c) => ({ ...c, drafts_only: on }))}
        />
      </Field>

      <Separator />

      {state && (
        <ConnectionsSection
          state={state}
          disabled={!enabled}
          update={update}
          onRefresh={refresh}
          onAdd={() => setAdding(true)}
          onRemove={removeConnection}
        />
      )}

      <Separator />

      {state && <AppsSection state={state} disabled={!enabled} update={update} />}

      <ConnectionDialog open={adding} onOpenChange={setAdding} onAdd={addConnection} />
    </FieldGroup>
  );
}

type Update = (change: (choices: AgentChoices) => AgentChoices) => void;

// The connections: what Claude Code has and the person's own, each
// with its switch and, opened, its tools by kind.
function ConnectionsSection({
  state,
  disabled,
  update,
  onRefresh,
  onAdd,
  onRemove,
}: {
  state: AgentState;
  // Agent is switched off.
  disabled: boolean;
  update: Update;
  onRefresh: () => void;
  onAdd: () => void;
  onRemove: (name: string) => void;
}) {
  const [filter, setFilter] = useState("");
  // The connections whose tools are showing, by name. All closed to
  // begin with: one setup can have hundreds of tools.
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const { listing } = state;

  // By name, so a row keeps its place when it is switched on.
  const sorted = useMemo(
    () =>
      [...state.connections].sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
      ),
    [state.connections],
  );
  const needle = filter.trim().toLowerCase();
  const shown = sorted.filter(
    (c) => needle === "" || nameMatches(c.name, needle) || c.tools.some((t) => toolMatches(t, needle)),
  );
  const on = state.connections.filter((c) => c.enabled).length;

  const setOpenFor = (name: string, show: boolean) =>
    setOpen((before) => {
      const next = new Set(before);
      if (show) next.add(name);
      else next.delete(name);
      return next;
    });

  const change = (name: string, patch: { enabled?: boolean; allowed_tools?: string[] }) =>
    update((c) => ({
      ...c,
      connections: c.connections.map((x) => (x.name === name ? { ...x, ...patch } : x)),
    }));

  return (
    <>
      <SectionHeader>Connections</SectionHeader>
      <FieldDescription data-testid="agent-connections-intro">
        Nothing is switched on until you switch it on. Looking the
        connections up starts Claude Code for a few seconds. No tool
        is called and the model is not asked anything.
      </FieldDescription>
      {disabled && (
        <FieldDescription data-testid="agent-off-note">
          Agent is switched off, so no ask uses a connection. Switch
          Agent on to choose connections and tools.
        </FieldDescription>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={disabled || listing.refreshing}
          data-testid="agent-refresh"
          onClick={onRefresh}
        >
          {listing.refreshing ? (
            <Spinner data-icon="inline-start" className="motion-reduce:animate-none" />
          ) : (
            <RefreshCw data-icon="inline-start" />
          )}
          {listing.refreshing ? "Looking up" : "Refresh"}
        </Button>
        <Button variant="outline" size="sm" disabled={disabled} onClick={onAdd}>
          <Plus data-icon="inline-start" />
          Add connection
        </Button>
        <span
          className="text-xs text-muted-foreground tabular-nums"
          role="status"
          data-testid="agent-listing-state"
        >
          {listing.refreshing
            ? "Asking Claude Code which connections it has."
            : listing.refreshed_at_ms !== null
              ? `Looked up at ${formatClock(listing.refreshed_at_ms)}. ${on} of ${state.connections.length} switched on.`
              : "Not looked up since the app started."}
        </span>
      </div>

      {listing.error && (
        <Alert variant="destructive" data-testid="agent-listing-error">
          <CircleAlert />
          <AlertTitle>The connections could not be looked up</AlertTitle>
          <AlertDescription>{listing.error}</AlertDescription>
        </Alert>
      )}
      {listing.custom_error && (
        <FieldDescription className="text-destructive" data-testid="agent-custom-error">
          Your own connections could not be looked up: {listing.custom_error}
        </FieldDescription>
      )}

      {state.connections.length === 0 ? (
        <Empty className="border" data-testid="agent-connections-empty">
          <EmptyHeader>
            <EmptyTitle>No connections yet</EmptyTitle>
            <EmptyDescription>
              Press Refresh to look up the connections Claude Code has
              on this computer, or add one of your own.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <>
          {/* Control choice: free text that narrows a long list. One
              setup has some thirty connections and several hundred
              tools; a filter finds "draft" faster than scrolling. */}
          <Input
            aria-label="Filter connections and tools"
            placeholder="Filter by connection or tool"
            value={filter}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            onChange={(e) => setFilter(e.target.value)}
          />
          {shown.length === 0 ? (
            <FieldDescription data-testid="agent-filter-empty">
              No connection or tool matches.
            </FieldDescription>
          ) : (
            <ul
              className="flex flex-col gap-2"
              aria-label="Connections"
              data-testid="agent-connection-list"
            >
              {shown.map((connection) => (
                <ConnectionRow
                  key={`${connection.kind}:${connection.name}`}
                  connection={connection}
                  draftsOnly={state.drafts_only}
                  disabled={disabled}
                  // A match on the connection's own name shows all
                  // of its tools; otherwise only the tools that match.
                  needle={nameMatches(connection.name, needle) ? "" : needle}
                  open={open.has(connection.name)}
                  onOpenChange={(show) => setOpenFor(connection.name, show)}
                  onEnabledChange={(enabled) => {
                    change(connection.name, { enabled });
                    // Switched on: the next thing to do is tick tools.
                    if (enabled && connection.tools.length > 0) setOpenFor(connection.name, true);
                  }}
                  onToolsChange={(allowed_tools) => change(connection.name, { allowed_tools })}
                  onRemove={() => onRemove(connection.name)}
                />
              ))}
            </ul>
          )}
        </>
      )}
    </>
  );
}

function nameMatches(name: string, needle: string): boolean {
  return name.toLowerCase().includes(needle);
}

function toolMatches(tool: AgentTool, needle: string): boolean {
  return tool.name.toLowerCase().includes(needle) || tool.title.toLowerCase().includes(needle);
}

function ConnectionRow({
  connection,
  draftsOnly,
  disabled,
  needle,
  open,
  onOpenChange,
  onEnabledChange,
  onToolsChange,
  onRemove,
}: {
  connection: AgentConnection;
  draftsOnly: boolean;
  // Agent is switched off.
  disabled: boolean;
  // Lower case; "" shows every tool.
  needle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onEnabledChange: (enabled: boolean) => void;
  onToolsChange: (tools: string[]) => void;
  onRemove: () => void;
}) {
  const { name, tools, allowed_tools: allowed } = connection;
  const ticked = tools.filter((t) => allowed.includes(t.name)).length;
  const hasTools = tools.length > 0;
  // The person's own entry, and a connection Claude Code no longer
  // has, can be taken off the list. Any other one would come back
  // with the next lookup.
  const removable = connection.kind === "custom" || connection.state === "missing";
  const stateVariant =
    connection.state === "connected"
      ? "secondary"
      : connection.state === "failed" || connection.state === "missing"
        ? "destructive"
        : "outline";

  return (
    <li
      className="rounded-lg border"
      data-testid="agent-connection"
      data-name={name}
      data-state={connection.state}
      data-enabled={connection.enabled ? "true" : "false"}
    >
      <Collapsible open={open && hasTools} onOpenChange={onOpenChange}>
        {/* The row is a tab stop: Remove is hidden until the row is
            hovered or has keyboard focus (`.ow-hover-host` in
            App.css), and a hidden button cannot be tabbed to. */}
        <div
          className="ow-hover-host flex min-h-12 items-center gap-2 rounded-lg py-1.5 pr-3 pl-1.5 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          tabIndex={removable ? 0 : undefined}
        >
          <CollapsibleTrigger
            render={<Button variant="ghost" size="icon-sm" className="shrink-0" />}
            disabled={!hasTools}
            aria-label={`${open ? "Hide" : "Show"} the tools of ${name}`}
          >
            {/* State morph of a toggle icon (T2): one short turn. */}
            <ChevronRight
              className={cn(
                "transition-transform duration-150 motion-reduce:transition-none",
                open && hasTools && "rotate-90",
              )}
            />
          </CollapsibleTrigger>
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-sm font-medium" data-testid="agent-connection-name">
              {name}
            </span>
            <span
              className="truncate text-xs text-muted-foreground tabular-nums"
              data-testid="agent-connection-summary"
            >
              {hasTools ? `${ticked} of ${tools.length} tools ticked` : "No tools known"}
              {connection.via === "command" && ". Your own, started with a command"}
              {connection.via === "url" && ". Your own, reached at a URL"}
            </span>
          </div>
          <Badge variant={stateVariant} data-testid="agent-connection-state">
            {AGENT_STATE_LABELS[connection.state] ?? connection.state}
          </Badge>
          {removable && (
            <Button
              variant="ghost"
              size="icon-sm"
              className="ow-hover-reveal shrink-0"
              tabIndex={0}
              disabled={disabled}
              aria-label={`Remove ${name}`}
              onClick={onRemove}
            >
              <X />
            </Button>
          )}
          <Switch
            aria-label={`Use ${name}`}
            checked={connection.enabled}
            disabled={disabled}
            onCheckedChange={onEnabledChange}
          />
        </div>
        {connection.enabled && !connection.usable && (
          <p
            className="border-t px-3 py-2 text-xs text-muted-foreground"
            data-testid="agent-connection-unusable"
          >
            {connection.state === "missing"
              ? "An ask cannot use this connection: Claude Code no longer has it. Remove it here, or add it back in Claude Code and press Refresh."
              : "An ask cannot use this connection yet: it has not been looked up since the app started. Press Refresh."}
          </p>
        )}
        <CollapsibleContent>
          <ToolList
            connection={connection}
            draftsOnly={draftsOnly}
            disabled={disabled}
            needle={needle}
            onToolsChange={onToolsChange}
          />
        </CollapsibleContent>
      </Collapsible>
    </li>
  );
}

// The tools of one connection as checkboxes, grouped by what they do.
function ToolList({
  connection,
  draftsOnly,
  disabled,
  needle,
  onToolsChange,
}: {
  connection: AgentConnection;
  draftsOnly: boolean;
  disabled: boolean;
  needle: string;
  onToolsChange: (tools: string[]) => void;
}) {
  const id = useId();
  const { tools, allowed_tools: allowed } = connection;
  // Ticks only count on a connection that is switched on.
  const off = disabled || !connection.enabled;
  const visible = needle === "" ? tools : tools.filter((t) => toolMatches(t, needle));

  const setTicked = (names: string[], tick: boolean) => {
    const rest = allowed.filter((t) => !names.includes(t));
    onToolsChange(tick ? [...rest, ...names] : rest);
  };

  return (
    <div className="flex flex-col gap-4 border-t p-3" data-testid="agent-tools">
      {!connection.enabled && (
        <FieldDescription data-testid="agent-tools-off">
          Switch the connection on to tick its tools.
        </FieldDescription>
      )}
      {visible.length === 0 && <FieldDescription>No tool matches.</FieldDescription>}
      {AGENT_TOOL_KINDS.map((group) => {
        const inGroup = visible.filter((t) => t.kind === group.kind);
        if (inGroup.length === 0) return null;
        return (
          <ToolGroup
            key={group.kind}
            idPrefix={`${id}-${group.kind}`}
            kind={group.kind}
            label={group.label}
            description={group.description}
            tools={inGroup}
            allowed={allowed}
            off={off}
            draftsOnly={draftsOnly}
            onTick={setTicked}
          />
        );
      })}
    </div>
  );
}

function ToolGroup({
  idPrefix,
  kind,
  label,
  description,
  tools,
  allowed,
  off,
  draftsOnly,
  onTick,
}: {
  idPrefix: string;
  kind: AgentToolKind;
  label: string;
  description: string;
  tools: AgentTool[];
  allowed: string[];
  // The connection, or Agent, is switched off.
  off: boolean;
  draftsOnly: boolean;
  onTick: (names: string[], tick: boolean) => void;
}) {
  const act = kind === "act";
  // With drafts only, a tool that acts cannot run, ticked or not.
  const unavailable = act && draftsOnly;
  const ticked = tools.filter((t) => allowed.includes(t.name)).length;
  const all = ticked === tools.length;

  return (
    <FieldSet
      className="gap-2"
      data-testid={`agent-tools-${kind}`}
      data-unavailable={unavailable ? "true" : undefined}
    >
      <FieldLegend variant="label" className="mb-0 flex w-full items-center gap-2">
        <span>{label}</span>
        <span className="font-normal text-muted-foreground tabular-nums">
          {ticked} of {tools.length}
        </span>
        {act && (
          <Badge variant={unavailable ? "outline" : "destructive"} data-testid="agent-act-badge">
            {unavailable ? "Unavailable" : "Asks first"}
          </Badge>
        )}
      </FieldLegend>
      <div className="flex items-start justify-between gap-3">
        <FieldDescription data-testid={`agent-tools-${kind}-note`}>
          {description}{" "}
          {act &&
            (unavailable
              ? "Unavailable while Drafts only is on: none of these can run."
              : "Each of these asks you in the answer panel before it runs, every time.")}
        </FieldDescription>
        {/* Not for the tools that act: each of those is ticked on
            purpose, one at a time. */}
        {!act && (
          <Button
            variant="ghost"
            size="xs"
            className="shrink-0"
            disabled={off}
            onClick={() =>
              onTick(
                tools.map((t) => t.name),
                !all,
              )
            }
          >
            {all ? "Untick all" : "Tick all"}
          </Button>
        )}
      </div>
      <FieldGroup data-slot="checkbox-group" className="max-h-64 gap-2 overflow-y-auto py-0.5">
        {tools.map((tool) => {
          const boxId = `${idPrefix}-${tool.name}`;
          const blocked = off || unavailable;
          return (
            <Field
              key={tool.name}
              orientation="horizontal"
              data-disabled={blocked ? "true" : undefined}
              data-testid="agent-tool"
              data-tool={tool.name}
            >
              <Checkbox
                id={boxId}
                checked={allowed.includes(tool.name)}
                disabled={blocked}
                onCheckedChange={(checked) => onTick([tool.name], checked === true)}
              />
              <FieldLabel htmlFor={boxId} className="min-w-0 flex-wrap items-baseline font-normal">
                <span>{toolLabel(tool)}</span>
                {tool.title.trim() !== "" && tool.title !== tool.name && (
                  <span className="font-mono text-xs text-muted-foreground">{tool.name}</span>
                )}
              </FieldLabel>
            </Field>
          );
        })}
      </FieldGroup>
    </FieldSet>
  );
}

// Which app belongs to which connection. An ask made while one of
// these apps is in front reads from the connection and takes no
// screenshot, when the connection is on and has a tool that can run.
function AppsSection({
  state,
  disabled,
  update,
}: {
  state: AgentState;
  disabled: boolean;
  update: Update;
}) {
  const rows = state.app_connections;
  const running = useAgentRunningApps(!disabled);
  const [appId, setAppId] = useState<string | null>(null);
  const [connection, setConnection] = useState<string | null>(null);

  const listed = new Set(rows.map((r) => r.app_id));
  const openApps = running.filter((a) => !listed.has(a.app_id.toLowerCase()));
  const appItems = openApps.map((a) => ({ value: a.app_id, label: agentAppLabel(a) }));
  const connectionItems = [...state.connections]
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }))
    .map((c) => ({ value: c.name, label: c.name }));

  // The table as it is stored: what `agent_set` gets back.
  const plain = (): AgentAppConnection[] =>
    rows.map(({ app_id, app_name, connection: to }) => ({ app_id, app_name, connection: to }));
  const save = (table: AgentAppConnection[] | null) =>
    update((c) => ({ ...c, app_connections: table }));

  const picked = openApps.find((a) => a.app_id === appId) ?? null;
  const onAdd = () => {
    if (!picked || !connection) return;
    save([...plain(), { app_id: picked.app_id, app_name: picked.app_name, connection }]);
    setAppId(null);
    setConnection(null);
  };

  return (
    <>
      <SectionHeader>Apps</SectionHeader>
      <FieldDescription data-testid="agent-apps-intro">
        When you ask while one of these apps is in front, the ask
        reads from the app's connection and takes no screenshot. The
        connection has to be switched on with a tool ticked. For every
        other app the Screen setting under Ask applies. A question
        that contains one of your screen phrases still includes the
        screen.
      </FieldDescription>

      {rows.length === 0 ? (
        <FieldDescription data-testid="agent-apps-empty">
          No apps. Every ask follows the Screen setting under Ask.
        </FieldDescription>
      ) : (
        <ul
          className="flex flex-col rounded-lg border"
          aria-label="Apps and their connections"
          data-testid="agent-app-list"
        >
          {rows.map((row) => {
            // The stored name when no connection is called that, so
            // the select can show what the row says.
            const value = row.known ?? row.connection;
            const items = row.known
              ? connectionItems
              : [{ value: row.connection, label: row.connection }, ...connectionItems];
            return (
              <li
                key={row.app_id}
                className="ow-hover-host flex min-h-12 items-center gap-2 border-b py-1.5 pr-1.5 pl-3 outline-none last:border-b-0 focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-inset"
                data-testid="agent-app-row"
                data-app={row.app_id}
                data-active={row.active ? "true" : "false"}
                tabIndex={0}
              >
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-medium">{agentAppLabel(row)}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {row.app_name.trim() !== "" && <span className="font-mono">{row.app_id}</span>}
                    {row.app_name.trim() !== "" && ". "}
                    <span data-testid="agent-app-note">
                      {row.active
                        ? "In use."
                        : row.known
                          ? "Not in use: the connection is off or has no tool that can run."
                          : "Not in use: there is no connection of this name."}
                    </span>
                  </span>
                </div>
                {/* Control choice: one of the connections on the list
                    above. A discrete set, so a Select. */}
                <Select
                  items={items}
                  value={value}
                  disabled={disabled}
                  onValueChange={(v) => {
                    if (typeof v !== "string" || !v || v === value) return;
                    save(plain().map((r) => (r.app_id === row.app_id ? { ...r, connection: v } : r)));
                  }}
                >
                  <SelectTrigger
                    className="w-[180px]"
                    aria-label={`Connection for ${agentAppLabel(row)}`}
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
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="ow-hover-reveal shrink-0"
                  tabIndex={0}
                  disabled={disabled}
                  aria-label={`Remove ${agentAppLabel(row)}`}
                  onClick={() => save(plain().filter((r) => r.app_id !== row.app_id))}
                >
                  <X />
                </Button>
              </li>
            );
          })}
        </ul>
      )}

      <Field data-disabled={disabled ? "true" : undefined} data-testid="agent-app-add">
        <FieldLabel>Add an app</FieldLabel>
        <div className="flex flex-wrap items-center gap-2">
          <Select
            items={appItems}
            value={picked ? picked.app_id : null}
            disabled={disabled || openApps.length === 0}
            onValueChange={(v) => setAppId(typeof v === "string" ? v : null)}
          >
            <SelectTrigger className="w-[200px]" aria-label="App to add">
              <SelectValue placeholder={openApps.length === 0 ? "No other open apps" : "An open app"} />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false} align="start">
              <SelectGroup>
                {appItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <Select
            items={connectionItems}
            value={connection}
            disabled={disabled || connectionItems.length === 0}
            onValueChange={(v) => setConnection(typeof v === "string" ? v : null)}
          >
            <SelectTrigger className="w-[200px]" aria-label="Connection for the app to add">
              <SelectValue
                placeholder={connectionItems.length === 0 ? "No connections yet" : "Its connection"}
              />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false} align="start">
              <SelectGroup>
                {connectionItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            disabled={disabled || !picked || !connection}
            data-testid="agent-app-add-button"
            onClick={onAdd}
          >
            <Plus data-icon="inline-start" />
            Add app
          </Button>
        </div>
        <FieldDescription>
          Open the app you want to add and come back here. An app that
          is not open can be added with{" "}
          <code className="font-mono text-xs">openwhisper agent apps set</code>.
        </FieldDescription>
      </Field>

      {!state.app_connections_builtin && (
        // In a row of its own: a direct child of a FieldGroup is
        // stretched to the full width.
        <div className="flex">
          <Button
            variant="outline"
            size="sm"
            disabled={disabled}
            data-testid="agent-apps-restore"
            onClick={() => save(null)}
          >
            Restore defaults
          </Button>
        </div>
      )}
    </>
  );
}

// Add a connection of the person's own: a command with its
// arguments, or a URL. The form lives in a child that mounts with
// the dialog, so every opening starts empty.
function ConnectionDialog({
  open,
  onOpenChange,
  onAdd,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  // Resolves to what went wrong, or null when it was added.
  onAdd: (connection: AgentCustomConnection) => Promise<string | null>;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" data-testid="agent-connection-dialog">
        <ConnectionForm onAdd={onAdd} onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function ConnectionForm({
  onAdd,
  onDone,
}: {
  onAdd: (connection: AgentCustomConnection) => Promise<string | null>;
  onDone: () => void;
}) {
  const [name, setName] = useState("");
  const [way, setWay] = useState<"command" | "url">("command");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [url, setUrl] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ready = name.trim() !== "" && (way === "command" ? command.trim() : url.trim()) !== "";

  const submit = async () => {
    if (!ready || saving) return;
    setSaving(true);
    setError(null);
    const problem = await onAdd(
      way === "command"
        ? {
            name,
            command,
            // One argument per line, as written.
            args: args.split("\n").filter((line) => line.trim() !== ""),
            url: "",
          }
        : { name, command: "", args: [], url },
    );
    setSaving(false);
    if (problem) setError(problem);
    else onDone();
  };

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <DialogHeader>
        <DialogTitle>Add a connection</DialogTitle>
        <DialogDescription>
          A tool server of your own. It is added switched off with no
          tool ticked. Press Refresh afterwards to look up its tools.
        </DialogDescription>
      </DialogHeader>

      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="agent-connection-name">Name</FieldLabel>
          <Input
            id="agent-connection-name"
            placeholder="notes"
            value={name}
            maxLength={NAME_MAX_CHARS}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel>How it is reached</FieldLabel>
          {/* Control choice: two ways, both visible at once. */}
          <ToggleGroup
            value={[way]}
            onValueChange={(v) => {
              const next = v[0];
              if (next === "command" || next === "url") setWay(next);
            }}
            variant="outline"
          >
            <ToggleGroupItem value="command">Command</ToggleGroupItem>
            <ToggleGroupItem value="url">URL</ToggleGroupItem>
          </ToggleGroup>
        </Field>
        {way === "command" ? (
          <>
            <Field>
              <FieldLabel htmlFor="agent-connection-command">Command</FieldLabel>
              <Input
                id="agent-connection-command"
                className="font-mono"
                placeholder="/usr/local/bin/notes-server"
                value={command}
                maxLength={CUSTOM_FIELD_MAX_CHARS}
                autoComplete="off"
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                onChange={(e) => setCommand(e.target.value)}
              />
              <FieldDescription>
                The program is started on this computer when the
                connections are looked up and when an ask uses it.
              </FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="agent-connection-args">Arguments</FieldLabel>
              <Textarea
                id="agent-connection-args"
                className="font-mono text-xs"
                rows={3}
                placeholder={"--stdio"}
                value={args}
                spellCheck={false}
                onChange={(e) => setArgs(e.target.value)}
              />
              <FieldDescription>
                Optional. One argument per line. Variables for its
                environment are set with{" "}
                <code className="font-mono text-xs">
                  openwhisper agent connections add-custom --env
                </code>
                .
              </FieldDescription>
            </Field>
          </>
        ) : (
          <Field>
            <FieldLabel htmlFor="agent-connection-url">URL</FieldLabel>
            <Input
              id="agent-connection-url"
              className="font-mono"
              placeholder="https://notes.example/mcp"
              value={url}
              maxLength={CUSTOM_FIELD_MAX_CHARS}
              autoComplete="off"
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              onChange={(e) => setUrl(e.target.value)}
            />
          </Field>
        )}
        {error && (
          <FieldDescription className="text-destructive" role="alert">
            {error}
          </FieldDescription>
        )}
      </FieldGroup>

      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>Cancel</DialogClose>
        <Button type="submit" disabled={!ready || saving}>
          Add connection
        </Button>
      </DialogFooter>
    </form>
  );
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
      {children}
    </h3>
  );
}
