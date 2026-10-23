/**
 * An agent's answer, drawn from its Markdown as elements and never as HTML.
 *
 * Raw HTML in it stays text: the tree's `html` nodes become text nodes before anything draws them. Images are not fetched, since a remote one would tell its host who read the
 * message: each becomes a line naming it. Links keep react-markdown's own rule, which drops `javascript:` and any other address it does not know, and open outside the app.
 */

import type { JSX } from "react";
import ReactMarkdown from "react-markdown";

/** The little of a Markdown tree's node that turning raw HTML into text needs. */
interface Node {
  readonly type: string;
  readonly children?: readonly Node[];
}

/** The same tree with every raw HTML node made a text node of the same characters. */
function asText(node: Node): Node {
  return { ...node, type: node.type === "html" ? "text" : node.type, ...(node.children === undefined ? {} : { children: node.children.map(asText) }) };
}

/** A remark plugin that hands on the tree with its raw HTML as text. */
function htmlAsText(): (tree: Node) => Node {
  return asText;
}

export function Markdown({ text }: { readonly text: string }): JSX.Element {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[htmlAsText]}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
          img: ({ alt, src }) => (
            <span className="text-muted-foreground">
              [image{alt === undefined || alt === "" ? "" : `: ${alt}`}] {typeof src === "string" ? src : ""}
            </span>
          )
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
