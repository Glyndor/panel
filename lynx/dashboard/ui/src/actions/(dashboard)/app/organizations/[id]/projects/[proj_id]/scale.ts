"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { BACKEND_URL, validateId } from "@/lib/api";

export async function addHorizontalScale(
	orgId: string,
	projId: string,
	targetAgentId: string,
	image: string,
	replicaCount: number,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	const oid = validateId(orgId);
	const pid = validateId(projId);
	const aid = validateId(targetAgentId);
	const res = await fetch(`${BACKEND_URL}/organizations/${oid}/projects/${pid}/scale/horizontal`, {
		body: JSON.stringify({
			image,
			replica_count: replicaCount,
			target_agent_id: aid,
		}),
		headers: {
			Authorization: `Bearer ${tok}`,
			"Content-Type": "application/json",
		},
		method: "POST",
	});

	if (!res.ok) return { error: `${res.status}`, ok: false };
	revalidatePath(`/app/organizations/${orgId}/projects/${projId}`);
	return { ok: true };
}

export async function teardownHorizontalScale(
	orgId: string,
	projId: string,
	tunnelId: string,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	const oid2 = validateId(orgId);
	const pid2 = validateId(projId);
	const tid = validateId(tunnelId);
	const res = await fetch(`${BACKEND_URL}/organizations/${oid2}/projects/${pid2}/scale/horizontal/${tid}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "DELETE",
	});

	if (!res.ok) return { error: `${res.status}`, ok: false };
	revalidatePath(`/app/organizations/${orgId}/projects/${projId}`);
	return { ok: true };
}
