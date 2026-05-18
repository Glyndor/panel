"use server";

import { apiFetch } from "@/lib/api";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

type LoginResult =
	| { success: true }
	| { success: false; error: string; retryAfter?: number };

export async function loginAction(
	locale: string,
	_prev: unknown,
	formData: FormData,
): Promise<LoginResult> {
	const username = (formData.get("username") as string | null)?.trim() ?? "";
	const password = (formData.get("password") as string | null) ?? "";

	const result = await apiFetch<{
		access_token: string;
		refresh_token: string;
		expires_in: number;
	}>("/auth/login", {
		method: "POST",
		body: JSON.stringify({ username, password }),
	});

	if (!result.ok) {
		if (result.error === "invalid_credentials") {
			return { success: false, error: "invalidCredentials" };
		}
		if (result.error === "rate_limited") {
			const minutes = result.retryAfter
				? Math.ceil(result.retryAfter / 60)
				: 15;
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

	redirect(`/${locale}/app`);
}
