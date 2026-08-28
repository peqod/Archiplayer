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

test("anything else stays readable as a plain sentence", () => {
  assert.equal(friendlyError("show page was empty"), "Show page was empty.");
  assert.equal(friendlyError("That episode has no audio archive."), "That episode has no audio archive.");
  assert.equal(friendlyError(new Error("Error: unknown show KG")), "Unknown show KG.");
  assert.equal(friendlyError(""), "Something went wrong.");
});

test("error text is pulled from strings, Errors, and message-carrying objects alike", () => {
  assert.equal(errorText("plain"), "plain");
  assert.equal(errorText(new Error("boom")), "boom");
  assert.equal(errorText({ message: "wrapped" }), "wrapped");
});
