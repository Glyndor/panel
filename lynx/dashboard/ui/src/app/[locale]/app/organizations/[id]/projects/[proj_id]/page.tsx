import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import Link from "next/link";
import { ChevronRight } from "lucide-react";
import { ResourceForm } from "./ResourceForm";
import { ContainerCard } from "./ContainerCard";
import { DeployForm } from "./DeployForm";

interface Project {
	id: string;
	name: string;
	slug: string;
	agent_id: string;
	organization_id: string;
	created_at: string;
}

interface Container {
	Names: string[];
	Image: string;
	Status: string;
	State: string;
}

async function fetchProject(
	token: string,
	orgId: string,
	projId: string,
): Promise<Project | null> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/organizations/${orgId}/projects/${projId}`,
			{ headers: { Authorization: `Bearer ${token}` }, cache: "no-store" },
		);
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}

async function fetchContainers(
	token: string,
	orgId: string,
	projId: string,
): Promise<Container[]> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/organizations/${orgId}/projects/${projId}/containers`,
			{ headers: { Authorization: `Bearer ${token}` }, cache: "no-store" },
		);
		if (!res.ok) return [];
		const data = (await res.json()) as { containers?: Container[] } | Container[];
		return Array.isArray(data) ? data : (data.containers ?? []);
	} catch {
		return [];
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

	const [project, containers] = await Promise.all([
		fetchProject(tok, orgId, projId),
		fetchContainers(tok, orgId, projId),
	]);

	if (!project) notFound();

	const containerLabels = {
		start: t("cStart"),
		stop: t("cStop"),
		restart: t("cRestart"),
		remove: t("cRemove"),
		success: t("cActionSuccess"),
		error: t("cActionError"),
	};

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
					{t("containers")} ({containers.length})
				</h2>
				{containers.length === 0 ? (
					<p className="text-sm text-muted-foreground">{t("noContainers")}</p>
				) : (
					<div className="rounded-lg border divide-y">
						{containers.map((c) => (
							<ContainerCard
								key={c.Names[0] ?? c.Image}
								orgId={orgId}
								projId={projId}
								container={c}
								labels={containerLabels}
							/>
						))}
					</div>
				)}
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("deploy")}
				</h2>
				<div className="rounded-lg border p-4">
					<DeployForm
						orgId={orgId}
						projId={projId}
						labels={{
							name: t("cName"),
							image: t("cImage"),
							ports: t("cPorts"),
							env: t("cEnv"),
							cpus: t("cpus"),
							memoryMb: t("memoryMb"),
							deploy: t("deployBtn"),
							success: t("deploySuccess"),
							error: t("deployError"),
						}}
					/>
				</div>
			</section>

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
