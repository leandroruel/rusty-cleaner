import * as Sentry from "@sentry/browser";

/// Sentry crash reporting — opt-in per LGPD. The DSN comes from a
/// build-time env var (`VITE_SENTRY_DSN`); when absent (local dev,
/// forks), Sentry.init is a no-op and nothing is sent.
const DSN = import.meta.env.VITE_SENTRY_DSN as string | undefined;

const OPT_IN_KEY = "telemetry-opt-in";

export function initTelemetry(): void {
  if (!DSN || !isOptedIn()) return;
  Sentry.init({
    dsn: DSN,
    release: `rusty-cleaner@${__APP_VERSION__}`,
    environment: import.meta.env.MODE,
    // Strip file paths (they contain usernames) from stack frames and
    // event data before anything leaves the machine.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    beforeSend: scrubEvent as any,
    denyUrls: [/localhost/, /127\.0\.0\.1/],
  });
}

export function isOptedIn(): boolean {
  return localStorage.getItem(OPT_IN_KEY) === "true";
}

export function setOptedIn(enabled: boolean): void {
  if (enabled) {
    localStorage.setItem(OPT_IN_KEY, "true");
    initTelemetry();
  } else {
    localStorage.setItem(OPT_IN_KEY, "false");
    void Sentry.close(0);
  }
}

/// Captures an error or message for Sentry. No-op when not opted in.
export function captureError(error: unknown): void {
  if (!isOptedIn() || !DSN) return;
  if (error instanceof Error) {
    Sentry.captureException(error);
  } else {
    Sentry.captureMessage(String(error), "error");
  }
}

/// Captures a backend error surfaced to the frontend via IPC.
export function captureBackendError(context: string, error: unknown): void {
  if (!isOptedIn() || !DSN) return;
  Sentry.captureMessage(`${context}: ${String(error)}`, "error");
}

/// Removes personally identifiable information before an event is sent.
function scrubEvent(
  event: Sentry.Event,
): Sentry.Event | null {
  // Drop events with nothing to report.
  if (!event.exception && !event.message) return null;

  // Strip user info and request data (may contain file paths / IPs).
  delete event.user;
  delete event.request;

  // Scrub absolute paths from exception values — panics and errors often
  // embed paths containing the username.
  if (event.exception?.values) {
    for (const exception of event.exception.values) {
      if (exception.value) {
        exception.value = exception.value.replace(
          /\/[A-Za-z0-9._-]+\/([A-Za-z0-9._-]+)\//g,
          "/$1/[redacted]/",
        );
      }
    }
  }

  return event;
}

// Version injected at build time via a Vite define.
declare const __APP_VERSION__: string;
