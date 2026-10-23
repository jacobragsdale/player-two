/** Text someone else wrote, drawn as text and nothing else: React escapes it, and it is never parsed as HTML or Markdown. */

import type { JSX } from "react";

export function Text({ text }: { readonly text: string }): JSX.Element {
  return <p className="wrap-anywhere whitespace-pre-wrap">{text}</p>;
}
