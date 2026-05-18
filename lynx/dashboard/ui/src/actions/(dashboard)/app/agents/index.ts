"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function resolveNftables(
	agentId: string,
	action: "restore" | "accept",
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/agents/${agentId}/nftables-resolve`,
			{
				method: "POST",
				headers: {
					Authorization: `Bearer ${await token()}`,
					"Content-Type": "application/json",
				},
				body: JSON.stringify({ action }),
			},
		);
		if (!res.ok) {
			const body = (await res.json()) as { error?: string; detail?: string };
			return { ok: false, error: body.detail ?? body.error ?? "server_error" };
		}
		revalidatePath("/[locale]/app/agents", "page");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
