// Re-export shim — kept until Task 7 deletes this file. Anything still
// importing MainWindowShell or its props type now resolves to DiagnosticsPane.
export {
  DiagnosticsPane as MainWindowShell,
  type DiagnosticsPaneProps as MainWindowShellProps,
  type Platform,
} from "./diagnostics-pane";
