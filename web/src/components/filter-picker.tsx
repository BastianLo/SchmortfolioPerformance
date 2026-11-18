import * as Popover from "@radix-ui/react-popover";
import { Briefcase, Filter, Wallet } from "lucide-react";
import { useAccounts, usePortfolios } from "@/api/hooks";
import { useClientId } from "@/lib/client-id";
import { useFilter } from "@/lib/filter-context";
import { cn } from "@/lib/utils";

export function FilterPicker() {
  const clientId = useClientId();
  const { data: accounts } = useAccounts(clientId);
  const { data: portfolios } = usePortfolios(clientId);
  const { selected, setSelected } = useFilter();
  const toggle = (id: string) =>
    setSelected(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]);
  const label = selected.length === 0 ? "Alle" : `${selected.length} ausgewählt`;
  const item = (id: string, name: string, icon: React.ReactNode, retired: boolean) => (
    <label
      key={id}
      className={cn(
        "flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-surface-2",
        retired && "text-muted",
      )}
    >
      <input type="checkbox" className="size-4" checked={selected.includes(id)} onChange={() => toggle(id)} />
      {icon}
      <span className="truncate">{name}</span>
    </label>
  );
  return (
    <Popover.Root>
      <Popover.Trigger
        className={cn(
          "inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg bg-surface-2 px-2.5 text-xs font-medium text-muted",
          selected.length > 0 && "bg-accent/15 text-accent",
        )}
      >
        <Filter className="size-3.5" />
        {label}
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          className="z-50 w-64 rounded-xl border border-border bg-surface p-2 shadow-xl"
        >
          <div className="px-2 pb-1 text-xs font-medium text-muted">Depots</div>
          {portfolios?.map((p) => item(p.id, p.name, <Briefcase className="size-3.5 text-muted" />, p.retired))}
          <div className="px-2 pt-2 pb-1 text-xs font-medium text-muted">Konten</div>
          {accounts?.map((a) => item(a.id, a.name, <Wallet className="size-3.5 text-muted" />, a.retired))}
          <button
            type="button"
            onClick={() => setSelected([])}
            className="mt-2 w-full cursor-pointer rounded-md px-2 py-1.5 text-left text-xs text-accent hover:bg-surface-2"
          >
            Alle anzeigen
          </button>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
