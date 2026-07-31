import { isTauri } from "@tauri-apps/api/core";
import {
  ACTIONS,
  defaultsFor,
  isGlobalSafe,
  normalizeAccel,
  parseStored,
  serializeShortcuts,
  type ActionId,
  type Bindings,
  type Scope,
} from "./shortcuts";

const LS_KEY = "ap.shortcuts";

export type ActionHandlers = Record<ActionId, () => void>;

/**
 * Owns the bindings and the OS registrations. Behaviour lives in the layout, which
 * hands its handlers over with `attach()`, the same way the player is handed its
 * <audio> element.
 */
class ShortcutStore {
  /** Master switch. Off unregisters every OS grab rather than swallowing keys. */
  enabled = $state(true);
  local = $state<Bindings>(defaultsFor("local"));
  global = $state<Bindings>(defaultsFor("global"));
  /** Accelerators the OS refused, keyed by accelerator, for the settings fold. */
  errors = $state<Record<string, string>>({});
  /** Raised while the settings fold is recording, so a press cannot also act. */
  recording = $state(false);

  #handlers: ActionHandlers | null = null;
  // Registration is async and a rebind is two round trips. Runs are queued so a
  // burst cannot interleave one sync's registrations with the next one's clear-out,
  // which would leave a grab held that nothing short of a restart releases.
  #queue: Promise<void> = Promise.resolve();

  load() {
    let raw: string | null = null;
    try {
      raw = localStorage.getItem(LS_KEY);
    } catch {
      // Storage can be unavailable in restricted webviews; defaults still work.
    }
    const stored = parseStored(raw);
    this.enabled = stored.enabled;
    this.local = stored.local;
    this.global = stored.global;
    if (stored.needsSave) {
      try {
        localStorage.setItem(LS_KEY, serializeShortcuts(stored));
      } catch {
        // The in-memory migration is enough to release unsafe grabs this session.
      }
    }
  }

  attach(handlers: ActionHandlers) {
    this.#handlers = handlers;
    void this.syncGlobal();
  }

  /** Run an action by id. Safe before `attach()` and for ids with no handler. */
  run(id: ActionId) {
    this.#handlers?.[id]?.();
  }

  /** Bound in-app pairs, in list order, so the first match wins a clash. */
  localBindings(): [ActionId, string][] {
    return ACTIONS.map((a) => [a.id, this.local[a.id]] as const).filter(
      (pair): pair is [ActionId, string] => typeof pair[1] === "string",
    );
  }

  /**
   * Arm the settings recorder. Windows hands a registered combination to whoever
   * grabbed it and never delivers the key to the window, so the OS-wide tier has to
   * let go for the duration or its own bindings could not be typed back in.
   */
  beginRecording() {
    this.recording = true;
    void this.syncGlobal();
  }

  endRecording() {
    this.recording = false;
    void this.syncGlobal();
  }

  setEnabled(on: boolean) {
    this.enabled = on;
    this.#save();
  }

  /**
   * Bind an action, or unbind it with null. An OS-wide accelerator that would take
   * a bare key away from every other application is refused; the caller gets false
   * so it can say why.
   */
  bind(scope: Scope, id: ActionId, accel: string | null): boolean {
    const norm = normalizeAccel(accel);
    if (accel !== null && !norm) return false;
    if (scope === "global" && norm && !isGlobalSafe(norm)) return false;
    const next = { ...this[scope], [id]: norm };
    if (scope === "global") this.global = next;
    else this.local = next;
    this.#save();
    return true;
  }

  clear(scope: Scope, id: ActionId) {
    this.bind(scope, id, null);
  }

  resetAll() {
    this.enabled = true;
    this.local = defaultsFor("local");
    this.global = defaultsFor("global");
    this.#save();
  }

  isDefault(): boolean {
    const same = (a: Bindings, b: Bindings) => ACTIONS.every((x) => a[x.id] === b[x.id]);
    return this.enabled && same(this.local, defaultsFor("local")) && same(this.global, defaultsFor("global"));
  }

  #save() {
    try {
      localStorage.setItem(
        LS_KEY,
        serializeShortcuts({ enabled: this.enabled, local: this.local, global: this.global }),
      );
    } catch {
      // Same as above: an unwritable store costs persistence, not the session.
    }
    void this.syncGlobal();
  }

  /**
   * Hand the OS-wide tier to the OS. Always drops the previous grabs first, so
   * switching off is this same path with the register loop skipped and the keys
   * genuinely handed back to whatever else wants them.
   */
  syncGlobal(): Promise<void> {
    if (typeof window === "undefined" || !isTauri()) return Promise.resolve();
    const run = this.#queue.then(() => this.#applyGlobal());
    this.#queue = run.catch(() => {});
    return run;
  }

  async #applyGlobal() {
    const failures: Record<string, string> = {};
    const taken = new Set<string>();
    try {
      const { register, unregisterAll } = await import("@tauri-apps/plugin-global-shortcut");
      await unregisterAll();
      if (this.enabled && !this.recording) {
        for (const action of ACTIONS) {
          const accel = this.global[action.id];
          // Two actions on one key: the first listed takes it, matching how the
          // in-app tier resolves the same clash. Registering twice would only fail.
          if (!accel || taken.has(accel)) continue;
          taken.add(accel);
          try {
            await register(accel, (event) => {
              // Press and release both arrive; acting on both fires twice.
              if (event.state !== "Pressed") return;
              if (!this.enabled || this.recording) return;
              this.run(action.id);
            });
          } catch (err) {
            // Almost always another application already holding the combination.
            failures[accel] = String(err);
          }
        }
      }
    } catch (err) {
      failures["*"] = String(err);
    }
    this.errors = failures;
  }
}

export const shortcuts = new ShortcutStore();
