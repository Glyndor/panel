import { cookies } from "next/headers";
import Link from "next/link";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { CreateProjectDialog } from "@/components/(dashboard)/app/organizations/[id]/CreateProjectDialog";
import { InviteDialog } from "@/components/(dashboard)/app/organizations/[id]/InviteDialog";
import { RemoveMemberButton } from "@/components/(dashboard)/app/organizations/[id]/RemoveMemberButton";
import { Badge } from "@/components/ui/badge";
import { BACKEND_URL } from "@/lib/api";

interface Org {
	created_at: string;
	id: string;
	name: string;
	owner_id: string;
	slug: string;
}

interface Member {
	joined_at: string;
	role: string;
	user_id: string;
	username: string;
}

interface Project {
	agent_id: string;
	created_at: string;
	id: string;
	name: string;
	slug: string;
}

interface Agent {
	id: string;
	name: string;
	status: string;
}

async function fetchOrg(token: string, id: string): Promise<Org | null> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${id}`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
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
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

async function fetchAgents(token: string): Promise<Agent[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/agents`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

async function fetchProjects(token: string, id: string): Promise<Project[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${id}/projects`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

const ROLE_VARIANT: Record<string, "default" | "secondary" | "outline"> = {
	admin: "secondary",
	member: "outline",
	owner: "default",
	viewer: "outline",
};

export default async function OrgDetailPage({ params }: { params: Promise<{ locale: string; id: string }> }) {
	const { locale, id } = await params;
	const [t, jar] = await Promise.all([getTranslations({ locale, namespace: "app.organizations" }), cookies()]);
	const tok = jar.get("access_token")?.value ?? "";

	const [org, members, projects, agents] = await Promise.all([
		fetchOrg(tok, id),
		fetchMembers(tok, id),
		fetchProjects(tok, id),
		fetchAgents(tok),
	]);

	if (!org) notFound();

	const currentUserId = members.find((m) => m.role === "owner" && org.owner_id === m.user_id)?.user_id;

	return (
		<div className="flex flex-col p-6 gap-6 max-w-3xl">
			<div>
				<p className="text-xs text-muted-foreground mb-1">
					{t("slug")} {org.slug}
				</p>
				<h1 className="text-xl font-semibold">{org.name}</h1>
			</div>

			<section className="flex flex-col gap-3">
				<div className="flex items-center justify-between">
					<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("members")} ({members.length})
					</h2>
					<InviteDialog
						labels={{
							error: t("inviteError"),
							invite: t("inviteSubmit"),
							role: t("inviteRole"),
							success: t("inviteSuccess"),
							title: t("inviteTitle"),
							trigger: t("invite"),
							username: t("inviteUsername"),
						}}
						orgId={id}
					/>
				</div>

				<div className="rounded-lg border divide-y">
					{members.map((m) => (
						<div className="flex items-center justify-between px-4 py-3 gap-3" key={m.user_id}>
							<div className="flex items-center gap-3 min-w-0">
								<span className="text-sm font-medium truncate">{m.username}</span>
								<Badge variant={ROLE_VARIANT[m.role] ?? "outline"}>{m.role}</Badge>
							</div>
							{m.role !== "owner" && (
								<RemoveMemberButton
									errorMsg={t("removeMemberError")}
									label={t("removeMember")}
									orgId={id}
									successMsg={t("removeMemberSuccess")}
									userId={m.user_id}
								/>
							)}
						</div>
					))}
					{members.length === 0 && (
						<p className="px-4 py-6 text-sm text-muted-foreground text-center">{t("noMembers")}</p>
					)}
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<div className="flex items-center justify-between">
					<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("projects")} ({projects.length})
					</h2>
					<CreateProjectDialog
						agents={agents}
						labels={{
							agent: t("projectAgent"),
							create: t("projectCreate"),
							error: t("projectCreateError"),
							name: t("projectName"),
							noAgents: t("projectNoAgents"),
							slug: t("projectSlug"),
							slugConflict: t("projectSlugConflict"),
							success: t("projectCreateSuccess"),
							title: t("createProjectTitle"),
							trigger: t("createProject"),
						}}
						orgId={id}
					/>
				</div>
				{projects.length === 0 ? (
					<p className="text-sm text-muted-foreground">{t("noProjects")}</p>
				) : (
					<div className="rounded-lg border divide-y">
						{projects.map((p) => (
							<Link
								className="flex items-center justify-between px-4 py-3 gap-3 hover:bg-muted/50 transition-colors"
								href={`/${locale}/app/organizations/${id}/projects/${p.id}`}
								key={p.id}
							>
								<div className="min-w-0">
									<p className="text-sm font-medium truncate">{p.name}</p>
									<p className="text-xs text-muted-foreground">{p.slug}</p>
								</div>
								<Badge className="shrink-0 font-mono text-xs" variant="outline">
									{p.agent_id.slice(0, 8)}
								</Badge>
							</Link>
						))}
					</div>
				)}
			</section>

			<p className="text-xs text-muted-foreground opacity-60">
				{t("orgId")} {id}
			</p>
		</div>
	);
}
