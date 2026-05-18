import { Suspense } from "react";
import { getTranslations } from "next-intl/server";
import { redirect } from "next/navigation";
import { cookies } from "next/headers";
import { BACKEND_URL } from "@/lib/api";
import {
	listUsersAction,
	listRolesAction,
	listPermissionsAction,
	type UserRow,
	type RoleRow,
	type PermRef,
} from "@/actions/(dashboard)/app/admin/users";
import { UsersPanel } from "@/components/(dashboard)/app/admin/UsersPanel";
import { RolesPanel } from "@/components/(dashboard)/app/admin/RolesPanel";
import { Skeleton } from "@/components/ui/skeleton";

// ---------------------------------------------------------------------------
// Guard: redirect non-admins
// ---------------------------------------------------------------------------

async function assertAdmin(token: string, locale: string) {
	try {
		const res = await fetch(`${BACKEND_URL}/auth/me`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) redirect(`/${locale}/app`);
		const data = (await res.json()) as { is_admin?: boolean };
		if (!data.is_admin) redirect(`/${locale}/app`);
	} catch {
		redirect(`/${locale}/app`);
	}
}

// ---------------------------------------------------------------------------
// Async data loader
// ---------------------------------------------------------------------------

async function AdminData({ locale }: { locale: string }) {
	const t = await getTranslations({ locale, namespace: "admin" });

	const [users, roles, perms]: [UserRow[], RoleRow[], PermRef[]] = await Promise.all([
		listUsersAction(),
		listRolesAction(),
		listPermissionsAction(),
	]);

	const userLabels = {
		deleteUser: t("deleteUser"),
		deleteConfirm: t("deleteConfirm"),
		deleteSuccess: t("deleteSuccess"),
		deleteError: t("deleteError"),
		forcePasswordChange: t("forcePasswordChange"),
		forcePasswordChangeSuccess: t("forcePasswordChangeSuccess"),
		forcePasswordChangeError: t("forcePasswordChangeError"),
		addRole: t("addRole"),
		addRoleSuccess: t("addRoleSuccess"),
		addRoleError: t("addRoleError"),
		removeRole: t("removeRole"),
		removeRoleSuccess: t("removeRoleSuccess"),
		removeRoleError: t("removeRoleError"),
		noRoles: t("noRoles"),
		selectRole: t("selectRole"),
	};

	const roleLabels = {
		createRole: t("createRole"),
		createRoleSuccess: t("createRoleSuccess"),
		createRoleError: t("createRoleError"),
		deleteRole: t("deleteRole"),
		deleteRoleConfirm: t("deleteRoleConfirm"),
		deleteRoleSuccess: t("deleteRoleSuccess"),
		deleteRoleError: t("deleteRoleError"),
		addPermission: t("addPermission"),
		addPermissionSuccess: t("addPermissionSuccess"),
		addPermissionError: t("addPermissionError"),
		removePermission: t("removePermission"),
		removePermissionSuccess: t("removePermissionSuccess"),
		removePermissionError: t("removePermissionError"),
		roleName: t("roleName"),
		noPermissions: t("noPermissions"),
	};

	return (
		<>
			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("users")}
				</h2>
				<UsersPanel initial={users} roles={roles} labels={userLabels} />
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("roles")}
				</h2>
				<RolesPanel initial={roles} allPerms={perms} labels={roleLabels} />
			</section>
		</>
	);
}

function AdminSkeleton() {
	return (
		<>
			<div className="flex flex-col gap-3">
				<Skeleton className="h-4 w-20" />
				<div className="rounded-lg border divide-y">
					{[0, 1, 2].map((i) => (
						<div className="flex items-center gap-3 px-4 py-3" key={i}>
							<Skeleton className="h-4 flex-1 max-w-40" />
							<Skeleton className="h-5 w-16" />
							<Skeleton className="h-7 w-7 rounded-md" />
						</div>
					))}
				</div>
			</div>
			<div className="flex flex-col gap-3">
				<Skeleton className="h-4 w-16" />
				<div className="rounded-lg border divide-y">
					{[0, 1].map((i) => (
						<div className="flex flex-col gap-2 px-4 py-3" key={i}>
							<Skeleton className="h-4 w-28" />
							<div className="flex gap-1.5">
								<Skeleton className="h-5 w-20" />
								<Skeleton className="h-5 w-24" />
							</div>
						</div>
					))}
				</div>
			</div>
		</>
	);
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default async function AdminPage({
	params,
}: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "admin" });

	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	await assertAdmin(token, locale);

	return (
		<div className="flex flex-col p-6 gap-8">
			<h1 className="text-xl font-semibold">{t("title")}</h1>
			<Suspense fallback={<AdminSkeleton />}>
				<AdminData locale={locale} />
			</Suspense>
		</div>
	);
}
