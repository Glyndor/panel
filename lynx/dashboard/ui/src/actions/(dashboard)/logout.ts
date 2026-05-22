"use server";

import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { apiFetch } from "@/lib/api";

export async function logoutAction(locale: string) {
	const jar = await cookies();
	const accessToken = jar.get("access_token")?.value;

	if (accessToken) {
		await apiFetch("/auth/logout", {
			headers: { Authorization: `Bearer ${accessToken}` },
			method: "POST",
		});
	}

	jar.delete("access_token");
	jar.delete("refresh_token");
	redirect(`/${locale}/login`);
}
