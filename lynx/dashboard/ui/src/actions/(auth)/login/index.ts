"use server";

import { apiFetch } from "@/lib/api";
import { cookies } from "next/headers";
import type { LoginInput } from "@/schemas/(auth)/login";

type LoginResult =
	| { success: true; forcePasswordChange?: boolean }
	| { success: false; error: string; retryAfter?: number };

export async function loginAction(
	locale: string,
	data: LoginInput,
): Promise<LoginResult> {
	const result = await apiFetch<{
		access_token: string;
		refresh_token: string;
		expires_in: number;
		force_password_change: boolean;
	}>("/auth/login", {
		method: "POST",
		body: JSON.stringify({ username: data.username, password: data.password }),
	});

	if (!result.ok) {
		if (result.error === "invalid_credentials") {
			return { success: false, error: "invalidCredentials" };
		}
		if (result.error === "rate_limited") {
			const minutes = result.retryAfter ? Math.ceil(result.retryAfter / 60) : 15;
			return { success: false, error: "rateLimited", retryAfter: minutes };
		}
		return { success: false, error: "serverError" };
	}

	const jar = await cookies();
	const secure = process.env.NODE_ENV === "production";

	jar.set("access_token", result.data.access_token, {
		httpOnly: true,
		secure,
		sameSite: "strict",
		maxAge: result.data.expires_in,
		path: "/",
	});
	jar.set("refresh_token", result.data.refresh_token, {
		httpOnly: true,
		secure,
		sameSite: "strict",
		maxAge: 86400,
		path: "/",
	});

	return { success: true, forcePasswordChange: result.data.force_password_change };
}
