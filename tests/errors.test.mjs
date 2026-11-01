import assert from "node:assert/strict";
import { errorText, friendlyError, isOfflineError } from "../src/lib/errors.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("a dropped connection reads as one short line, never as the reqwest chain", () => {
  const raw =
    "request failed: error sending request for url (https://wfmu.org/playlists/): " +
    "error trying to connect: dns error: failed to lookup address information";
  assert.equal(isOfflineError(raw), true);
  assert.equal(friendlyError(raw), "No connection.");
});

test("the winsock and webview spellings of offline count too", () => {
  assert.equal(isOfflineError("request failed: os error 11001"), true);
  assert.equal(isOfflineError(new TypeError("Failed to fetch")), true);
  assert.equal(isOfflineError("read body failed: operation timed out"), true);
  assert.equal(friendlyError(new TypeError("Failed to fetch")), "No connection.");
});

test("a reachable server that answers badly is not reported as no connection", () => {
  assert.equal(isOfflineError("HTTP 503 for https://wfmu.org/playlists/"), false);
  assert.equal(friendlyError("HTTP 503 for https://wfmu.org/playlists/"), "WFMU is not responding right now.");
  assert.equal(friendlyError("HTTP 404 for https://wfmu.org/playlists/ZZ"), "That page is gone from WFMU.");
  assert.equal(friendlyError("HTTP 403 for https://wfmu.org/playlists/"), "WFMU refused the request.");
});

test("local library trouble gets its own line", () => {
  assert.equal(friendlyError("db error: no such column: air_date"), "Could not read the local library.");
});

test("browser media errors map by name, never by their developer text", () => {
  const abort = { name: "AbortError", message: "The play() request was interrupted by a call to pause()." };
  assert.equal(friendlyError(abort), "Playback stopped.");
  assert.equal(friendlyError({ name: "NotAllowedError", message: "play() failed" }), "Press play to start.");
  assert.equal(
    friendlyError({ name: "NotSupportedError", message: "The element has no supported sources." }),
    "This audio can't be played. The archive may be unavailable.",
  );
  assert.equal(friendlyError({ name: "SecurityError", message: "x" }), "Playback was blocked.");
});

test("known backend messages get plain copy", () => {
  assert.equal(friendlyError("HTTP 500 downloading audio"), "The download failed. Try again later.");
  assert.equal(friendlyError("database lock poisoned"), "Something went wrong. Restart Archiplayer.");
  assert.equal(
    friendlyError("catalog parse produced no shows (site layout changed?)"),
    "Could not read the WFMU show list. Try again later.",
  );
  assert.equal(friendlyError("the listen.m3u response carries no audio source"), "No audio found for this episode.");
  assert.equal(friendlyError("audio URL on evil.example is outside /x"), "No audio found for this episode.");
  assert.equal(friendlyError("download destination already exists: C:\\\\a.mp3"), "This episode is already downloaded.");
  assert.equal(friendlyError("Access is denied. (os error 5)"), "Could not save the download. Check the folder.");
  assert.equal(friendlyError("show page was empty"), "This show has no episodes yet.");
});

test("log-like text never reaches the user", () => {
  assert.equal(friendlyError("thread 'main' panicked at src/lib.rs:10:5"), "Something went wrong. Try again.");
  assert.equal(friendlyError("bad thing at https://wfmu.org/x"), "Something went wrong. Try again.");
  assert.equal(friendlyError("x".repeat(120)), "Something went wrong. Try again.");
  assert.equal(friendlyError("tauri::ipc failed", "The download failed."), "The download failed.");
});

test("short plain text stays readable as a sentence", () => {
  assert.equal(friendlyError("That episode has no audio archive."), "That episode has no audio archive.");
  assert.equal(friendlyError(new Error("Error: unknown show KG")), "Unknown show KG.");
  assert.equal(friendlyError("this episode is already downloading"), "This episode is already downloading.");
  assert.equal(friendlyError(""), "Something went wrong.");
});

test("error text is pulled from strings, Errors, and message-carrying objects alike", () => {
  assert.equal(errorText("plain"), "plain");
  assert.equal(errorText(new Error("boom")), "boom");
  assert.equal(errorText({ message: "wrapped" }), "wrapped");
});
