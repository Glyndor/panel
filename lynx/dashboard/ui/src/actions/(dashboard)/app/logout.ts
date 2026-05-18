"use server";

import { apiFetch } from "@/lib/api";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

export async function logoutAction(locale: string) {
	const jar = await cookies();
	const accessToken = jar.get("access_token")?.value;

	if (accessToken) {
		await apiFetch("/auth/logout", {
			method: "POST",
			headers: { Authorization: `Bearer ${accessToken}` },
		});
	}

	jar.delete("access_token");
	jar.delete("refresh_token");
	redirect(`/${locale}/login`);
}
