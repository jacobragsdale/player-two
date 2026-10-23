import { invoke } from "@tauri-apps/api/core";

import type { Request, Response } from "./desk.ts";

/** Sends one request to the desk. The types are written from the desk's Rust, so they hold on both sides. */
export async function ask(request: Request): Promise<Response> {
  return invoke<Response>("desk", { request });
}
