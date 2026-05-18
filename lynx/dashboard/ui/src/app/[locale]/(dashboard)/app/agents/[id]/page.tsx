import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import Link from "next/link";
import { BACKEND_URL } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { ChevronRight, Shield } from "lucide-react";
import { AgentDetailActions } from "@/components/(dashboard)/app/agents/detail/AgentDetailActions";
import { MetricsPanel } from "@/components/(dashboard)/app/agents/detail/MetricsPanel";
import { NftablesAlert } from "@/components/(dashboard)/app/agents/NftablesAlert";
import { NftRulesPanel } from "@/components/(dashboard)/app/agents/detail/NftRulesPanel";
import {
	listGlobalRules,
	listLocalRules,
	createGlobalRule,
	deleteGlobalRule,
	pushGlobalRules,
	createLocalRule,
	deleteLocalRule,
	pushLocalRules,
} from "@/actions/(dashboard)/app/agents/nftables";

interface Agent {
	id: string;
	name: string;
	wg_ip: string;
	wg_endpoint: string | null;
	status: "online" | "lockdown" | "offline";
	version: string | null;
	last_heartbeat: string | null;
	created_at: string;
}

interface NftStatus {
	diverged: boolean;
	detail?: string | null;
}

const STATUS_BADGE: Record<
	Agent["status"],
	"default" | "destructive" | "secondary"
> = {
	online: "default",
	lockdown: "destructive",
	offline: "secondary",
};

