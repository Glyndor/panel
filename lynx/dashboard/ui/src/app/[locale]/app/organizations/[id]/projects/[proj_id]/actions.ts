"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function updateContainerResources(
	orgId: string,
	projId: string,
	containerName: string,
	cpus: number | null,
	memoryMb: number | null,
): Promise<{ ok: boolean; error?: string }> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/organizations/${orgId}/projects/${projId}/resources`,
			{
				method: "PUT",
				headers: {
					Authorization: `Bearer ${await token()}`,
					"Content-Type": "application/json",
				},
				body: JSON.stringify({
					container_name: containerName,
					cpus: cpus ?? undefined,
					memory_mb: memoryMb ?? undefined,
				}),
			},
		);
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
