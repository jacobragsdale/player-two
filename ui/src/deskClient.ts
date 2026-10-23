import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Event, Request, Response } from "./desk.ts";

/** Sends one request to the desk. The types are written from the desk's Rust, so they hold on both sides. */
export async function ask(request: Request): Promise<Response> {
  return invoke<Response>("desk", { request });
}

/** Hears what the desk says unasked, until the returned function is called. */
export function onDesk(handler: (event: Event) => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  listen<Event>("desk", (event) => {
    handler(event.payload);
  }).then(
    (stop) => {
      if (stopped) {
        stop();
      } else {
        unlisten = stop;
      }
    },
    (error: unknown) => {
      console.error(error);
    }
  );
  return () => {
    stopped = true;
    unlisten?.();
  };
}

/** A failure from the desk, in its own words. */
export function problem(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  return error instanceof Error ? error.message : String(error);
}
