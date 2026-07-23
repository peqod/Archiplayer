import { readFile } from "node:fs/promises";

const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const tauriConfig = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
const cargoToml = await readFile(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8");
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

// Cargo.lock has several packages pinned to the same version, so anchor to the
// archiplayer entry specifically (name line immediately followed by version).
const cargoLock = await readFile(new URL("../src-tauri/Cargo.lock", import.meta.url), "utf8");
const cargoLockVersion = cargoLock.match(/name = "archiplayer"\r?\nversion = "([^"]+)"/)?.[1];

const versions = new Set([packageJson.version, tauriConfig.version, cargoVersion, cargoLockVersion]);

if (versions.size !== 1 || versions.has(undefined)) {
  throw new Error(
    `Version mismatch: package=${packageJson.version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}, cargo-lock=${cargoLockVersion}`,
  );
}

const tag = process.argv[2];
if (tag && tag !== `v${packageJson.version}`) {
  throw new Error(`Tag ${tag} does not match project version v${packageJson.version}`);
}

console.log(`Release version v${packageJson.version} is consistent.`);
