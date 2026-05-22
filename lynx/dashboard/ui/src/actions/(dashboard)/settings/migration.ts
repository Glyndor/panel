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
			cache: "no-store",
			headers: { Authorization: `Bearer ${tok}` },
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
			headers: { Authorization: `Bearer ${tok}` },
			method: "POST",
		});
		if (!res.ok) return { error: `${res.status}`, ok: false };
		const data = (await res.json()) as { migration_token: string };
		return { migration_token: data.migration_token, ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function startMigration(
	targetUrl: string,
	migrationToken: string,
): Promise<{ ok: boolean; error?: string }> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/start`, {
			body: JSON.stringify({ migration_token: migrationToken, target_url: targetUrl }),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) return { error: `${res.status}`, ok: false };
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function abortMigration(): Promise<{ ok: boolean; error?: string }> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/abort`, {
			headers: { Authorization: `Bearer ${tok}` },
			method: "POST",
		});
		if (!res.ok) return { error: `${res.status}`, ok: false };
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function confirmMigrationShutdown(): Promise<{
	ok: boolean;
	error?: string;
}> {
	const tok = await authToken();
	try {
		const res = await fetch(`${BACKEND_URL}/migration/confirm-shutdown`, {
			headers: { Authorization: `Bearer ${tok}` },
			method: "POST",
		});
		if (!res.ok) return { error: `${res.status}`, ok: false };
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
