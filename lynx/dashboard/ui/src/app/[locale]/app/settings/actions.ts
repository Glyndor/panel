"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

export async function revokeSession(
	sessionId: string,
): Promise<{ ok: boolean }> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/admin/sessions/${sessionId}`, {
			method: "DELETE",
			headers: { Authorization: `Bearer ${token}` },
		});
		return { ok: res.ok };
	} catch {
		return { ok: false };
	}
}

export async function rotateKeys(locale: string): Promise<void> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		await fetch(`${BACKEND_URL}/admin/rotate`, {
			method: "POST",
			headers: {
				Authorization: `Bearer ${token}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify({ scope: "jwt_keys", reason: "manual" }),
		});
	} catch {
		// Rotation may still have succeeded on the backend
	}

	jar.delete("access_token");
	jar.delete("refresh_token");
	redirect(`/${locale}/login`);
}
