"use server";

import { cookies } from "next/headers";
import { BACKEND_URL } from "@/lib/api";

async function authToken(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function getMigrationStatus(): Promise<{
	status: string;
	role: string;
	target_url: string | null;
	agents_total: number;
	agents_confirmed: number;
	error_message: string | null;
	started_at: string | null;
} | null> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration`, {
			headers: { Authorization: `Bearer ${tok}` },
			cache: "no-store",
		});
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}

export async function prepareMigration(): Promise<{
	ok: boolean;
	migration_token?: string;
	error?: string;
}> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/prepare`, {
			method: "POST",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return { ok: false, error: `${res.status}` };
		const data = (await res.json()) as { migration_token: string };
		return { ok: true, migration_token: data.migration_token };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function startMigration(
	targetUrl: string,
	migrationToken: string,
): Promise<{ ok: boolean; error?: string }> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/start`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${tok}`,
			},
			body: JSON.stringify({ target_url: targetUrl, migration_token: migrationToken }),
		});
		if (!res.ok) return { ok: false, error: `${res.status}` };
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function abortMigration(): Promise<{ ok: boolean; error?: string }> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/abort`, {
			method: "POST",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return { ok: false, error: `${res.status}` };
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function confirmMigrationShutdown(): Promise<{
	ok: boolean;
	error?: string;
}> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/confirm-shutdown`, {
			method: "POST",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return { ok: false, error: `${res.status}` };
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
