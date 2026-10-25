import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import type { RunView } from "./desk.ts";
import { Run, Waiting } from "./Run.tsx";

const mine = [{ person: "alice", place: "machine" }];

test("a tag of your own that found its place busy waits, then offers Run now", () => {
  const busy = renderToStaticMarkup(<Waiting tags={mine} me="alice" author="alice" chat="#ops" request={1} busy={["machine"]} />);
  expect(busy).toContain("your computer is busy");
  expect(busy).not.toContain("Run now");
  const free = renderToStaticMarkup(<Waiting tags={mine} me="alice" author="alice" chat="#ops" request={1} busy={[]} />);
  expect(free).toContain("Run now");
});

test("a teammate's tag waits for them, and its asker may withdraw it", () => {
  const page = renderToStaticMarkup(<Waiting tags={[{ person: "bob", place: "uploader" }]} me="alice" author="alice" chat="#ops" request={1} busy={[]} />);
  expect(page).toContain("waiting for bob to approve");
  expect(page).toContain("Withdraw");
});

function run(state: RunView["state"], steps: number): RunView {
  const activity = Array.from({ length: steps }, (_, at) => ({ id: `t${String(at)}`, verb: "read", object: `file-${String(at)}` }));
  return {
    id: 2,
    thread: 1,
    owner: "bob",
    place: "machine",
    runs_in: "windows",
    keep_going: false,
    state,
    reason: null,
    activity,
    result: state === "done" ? "Fixed." : null,
    may_stop: false,
    mine: false
  };
}

test("a working run shows its latest steps, and an ended one none, with the rest a click away", () => {
  const working = renderToStaticMarkup(<Run run={run("working", 7)} chat="#ops" me="alice" />);
  expect(working).not.toContain("file-1<");
  expect(working).toContain("file-2");
  expect(working).toContain("file-6");
  expect(working).toContain("Show all 7 steps");
  const done = renderToStaticMarkup(<Run run={run("done", 7)} chat="#ops" me="alice" />);
  expect(done).not.toContain("file-");
  expect(done).toContain("Show all 7 steps");
  expect(done).toContain("Fixed.");
  const short = renderToStaticMarkup(<Run run={run("working", 3)} chat="#ops" me="alice" />);
  expect(short).toContain("file-0");
  expect(short).not.toContain("Show all");
});
