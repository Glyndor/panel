"use server";

import { apiFetch } from "@/lib/api";
import type { RegisterInput } from "@/schemas/(auth)/register";

type RegisterResult = { success: true } | { success: false; error: string };

export async function registerAction(_locale: string, data: RegisterInput): Promise<RegisterResult> {
	const result = await apiFetch<void>("/auth/register", {
		body: JSON.stringify({
			email: data.email,
			password: data.password,
			username: data.username,
		}),
		method: "POST",
	});

	if (!result.ok) {
		if (result.error === "conflict") {
			return { error: "usernameTaken", success: false };
		}
		return { error: "serverError", success: false };
	}

	return { success: true };
}
