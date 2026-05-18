"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { BACKEND_URL, validateId } from "@/lib/api";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

function base(orgId: string, projId: string) {
	return `${BACKEND_URL}/organizations/${validateId(orgId)}/projects/${validateId(projId)}/containers`;
}

async function post(url: string): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(url, {
			headers: { Authorization: `Bearer ${await token()}` },
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string; detail?: string };
			return { error: body.detail ?? body.error ?? "server_error", ok: false };
		}
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function containerAction(
	orgId: string,
	projId: string,
	name: string,
	action: "start" | "stop" | "restart" | "remove",
): Promise<{ ok: boolean; error?: string }> {
	const result = await post(`${base(orgId, projId)}/${name}/${action}`);
	if (result.ok) {
		revalidatePath(`/[locale]/app/organizations/${orgId}/projects/${projId}`, "page");
	}
	return result;
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
		const res = await fetch(base(orgId, projId), {
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
		revalidatePath(`/[locale]/app/organizations/${orgId}/projects/${projId}`, "page");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
