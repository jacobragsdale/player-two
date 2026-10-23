import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import { ago, followsOwnRun, PersonLine } from "./Chat.tsx";
import type { ChatView, Item, RunView } from "./desk.ts";

test("a time ago is said in round words", () => {
  const now = 10 * 86_400_000;
  expect(ago(now - 20_000, now)).toBe("just now");
  expect(ago(now - 5 * 60_000, now)).toBe("5 min ago");
  expect(ago(now - 2 * 3_600_000, now)).toBe("2 h ago");
  expect(ago(now - 86_400_000, now)).toBe("a day ago");
  expect(ago(now - 3 * 86_400_000, now)).toBe("3 days ago");
});

test("a direct chat says whether the person is here and what they share", () => {
  const here = renderToStaticMarkup(
    <PersonLine
      person={{
        name: "bob",
        here: true,
        places: [
          { name: "machine", kind: "machine" },
          { name: "uploader", kind: "repo" }
        ]
      }}
    />
  );
  expect(here).toContain("here now · shares @bob, @bob/uploader");
  const away = renderToStaticMarkup(<PersonLine person={{ name: "carol", here: false, last_seen: Date.now() - 2 * 3_600_000, places: [] }} />);
  expect(away).toContain("last seen 2 h ago · no agent set up yet");
});

test("a follow-up goes to your own agent only when the newest run you asked for here is yours", () => {
  const run = (id: number, mine: boolean): RunView => ({
    id,
    thread: 1,
    owner: mine ? "alice" : "bob",
    place: "machine",
    runs_in: "windows",
    mode: "ask",
    steps_by: "owner",
    state: "done",
    reason: null,
    activity: [],
    prompts: [],
    result: null,
    may_answer: mine,
    may_stop: true,
    mine
  });
  const item = (id: number, runs: RunView[]): Item => ({ id, author: "alice", ts: 0, text: "", tags: [], replies: 0, runs, waiting: [], follows: null, unknown: [] });
  const view = (items: Item[]): ChatView => ({ chat: "#ops", title: "#ops", members: [], person: null, thread: 1, items, has_older: false, first_unread: null });
  expect(followsOwnRun(view([item(1, [run(2, true)])]), "alice")).toBe(true);
  expect(followsOwnRun(view([item(1, [run(2, true)]), item(3, [run(4, false)])]), "alice")).toBe(false);
  expect(followsOwnRun(view([item(1, [])]), "alice")).toBe(false);
});
