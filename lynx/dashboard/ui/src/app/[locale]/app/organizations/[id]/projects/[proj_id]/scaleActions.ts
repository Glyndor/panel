"use server";

import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";
import { BACKEND_URL } from "@/lib/api";

export async function addHorizontalScale(
	orgId: string,
	projId: string,
	targetAgentId: string,
	image: string,
	replicaCount: number,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	const res = await fetch(
		`${BACKEND_URL}/organizations/${orgId}/projects/${projId}/scale/horizontal`,
		{
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${tok}`,
			},
			body: JSON.stringify({
				target_agent_id: targetAgentId,
				image,
				replica_count: replicaCount,
			}),
		},
	);

	if (!res.ok) return { ok: false, error: `${res.status}` };
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

	const res = await fetch(
		`${BACKEND_URL}/organizations/${orgId}/projects/${projId}/scale/horizontal/${tunnelId}`,
		{
			method: "DELETE",
			headers: { Authorization: `Bearer ${tok}` },
		},
	);

	if (!res.ok) return { ok: false, error: `${res.status}` };
	revalidatePath(`/app/organizations/${orgId}/projects/${projId}`);
	return { ok: true };
}
