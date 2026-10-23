import type { JSX } from "react";

import { title } from "./title.ts";

export function App(): JSX.Element {
  return <main>{title()}</main>;
}
