/** What the window shows instead of the chats when the hub runs another version than this app. */

import type { JSX } from "react";

export function WrongVersion({ hub, app }: { readonly hub: string; readonly app: string }): JSX.Element {
  return (
    <main className="mx-auto flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-3 pt-[15vh]">
      <h1 className="text-xl font-bold">Player Two needs updating</h1>
      <p>
        Your team&rsquo;s hub runs version {hub}, and this app is version {app}. They must match before you can chat.
      </p>
    </main>
  );
}
