"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function createProject(
	orgId: string,
	name: string,
	slug: string,
	agentId: string,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${orgId}/projects`, {
			method: "POST",
			headers: {
				Authorization: `Bearer ${await token()}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify({ name, slug, agent_id: agentId }),
		});
		if (!res.ok) {
			const body = (await res.json()) as {
				error?: string;
				detail?: string;
			};
			if (body.error === "conflict") {
				return { ok: false, error: "slug_conflict" };
			}
			return { ok: false, error: body.detail ?? body.error ?? "server_error" };
		}
		revalidatePath(`/[locale]/app/organizations/${orgId}`, "page");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
