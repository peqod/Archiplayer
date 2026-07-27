<script lang="ts">
  import { shortcuts } from "$lib/shortcuts.svelte";
  import {
    ACTIONS,
    accelFromEvent,
    accelLabel,
    duplicates,
    type ActionId,
    type Scope,
  } from "$lib/shortcuts";

  let armed = $state<{ scope: Scope; id: ActionId } | null>(null);
  let hint = $state<string | null>(null);

  const localDups = $derived(new Set(duplicates(shortcuts.local)));
  const globalDups = $derived(new Set(duplicates(shortcuts.global)));

  function isArmed(scope: Scope, id: ActionId) {
    return armed?.scope === scope && armed?.id === id;
  }

  function arm(scope: Scope, id: ActionId) {
    if (isArmed(scope, id)) return disarm();
    armed = { scope, id };
    hint = null;
    shortcuts.beginRecording();
  }

  function disarm() {
    armed = null;
    hint = null;
    shortcuts.endRecording();
  }

  function onRecord(e: KeyboardEvent) {
    if (!armed) return;
    // Capture phase: the press belongs to the recorder, not to the binding it may
    // already match, and not to the page's scroll.
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") return disarm();
    if (e.code === "Backspace" || e.code === "Delete") {
      shortcuts.clear(armed.scope, armed.id);
      return disarm();
    }
    const accel = accelFromEvent(e);
    // A modifier on its own is on the way to a binding: keep waiting for the key.
    if (!accel) return;
    if (!shortcuts.bind(armed.scope, armed.id, accel)) {
      hint =
        armed.scope === "global"
          ? `${accelLabel(accel) || "That key"} needs Ctrl, Alt or Win to work outside the app.`
          : "That key cannot be used as a shortcut.";
      return;
    }
    disarm();
  }

  function cellLabel(scope: Scope, id: ActionId, action: string) {
    const accel = scope === "global" ? shortcuts.global[id] : shortcuts.local[id];
    const where = scope === "global" ? "anywhere" : "in app";
    const set = accel ? accelLabel(accel) : "not set";
    return `${action}, ${where}: ${set}. Press to change.`;
  }
</script>

<svelte:window onkeydowncapture={onRecord} />

<div class="sc" class:off={!shortcuts.enabled}>
  <div class="sc-head">
    <button
      class="sc-switch"
      class:on={shortcuts.enabled}
      role="switch"
      aria-checked={shortcuts.enabled}
      onclick={() => {
        if (armed) disarm();
        shortcuts.setEnabled(!shortcuts.enabled);
      }}
    >
      <span class="sc-dot"></span>
      Shortcuts {shortcuts.enabled ? "on" : "off"}
    </button>
    {#if !shortcuts.isDefault()}
      <button class="reset" onclick={() => { if (armed) disarm(); shortcuts.resetAll(); }}>
        Reset to defaults
      </button>
    {/if}
  </div>

  <div class="grid" aria-hidden={!shortcuts.enabled}>
    <span class="col-head"></span>
    <span class="col-head">In app</span>
    <span class="col-head">Anywhere</span>

    {#each ACTIONS as action (action.id)}
      {@const local = shortcuts.local[action.id]}
      {@const global = shortcuts.global[action.id]}
      <span class="act">{action.label}</span>
      <button
        class="key"
        class:armed={isArmed("local", action.id)}
        class:clash={!!local && localDups.has(local)}
        class:unset={!local}
        disabled={!shortcuts.enabled}
        aria-label={cellLabel("local", action.id, action.label)}
        onclick={() => arm("local", action.id)}
      >
        {#if isArmed("local", action.id)}
          Press a key…
        {:else}
          {accelLabel(local) || "—"}
        {/if}
      </button>
      <button
        class="key"
        class:armed={isArmed("global", action.id)}
        class:clash={!!global && (globalDups.has(global) || global in shortcuts.errors)}
        class:unset={!global}
        disabled={!shortcuts.enabled}
        aria-label={cellLabel("global", action.id, action.label)}
        title={global && shortcuts.errors[global] ? shortcuts.errors[global] : undefined}
        onclick={() => arm("global", action.id)}
      >
        {#if isArmed("global", action.id)}
          Press a key…
        {:else}
          {accelLabel(global) || "—"}
        {/if}
      </button>
    {/each}
  </div>

  <p class="note">
    {#if armed}
      <strong>Esc</strong> cancels, <strong>Backspace</strong> clears the binding.
    {:else if !shortcuts.enabled}
      Every key is handed back while this is off, including the ones that work outside the app.
    {:else}
      “In app” keys only fire while Archiplayer has focus. “Anywhere” keys are taken from
      the whole system, so they need Ctrl, Alt or Win. Media keys work on their own.
    {/if}
  </p>

  {#if hint && armed}
    <p class="warn">{hint}</p>
  {/if}
  {#if localDups.size || globalDups.size}
    <p class="warn">
      The same key is on two actions. The one higher in the list wins.
    </p>
  {/if}
  {#each Object.entries(shortcuts.errors) as [accel, message] (accel)}
    <p class="warn">
      {accel === "*" ? "Shortcuts could not be registered" : `${accelLabel(accel)} is already taken by another program`} — {message}
    </p>
  {/each}
</div>

<style>
  .sc-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 12px;
  }
  .sc-switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 999px;
    color: var(--c-dim);
    padding: 5px 12px 5px 8px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .sc-switch.on {
    color: var(--c-accent);
    border-color: var(--c-accent);
  }
  .sc-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--c-dim);
  }
  .sc-switch.on .sc-dot {
    background: var(--c-accent);
  }
  .reset {
    background: none;
    border: 1px solid var(--c-border);
    color: var(--c-dim);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
    font-size: 12px;
  }
  .reset:hover {
    color: var(--c-accent);
    border-color: var(--c-accent);
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(120px, 1fr) minmax(96px, 140px) minmax(96px, 170px);
    gap: 6px;
    align-items: center;
  }
  .sc.off .grid {
    opacity: 0.45;
  }
  .col-head {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--c-dim);
    padding-bottom: 2px;
  }
  .act {
    font-size: 13px;
  }
  .key {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    color: var(--c-text);
    padding: 6px 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .key:hover:not(:disabled) {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }
  .key.unset {
    color: var(--c-dim);
    font-weight: 400;
  }
  .key.armed {
    border-color: var(--c-accent);
    color: var(--c-accent);
    font-weight: 400;
  }
  .key.clash {
    border-color: var(--c-danger);
    color: var(--c-danger);
  }
  .key:disabled {
    cursor: default;
  }
  .note {
    font-size: 12px;
    color: var(--c-dim);
    margin: 12px 0 0;
  }
  .warn {
    font-size: 12px;
    color: var(--c-danger);
    margin: 6px 0 0;
  }
</style>
