import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import {
	listPermissionsAction,
	listRolesAction,
	listUsersAction,
	type PermRef,
	type RoleRow,
	type UserRow,
} from "@/actions/(dashboard)/app/admin/users";
import { RolesPanel } from "@/components/(dashboard)/app/admin/RolesPanel";
import { UsersPanel } from "@/components/(dashboard)/app/admin/UsersPanel";
import { Skeleton } from "@/components/ui/skeleton";
import { BACKEND_URL } from "@/lib/api";

// ---------------------------------------------------------------------------
// Guard: redirect non-admins
// ---------------------------------------------------------------------------

async function assertAdmin(token: string, locale: string) {
	try {
		const res = await fetch(`${BACKEND_URL}/auth/me`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
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
		addRole: t("addRole"),
		addRoleError: t("addRoleError"),
		addRoleSuccess: t("addRoleSuccess"),
		deleteConfirm: t("deleteConfirm"),
		deleteError: t("deleteError"),
		deleteSuccess: t("deleteSuccess"),
		deleteUser: t("deleteUser"),
		forcePasswordChange: t("forcePasswordChange"),
		forcePasswordChangeError: t("forcePasswordChangeError"),
		forcePasswordChangeSuccess: t("forcePasswordChangeSuccess"),
		noRoles: t("noRoles"),
		removeRole: t("removeRole"),
		removeRoleError: t("removeRoleError"),
		removeRoleSuccess: t("removeRoleSuccess"),
		selectRole: t("selectRole"),
	};

	const roleLabels = {
		addPermission: t("addPermission"),
		addPermissionError: t("addPermissionError"),
		addPermissionSuccess: t("addPermissionSuccess"),
		createRole: t("createRole"),
		createRoleError: t("createRoleError"),
		createRoleSuccess: t("createRoleSuccess"),
		deleteRole: t("deleteRole"),
		deleteRoleConfirm: t("deleteRoleConfirm"),
		deleteRoleError: t("deleteRoleError"),
		deleteRoleSuccess: t("deleteRoleSuccess"),
		noPermissions: t("noPermissions"),
		removePermission: t("removePermission"),
		removePermissionError: t("removePermissionError"),
		removePermissionSuccess: t("removePermissionSuccess"),
		roleName: t("roleName"),
	};

	return (
		<>
			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("users")}</h2>
				<UsersPanel initial={users} labels={userLabels} roles={roles} />
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("roles")}</h2>
				<RolesPanel allPerms={perms} initial={roles} labels={roleLabels} />
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

export default async function AdminPage({ params }: { params: Promise<{ locale: string }> }) {
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
