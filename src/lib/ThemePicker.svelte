<script lang="ts">
  import { theme, PRESETS, TOKENS, type TokenKey } from "$lib/theme.svelte";

  function onPick(key: TokenKey, e: Event) {
    theme.setToken(key, (e.target as HTMLInputElement).value);
  }
</script>

<div class="tp">
  <div class="presets">
    {#each PRESETS as p (p.id)}
      <button
        class="preset"
        class:on={theme.activeId === p.id}
        onclick={() => theme.select(p.id)}
        style="
          --sw-bg:{p.palette.bg};
          --sw-surface:{p.palette.surface};
          --sw-accent:{p.palette.accent};
          --sw-gold:{p.palette.gold};
          --sw-text:{p.palette.text};
        "
      >
        <span class="swatches">
          <span class="sw" style="background:{p.palette.bg}"></span>
          <span class="sw" style="background:{p.palette.accent}"></span>
          <span class="sw" style="background:{p.palette.gold}"></span>
          <span class="sw" style="background:{p.palette.text}"></span>
        </span>
        <span class="p-name">{p.name}</span>
        <span class="p-note">{p.note}</span>
      </button>
    {/each}
  </div>

  <div class="custom">
    <div class="custom-head">
      <span>Customize “{theme.active.name}”</span>
      {#if theme.hasOverrides()}
        <button class="reset" onclick={() => theme.resetActive()}>Reset to preset</button>
      {/if}
    </div>
    <div class="pickers">
      {#each TOKENS as t (t.key)}
        <label class="picker">
          <input
            type="color"
            value={theme.palette[t.key]}
            oninput={(e) => onPick(t.key, e)}
          />
          <span class="pk-label">{t.label}</span>
        </label>
      {/each}
    </div>
  </div>
</div>

<style>
  .presets {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px;
  }
  .preset {
    text-align: left;
    background: var(--sw-surface);
    border: 2px solid var(--c-border);
    border-radius: 10px;
    padding: 10px;
    cursor: pointer;
    color: var(--sw-text);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .preset.on {
    border-color: var(--sw-accent);
  }
  .swatches {
    display: flex;
    gap: 4px;
    margin-bottom: 4px;
  }
  .sw {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    border: 1px solid rgba(128, 128, 128, 0.35);
  }
  .p-name {
    font-weight: 700;
    font-size: 14px;
  }
  .p-note {
    font-size: 11px;
    opacity: 0.75;
  }
  .custom {
    margin-top: 16px;
  }
  .custom-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    color: var(--c-dim);
    margin-bottom: 8px;
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
  .pickers {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
  }
  .picker {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    padding: 6px 8px;
    cursor: pointer;
  }
  .picker input[type="color"] {
    width: 28px;
    height: 28px;
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
  }
  .pk-label {
    font-size: 12px;
    color: var(--c-dim);
  }
</style>
