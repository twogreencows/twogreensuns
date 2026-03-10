/**
 * Server-side API client.
 * Runs only on the Next.js Node process (Server Components, Route Handlers).
 * Reads TWOGREENSUNS_SERVER_URL from the environment and calls the Rust server directly.
 */

import type { StatusResponse, AppConfig } from "./types";

const BASE = `${process.env.TWOGREENSUNS_SERVER_URL ?? "http://twogreensuns.local:8080"}/api/v1`;

async function get<T>(path: string, revalidate = 10): Promise<T> {
  const res = await fetch(`${BASE}/${path}`, { next: { revalidate } });
  if (!res.ok) throw new Error(`GET ${path} → HTTP ${res.status}`);
  return res.json() as Promise<T>;
}

async function post(path: string): Promise<void> {
  const res = await fetch(`${BASE}/${path}`, { method: "POST", cache: "no-store" });
  if (!res.ok) throw new Error(`POST ${path} → HTTP ${res.status}`);
}

async function put<B>(path: string, body: B): Promise<void> {
  const res = await fetch(`${BASE}/${path}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    cache: "no-store",
  });
  if (!res.ok) throw new Error(`PUT ${path} → HTTP ${res.status}`);
}

export const serverApi = {
  status: ()                  => get<StatusResponse>("status"),
  getConfig: ()               => get<AppConfig>("config", 0),
  putConfig: (cfg: AppConfig) => put("config", cfg),
  setLogLevel: (level: string) => put("log/level", { level }),
  teslaDiscover: ()           => post("tesla/discover"),
  teslaStart: ()              => post("tesla/charge/start"),
  teslaStop: ()               => post("tesla/charge/stop"),
  teslaSetAmps: (n: number)   => post(`tesla/charge/amps/${n}`),
};
