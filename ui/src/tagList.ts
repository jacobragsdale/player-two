import type { Person } from "./desk.ts";

/** The tag being typed just before the caret: what follows an `@` that starts the text or follows a space. */
const BEING_TYPED = /(?:^|\s)@([a-z0-9-]*(?:\/[a-z0-9._-]*)?)$/;

/** The partial tag before the caret, without its `@`, or null when no tag is being typed. */
export function typedTag(before: string): string | null {
  const found = BEING_TYPED.exec(before);
  return found?.[1] ?? null;
}

/** What can be tagged, `name` for a machine and `name/place` for a checkout, matching what was typed: the chat's members first, then everyone else, each person's machine before their checkouts. */
export function candidates(typed: string, people: readonly Person[], members: readonly string[]): string[] {
  const rank = (person: Person): number => (members.includes(person.name) ? 0 : 1);
  const ordered = [...people].sort((a, b) => {
    const byMembership = rank(a) - rank(b);
    return byMembership === 0 ? a.name.localeCompare(b.name) : byMembership;
  });
  const all = ordered.flatMap((person) => person.places.map((place) => (place.kind === "machine" ? person.name : `${person.name}/${place.name}`)));
  return all.filter((tag) => tag.startsWith(typed) || (typed.length > 1 && tag.includes(typed))).slice(0, 8);
}
