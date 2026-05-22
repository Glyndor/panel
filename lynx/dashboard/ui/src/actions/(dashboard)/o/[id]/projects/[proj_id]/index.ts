"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { BACKEND_URL, validateId } from "@/lib/api";

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
		const oid = validateId(orgId);
		const pid = validateId(projId);
		const res = await fetch(`${BACKEND_URL}/organizations/${oid}/projects/${pid}/resources`, {
			body: JSON.stringify({
				container_name: containerName,
				cpus: cpus ?? undefined,
				memory_mb: memoryMb ?? undefined,
			}),
			headers: {
				Authorization: `Bearer ${await token()}`,
				"Content-Type": "application/json",
			},
			method: "PUT",
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
