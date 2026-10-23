import type { JSX } from "react";

import { Badge } from "@/components/ui/badge";

import { title } from "./title.ts";

export function App(): JSX.Element {
  return (
    <main className="flex h-full items-center justify-center gap-2">
      <h1 className="text-lg font-semibold">{title()}</h1>
      <Badge variant="secondary">{"v1"}</Badge>
    </main>
  );
}
