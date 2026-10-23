import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import { Waiting } from "./Run.tsx";

const mine = [{ person: "alice", place: "machine" }];

test("a tag of your own that found its place busy waits, then offers Run now at your mode", () => {
  const busy = renderToStaticMarkup(<Waiting tags={mine} me="alice" author="alice" chat="#ops" request={1} busy={["machine"]} mode="ask" />);
  expect(busy).toContain("your computer is busy");
  expect(busy).not.toContain("Run now");
  const free = renderToStaticMarkup(<Waiting tags={mine} me="alice" author="alice" chat="#ops" request={1} busy={[]} mode="full" />);
  expect(free).toContain("Run now · Full");
});

test("a teammate's tag waits for them, and its asker may withdraw it", () => {
  const page = renderToStaticMarkup(<Waiting tags={[{ person: "bob", place: "uploader" }]} me="alice" author="alice" chat="#ops" request={1} busy={[]} mode="ask" />);
  expect(page).toContain("waiting for bob to approve");
  expect(page).toContain("Withdraw");
});
