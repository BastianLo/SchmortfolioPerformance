import * as React from "react";
import { type PeriodKey, periodRange } from "./period";
import { setStorage, storage } from "./utils";

type Ctx = { period: PeriodKey; setPeriod: (p: PeriodKey) => void; range: { from?: string; to?: string } };
const PeriodContext = React.createContext<Ctx | null>(null);

export function PeriodProvider({ children }: { children: React.ReactNode }) {
  const [period, set] = React.useState<PeriodKey>(() => storage<PeriodKey>("period", "1y"));
  const range = React.useMemo(() => periodRange(period), [period]);
  const setPeriod = (p: PeriodKey) => {
    set(p);
    setStorage("period", p);
  };
  return <PeriodContext.Provider value={{ period, setPeriod, range }}>{children}</PeriodContext.Provider>;
}

export function usePeriod() {
  const c = React.useContext(PeriodContext);
  if (!c) throw new Error("PeriodProvider missing");
  return c;
}
