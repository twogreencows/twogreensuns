/**
 * Client-side API client.
 * Runs in the browser ("use client" components).
 * Calls /api/v1/... on the Next.js server, which proxies to the Rust server.
 * Never talks to the Rust server directly.
 */

import type { StatusResponse, AppConfig } from "./types";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`/api/v1/${path}`);
  if (!res.ok) throw new Error(`GET ${path} → HTTP ${res.status}`);
  return res.json() as Promise<T>;
}

async function post(path: string): Promise<void> {
  const res = await fetch(`/api/v1/${path}`, { method: "POST" });
  if (!res.ok) throw new Error(`POST ${path} → HTTP ${res.status}`);
}

async function put<B>(path: string, body: B): Promise<void> {
  const res = await fetch(`/api/v1/${path}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`PUT ${path} → HTTP ${res.status}`);
}

export const clientApi = {
  status: ()                   => get<StatusResponse>("status"),
  getConfig: ()                => get<AppConfig>("config"),
  putConfig: (cfg: AppConfig)  => put("config", cfg),
  setLogLevel: (level: string) => put("log/level", { level }),
  teslaDiscover: ()            => post("tesla/discover"),
  teslaStart: ()               => post("tesla/charge/start"),
  teslaStop: ()                => post("tesla/charge/stop"),
  teslaSetAmps: (n: number)    => post(`tesla/charge/amps/${n}`),
};
