/**
 * Catch-all proxy Route Handler.
 * Forwards every /api/v1/* request from the browser to the Rust server.
 * The Rust server URL never leaves the Next.js process.
 */

import { NextRequest, NextResponse } from "next/server";

const RUST_SERVER =
  process.env.TWOGREENSUNS_SERVER_URL ?? "http://twogreensuns.local:8080";

type RouteContext = { params: Promise<{ path: string[] }> };

async function proxy(req: NextRequest, ctx: RouteContext): Promise<NextResponse> {
  const { path } = await ctx.params;
  const rustUrl = `${RUST_SERVER}/api/v1/${path.join("/")}`;

  const init: RequestInit = { method: req.method };

  const contentType = req.headers.get("content-type");
  if (contentType) {
    init.headers = { "content-type": contentType };
  }

  if (req.method !== "GET" && req.method !== "HEAD") {
    const text = await req.text();
    if (text) init.body = text;
  }

  const rustRes = await fetch(rustUrl, init);
  const body = await rustRes.text();

  return new NextResponse(body, {
    status: rustRes.status,
    headers: {
      "content-type": rustRes.headers.get("content-type") ?? "application/json",
    },
  });
}

export const GET  = (req: NextRequest, ctx: RouteContext) => proxy(req, ctx);
export const POST = (req: NextRequest, ctx: RouteContext) => proxy(req, ctx);
export const PUT  = (req: NextRequest, ctx: RouteContext) => proxy(req, ctx);
