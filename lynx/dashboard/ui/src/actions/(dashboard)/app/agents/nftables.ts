"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";

async function tok(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export interface NftRule {
	id: string;
	scope: "global" | "local";
	agent_id: string | null;
	kind: string;
	port: number | null;
	protocol: string | null;
	ip_list: string[];
	ip_version: string;
	rate_per_min: number | null;
	description: string | null;
	priority: number;
	enabled: boolean;
	created_at: string;
}

export interface CreateRulePayload {
	kind: string;
	port?: number;
	protocol?: string;
	ip_list?: string[];
	rate_per_min?: number;
	description?: string;
	priority?: number;
}

// --- Global rules ---

export async function listGlobalRules(): Promise<NftRule[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/nftables/global`, {
			headers: { Authorization: `Bearer ${await tok()}` },
			cache: "no-store",
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

export async function createGlobalRule(
	payload: CreateRulePayload,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(`${BACKEND_URL}/nftables/global`, {
			method: "POST",
			headers: {
				Authorization: `Bearer ${await tok()}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify(payload),
		});
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { ok: false, error: body.error ?? "server_error" };
		}
		revalidatePath("/app/agents");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function deleteGlobalRule(
	ruleId: string,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(`${BACKEND_URL}/nftables/global/${ruleId}`, {
			method: "DELETE",
			headers: { Authorization: `Bearer ${await tok()}` },
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		revalidatePath("/app/agents");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
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
			method: "POST",
			headers: { Authorization: `Bearer ${await tok()}` },
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		const data = (await res.json()) as { pushed: number; failed: number };
		return { ok: true, pushed: data.pushed, failed: data.failed };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

// --- Local rules (per agent) ---

export async function listLocalRules(agentId: string): Promise<NftRule[]> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/nftables/agents/${agentId}/local`,
			{
				headers: { Authorization: `Bearer ${await tok()}` },
				cache: "no-store",
			},
		);
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
		const res = await fetch(
			`${BACKEND_URL}/nftables/agents/${agentId}/local`,
			{
				method: "POST",
				headers: {
					Authorization: `Bearer ${await tok()}`,
					"Content-Type": "application/json",
				},
				body: JSON.stringify(payload),
			},
		);
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { ok: false, error: body.error ?? "server_error" };
		}
		revalidatePath(`/app/agents`);
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function deleteLocalRule(
	agentId: string,
	ruleId: string,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/nftables/agents/${agentId}/local/${ruleId}`,
			{
				method: "DELETE",
				headers: { Authorization: `Bearer ${await tok()}` },
			},
		);
		if (!res.ok) return { ok: false, error: "server_error" };
		revalidatePath(`/app/agents`);
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function pushLocalRules(
	agentId: string,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/nftables/agents/${agentId}/local/push`,
			{
				method: "POST",
				headers: { Authorization: `Bearer ${await tok()}` },
			},
		);
		if (!res.ok) return { ok: false, error: "server_error" };
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
