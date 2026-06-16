import {
  getTimerState,
  onRoundChange,
  onTimerPaused,
  onTimerReset,
  onTimerResumed,
  onTimerTick,
} from '$lib/ipc';
import { timerState } from '$lib/stores/timer';
import type { TimerState } from '$lib/types';
import type { UnlistenFn } from '@tauri-apps/api/event';

interface TimerStateSyncOptions {
  onRoundChange?: (snap: TimerState) => void;
}

/** Hydrate the shared timer store and subscribe to all timer lifecycle events. */
export async function syncTimerState(options: TimerStateSyncOptions = {}): Promise<UnlistenFn[]> {
  const initial = await getTimerState();
  timerState.set(initial);

  return [
    await onTimerTick(({ elapsed_secs, total_secs }) => {
      timerState.update((s) => ({
        ...s,
        elapsed_secs,
        total_secs,
        is_running: true,
        is_paused: false,
      }));
    }),
    await onTimerPaused(({ elapsed_secs }) => {
      timerState.update((s) => ({
        ...s,
        elapsed_secs,
        is_running: false,
        is_paused: true,
      }));
    }),
    await onTimerResumed(({ elapsed_secs }) => {
      timerState.update((s) => ({
        ...s,
        elapsed_secs,
        is_running: true,
        is_paused: false,
      }));
    }),
    await onRoundChange((snap) => {
      timerState.set(snap);
      options.onRoundChange?.(snap);
    }),
    await onTimerReset((snap) => {
      timerState.set(snap);
    }),
  ];
}
