import { FolderOpen } from "lucide-react";
import Link from "next/link";
import { getTranslations } from "next-intl/server";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { BACKEND_URL } from "@/lib/api";

interface Org {
	id: string;
	name: string;
	slug: string;
}

interface Project {
	agent_id: string;
	created_at: string;
	id: string;
	name: string;
	slug: string;
}

interface ProjectWithOrg extends Project {
	org_id: string;
	org_name: string;
}

async function fetchOrgs(token: string): Promise<Org[]> {
	if (!token) return [];
	try {
		const res = await fetch(`${BACKEND_URL}/organizations`, {
			headers: { Authorization: `Bearer ${token}` },
			next: { revalidate: 30 },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

async function fetchProjectsForOrg(token: string, orgId: string): Promise<Project[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${orgId}/projects`, {
			headers: { Authorization: `Bearer ${token}` },
			next: { revalidate: 30 },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

async function fetchAllProjects(token: string): Promise<ProjectWithOrg[]> {
	const orgs = await fetchOrgs(token);
	if (orgs.length === 0) return [];
	const perOrg = await Promise.all(
		orgs.map(async (org) => {
			const projects = await fetchProjectsForOrg(token, org.id);
			return projects.map((p) => ({ ...p, org_id: org.id, org_name: org.name }));
		}),
	);
	return perOrg.flat();
}

export async function ProjectList({ token, locale }: { token: string; locale: string }) {
	const [t, projects] = await Promise.all([
		getTranslations({ locale, namespace: "app.projects" }),
		fetchAllProjects(token),
	]);

	if (projects.length === 0) {
		return (
			<div className="flex flex-1 items-center justify-center rounded-lg border border-dashed min-h-64">
				<div className="text-center max-w-xs">
					<FolderOpen className="mx-auto mb-2 size-8 text-muted-foreground" />
					<p className="text-sm font-medium">{t("noProjects")}</p>
				</div>
			</div>
		);
	}

	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{projects.map((p) => (
				<Link href={`/${locale}/app/p/${p.id}?org=${p.org_id}`} key={p.id}>
					<Card className="hover:border-foreground/20 transition-colors cursor-pointer h-full select-none">
						<CardHeader className="pb-2">
							<CardTitle className="text-base truncate flex items-center gap-2">
								<FolderOpen className="size-4 shrink-0 text-muted-foreground" />
								{p.name}
							</CardTitle>
						</CardHeader>
						<CardContent className="space-y-1 text-sm text-muted-foreground">
							<p>
								<span className="font-medium text-foreground">{t("slug")}</span> {p.slug}
							</p>
							<p>
								<span className="font-medium text-foreground">{t("org")}</span> {p.org_name}
							</p>
							<Badge className="font-mono text-xs mt-1" variant="outline">
								{p.agent_id.slice(0, 8)}
							</Badge>
						</CardContent>
					</Card>
				</Link>
			))}
		</div>
	);
}
