export const BACKEND_URL = process.env.BACKEND_URL ?? "http://localhost:8080";

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function validateId(id: string): string {
	if (!UUID_RE.test(id)) throw new Error("invalid_id");
	return id;
}

export type ApiResult<T> = { ok: true; data: T } | { ok: false; error: string; retryAfter?: number | undefined };

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<ApiResult<T>> {
	try {
		const res = await fetch(`${BACKEND_URL}${path}`, {
			...init,
			headers: {
				"Content-Type": "application/json",
				...(init?.headers ?? {}),
			},
		});

		const body = (await res.json()) as T | { error: string; retry_after?: number };

		if (!res.ok) {
			const err = body as { error: string; retry_after?: number };
			return {
				error: err.error ?? "server_error",
				ok: false as const,
				retryAfter: err.retry_after,
			};
		}

		return { data: body as T, ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
