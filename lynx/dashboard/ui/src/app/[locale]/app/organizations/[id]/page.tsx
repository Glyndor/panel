import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { InviteDialog } from "./InviteDialog";
import { RemoveMemberButton } from "./RemoveMemberButton";

interface Org {
	id: string;
	name: string;
	slug: string;
	owner_id: string;
	created_at: string;
}

interface Member {
	user_id: string;
	username: string;
	role: string;
	joined_at: string;
}

async function fetchOrg(token: string, id: string): Promise<Org | null> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${id}`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}

async function fetchMembers(token: string, id: string): Promise<Member[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${id}/members`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

const ROLE_VARIANT: Record<string, "default" | "secondary" | "outline"> = {
	owner: "default",
	admin: "secondary",
	member: "outline",
	viewer: "outline",
};

export default async function OrgDetailPage({
	params,
}: {
	params: Promise<{ locale: string; id: string }>;
}) {
	const { locale, id } = await params;
	const [t, jar] = await Promise.all([
		getTranslations({ locale, namespace: "app.organizations" }),
		cookies(),
	]);
	const tok = jar.get("access_token")?.value ?? "";

	const [org, members] = await Promise.all([
		fetchOrg(tok, id),
		fetchMembers(tok, id),
	]);

	if (!org) notFound();

	const currentUserId = members.find(
		(m) => m.role === "owner" && org.owner_id === m.user_id,
	)?.user_id;

	return (
		<div className="flex flex-col p-6 gap-6 max-w-3xl">
			<div>
				<p className="text-xs text-muted-foreground mb-1">{t("slug")}{" "}{org.slug}</p>
				<h1 className="text-xl font-semibold">{org.name}</h1>
			</div>

			<section className="flex flex-col gap-3">
				<div className="flex items-center justify-between">
					<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("members")} ({members.length})
					</h2>
					<InviteDialog
						orgId={id}
						labels={{
							trigger: t("invite"),
							title: t("inviteTitle"),
							username: t("inviteUsername"),
							role: t("inviteRole"),
							invite: t("inviteSubmit"),
							success: t("inviteSuccess"),
							error: t("inviteError"),
						}}
					/>
				</div>

				<div className="rounded-lg border divide-y">
					{members.map((m) => (
						<div
							key={m.user_id}
							className="flex items-center justify-between px-4 py-3 gap-3"
						>
							<div className="flex items-center gap-3 min-w-0">
								<span className="text-sm font-medium truncate">
									{m.username}
								</span>
								<Badge variant={ROLE_VARIANT[m.role] ?? "outline"}>
									{m.role}
								</Badge>
							</div>
							{m.role !== "owner" && (
								<RemoveMemberButton
									orgId={id}
									userId={m.user_id}
									label={t("removeMember")}
									successMsg={t("removeMemberSuccess")}
									errorMsg={t("removeMemberError")}
								/>
							)}
						</div>
					))}
					{members.length === 0 && (
						<p className="px-4 py-6 text-sm text-muted-foreground text-center">
							{t("noMembers")}
						</p>
					)}
				</div>
			</section>

			<p className="text-xs text-muted-foreground opacity-60">
				{t("orgId")}{" "}{id}
			</p>
		</div>
	);
}
