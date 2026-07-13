import { spawn } from "node:child_process";
import { watch } from "node:fs";
import { access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

const settingsRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const projectRoot = resolve(settingsRoot, "..");
const tsc = resolve(settingsRoot, "node_modules/typescript/bin/tsc");
const vite = resolve(settingsRoot, "node_modules/vite/bin/vite.js");
const electron = resolve(settingsRoot, "node_modules/.bin/electron");
const electronOutput = resolve(settingsRoot, "dist-electron");
const rustSource = resolve(projectRoot, "src");
const devUrl = "http://127.0.0.1:5173";

let electronProcess = null;
let stopping = false;
let restartTimer;
let rustBuilding = false;
let rustBuildQueued = false;

function start(command, args, options = {}) {
  return spawn(command, args, { cwd: settingsRoot, stdio: "inherit", ...options });
}

function run(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = start(command, args, options);
    child.once("error", reject);
    child.once("exit", code => code === 0 ? resolvePromise() : reject(new Error(`${command} exited with code ${code}`)));
  });
}

async function waitForDevServer() {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(devUrl);
      if (response.ok) return;
    } catch {
      // Vite is still starting.
    }
    await delay(100);
  }
  throw new Error(`Vite did not start at ${devUrl}`);
}

function launchElectron() {
  if (stopping || electronProcess) return;
  const child = start(electron, [".", "--no-sandbox", `--user-data-dir=${join(tmpdir(), "codex-voice-settings-dev")}`], {
    env: {
      ...process.env,
      VITE_DEV_SERVER_URL: devUrl,
      CODEX_VOICE_BIN: resolve(projectRoot, "target/debug/codex-voice")
    }
  });
  electronProcess = child;
  child.once("exit", () => {
    if (electronProcess === child) electronProcess = null;
  });
}

function restartElectron() {
  if (stopping) return;
  if (!electronProcess) return launchElectron();
  const child = electronProcess;
  child.once("exit", launchElectron);
  child.kill("SIGTERM");
}

function scheduleElectronRestart() {
  clearTimeout(restartTimer);
  restartTimer = setTimeout(restartElectron, 200);
}

async function rebuildRust() {
  if (rustBuilding) {
    rustBuildQueued = true;
    return;
  }
  rustBuilding = true;
  try {
    await run("cargo", ["build"], { cwd: projectRoot });
    scheduleElectronRestart();
  } catch (error) {
    console.error(`\nRust rebuild failed; keeping the current preview running.\n${error.message}\n`);
  } finally {
    rustBuilding = false;
    if (rustBuildQueued) {
      rustBuildQueued = false;
      void rebuildRust();
    }
  }
}

function stop(child) {
  if (child && child.exitCode === null) child.kill("SIGTERM");
}

async function main() {
  await access(tsc);
  await access(vite);
  await access(electron);
  await run(process.execPath, [tsc, "-p", "tsconfig.electron.json"]);
  await run("cargo", ["build"], { cwd: projectRoot });

  const viteProcess = start(process.execPath, [vite]);
  const typecheckProcess = start(process.execPath, [tsc, "-p", "tsconfig.electron.json", "--watch"]);
  await waitForDevServer();
  launchElectron();

  watch(electronOutput, scheduleElectronRestart);
  watch(rustSource, (_event, fileName) => {
    if (!fileName) return;
    if (fileName.endsWith(".rs")) void rebuildRust();
    else if (fileName === "overlay.py") scheduleElectronRestart();
  });

  const shutdown = () => {
    if (stopping) return;
    stopping = true;
    clearTimeout(restartTimer);
    stop(electronProcess);
    stop(typecheckProcess);
    stop(viteProcess);
  };
  process.once("SIGINT", shutdown);
  process.once("SIGTERM", shutdown);
}

main().catch(error => {
  console.error(`\nCould not start Codex Voice development mode.\n${error.message}\n`);
  process.exitCode = 1;
});
