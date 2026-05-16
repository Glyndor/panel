import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import Link from "next/link";
import { ChevronRight } from "lucide-react";
import { ResourceForm } from "./ResourceForm";

interface Project {
	id: string;
	name: string;
	slug: string;
	agent_id: string;
	organization_id: string;
	created_at: string;
}

async function fetchProject(
	token: string,
	orgId: string,
	projId: string,
): Promise<Project | null> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/organizations/${orgId}/projects/${projId}`,
			{
				headers: { Authorization: `Bearer ${token}` },
				cache: "no-store",
			},
		);
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}

export default async function ProjectDetailPage({
	params,
}: {
	params: Promise<{ locale: string; id: string; proj_id: string }>;
}) {
	const { locale, id: orgId, proj_id: projId } = await params;
	const [t, jar] = await Promise.all([
		getTranslations({ locale, namespace: "app.projects" }),
		cookies(),
	]);
	const tok = jar.get("access_token")?.value ?? "";
	const project = await fetchProject(tok, orgId, projId);

	if (!project) notFound();

	return (
		<div className="flex flex-col p-6 gap-6 max-w-3xl">
			<nav className="flex items-center gap-1 text-sm text-muted-foreground">
				<Link
					href={`/${locale}/app/organizations`}
					className="hover:text-foreground transition-colors"
				>
					{t("orgs")}
				</Link>
				<ChevronRight className="size-3.5" />
				<Link
					href={`/${locale}/app/organizations/${orgId}`}
					className="hover:text-foreground transition-colors"
				>
					{t("org")}
				</Link>
				<ChevronRight className="size-3.5" />
				<span className="text-foreground">{project.name}</span>
			</nav>

			<div>
				<p className="text-xs text-muted-foreground mb-1">
					{t("slug")} {project.slug}
				</p>
				<h1 className="text-xl font-semibold">{project.name}</h1>
			</div>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("verticalScale")}
				</h2>
				<div className="rounded-lg border p-4">
					<p className="text-sm text-muted-foreground mb-4">
						{t("verticalScaleDesc")}
					</p>
					<ResourceForm
						orgId={orgId}
						projId={projId}
						labels={{
							containerName: t("containerName"),
							cpus: t("cpus"),
							memoryMb: t("memoryMb"),
							apply: t("apply"),
							success: t("applySuccess"),
							error: t("applyError"),
						}}
					/>
				</div>
			</section>

			<p className="text-xs text-muted-foreground opacity-60">
				{t("projectId")} {projId}
			</p>
		</div>
	);
}
