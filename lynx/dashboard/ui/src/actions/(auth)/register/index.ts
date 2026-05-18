"use server";

import { apiFetch } from "@/lib/api";
import type { RegisterInput } from "@/schemas/(auth)/register";

type RegisterResult =
	| { success: true }
	| { success: false; error: string };

export async function registerAction(
	_locale: string,
	data: RegisterInput,
): Promise<RegisterResult> {
	const result = await apiFetch<void>("/auth/register", {
		method: "POST",
		body: JSON.stringify({
			username: data.username,
			email: data.email,
			password: data.password,
		}),
	});

	if (!result.ok) {
		if (result.error === "conflict") {
			return { success: false, error: "usernameTaken" };
		}
		return { success: false, error: "serverError" };
	}

	return { success: true };
}
