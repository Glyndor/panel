"use server";

import { cookies } from "next/headers";
import { apiFetch } from "@/lib/api";
import type { LoginInput } from "@/schemas/(auth)/login";

type LoginResult =
	| { success: true; forcePasswordChange?: boolean }
	| { success: false; error: string; retryAfter?: number };

export async function loginAction(locale: string, data: LoginInput): Promise<LoginResult> {
	const result = await apiFetch<{
		access_token: string;
		refresh_token: string;
		expires_in: number;
		force_password_change: boolean;
		theme: string;
	}>("/auth/login", {
		body: JSON.stringify({ password: data.password, username: data.username }),
		method: "POST",
	});

	if (!result.ok) {
		if (result.error === "invalid_credentials") {
			return { error: "invalidCredentials", success: false };
		}
		if (result.error === "rate_limited") {
			const minutes = result.retryAfter ? Math.ceil(result.retryAfter / 60) : 15;
			return { error: "rateLimited", retryAfter: minutes, success: false };
		}
		return { error: "serverError", success: false };
	}

	const jar = await cookies();
	const secure = process.env.NODE_ENV === "production";

	jar.set("access_token", result.data.access_token, {
		httpOnly: true,
		maxAge: result.data.expires_in,
		path: "/",
		sameSite: "strict",
		secure,
	});
	jar.set("refresh_token", result.data.refresh_token, {
		httpOnly: true,
		maxAge: 86400,
		path: "/",
		sameSite: "strict",
		secure,
	});
	jar.set("theme_preference", result.data.theme ?? "system", {
		httpOnly: false,
		maxAge: 60 * 60 * 24 * 365,
		path: "/",
		sameSite: "strict",
		secure,
	});

	return { forcePasswordChange: result.data.force_password_change, success: true };
}
