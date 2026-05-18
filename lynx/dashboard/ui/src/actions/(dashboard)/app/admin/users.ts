"use server";

import { cookies } from "next/headers";
import { apiFetch } from "@/lib/api";

async function token(): Promise<string> {
	const jar = await cookies();
	return jar.get("access_token")?.value ?? "";
}

export type RoleRef = { id: string; name: string };
export type PermRef = { id: string; key: string };

export type UserRow = {
	id: string;
	username: string;
	force_password_change: boolean;
	created_at: string;
	roles: RoleRef[];
};

export type RoleRow = {
	id: string;
	name: string;
	permissions: PermRef[];
};

// ── Users ──────────────────────────────────────────────────────────────────

export async function listUsersAction(): Promise<UserRow[]> {
	const tok = await token();
	const res = await apiFetch<UserRow[]>("/admin/users", {
		headers: { Authorization: `Bearer ${tok}` },
	});
	return res.ok ? res.data : [];
}

export async function deleteUserAction(userId: string): Promise<{ success: boolean; error?: string }> {
	const tok = await token();
	const res = await apiFetch(`/admin/users/${userId}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "DELETE",
	});
	return res.ok ? { success: true } : { error: (res as { ok: false; error: string }).error, success: false };
}

export async function forcePasswordChangeAction(userId: string): Promise<{ success: boolean }> {
	const tok = await token();
	const res = await apiFetch(`/admin/users/${userId}/force-password-change`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "POST",
	});
	return { success: res.ok };
}

export async function addUserRoleAction(userId: string, roleId: string): Promise<{ success: boolean }> {
	const tok = await token();
	const res = await apiFetch(`/admin/users/${userId}/roles/${roleId}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "POST",
	});
	return { success: res.ok };
}

export async function removeUserRoleAction(userId: string, roleId: string): Promise<{ success: boolean }> {
	const tok = await token();
	const res = await apiFetch(`/admin/users/${userId}/roles/${roleId}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "DELETE",
	});
	return { success: res.ok };
}

// ── Roles ──────────────────────────────────────────────────────────────────

export async function listRolesAction(): Promise<RoleRow[]> {
	const tok = await token();
	const res = await apiFetch<RoleRow[]>("/admin/roles", {
		headers: { Authorization: `Bearer ${tok}` },
	});
	return res.ok ? res.data : [];
}

export async function listPermissionsAction(): Promise<PermRef[]> {
	const tok = await token();
	const res = await apiFetch<PermRef[]>("/admin/permissions", {
		headers: { Authorization: `Bearer ${tok}` },
	});
	return res.ok ? res.data : [];
}

export async function createRoleAction(name: string): Promise<{ success: boolean; id?: string; error?: string }> {
	const tok = await token();
	const res = await apiFetch<{ id: string; name: string }>("/admin/roles", {
		body: JSON.stringify({ name }),
		headers: { Authorization: `Bearer ${tok}` },
		method: "POST",
	});
	if (res.ok) return { id: res.data.id, success: true };
	return { error: (res as { ok: false; error: string }).error, success: false };
}

export async function deleteRoleAction(roleId: string): Promise<{ success: boolean; error?: string }> {
	const tok = await token();
	const res = await apiFetch(`/admin/roles/${roleId}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "DELETE",
	});
	return res.ok ? { success: true } : { error: (res as { ok: false; error: string }).error, success: false };
}

export async function addRolePermissionAction(roleId: string, permId: string): Promise<{ success: boolean }> {
	const tok = await token();
	const res = await apiFetch(`/admin/roles/${roleId}/permissions/${permId}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "POST",
	});
	return { success: res.ok };
}

export async function removeRolePermissionAction(
	roleId: string,
	permId: string,
): Promise<{ success: boolean; error?: string }> {
	const tok = await token();
	const res = await apiFetch(`/admin/roles/${roleId}/permissions/${permId}`, {
		headers: { Authorization: `Bearer ${tok}` },
		method: "DELETE",
	});
	return res.ok ? { success: true } : { error: (res as { ok: false; error: string }).error, success: false };
}
