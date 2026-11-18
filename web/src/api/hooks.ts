import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "./client";

type Period = { from?: string; to?: string };

export const qk = {
  me: ["me"] as const,
  authConfig: ["auth-config"] as const,
  clients: ["clients"] as const,
  client: (id: string) => ["client", id] as const,
};

export function useAuthConfig() {
  return useQuery({ queryKey: qk.authConfig, queryFn: () => unwrap(api.GET("/auth/config")) });
}

export function useMe() {
  return useQuery({ queryKey: qk.me, queryFn: () => unwrap(api.GET("/auth/me")), retry: false });
}

export function useClients() {
  return useQuery({ queryKey: qk.clients, queryFn: () => unwrap(api.GET("/clients")) });
}

export function useClient(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "detail"],
    queryFn: () => unwrap(api.GET("/clients/{id}", { params: { path: { id } } })),
  });
}

export function useDashboard(id: string, period: Period, taxonomy?: string, filter?: string) {
  return useQuery({
    queryKey: [...qk.client(id), "dashboard", period, taxonomy, filter],
    queryFn: () =>
      unwrap(
        api.GET("/clients/{id}/reports/dashboard", {
          params: { path: { id }, query: { ...period, taxonomy, filter } },
        }),
      ),
    placeholderData: keepPreviousData,
  });
}

export function useHoldings(id: string, date?: string, filter?: string) {
  return useQuery({
    queryKey: [...qk.client(id), "holdings", date, filter],
    queryFn: () =>
      unwrap(api.GET("/clients/{id}/reports/holdings", { params: { path: { id }, query: { date, filter } } })),
    placeholderData: keepPreviousData,
  });
}

export function usePerformance(id: string, period: Period, interval?: string, filter?: string) {
  return useQuery({
    queryKey: [...qk.client(id), "performance", period, interval, filter],
    queryFn: () =>
      unwrap(
        api.GET("/clients/{id}/reports/performance", {
          params: { path: { id }, query: { ...period, interval, filter } },
        }),
      ),
    placeholderData: keepPreviousData,
  });
}

export function useEarnings(id: string, period: Period, filter?: string) {
  return useQuery({
    queryKey: [...qk.client(id), "earnings", period, filter],
    queryFn: () =>
      unwrap(api.GET("/clients/{id}/reports/earnings", { params: { path: { id }, query: { ...period, filter } } })),
    placeholderData: keepPreviousData,
  });
}

export function useSecurities(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "securities"],
    queryFn: () => unwrap(api.GET("/clients/{id}/securities", { params: { path: { id } } })),
  });
}

export function useSecurity(id: string, sid: string) {
  return useQuery({
    queryKey: [...qk.client(id), "security", sid],
    queryFn: () => unwrap(api.GET("/clients/{id}/securities/{sid}", { params: { path: { id, sid } } })),
  });
}

export function usePrices(id: string, sid: string) {
  return useQuery({
    queryKey: [...qk.client(id), "prices", sid],
    queryFn: () => unwrap(api.GET("/clients/{id}/securities/{sid}/prices", { params: { path: { id, sid } } })),
  });
}

export function useAccounts(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "accounts"],
    queryFn: () => unwrap(api.GET("/clients/{id}/accounts", { params: { path: { id } } })),
  });
}

export function usePortfolios(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "portfolios"],
    queryFn: () => unwrap(api.GET("/clients/{id}/portfolios", { params: { path: { id } } })),
  });
}

export type TxQuery = {
  owner?: string;
  security?: string;
  types?: string;
  q?: string;
  from?: string;
  to?: string;
};

export function useTransactions(id: string, query: TxQuery) {
  return useQuery({
    queryKey: [...qk.client(id), "transactions", query],
    queryFn: () =>
      unwrap(api.GET("/clients/{id}/transactions", { params: { path: { id }, query: { ...query, limit: 10000 } } })),
    placeholderData: keepPreviousData,
  });
}

export function useTaxonomies(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "taxonomies"],
    queryFn: () => unwrap(api.GET("/clients/{id}/taxonomies", { params: { path: { id } } })),
  });
}

export function useTaxonomy(id: string, tid: string | undefined, filter?: string) {
  return useQuery({
    queryKey: [...qk.client(id), "taxonomy", tid, filter],
    queryFn: () =>
      unwrap(api.GET("/clients/{id}/reports/taxonomies/{tid}", { params: { path: { id, tid: tid as string } } })),
    enabled: !!tid,
  });
}

export function useExchangeRates() {
  return useQuery({ queryKey: ["fx"], queryFn: () => unwrap(api.GET("/exchange-rates")) });
}

/** Invalidates everything that belongs to a client after a write. */
export function useInvalidateClient(id: string) {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: qk.client(id) });
}

export function useDeleteTransaction(id: string) {
  const invalidate = useInvalidateClient(id);
  return useMutation({
    mutationFn: (tid: string) =>
      unwrap(api.DELETE("/clients/{id}/transactions/{tid}", { params: { path: { id, tid } } })),
    onSuccess: invalidate,
  });
}

export function usePlans(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "plans"],
    queryFn: () => unwrap(api.GET("/clients/{id}/plans", { params: { path: { id } } })),
  });
}

export function useWatchlists(id: string) {
  return useQuery({
    queryKey: [...qk.client(id), "watchlists"],
    queryFn: () => unwrap(api.GET("/clients/{id}/watchlists", { params: { path: { id } } })),
  });
}