async function fetchAgent(token: string, id: string): Promise<Agent | null> {
	try {
		const res = await fetch(`${BACKEND_URL}/agents/${id}`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (res.status === 404) return null;
		if (!res.ok) return null;
		return res.json();
	} catch {
		return null;
	}
}

async function fetchNftStatus(
	token: string,
	id: string,
): Promise<NftStatus> {
	try {
		const res = await fetch(`${BACKEND_URL}/agents/${id}/nftables-status`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) return { diverged: false };
		return res.json();
	} catch {
		return { diverged: false };
	}
}

function formatTime(ts: string | null): string {
	if (!ts) return "—";
	return new Date(ts).toLocaleString("en-GB", {
		year: "numeric",
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
		second: "2-digit",
		hour12: false,
	});
}

function formatHeartbeat(ts: string | null): string {
	if (!ts) return "—";
	const diff = Math.floor((Date.now() - new Date(ts).getTime()) / 1000);
	if (diff < 60) return `${diff}s ago`;
	if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
	return `${Math.floor(diff / 3600)}h ago`;
}

export default async function AgentDetailPage({
	params,
}: {
	params: Promise<{ locale: string; id: string }>;
}) {
	const { locale, id: agentId } = await params;
	const [t, jar] = await Promise.all([
		getTranslations({ locale, namespace: "app.agents" }),
		cookies(),
	]);
	const tok = jar.get("access_token")?.value ?? "";

	const [agent, nft, globalRules, localRules] = await Promise.all([
		fetchAgent(tok, agentId),
		fetchNftStatus(tok, agentId),
		listGlobalRules(),
		listLocalRules(agentId),
	]);

	if (!agent) notFound();

	return (
		<div className="flex flex-col p-6 gap-6 max-w-3xl">
			<nav className="flex items-center gap-1 text-sm text-muted-foreground">
				<Link
					href={`/${locale}/app/agents`}
					className="hover:text-foreground transition-colors"
				>
					{t("title")}
				</Link>
				<ChevronRight className="size-3.5" />
				<span className="text-foreground">{agent.name}</span>
			</nav>

			<div className="flex items-center gap-3 flex-wrap">
				<h1 className="text-xl font-semibold">{agent.name}</h1>
				<Badge variant={STATUS_BADGE[agent.status]}>
					{t(`status.${agent.status}`)}
				</Badge>
			</div>

			<div className="rounded-lg border p-4 flex flex-col gap-3">
				<div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
					<div>
						<p className="text-xs text-muted-foreground">{t("wgIpLabel")}</p>
						<p className="font-mono font-medium">{agent.wg_ip}</p>
					</div>
					{agent.wg_endpoint && (
						<div>
							<p className="text-xs text-muted-foreground">Endpoint</p>
							<p className="font-mono font-medium">{agent.wg_endpoint}</p>
						</div>
					)}
					<div>
						<p className="text-xs text-muted-foreground">{t("version")}</p>
						<p className="font-mono font-medium">{agent.version ?? "—"}</p>
					</div>
					<div>
						<p className="text-xs text-muted-foreground">{t("lastHeartbeat")}</p>
						<p className="font-medium">{formatHeartbeat(agent.last_heartbeat)}</p>
						{agent.last_heartbeat && (
							<p className="text-xs text-muted-foreground">
								{formatTime(agent.last_heartbeat)}
							</p>
						)}
					</div>
					<div>
						<p className="text-xs text-muted-foreground">ID</p>
						<p className="font-mono text-xs text-muted-foreground select-all">
							{agent.id}
						</p>
					</div>
				</div>
			</div>

			{agent.status === "online" && (
				<div className="rounded-lg border p-4">
					<MetricsPanel
						agentId={agent.id}
						labels={{
							metrics: t("metricsLive"),
							cpu: t("metricsCpu"),
							memory: t("metricsMemory"),
							disk: t("metricsDisk"),
							connecting: t("metricsConnecting"),
							agentOffline: t("metricsAgentOffline"),
							offline: t("metricsOffline"),
						}}
					/>
				</div>
			)}

			<div className="rounded-lg border p-4 flex flex-col gap-4">
				<div className="flex items-center gap-2 text-sm font-medium">
					<Shield className="size-3.5" />
					{t("nftRules")}
				</div>
				<p className="text-xs text-muted-foreground">{t("nftRulesDesc")}</p>

				{nft.diverged && (
					<NftablesAlert
						agentId={agent.id}
						detail={nft.detail ?? null}
						labels={{
							title: t("nftDiverged"),
							restore: t("nftRestore"),
							accept: t("nftAccept"),
							restoreSuccess: t("nftRestoreSuccess"),
							acceptSuccess: t("nftAcceptSuccess"),
							error: t("nftError"),
						}}
					/>
				)}

				<div className="flex flex-col gap-1">
					<p className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("nftGlobal")}
					</p>
					<NftRulesPanel
						initialRules={globalRules}
						labels={{
							addRule: t("nftAddRule"),
							kind: t("nftKind"),
							port: t("nftPort"),
							protocol: t("nftProtocol"),
							ipList: t("nftIpList"),
							ratePerMin: t("nftRatePerMin"),
							description: t("nftDescription"),
							priority: t("nftPriority"),
							create: t("nftCreate"),
							createSuccess: t("nftCreateSuccess"),
							createError: t("nftCreateError"),
							deleteSuccess: t("nftDeleteSuccess"),
							deleteError: t("nftDeleteError"),
							push: t("nftPush"),
							pushSuccess: t("nftPushSuccess"),
							pushError: t("nftPushError"),
							noRules: t("nftNoRules"),
							kindAllowPort: t("nftKindAllowPort"),
							kindBlockPort: t("nftKindBlockPort"),
							kindAllowIp: t("nftKindAllowIp"),
							kindBlockIp: t("nftKindBlockIp"),
							kindRateLimit: t("nftKindRateLimit"),
							protoTcp: t("nftProtoTcp"),
							protoUdp: t("nftProtoUdp"),
							protoBoth: t("nftProtoBoth"),
						}}
						onCreateRule={createGlobalRule}
						onDeleteRule={deleteGlobalRule}
						onPush={pushGlobalRules}
					/>
				</div>

				<div className="border-t pt-3 flex flex-col gap-1">
					<p className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("nftLocal")}
					</p>
					<NftRulesPanel
						initialRules={localRules}
						labels={{
							addRule: t("nftAddRule"),
							kind: t("nftKind"),
							port: t("nftPort"),
							protocol: t("nftProtocol"),
							ipList: t("nftIpList"),
							ratePerMin: t("nftRatePerMin"),
							description: t("nftDescription"),
							priority: t("nftPriority"),
							create: t("nftCreate"),
							createSuccess: t("nftCreateSuccess"),
							createError: t("nftCreateError"),
							deleteSuccess: t("nftDeleteSuccess"),
							deleteError: t("nftDeleteError"),
							push: t("nftPush"),
							pushSuccess: t("nftPushSuccess"),
							pushError: t("nftPushError"),
							noRules: t("nftNoRules"),
							kindAllowPort: t("nftKindAllowPort"),
							kindBlockPort: t("nftKindBlockPort"),
							kindAllowIp: t("nftKindAllowIp"),
							kindBlockIp: t("nftKindBlockIp"),
							kindRateLimit: t("nftKindRateLimit"),
							protoTcp: t("nftProtoTcp"),
							protoUdp: t("nftProtoUdp"),
							protoBoth: t("nftProtoBoth"),
						}}
						onCreateRule={createLocalRule.bind(null, agentId)}
						onDeleteRule={deleteLocalRule.bind(null, agentId)}
						onPush={pushLocalRules.bind(null, agentId)}
					/>
				</div>
			</div>

			<div className="rounded-lg border p-4 flex flex-col gap-3">
				<p className="text-sm font-medium">{t("auditLog")}</p>
				<Link
					href={`/${locale}/app/agents/${agent.id}/audit-log`}
					className="text-sm text-muted-foreground underline underline-offset-2 hover:text-foreground transition-colors w-fit"
				>
					{t("auditLog")} →
				</Link>
			</div>

			<div className="rounded-lg border border-destructive/30 p-4 flex flex-col gap-3">
				<AgentDetailActions
					agentId={agent.id}
					locale={locale}
					labels={{
						reboot: t("reboot"),
						rebootConfirm: t("rebootConfirm"),
						rebootSuccess: t("rebootSuccess"),
						rebootError: t("rebootError"),
						deleteAgent: t("deleteAgent"),
						deleteConfirm: t("deleteConfirm"),
						deleteError: t("deleteError"),
					}}
				/>
			</div>
		</div>
	);
}
