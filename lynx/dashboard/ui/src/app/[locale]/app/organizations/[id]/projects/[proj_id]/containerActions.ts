"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

function base(orgId: string, projId: string) {
	return `${BACKEND_URL}/organizations/${orgId}/projects/${projId}/containers`;
}

async function post(url: string): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(url, {
			method: "POST",
			headers: { Authorization: `Bearer ${await token()}` },
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string; detail?: string };
			return { ok: false, error: body.detail ?? body.error ?? "server_error" };
		}
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
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
		revalidatePath(
			`/[locale]/app/organizations/${orgId}/projects/${projId}`,
			"page",
		);
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
			method: "POST",
			headers: {
				Authorization: `Bearer ${await token()}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify(payload),
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string; detail?: string };
			return { ok: false, error: body.detail ?? body.error ?? "server_error" };
		}
		revalidatePath(
			`/[locale]/app/organizations/${orgId}/projects/${projId}`,
			"page",
		);
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
