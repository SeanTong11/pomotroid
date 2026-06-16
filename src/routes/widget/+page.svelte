<script lang="ts">
  import '../../app.css';
  import { onMount } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { Menu, PredefinedMenuItem } from '@tauri-apps/api/menu';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { cursorPosition } from '@tauri-apps/api/window';
  import {
    appExit,
    getSettings,
    getThemes,
    onSettingsChanged,
    onThemesChanged,
    restoreMainWindow,
    timerRestartRound,
    timerSkip,
    timerToggle,
  } from '$lib/ipc';
  import { timerState } from '$lib/stores/timer';
  import { settings } from '$lib/stores/settings';
  import { applyTheme } from '$lib/stores/theme';
  import { resolveThemeName } from '$lib/utils/theme';
  import { syncTimerState } from '$lib/utils/timerStateSync';
  import { setLocale } from '$lib/locale.svelte.js';

  const DIAL_RADIUS = 60;
  const DIAL_STROKE_WIDTH = 6;
  const CIRCUMFERENCE = 2 * Math.PI * DIAL_RADIUS;
  // Keep hover affordances inside the painted ring. The transparent Tauri
  // window is still rectangular, so this guards only widget UI state.
  const HOVER_RADIUS = DIAL_RADIUS + DIAL_STROKE_WIDTH / 2;
  const CURSOR_POLL_MS = 80;

  let hovered = $state(false);
  let snap = $derived($timerState);
  let widgetEl: HTMLElement | undefined;
  let cursorPassthrough = false;
  const widgetWindow = getCurrentWebviewWindow();

  let remaining = $derived(Math.max(0, snap.total_secs - snap.elapsed_secs));
  let minutes = $derived(Math.floor(remaining / 60));
  let seconds = $derived(remaining % 60);
  let display = $derived(`${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`);
  let progress = $derived(snap.total_secs > 0 ? snap.elapsed_secs / snap.total_secs : 0);
  let dashOffset = $derived(CIRCUMFERENCE * ($settings.dial_countdown ? progress : 1 - progress));
  let roundClass = $derived(`round-${snap.round_type}`);

  function onControlPointer(e: MouseEvent) {
    e.stopPropagation();
  }

  function isWidgetPointerHit(e: MouseEvent | PointerEvent) {
    const rect = widgetEl?.getBoundingClientRect();
    if (!rect) return false;

    const x = e.clientX - rect.left - rect.width / 2;
    const y = e.clientY - rect.top - rect.height / 2;
    return x * x + y * y <= HOVER_RADIUS * HOVER_RADIUS;
  }

  async function setCursorPassthrough(ignore: boolean) {
    if (cursorPassthrough === ignore) return;
    cursorPassthrough = ignore;
    try {
      await widgetWindow.setIgnoreCursorEvents(ignore);
    } catch {
      cursorPassthrough = !ignore;
    }
  }

  async function syncCursorPassthrough() {
    const rect = widgetEl?.getBoundingClientRect();
    if (!rect) return;

    try {
      const [cursor, pos, size] = await Promise.all([
        cursorPosition(),
        widgetWindow.outerPosition(),
        widgetWindow.outerSize(),
      ]);
      const scale = size.width / rect.width;
      const x = cursor.x - pos.x - size.width / 2;
      const y = cursor.y - pos.y - size.height / 2;
      const radius = HOVER_RADIUS * scale;
      await setCursorPassthrough(x * x + y * y > radius * radius);
    } catch {
      await setCursorPassthrough(false);
    }
  }

  function updateWidgetHover(e: PointerEvent) {
    const hit = isWidgetPointerHit(e);
    hovered = hit;
    void setCursorPassthrough(!hit);
  }

  function clearWidgetHover() {
    hovered = false;
    void setCursorPassthrough(true);
  }

  function onPrimaryPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;

    e.preventDefault();
    e.stopPropagation();
    timerToggle();
  }

  async function showContextMenu(e: MouseEvent) {
    if (!isWidgetPointerHit(e)) return;

    e.preventDefault();
    e.stopPropagation();

    const controlsEnabled = snap.is_running || snap.is_paused;
    const separator = await PredefinedMenuItem.new({ item: 'Separator' });
    const menu = await Menu.new({
      items: [
        {
          text: snap.is_running ? 'Pause' : snap.is_paused ? 'Resume' : 'Start',
          action: () => {
            timerToggle();
          },
        },
        {
          text: 'Skip',
          enabled: controlsEnabled,
          action: () => {
            timerSkip();
          },
        },
        {
          text: 'Reset Round',
          enabled: controlsEnabled,
          action: () => {
            timerRestartRound();
          },
        },
        separator,
        {
          text: 'Show',
          action: () => {
            restoreMainWindow();
          },
        },
        {
          text: 'Exit',
          action: () => {
            appExit();
          },
        },
      ],
    });

    await menu.popup();
  }

  async function startDrag(e: MouseEvent) {
    if (e.button !== 0 || e.detail !== 1) return;
    if (!isWidgetPointerHit(e)) return;
    if ((e.target as HTMLElement).closest('button')) return;

    await getCurrentWebviewWindow().startDragging();
  }

  async function onDoubleClick(e: MouseEvent) {
    if (!isWidgetPointerHit(e)) return;

    const target = e.target;
    if (target instanceof Element && target.closest('button')) return;

    e.stopPropagation();
    await restoreMainWindow();
  }

  onMount(() => {
    const cleanups: UnlistenFn[] = [];
    const cursorPoll = window.setInterval(() => {
      void syncCursorPassthrough();
    }, CURSOR_POLL_MS);
    void syncCursorPassthrough();

    (async () => {
      const s = await getSettings();
      settings.set(s);
      setLocale(s.language);

      const themes = await getThemes();
      const osDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      const active = themes.find((t) => t.name === resolveThemeName(s, osDark)) ?? themes[0];
      if (active) applyTheme(active);

      cleanups.push(...(await syncTimerState()));

      const mq = window.matchMedia('(prefers-color-scheme: dark)');
      const mqListener = async (e: MediaQueryListEvent) => {
        if ($settings.theme_mode !== 'auto') return;
        const allThemes = await getThemes();
        const t = allThemes.find((th) => th.name === resolveThemeName($settings, e.matches));
        if (t) applyTheme(t);
      };
      mq.addEventListener('change', mqListener);
      cleanups.push(() => mq.removeEventListener('change', mqListener));

      cleanups.push(
        await onSettingsChanged(async (updated) => {
          const prevMode = $settings.theme_mode;
          const prevLight = $settings.theme_light;
          const prevDark = $settings.theme_dark;
          const prevLanguage = $settings.language;
          settings.set(updated);
          if (updated.language !== prevLanguage) {
            setLocale(updated.language);
          }
          if (
            updated.theme_mode !== prevMode ||
            updated.theme_light !== prevLight ||
            updated.theme_dark !== prevDark
          ) {
            const allThemes = await getThemes();
            const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            const t = allThemes.find((th) => th.name === resolveThemeName(updated, dark));
            if (t) applyTheme(t);
          }
        }),
        await onThemesChanged((updated) => {
          const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
          const current =
            updated.find((t) => t.name === resolveThemeName($settings, dark)) ?? updated[0];
          if (current) applyTheme(current);
        })
      );
    })();

    return () => {
      window.clearInterval(cursorPoll);
      void setCursorPassthrough(false);
      for (const fn of cleanups) fn();
    };
  });
