"use server";

import { cookies } from "next/headers";
import { BACKEND_URL, validateId } from "@/lib/api";

export async function revokeSession(sessionId: string): Promise<{ ok: boolean }> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		const sid = validateId(sessionId);
		const res = await fetch(`${BACKEND_URL}/admin/sessions/${sid}`, {
			headers: { Authorization: `Bearer ${token}` },
			method: "DELETE",
		});
		return { ok: res.ok };
	} catch {
		return { ok: false };
	}
}
