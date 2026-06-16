const DEFAULT_ATTEMPTS = 8;
const DEFAULT_DELAY_MS = 50;

interface StartupRetryOptions {
  attempts?: number;
  delayMs?: number;
}

function isBackendStateNotReady(error: unknown): boolean {
  const message = String(error);
  return message.includes('state not managed') && message.includes('.manage()');
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Retry early IPC calls that can race Tauri state registration during startup. */
export async function invokeWhenBackendReady<T>(
  operation: () => Promise<T>,
  options: StartupRetryOptions = {}
): Promise<T> {
  const attempts = options.attempts ?? DEFAULT_ATTEMPTS;
  const delayMs = options.delayMs ?? DEFAULT_DELAY_MS;

  for (let attempt = 1; ; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      if (!isBackendStateNotReady(error) || attempt >= attempts) {
        throw error;
      }
      await delay(delayMs);
    }
  }
}
