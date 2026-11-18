import { isoDate } from "./format";

export type PeriodKey = "1m" | "3m" | "ytd" | "1y" | "3y" | "5y" | "max";

export const PERIODS: { key: PeriodKey; label: string }[] = [
  { key: "1m", label: "1M" },
  { key: "3m", label: "3M" },
  { key: "ytd", label: "YTD" },
  { key: "1y", label: "1J" },
  { key: "3y", label: "3J" },
  { key: "5y", label: "5J" },
  { key: "max", label: "Max" },
];

/** Returns `{from, to}` query params for a period (undefined = server default). */
export function periodRange(key: PeriodKey): { from?: string; to?: string } {
  const now = new Date();
  const d = new Date(now);
  switch (key) {
    case "1m":
      d.setMonth(d.getMonth() - 1);
      break;
    case "3m":
      d.setMonth(d.getMonth() - 3);
      break;
    case "ytd":
      return { from: `${now.getFullYear() - 1}-12-31` };
    case "1y":
      d.setFullYear(d.getFullYear() - 1);
      break;
    case "3y":
      d.setFullYear(d.getFullYear() - 3);
      break;
    case "5y":
      d.setFullYear(d.getFullYear() - 5);
      break;
    case "max":
      return {};
  }
  return { from: isoDate(d) };
}
