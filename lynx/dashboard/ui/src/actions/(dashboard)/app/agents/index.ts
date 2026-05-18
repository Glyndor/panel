"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { BACKEND_URL, validateId } from "@/lib/api";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function rebootAgent(agentId: string): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";
	try {
		const id = validateId(agentId);
		const res = await fetch(`${BACKEND_URL}/agents/${id}/reboot`, {
			headers: { Authorization: `Bearer ${tok}` },
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { error: body.error ?? "server_error", ok: false };
		}
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function deleteAgent(agentId: string, locale: string): Promise<void> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";
	const id = validateId(agentId);
	await fetch(`${BACKEND_URL}/agents/${id}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "DELETE",
	});
	revalidatePath(`/${locale}/app/agents`);
	redirect(`/${locale}/app/agents`);
}

export async function resolveNftables(
	agentId: string,
	action: "restore" | "accept",
): Promise<{ ok: boolean; error?: string }> {
	try {
		const id = validateId(agentId);
		const res = await fetch(`${BACKEND_URL}/agents/${id}/nftables-resolve`, {
			body: JSON.stringify({ action }),
			headers: {
				Authorization: `Bearer ${await token()}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string; detail?: string };
			return { error: body.detail ?? body.error ?? "server_error", ok: false };
		}
		revalidatePath("/[locale]/app/agents", "page");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
