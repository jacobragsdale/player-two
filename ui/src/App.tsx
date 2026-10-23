import { useEffect, useState } from "react";
import type { JSX } from "react";

import { Badge } from "@/components/ui/badge";

import { ask } from "./deskClient.ts";
import { title } from "./title.ts";

export function App(): JSX.Element {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    ask({ type: "hello" }).then(
      (response) => {
        setVersion(response.type === "hello" ? response.version : null);
      },
      () => {
        setVersion(null);
      }
    );
  }, []);
  return (
    <main className="flex h-full items-center justify-center gap-2">
      <h1 className="text-lg font-semibold">{title()}</h1>
      <Badge variant="secondary">{version === null ? "starting" : `desk ${version}`}</Badge>
    </main>
  );
}
