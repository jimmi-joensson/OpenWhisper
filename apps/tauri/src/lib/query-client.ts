import { QueryClient } from "@tanstack/react-query";

// One query cache for the main window. The "server" is the Rust side
// of the app, reached through `invoke`: it answers in milliseconds
// and a failure will not fix itself, so nothing is retried. Data
// counts as stale at once, so a screen re-reads when it opens and
// when the window gets focus again (the headless CLI can change the
// same settings and store while the app runs).
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false },
    mutations: { retry: false },
  },
});
