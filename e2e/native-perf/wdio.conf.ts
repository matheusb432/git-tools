import { spawn, type ChildProcess } from "node:child_process";
import { createConnection, createServer, type Server } from "node:net";
import { captureFailureScreenshot, evidenceEnabled, flushEvidence, pruneEvidenceRoots } from "./src/evidence";

type PortReservation = {
  readonly server: Server;
  readonly port: number;
};

let tauriDriver: ChildProcess | null = null;
let shuttingDown = false;
let driverReservation: PortReservation | null = await reservePort();
let nativeDriverReservation: PortReservation | null = await reservePort();

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}

function requiredReservation(reservation: PortReservation | null, label: string): PortReservation {
  if (reservation === null) {
    throw new Error(`Missing reserved ${label} port`);
  }
  return reservation;
}

const driverPort = requiredReservation(driverReservation, "tauri-driver").port;
const nativeDriverPort = requiredReservation(nativeDriverReservation, "native driver").port;
process.env.GTL_NATIVE_PERF_TAURI_DRIVER_PORT = String(driverPort);
process.env.GTL_NATIVE_PERF_NATIVE_DRIVER_PORT = String(nativeDriverPort);

function closeTauriDriver(): void {
  shuttingDown = true;
  tauriDriver?.kill();
  tauriDriver = null;
}

function installShutdownHook(signal: NodeJS.Signals): void {
  process.on(signal, () => {
    closeTauriDriver();
    process.exit(signal === "SIGINT" ? 130 : 143);
  });
}

process.on("exit", closeTauriDriver);
installShutdownHook("SIGINT");
installShutdownHook("SIGTERM");

async function reservePort(): Promise<PortReservation> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });

  const address = server.address();
  if (address === null || typeof address === "string") {
    server.close();
    throw new Error("Could not determine reserved localhost port");
  }

  return { server, port: address.port };
}

async function releasePort(reservation: PortReservation): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    reservation.server.close((error) => {
      if (error === undefined) resolve();
      else reject(error);
    });
  });
}

async function releaseReservedPorts(): Promise<void> {
  const reservations = [driverReservation, nativeDriverReservation].flatMap((value) => (value === null ? [] : [value]));
  driverReservation = null;
  nativeDriverReservation = null;
  await Promise.all(reservations.map((reservation) => releasePort(reservation)));
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function waitForPortReady(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() <= deadline) {
    const connected = await new Promise<boolean>((resolve) => {
      const socket = createConnection({ host: "127.0.0.1", port });
      const finish = (value: boolean): void => {
        socket.removeAllListeners();
        socket.destroy();
        resolve(value);
      };

      socket.once("connect", () => finish(true));
      socket.once("error", () => finish(false));
    });

    if (connected) {
      return;
    }

    if (tauriDriver?.exitCode !== null) {
      throw new Error(`tauri-driver exited before accepting connections on port ${port}`);
    }

    await sleep(25);
  }

  throw new Error(`tauri-driver did not become ready on port ${port} within ${timeoutMs}ms`);
}

export const config = {
  host: "127.0.0.1",
  port: driverPort,
  logLevel: "warn",
  specs: ["./specs/native-diff.perf.ts", "./specs/native-diff.functional.ts"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application: requiredEnv("GTL_NATIVE_PERF_APP_BINARY"),
      },
    },
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    timeout: 120_000,
  },
  onPrepare: (): void => {
    if (evidenceEnabled()) {
      pruneEvidenceRoots();
    }
  },
  beforeSession: async (): Promise<void> => {
    await releaseReservedPorts();

    tauriDriver = spawn(
      requiredEnv("GTL_NATIVE_PERF_TAURI_DRIVER"),
      ["--port", String(driverPort), "--native-port", String(nativeDriverPort)],
      {
        env: {
          ...process.env,
        },
        stdio: "inherit",
      },
    );

    tauriDriver.on("error", (error) => {
      console.error("tauri-driver failed to start:", error);
    });

    tauriDriver.on("exit", (code) => {
      if (!shuttingDown) {
        console.error(`tauri-driver exited unexpectedly with code ${code}`);
      }
    });

    await waitForPortReady(driverPort, 15_000);
  },
  afterTest: async (test: { title: string }, _context: unknown, result: { error?: unknown }): Promise<void> => {
    if (result.error !== undefined) {
      await captureFailureScreenshot(test.title);
    }
  },
  after: async (): Promise<void> => {
    if (typeof browser !== "undefined") {
      await flushEvidence(browser.capabilities);
    }
  },
  afterSession: (): void => {
    closeTauriDriver();
  },
  onComplete: (): void => {
    closeTauriDriver();
  },
};
