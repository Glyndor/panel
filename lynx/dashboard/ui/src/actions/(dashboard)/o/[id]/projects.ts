"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { BACKEND_URL, validateId } from "@/lib/api";

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
		const oid = validateId(orgId);
		const aid = validateId(agentId);
		const res = await fetch(`${BACKEND_URL}/organizations/${oid}/projects`, {
			body: JSON.stringify({ agent_id: aid, name, slug }),
			headers: {
				Authorization: `Bearer ${await token()}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json()) as {
				error?: string;
				detail?: string;
			};
			if (body.error === "conflict") {
				return { error: "slug_conflict", ok: false };
			}
			return { error: body.detail ?? body.error ?? "server_error", ok: false };
		}
		revalidatePath(`/[locale]/app/o/${orgId}`, "page");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