</script>

<svelte:window oncontextmenu={showContextMenu} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<main
  bind:this={widgetEl}
  class={`widget ${roundClass}`}
  class:hovered
  onpointerenter={updateWidgetHover}
  onpointermove={updateWidgetHover}
  onpointerleave={clearWidgetHover}
  onmousedown={startDrag}
  ondblclick={onDoubleClick}
>
  <svg class="dial" viewBox="0 0 160 160" aria-hidden="true">
    <circle class="track" cx="80" cy="80" r={DIAL_RADIUS} stroke-width={DIAL_STROKE_WIDTH} />
    <circle
      class="progress"
      cx="80"
      cy="80"
      r={DIAL_RADIUS}
      stroke-width={DIAL_STROKE_WIDTH}
      stroke-dasharray={CIRCUMFERENCE}
      stroke-dashoffset={dashOffset}
    />
  </svg>

  <div class="content">
    <div class="time">{display}</div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="controls" onmousedown={onControlPointer}>
      <button class="side" onclick={timerRestartRound} aria-label="Restart round">
        <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true">
          <polygon points="15,1 6,8 15,15" fill="currentColor" />
          <rect x="1" y="1" width="3" height="14" rx="1" fill="currentColor" />
        </svg>
      </button>

      <button
        class="primary"
        type="button"
        onpointerdown={onPrimaryPointerDown}
        aria-label={snap.is_running ? 'Pause' : 'Play'}
      >
        {#if snap.is_running}
          <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="5" y="3" width="5" height="18" rx="1.5" fill="currentColor" />
            <rect x="14" y="3" width="5" height="18" rx="1.5" fill="currentColor" />
          </svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
            <polygon points="5,3 21,12 5,21" fill="currentColor" />
          </svg>
        {/if}
      </button>

      <button class="side" onclick={timerSkip} aria-label="Skip round">
        <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true">
          <polygon points="1,1 10,8 1,15" fill="currentColor" />
          <rect x="12" y="1" width="3" height="14" rx="1" fill="currentColor" />
        </svg>
      </button>
    </div>

    <div class="round-mark" aria-hidden="true">
      {#if snap.round_type === 'work'}
        <img class="tomato-icon" src="/icons/widget/tomato.svg" alt="" draggable="false" />
      {:else}
        <span class="coffee-icon"></span>
      {/if}
    </div>
  </div>
</main>

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }

  :global(body) {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .widget {
    --round-color: var(--color-focus-round);
    --time-color: color-mix(in srgb, var(--color-foreground) 84%, #333 16%);
    --side-color: color-mix(in srgb, var(--color-foreground) 34%, #4a4a4a 66%);
    width: 160px;
    height: 160px;
    position: relative;
    display: grid;
    place-items: center;
    color: var(--color-foreground);
    cursor: default;
  }

  .round-short-break {
    --round-color: var(--color-short-round);
  }

  .round-long-break {
    --round-color: var(--color-long-round);
  }

  .dial {
    position: absolute;
    inset: 0;
    width: 160px;
    height: 160px;
    overflow: visible;
  }

  .track,
  .progress {
    fill: none;
  }

  .track {
    stroke: color-mix(in oklch, var(--color-foreground) 22%, transparent);
  }

  .progress {
    stroke: var(--round-color);
    stroke-linecap: round;
    transform: rotate(-90deg);
    transform-origin: 80px 80px;
    transition:
      opacity 0.18s ease,
      stroke-dashoffset 0.5s ease;
  }

  .hovered .progress {
    opacity: 0.45;
  }

  .content {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100px;
    min-height: 74px;
  }

  .time {
    font-family: 'Mona Sans Mono', monospace;
    font-size: 1.55rem;
    font-weight: 600;
    font-stretch: 85%;
    line-height: 1;
    color: rgb(255 255 255 / 0.94);
    -webkit-text-stroke: 1px rgb(0 0 0 / 0.62);
    paint-order: stroke fill;
    text-shadow: 0 1px 1px rgb(0 0 0 / 0.45);
    transform: translateY(0);
    transition:
      font-size 0.18s ease,
      margin 0.18s ease,
      transform 0.18s ease;
  }

  .hovered .time {
    font-size: 1.05rem;
    margin-bottom: 10px;
    transform: translateY(-7px);
  }

  .round-mark {
    position: absolute;
    top: calc(50% + 22px);
    height: 18px;
    color: var(--round-color);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 1;
    pointer-events: none;
    transform: translateY(0) scale(1);
    transition:
      opacity 0.16s ease,
      transform 0.16s ease;
  }

  .tomato-icon {
    width: 16px;
    height: 16px;
    display: block;
  }

  .coffee-icon {
    width: 17px;
    height: 17px;
    display: block;
    background: var(--round-color);
    -webkit-mask: url('/icons/widget/coffee.svg') center / contain no-repeat;
    mask: url('/icons/widget/coffee.svg') center / contain no-repeat;
  }

  .hovered .round-mark {
    opacity: 0;
    transform: translateY(5px) scale(0.94);
  }

  .controls {
    position: absolute;
    top: calc(50% + 6px);
    display: flex;
    align-items: center;
    gap: 10px;
    opacity: 0;
    transform: translateY(8px) scale(0.96);
    transition:
      opacity 0.18s ease,
      transform 0.18s ease;
  }

  .hovered .controls {
    opacity: 1;
    transform: translateY(0) scale(1);
  }

  button {
    border: none;
    box-sizing: border-box;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--color-foreground);
    text-shadow: none;
    transition:
      color 0.12s ease,
      background 0.12s ease;
  }

  button svg {
    pointer-events: none;
  }

  .side {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: transparent;
    color: var(--side-color);
    opacity: 0.7;
    pointer-events: none;
  }

  .hovered .side {
    pointer-events: auto;
  }

  .side:hover {
    color: color-mix(in srgb, var(--color-foreground) 42%, #222 58%);
    background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
    opacity: 1;
  }

  .primary {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--round-color);
    color: white;
    box-shadow: 0 2px 5px rgb(0 0 0 / 0.18);
  }
</style>
