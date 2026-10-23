import { expect, test } from "vitest";

import type { Person } from "./desk.ts";
import { candidates, typedTag } from "./tagList.ts";

const person = (name: string, places: readonly string[]): Person => ({ name, here: true, places: places.map((place) => ({ name: place, kind: place === "machine" ? "machine" : "repo" })) });
const people = [person("bob", ["machine", "uploader"]), person("alice", ["machine", "notes"]), person("carol", [])];

test("a tag is being typed after an @ that starts a word", () => {
  expect(typedTag("@al")).toBe("al");
  expect(typedTag("hi @bob/up")).toBe("bob/up");
  expect(typedTag("mail@example")).toBeNull();
  expect(typedTag("@bob done")).toBeNull();
});

test("the chat's members come first, each machine before its checkouts", () => {
  expect(candidates("", people, ["bob"])).toEqual(["bob", "bob/uploader", "alice", "alice/notes"]);
  expect(candidates("al", people, [])).toEqual(["alice", "alice/notes"]);
  expect(candidates("up", people, [])).toEqual(["bob/uploader"]);
});
