"use server";

import { apiFetch } from "@/lib/api";
import { redirect } from "next/navigation";

const RESERVED = [
	"admin",
	"root",
	"system",
	"lynx",
	"support",
	"api",
	"null",
	"undefined",
];

type RegisterResult =
	| { success: true }
	| { success: false; error: string; field?: string };

export async function registerAction(
	locale: string,
	_prev: unknown,
	formData: FormData,
): Promise<RegisterResult> {
	const username = (formData.get("username") as string | null)?.trim() ?? "";
	const email = (formData.get("email") as string | null)?.trim() ?? "";
	const password = (formData.get("password") as string | null) ?? "";

	// Username validation (server-side echo of client validation)
	if (username.length < 3 || username.length > 32) {
		return {
			success: false,
			error: username.length < 3 ? "usernameMin" : "usernameMax",
			field: "username",
		};
	}
	if (!/^[a-z0-9_-]+$/.test(username)) {
		return { success: false, error: "usernameChars", field: "username" };
	}
	if (/^[-_]|[-_]$/.test(username)) {
		return { success: false, error: "usernameEdge", field: "username" };
	}
	if (RESERVED.includes(username)) {
		return { success: false, error: "usernameReserved", field: "username" };
	}

	const result = await apiFetch<void>("/auth/register", {
		method: "POST",
		body: JSON.stringify({ username, email, password }),
	});

	if (!result.ok) {
		if (result.error === "conflict") {
			return { success: false, error: "usernameTaken" };
		}
		return { success: false, error: "serverError" };
	}

	redirect(`/${locale}/login`);
}
