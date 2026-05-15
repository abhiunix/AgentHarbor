import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(__dirname, "..");

function resolveTauriBinary(): string {
  const overrideEnv = process.env.AGENTHARBOR_TAURI_BIN;
  if (overrideEnv && existsSync(overrideEnv)) return overrideEnv;
  const candidates = [
    resolve(projectRoot, "src-tauri/target/debug/agentharbor"),
    resolve(projectRoot, "src-tauri/target/release/agentharbor"),
  ];
  const found = candidates.find((p) => existsSync(p));
  if (!found) {
    throw new Error(
      `Tauri debug binary not found. Build it first:\n  npm run tauri build -- --debug --no-bundle\nOr set AGENTHARBOR_TAURI_BIN to a built binary path.`,
    );
  }
  return found;
}

let tauriDriver: ChildProcessWithoutNullStreams | null = null;

export const config: WebdriverIO.Config = {
  runner: "local",
  tsConfigPath: resolve(__dirname, "tsconfig.json"),
  specs: [resolve(__dirname, "tests/**/*.spec.ts")],
  maxInstances: 1,
  capabilities: [
    {
      "tauri:options": {
        application: resolveTauriBinary(),
      },
      browserName: "wry",
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "warn",
  bail: 0,
  waitforTimeout: 15_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 3,
  hostname: "127.0.0.1",
  port: 4444,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 120_000,
  },

  onPrepare() {
    // Probe but don't gate on `--version` — older tauri-driver releases either
    // omit the flag or print to stderr and exit non-zero. The real liveness
    // check is the `spawn` in beforeSession; if the binary is missing there,
    // tests will fail with a clear error.
    const check = spawnSync("tauri-driver", ["--version"], { encoding: "utf-8" });
    if (check.error) {
      // eslint-disable-next-line no-console
      console.warn(
        "[wdio] tauri-driver --version probe errored (continuing):",
        check.error.message,
      );
    }
  },

  beforeSession() {
    tauriDriver = spawn("tauri-driver", [], { stdio: "pipe" });
    tauriDriver.stderr.on("data", (chunk) => {
      process.stderr.write(`[tauri-driver] ${chunk}`);
    });
  },

  afterSession() {
    if (tauriDriver) {
      tauriDriver.kill("SIGTERM");
      tauriDriver = null;
    }
  },
};
