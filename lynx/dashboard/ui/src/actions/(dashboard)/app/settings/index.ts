"use server";

import { BACKEND_URL } from "@/lib/api";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";

export async function revokeSession(
	sessionId: string,
): Promise<{ ok: boolean }> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/admin/sessions/${sessionId}`, {
			method: "DELETE",
			headers: { Authorization: `Bearer ${token}` },
		});
		return { ok: res.ok };
	} catch {
		return { ok: false };
	}
}

export async function rotateKeys(locale: string): Promise<void> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		await fetch(`${BACKEND_URL}/admin/rotate`, {
			method: "POST",
			headers: {
				Authorization: `Bearer ${token}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify({ scope: "jwt_keys", reason: "manual" }),
		});
	} catch {
		// Rotation may still have succeeded on the backend
	}

	jar.delete("access_token");
	jar.delete("refresh_token");
	redirect(`/${locale}/login`);
}

export interface BrandingPayload {
	company_name?: string;
	logo_url?: string | null;
	primary_color?: string;
	secondary_color?: string;
	accent_color?: string;
}

export interface UpdateCheckResult {
	current_version: string;
	latest_version: string;
	update_available: boolean;
	release_url: string | null;
}

export async function checkForUpdates(): Promise<{
	ok: boolean;
	data?: UpdateCheckResult;
	error?: string;
}> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";
	try {
		const res = await fetch(`${BACKEND_URL}/admin/update-check`, {
			headers: { Authorization: `Bearer ${tok}` },
			cache: "no-store",
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		return { ok: true, data: (await res.json()) as UpdateCheckResult };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function triggerUpdate(
	version: string,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";
	try {
		const res = await fetch(`${BACKEND_URL}/admin/trigger-update`, {
			method: "POST",
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify({ version, channel: "stable" }),
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string };
			return { ok: false, error: body.error ?? "server_error" };
		}
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function updateBranding(
	payload: BrandingPayload,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/admin/branding`, {
			method: "PUT",
			headers: {
				Authorization: `Bearer ${token}`,
				"Content-Type": "application/json",
			},
			body: JSON.stringify(payload),
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string };
			return { ok: false, error: body.error ?? "server_error" };
		}
		revalidatePath("/", "layout");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

// Domain actions

export async function configureDomain(
	domain: string,
	email: string,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${tok}`,
			},
			body: JSON.stringify({ domain, email }),
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function verifyDomain(): Promise<{
	ok: boolean;
	dns_ok?: boolean;
	domain?: string;
	error?: string;
}> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain/verify`, {
			method: "POST",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		const data = (await res.json()) as { dns_ok: boolean; domain: string };
		return { ok: true, dns_ok: data.dns_ok, domain: data.domain };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function setHsts(
	enabled: boolean,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain/hsts`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${tok}`,
			},
			body: JSON.stringify({ enabled }),
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function uploadCert(
	certType: "cloudflare" | "custom",
	certPem: string,
	keyPem?: string,
): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain/cert/upload`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${tok}`,
			},
			body: JSON.stringify({
				cert_type: certType,
				cert_pem: certPem,
				key_pem: keyPem ?? null,
			}),
		});
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { ok: false, error: body.error ?? "server_error" };
		}
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}

export async function closePort19443(): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain/close-port`, {
			method: "POST",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return { ok: false, error: "server_error" };
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { ok: false, error: "network_error" };
	}
}
