"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { BACKEND_URL, validateId, validateName } from "@/lib/api";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

const ACTIONS = ["start", "stop", "restart", "remove"] as const;

export async function containerAction(
	orgId: string,
	projId: string,
	name: string,
	action: "start" | "stop" | "restart" | "remove",
): Promise<{ ok: boolean; error?: string }> {
	try {
		const oid = validateId(orgId);
		const pid = validateId(projId);
		const cname = validateName(name);
		if (!ACTIONS.includes(action)) return { error: "invalid_action", ok: false };
		const res = await fetch(`${BACKEND_URL}/organizations/${oid}/projects/${pid}/containers/${cname}/${action}`, {
			headers: { Authorization: `Bearer ${await token()}` },
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string; detail?: string };
			return { error: body.detail ?? body.error ?? "server_error", ok: false };
		}
		revalidatePath(`/[locale]/app/p/${projId}`, "page");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function deployContainer(
	orgId: string,
	projId: string,
	payload: {
		name: string;
		image: string;
		ports: string[];
		env: string[];
		cpus: number | null;
		memory_mb: number | null;
	},
): Promise<{ ok: boolean; error?: string }> {
	try {
		const oid = validateId(orgId);
		const pid = validateId(projId);
		const res = await fetch(`${BACKEND_URL}/organizations/${oid}/projects/${pid}/containers`, {
			body: JSON.stringify(payload),
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
		revalidatePath(`/[locale]/app/p/${projId}`, "page");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
