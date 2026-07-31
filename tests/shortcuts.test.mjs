import assert from "node:assert/strict";
import {
  ACTIONS,
  DEFAULT_GLOBAL,
  DEFAULT_LOCAL,
  accelFromEvent,
  accelLabel,
  duplicates,
  globalBindingIssue,
  isGlobalSafe,
  keyTokenFromCode,
  matchesAccel,
  normalizeAccel,
  parseStored,
  serializeShortcuts,
  shouldIgnoreKey,
} from "../src/lib/shortcuts.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

const press = (code, mods = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

test("a keypress becomes an accelerator with the modifiers in a fixed order", () => {
  assert.equal(accelFromEvent(press("KeyM")), "M");
  assert.equal(
    accelFromEvent(press("KeyM", { altKey: true, ctrlKey: true })),
    "Ctrl+Alt+M",
  );
  // Held in the other order, typed in the same one: a binding has one spelling.
  assert.equal(
    accelFromEvent(press("KeyM", { shiftKey: true, metaKey: true, ctrlKey: true })),
    "Ctrl+Shift+Super+M",
  );
  assert.equal(accelFromEvent(press("BracketRight", { ctrlKey: true, altKey: true })), "Ctrl+Alt+BracketRight");
});

test("keys that cannot be named are not bindable", () => {
  // A modifier held on its own is a press on the way to a binding, not a binding.
  assert.equal(accelFromEvent(press("ControlLeft")), null);
  assert.equal(accelFromEvent(press("ShiftRight")), null);
  assert.equal(accelFromEvent(press("MetaLeft")), null);
  // Escape is the recorder's way out, so it must never record.
  assert.equal(accelFromEvent(press("Escape")), null);
  assert.equal(keyTokenFromCode("Escape"), null);
  assert.equal(keyTokenFromCode("CapsLock"), null);
});

test("the key half comes from the physical key, not the character it types", () => {
  assert.equal(keyTokenFromCode("KeyJ"), "J");
  assert.equal(keyTokenFromCode("Digit4"), "4");
  assert.equal(keyTokenFromCode("F12"), "F12");
  assert.equal(keyTokenFromCode("Numpad7"), "Numpad7");
  assert.equal(keyTokenFromCode("MediaPlayPause"), "MediaPlayPause");
  // Shift+Comma types "<" on a US layout; the binding still reads as Comma.
  assert.equal(accelFromEvent(press("Comma", { shiftKey: true })), "Shift+Comma");
});

test("stored accelerators are re-read into one canonical spelling", () => {
  assert.equal(normalizeAccel("alt+ctrl+m"), "Ctrl+Alt+M");
  assert.equal(normalizeAccel("CommandOrControl+Shift+F"), "Ctrl+Shift+F");
  assert.equal(normalizeAccel("Ctrl+Alt+["), "Ctrl+Alt+BracketLeft");
  assert.equal(normalizeAccel(" Ctrl + Alt + R "), "Ctrl+Alt+R");
  assert.equal(normalizeAccel("Space"), "Space");
  // Nonsense, a lone modifier and a repeated modifier are all unbindable.
  assert.equal(normalizeAccel("Ctrl+Ctrl+M"), null);
  assert.equal(normalizeAccel("Ctrl"), null);
  assert.equal(normalizeAccel("Hyper+M"), null);
  assert.equal(normalizeAccel(""), null);
  assert.equal(normalizeAccel(null), null);
});

test("a binding fires on its own keypress and nothing else", () => {
  assert.equal(matchesAccel(press("KeyM"), "M"), true);
  assert.equal(matchesAccel(press("KeyM", { ctrlKey: true }), "M"), false);
  assert.equal(matchesAccel(press("KeyM"), "Ctrl+M"), false);
  assert.equal(matchesAccel(press("KeyM", { ctrlKey: true, altKey: true }), "alt+ctrl+m"), true);
  assert.equal(matchesAccel(press("KeyM"), null), false);
  // Shift is part of the binding, so the bare key does not answer for it.
  assert.equal(matchesAccel(press("Period", { shiftKey: true }), "Period"), false);
});

test("an OS-wide binding preserves text-entry modifiers", () => {
  assert.equal(isGlobalSafe("Ctrl+Shift+F6"), true);
  assert.equal(isGlobalSafe("Super+M"), true);
  assert.equal(isGlobalSafe("Super+Alt+M"), true);
  assert.equal(isGlobalSafe("MediaPlayPause"), true);
  // Windows reports AltGr as Ctrl+Alt, including when Shift produces capitals.
  assert.equal(isGlobalSafe("Ctrl+Alt+M"), false);
  assert.equal(isGlobalSafe("Ctrl+Alt+Shift+M"), false);
  assert.equal(isGlobalSafe("Ctrl+Alt+MediaPlayPause"), false);
  // Option is text entry on macOS, so Alt cannot qualify a chord on its own.
  assert.equal(isGlobalSafe("Alt+M"), false);
  assert.equal(isGlobalSafe("Alt+Shift+F6"), false);
  assert.equal(isGlobalSafe("Alt+MediaPlayPause"), false);
  // Registering these OS-wide would swallow the key in every other application.
  assert.equal(isGlobalSafe("Space"), false);
  assert.equal(isGlobalSafe("M"), false);
  assert.equal(isGlobalSafe("Shift+M"), false);
  assert.equal(isGlobalSafe(null), false);

  assert.equal(globalBindingIssue("Ctrl+Alt+S"), "text-entry");
  assert.equal(globalBindingIssue("Alt+S"), "text-entry");
  assert.equal(globalBindingIssue("Shift+S"), "needs-system-modifier");
  assert.equal(globalBindingIssue("Hyper+S"), "invalid");
  assert.equal(globalBindingIssue("Ctrl+S"), null);
});

test("two actions on one key is reported per tier, not across them", () => {
  assert.deepEqual(duplicates({ mute: "M", random: "R" }), []);
  assert.deepEqual(duplicates({ mute: "M", random: "m" }), ["M"]);
  assert.deepEqual(duplicates({ mute: "Ctrl+Alt+M", random: "alt+ctrl+m" }), ["Ctrl+Alt+M"]);
  // Unbound actions are not a clash with each other.
  assert.deepEqual(duplicates({ mute: null, random: null, "fav-song": null }), []);
  // The tiers never listen at the same time, so sharing a key across them is fine.
  assert.deepEqual(duplicates(DEFAULT_LOCAL), []);
  assert.deepEqual(duplicates(DEFAULT_GLOBAL), []);
});

test("every action ships with a local binding and safe global defaults", () => {
  for (const action of ACTIONS) {
    assert.equal(typeof DEFAULT_LOCAL[action.id], "string", `${action.id} has no default key`);
    const global = DEFAULT_GLOBAL[action.id];
    if (global !== null) assert.equal(isGlobalSafe(global), true, `${action.id} is unsafe globally`);
  }
  assert.deepEqual(DEFAULT_GLOBAL, {
    "play-pause": null,
    "prev-track": null,
    "next-track": null,
    "prev-episode": "Ctrl+Shift+F6",
    "next-episode": "Ctrl+Shift+F7",
    mute: "Ctrl+Shift+F8",
    "fav-song": "Ctrl+Shift+F9",
    "save-show": "Ctrl+Shift+F10",
    random: "Ctrl+Shift+F11",
  });
});

test("focused text fields keep every key; buttons keep only what activates them", () => {
  assert.equal(shouldIgnoreKey({ tagName: "INPUT" }, "M"), true);
  assert.equal(shouldIgnoreKey({ tagName: "TEXTAREA" }, "M"), true);
  assert.equal(shouldIgnoreKey({ tagName: "SELECT" }, "M"), true);
  assert.equal(shouldIgnoreKey({ tagName: "DIV", isContentEditable: true }, "M"), true);
  // The hidden range behind the seek track: arrows are the only keyboard seek.
  assert.equal(shouldIgnoreKey({ tagName: "INPUT" }, "ArrowLeft"), true);
  // Clicking the play button focuses it, and Space would then double-fire.
  assert.equal(shouldIgnoreKey({ tagName: "BUTTON" }, "Space"), true);
  assert.equal(shouldIgnoreKey({ tagName: "A" }, "Enter"), true);
  // Everything else still works with a control focused.
  assert.equal(shouldIgnoreKey({ tagName: "BUTTON" }, "M"), false);
  assert.equal(shouldIgnoreKey({ tagName: "BUTTON" }, "Ctrl+Space"), false);
  assert.equal(shouldIgnoreKey({ tagName: "DIV" }, "Space"), false);
  assert.equal(shouldIgnoreKey(null, "Space"), false);
});

test("saved settings survive a round trip", () => {
  const saved = serializeShortcuts({
    enabled: false,
    local: { ...DEFAULT_LOCAL, mute: "N" },
    global: { ...DEFAULT_GLOBAL, mute: null },
  });
  const read = parseStored(saved);

  assert.equal(read.enabled, false);
  assert.equal(read.local.mute, "N");
  assert.equal(read.global.mute, null);
  assert.equal(read.local["play-pause"], "Space");
  assert.equal(read.needsSave, false);
  assert.equal(JSON.parse(saved).v, 2);
});

test("v1 Ctrl+Alt globals migrate to the replacement preset", () => {
  const read = parseStored(JSON.stringify({
    v: 1,
    enabled: true,
    local: DEFAULT_LOCAL,
    global: {
      "play-pause": null,
      "prev-track": null,
      "next-track": null,
      "prev-episode": "Ctrl+Alt+BracketLeft",
      "next-episode": "Ctrl+Alt+BracketRight",
      mute: "Ctrl+Alt+M",
      "fav-song": "Ctrl+Alt+F",
      "save-show": "Ctrl+Alt+S",
      random: "Ctrl+Alt+R",
    },
  }));

  assert.deepEqual(read.global, DEFAULT_GLOBAL);
  assert.equal(read.needsSave, true);
  assert.equal(JSON.parse(serializeShortcuts(read)).v, 2);
});

test("v1 migration preserves safe custom bindings and never creates a clash", () => {
  const read = parseStored(JSON.stringify({
    v: 1,
    global: {
      "prev-episode": "Ctrl+Alt+BracketLeft",
      "next-episode": "Ctrl+Alt+BracketRight",
      mute: "Ctrl+Shift+F6",
      "fav-song": "Super+F",
      "save-show": "Alt+S",
      random: "Ctrl+Alt+R",
    },
  }));

  // The custom mute binding owns F6, so the migrated previous-episode action clears.
  assert.equal(read.global["prev-episode"], null);
  assert.equal(read.global["next-episode"], "Ctrl+Shift+F7");
  assert.equal(read.global.mute, "Ctrl+Shift+F6");
  assert.equal(read.global["fav-song"], "Super+F");
  // Alt-only was unsafe but was never one of the Ctrl+Alt bindings eligible to migrate.
  assert.equal(read.global["save-show"], null);
  assert.equal(read.global.random, "Ctrl+Shift+F11");
  assert.deepEqual(duplicates(read.global), []);
});

test("v2 records never revive a text-entry global binding", () => {
  const read = parseStored(JSON.stringify({
    v: 2,
    global: {
      mute: "Alt+M",
      "save-show": "Ctrl+Alt+S",
      random: "Ctrl+Shift+F12",
    },
  }));

  assert.equal(read.global.mute, null);
  assert.equal(read.global["save-show"], null);
  assert.equal(read.global.random, "Ctrl+Shift+F12");
  assert.equal(read.needsSave, false);
});

test("settings written before the master switch existed read as on", () => {
  // Every install upgrading into this feature has a blob with no `enabled` key,
  // and a missing switch must not read as off.
  assert.equal(parseStored('{"v":1,"local":{},"global":{}}').enabled, true);
  assert.equal(parseStored("{}").enabled, true);
  assert.equal(parseStored(null).enabled, true);
  assert.equal(parseStored("not json").enabled, true);
  assert.equal(parseStored('{"enabled":false}').enabled, false);
});

test("an unreadable or half-written file falls back to the defaults", () => {
  for (const raw of [null, "", "not json", "[]", '{"local":42}']) {
    assert.deepEqual(parseStored(raw).local, DEFAULT_LOCAL, `bad blob: ${raw}`);
    assert.deepEqual(parseStored(raw).global, DEFAULT_GLOBAL, `bad blob: ${raw}`);
  }
});

test("an action that no longer exists is dropped, and a new one keeps its default", () => {
  const read = parseStored(
    '{"v":1,"enabled":true,"local":{"mute":"N","shave-yak":"Y"},"global":{}}',
  );

  assert.equal(read.local.mute, "N");
  assert.equal("shave-yak" in read.local, false);
  // Untouched actions, including ones added after this file was written.
  assert.equal(read.local.random, DEFAULT_LOCAL.random);
  assert.equal(read.global.random, DEFAULT_GLOBAL.random);
});

test("a stored key that stopped being bindable unbinds instead of poisoning the tier", () => {
  const read = parseStored('{"local":{"mute":"Hyper+M","random":"Escape"}}');

  assert.equal(read.local.mute, null);
  assert.equal(read.local.random, null);
  assert.equal(read.local["play-pause"], "Space");
});

test("bindings read back the way they were typed", () => {
  assert.equal(accelLabel("Ctrl+Alt+BracketRight"), "Ctrl + Alt + ]");
  assert.equal(accelLabel("Space"), "Space");
  assert.equal(accelLabel("Super+M"), "Win + M");
  assert.equal(accelLabel("MediaPlayPause"), "Media play/pause");
  assert.equal(accelLabel(null), "");
});
