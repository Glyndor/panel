"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { BACKEND_URL, validateId } from "@/lib/api";

export async function revokeSession(sessionId: string): Promise<{ ok: boolean }> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		const sid = validateId(sessionId);
		const res = await fetch(`${BACKEND_URL}/admin/sessions/${sid}`, {
			headers: { Authorization: `Bearer ${token}` },
			method: "DELETE",
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
			body: JSON.stringify({ reason: "manual", scope: "jwt_keys" }),
			headers: {
				Authorization: `Bearer ${token}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
	} catch {
		// Rotation may still have succeeded on the backend
	}

	jar.delete("access_token");
	jar.delete("refresh_token");
	redirect(`/${locale}/login`);
}

export interface BrandingPayload {
	accent_color?: string;
	company_name?: string;
	logo_url?: string | null;
	primary_color?: string;
	secondary_color?: string;
}

export interface UpdateCheckResult {
	current_version: string;
	latest_version: string;
	release_url: string | null;
	update_available: boolean;
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
			cache: "no-store",
			headers: { Authorization: `Bearer ${tok}` },
		});
		if (!res.ok) return { error: "server_error", ok: false };
		return { data: (await res.json()) as UpdateCheckResult, ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function triggerUpdate(version: string): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";
	try {
		const res = await fetch(`${BACKEND_URL}/admin/trigger-update`, {
			body: JSON.stringify({ channel: "stable", version }),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string };
			return { error: body.error ?? "server_error", ok: false };
		}
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function updateBranding(payload: BrandingPayload): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/admin/branding`, {
			body: JSON.stringify(payload),
			headers: {
				Authorization: `Bearer ${token}`,
				"Content-Type": "application/json",
			},
			method: "PUT",
		});
		if (!res.ok) {
			const body = (await res.json()) as { error?: string };
			return { error: body.error ?? "server_error", ok: false };
		}
		revalidatePath("/", "layout");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

// Domain actions

export async function configureDomain(domain: string, email: string): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain`, {
			body: JSON.stringify({ domain, email }),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
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
			headers: { Authorization: `Bearer ${tok}` },
			method: "POST",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		const data = (await res.json()) as { dns_ok: boolean; domain: string };
		return { dns_ok: data.dns_ok, domain: data.domain, ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function setHsts(enabled: boolean): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain/hsts`, {
			body: JSON.stringify({ enabled }),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
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
			body: JSON.stringify({
				cert_pem: certPem,
				cert_type: certType,
				key_pem: keyPem ?? null,
			}),
			headers: {
				Authorization: `Bearer ${tok}`,
				"Content-Type": "application/json",
			},
			method: "POST",
		});
		if (!res.ok) {
			const body = (await res.json().catch(() => ({}))) as { error?: string };
			return { error: body.error ?? "server_error", ok: false };
		}
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}

export async function closePort19443(): Promise<{ ok: boolean; error?: string }> {
	const jar = await cookies();
	const tok = jar.get("access_token")?.value ?? "";

	try {
		const res = await fetch(`${BACKEND_URL}/domain/close-port`, {
			headers: { Authorization: `Bearer ${tok}` },
			method: "POST",
		});
		if (!res.ok) return { error: "server_error", ok: false };
		revalidatePath("/app/settings");
		return { ok: true };
	} catch {
		return { error: "network_error", ok: false };
	}
}
