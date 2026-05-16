export const BACKEND_URL =
	process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

export type ApiResult<T> =
	| { ok: true; data: T }
	| { ok: false; error: string; retryAfter?: number | undefined };

export async function apiFetch<T>(
	path: string,
	init?: RequestInit,
): Promise<ApiResult<T>> {
	try {
		const res = await fetch(`${BACKEND_URL}${path}`, {
			...init,
			headers: {
				"Content-Type": "application/json",
				...(init?.headers ?? {}),
			},
		});

		const body = (await res.json()) as
			| T
			| { error: string; retry_after?: number };

		if (!res.ok) {
			const err = body as { error: string; retry_after?: number };
			return {
				ok: false as const,
				error: err.error ?? "server_error",
				retryAfter: err.retry_after,
			};
		}

		return { ok: true, data: body as T };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
