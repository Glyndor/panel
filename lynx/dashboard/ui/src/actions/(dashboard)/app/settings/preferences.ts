"use server";

import { apiFetch } from "@/lib/api";
import { cookies } from "next/headers";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export async function updateThemeAction(theme: string): Promise<void> {
	const tok = await token();
	await apiFetch("/auth/me/preferences", {
		method: "POST",
		headers: { Authorization: `Bearer ${tok}` },
		body: JSON.stringify({ theme }),
	});
	const jar = await cookies();
	jar.set("theme_preference", theme, {
		path: "/",
		httpOnly: false,
		sameSite: "strict",
		secure: process.env.NODE_ENV === "production",
		maxAge: 60 * 60 * 24 * 365,
	});
}

export async function updateLocaleAction(locale: string): Promise<void> {
	const tok = await token();
	await apiFetch("/auth/me/preferences", {
		method: "POST",
		headers: { Authorization: `Bearer ${tok}` },
		body: JSON.stringify({ locale }),
	});
}
