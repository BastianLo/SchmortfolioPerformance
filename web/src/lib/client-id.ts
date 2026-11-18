import { useParams } from "@tanstack/react-router";

export function useClientId(): string {
  const p = useParams({ strict: false }) as { clientId?: string };
  if (!p.clientId) throw new Error("no client in route");
  return p.clientId;
}
