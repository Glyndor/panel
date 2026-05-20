"use server";

import { apiFetch } from "@/lib/api";

export type SecurityAlert = {
	id: string;
	kind: string;
	detail: string | null;
	agent_id: string | null;
	created_at: string;
};

export async function listAlertsAction(): Promise<SecurityAlert[]> {
	const result = await apiFetch<SecurityAlert[]>("/admin/alerts");
	if (!result.ok) return [];
	return result.data;
}

export async function acknowledgeAlertAction(id: string): Promise<{ success: boolean }> {
	const result = await apiFetch(`/admin/alerts/${id}/acknowledge`, {
		method: "POST",
	});
	return { success: result.ok };
}
