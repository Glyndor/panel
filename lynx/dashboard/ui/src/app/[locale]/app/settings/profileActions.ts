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
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${tok}`,
			},
			body: JSON.stringify({
				current_password: currentPassword,
				new_password: newPassword,
			}),
		});

		if (res.ok) {
			// Clear cookies — all sessions were invalidated
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
} | null> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/auth/me`, {
			headers: { Authorization: `Bearer ${tok}` },
			cache: "no-store",
		});
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}
