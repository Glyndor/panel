"use server";

import { cookies } from "next/headers";
import { BACKEND_URL } from "@/lib/api";

export async function changePassword(
	currentPassword: string,
	newPassword: string,
): Promise<{ ok: boolean; status?: number }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/auth/change-password`, {
			body: JSON.stringify({
				current_password: currentPassword,
				new_password: newPassword,
			}),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});

		if (res.ok) {
			jar.delete("access_token");
			jar.delete("refresh_token");
		}

		return { ok: res.ok, status: res.status };
	} catch {
		return { ok: false };
	}
}

export async function getMe(): Promise<{
	id: string;
	username: string;
	is_admin: boolean;
	single_session: boolean;
} | null> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/auth/me`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}

export async function toggleSingleSession(enabled: boolean): Promise<{ ok: boolean }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/auth/me/single-session`, {
			body: JSON.stringify({ enabled }),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		return { ok: res.ok };
	} catch {
		return { ok: false };
	}
}
