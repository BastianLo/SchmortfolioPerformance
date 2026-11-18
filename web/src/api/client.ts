import createClient from "openapi-fetch";
import type { components, paths } from "./schema";

export type Schemas = components["schemas"];

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export const api = createClient<paths>({ baseUrl: "/api/v1", credentials: "same-origin" });

/** Unwraps an openapi-fetch result, throwing `ApiError` on failure. */
export async function unwrap<T>(p: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<T> {
  const { data, error, response } = await p;
  if (!response.ok) {
    const detail = (error as { detail?: string } | undefined)?.detail ?? response.statusText ?? "Unbekannter Fehler";
    throw new ApiError(response.status, detail);
  }
  return data as T;
}

/** Upload helper for multipart forms (not covered by the typed client). */
export async function postForm<T>(path: string, form: FormData): Promise<T> {
  const res = await fetch(`/api/v1${path}`, { method: "POST", body: form, credentials: "same-origin" });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(res.status, body.detail ?? res.statusText);
  return body as T;
}
