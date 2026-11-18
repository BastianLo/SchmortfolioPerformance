import * as React from "react";
import { setStorage, storage } from "./utils";

type Ctx = { selected: string[]; setSelected: (ids: string[]) => void; filter?: string };
const FilterContext = React.createContext<Ctx | null>(null);

/** Selected portfolios/accounts for reports (per client, remembered). */
export function FilterProvider({ clientId, children }: { clientId: string; children: React.ReactNode }) {
  const key = `filter:${clientId}`;
  const [selected, set] = React.useState<string[]>(() => storage<string[]>(key, []));
  React.useEffect(() => set(storage<string[]>(key, [])), [key]);
  const setSelected = (ids: string[]) => {
    set(ids);
    setStorage(key, ids);
  };
  const filter = selected.length ? selected.join(",") : undefined;
  return <FilterContext.Provider value={{ selected, setSelected, filter }}>{children}</FilterContext.Provider>;
}

export function useFilter() {
  const c = React.useContext(FilterContext);
  if (!c) throw new Error("FilterProvider missing");
  return c;
}
