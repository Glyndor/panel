"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { BACKEND_URL, validateId } from "@/lib/api";

async function tok(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export interface NftRule {
	agent_id: string | null;
	created_at: string;
	description: string | null;
	enabled: boolean;
	id: string;
	ip_list: string[];
	ip_version: string;
	kind: string;
	port: number | null;
	priority: number;
	protocol: string | null;
	rate_per_min: number | null;
	scope: "global" | "local";
}

export interface CreateRulePayload {
	description?: string;
	ip_list?: string[];
	kind: string;
	port?: number;
	priority?: number;
	protocol?: string;
	rate_per_min?: number;
}

// --- Global rules ---

export async function listGlobalRules(): Promise<NftRule[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/nftables/global`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${await tok()}` },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

export async function createGlobalRule(payload: CreateRulePayload): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(`${BACKEND_URL}/nftables/global`, {
			body: JSON.stringify(payload),
			headers: {
				Authorization: `Bearer ${await tok()}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { error: body.error ?? "server_error", ok: false };
		}
		revalidatePath("/app/agents");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function deleteGlobalRule(ruleId: string): Promise<{ ok: boolean; error?: string }> {
	try {
		const id = validateId(ruleId);
		const res = await fetch(`${BACKEND_URL}/nftables/global/${id}`, {
			headers: { Authorization: `Bearer ${await tok()}` },
			method: "DELETE",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		revalidatePath("/app/agents");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function pushGlobalRules(): Promise<{
	ok: boolean;
	pushed?: number;
	failed?: number;
	error?: string;
}> {
	try {
		const res = await fetch(`${BACKEND_URL}/nftables/global/push`, {
			headers: { Authorization: `Bearer ${await tok()}` },
			method: "POST",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		const data = (await res.json()) as { pushed: number; failed: number };
		return { failed: data.failed, ok: true, pushed: data.pushed };
	} catch {
		return { error: "network_error", ok: false };
	}
}

// --- Local rules (per agent) ---

export async function listLocalRules(agentId: string): Promise<NftRule[]> {
	try {
		const id = validateId(agentId);
		const res = await fetch(`${BACKEND_URL}/nftables/agents/${id}/local`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${await tok()}` },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

export async function createLocalRule(
	agentId: string,
	payload: CreateRulePayload,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const id = validateId(agentId);
		const res = await fetch(`${BACKEND_URL}/nftables/agents/${id}/local`, {
			body: JSON.stringify(payload),
			headers: {
				Authorization: `Bearer ${await tok()}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { error: body.error ?? "server_error", ok: false };
		}
		revalidatePath(`/app/agents`);
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function deleteLocalRule(agentId: string, ruleId: string): Promise<{ ok: boolean; error?: string }> {
	try {
		const aid = validateId(agentId);
		const rid = validateId(ruleId);
		const res = await fetch(`${BACKEND_URL}/nftables/agents/${aid}/local/${rid}`, {
			headers: { Authorization: `Bearer ${await tok()}` },
			method: "DELETE",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		revalidatePath(`/app/agents`);
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function pushLocalRules(agentId: string): Promise<{ ok: boolean; error?: string }> {
	try {
		const id = validateId(agentId);
		const res = await fetch(`${BACKEND_URL}/nftables/agents/${id}/local/push`, {
			headers: { Authorization: `Bearer ${await tok()}` },
			method: "POST",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
