"use server";

import { cookies } from "next/headers";
import { apiFetch } from "@/lib/api";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function updateThemeAction(theme: string): Promise<void> {
	const tok = await token();
	await apiFetch("/auth/me/preferences", {
		body: JSON.stringify({ theme }),
		headers: { Authorization: `Bearer ${tok}` },
		method: "POST",
	});
	const jar = await cookies();
	jar.set("theme_preference", theme, {
		httpOnly: false,
		maxAge: 60 * 60 * 24 * 365,
		path: "/",
		sameSite: "strict",
		secure: process.env.NODE_ENV === "production",
	});
}

export async function updateLocaleAction(locale: string): Promise<void> {
	const tok = await token();
	await apiFetch("/auth/me/preferences", {
		body: JSON.stringify({ locale }),
		headers: { Authorization: `Bearer ${tok}` },
		method: "POST",
	});
}
