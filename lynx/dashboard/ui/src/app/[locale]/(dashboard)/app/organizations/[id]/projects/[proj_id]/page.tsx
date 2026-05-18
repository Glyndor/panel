import { ChevronRight } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { ContainerCard } from "@/components/(dashboard)/app/organizations/[id]/projects/[proj_id]/ContainerCard";
import { DeployForm } from "@/components/(dashboard)/app/organizations/[id]/projects/[proj_id]/DeployForm";
import { HorizontalScaleSection } from "@/components/(dashboard)/app/organizations/[id]/projects/[proj_id]/HorizontalScaleSection";
import { ResourceForm } from "@/components/(dashboard)/app/organizations/[id]/projects/[proj_id]/ResourceForm";
import { BACKEND_URL } from "@/lib/api";

interface Project {
	agent_id: string;
	created_at: string;
	id: string;
	name: string;
	organization_id: string;
	slug: string;
}

interface Container {
	Image: string;
	Names: string[];
	State: string;
	Status: string;
}

interface Agent {
	id: string;
	name: string;
	status: string;
	wg_ip: string;
}

interface Tunnel {
	agent_a_wg_ip: string;
	agent_b_id: string;
	agent_b_wg_ip: string;
	id: string;
	replica_count: number;
	status: string;
}

async function fetchProject(token: string, orgId: string, projId: string): Promise<Project | null> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${orgId}/projects/${projId}`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
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

async function fetchTunnels(token: string, orgId: string, projId: string): Promise<Tunnel[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${orgId}/projects/${projId}/scale/horizontal`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

async function fetchContainers(token: string, orgId: string, projId: string): Promise<Container[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/organizations/${orgId}/projects/${projId}/containers`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
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
	const [t, jar] = await Promise.all([getTranslations({ locale, namespace: "app.projects" }), cookies()]);
	const tok = jar.get("access_token")?.value ?? "";

	const [project, containers, tunnels, agents] = await Promise.all([
		fetchProject(tok, orgId, projId),
		fetchContainers(tok, orgId, projId),
		fetchTunnels(tok, orgId, projId),
		fetchAgents(tok),
	]);

	if (!project) notFound();

	const containerLabels = {
		error: t("cActionError"),
		remove: t("cRemove"),
		restart: t("cRestart"),
		start: t("cStart"),
		stop: t("cStop"),
		success: t("cActionSuccess"),
	};

	return (
		<div className="flex flex-col p-6 gap-6 max-w-3xl">
			<nav className="flex items-center gap-1 text-sm text-muted-foreground">
				<Link className="hover:text-foreground transition-colors" href={`/${locale}/app/organizations`}>
					{t("orgs")}
				</Link>
				<ChevronRight className="size-3.5" />
				<Link
					className="hover:text-foreground transition-colors"
					href={`/${locale}/app/organizations/${orgId}`}
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
								container={c}
								key={c.Names[0] ?? c.Image}
								labels={containerLabels}
								orgId={orgId}
								projId={projId}
							/>
						))}
					</div>
				)}
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("deploy")}</h2>
				<div className="rounded-lg border p-4">
					<DeployForm
						labels={{
							cpus: t("cpus"),
							deploy: t("deployBtn"),
							env: t("cEnv"),
							error: t("deployError"),
							image: t("cImage"),
							memoryMb: t("memoryMb"),
							name: t("cName"),
							ports: t("cPorts"),
							success: t("deploySuccess"),
						}}
						orgId={orgId}
						projId={projId}
					/>
				</div>
			</section>

			<HorizontalScaleSection
				agents={agents.filter((a) => a.id !== project.agent_id)}
				labels={{
					addBtn: t("addTunnel"),
					agentB: t("tunnelTargetAgent"),
					confirm: t("tunnelConfirm"),
					desc: t("horizontalScaleDesc"),
					dialogTitle: t("addTunnelTitle"),
					error: t("tunnelError"),
					image: t("tunnelImage"),
					noTunnels: t("noTunnels"),
					replicaCount: t("tunnelReplicaCount"),
					replicas: t("tunnelReplicas"),
					status: "Status",
					success: t("tunnelSuccess"),
					targetAgent: t("tunnelTargetAgent"),
					teardownError: t("tunnelTeardownError"),
					teardownSuccess: t("tunnelTeardownSuccess"),
					title: t("horizontalScale"),
				}}
				orgId={orgId}
				projId={projId}
				tunnels={tunnels}
			/>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("verticalScale")}
				</h2>
				<div className="rounded-lg border p-4">
					<p className="text-sm text-muted-foreground mb-4">{t("verticalScaleDesc")}</p>
					<ResourceForm
						labels={{
							apply: t("apply"),
							containerName: t("containerName"),
							cpus: t("cpus"),
							error: t("applyError"),
							memoryMb: t("memoryMb"),
							success: t("applySuccess"),
						}}
						orgId={orgId}
						projId={projId}
					/>
				</div>
			</section>

			<p className="text-xs text-muted-foreground opacity-60">
				{t("projectId")} {projId}
			</p>
		</div>
	);
}
