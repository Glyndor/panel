import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

type Agent = {
	id: string;
	name: string;
	status: "online" | "lockdown" | "offline";
	wg_ip: string;
	version: string | null;
	last_heartbeat: string | null;
};

async function fetchAgents(token: string): Promise<Agent[]> {
	if (!token) return [];
	try {
		const res = await fetch(`${BACKEND_URL}/agents`, {
			headers: { Authorization: `Bearer ${token}` },
			next: { revalidate: 30 },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

const STATUS_BADGE: Record<
	Agent["status"],
	"default" | "destructive" | "secondary"
> = {
	online: "default",
	lockdown: "destructive",
	offline: "secondary",
};

function formatHeartbeat(ts: string | null): string {
	if (!ts) return "—";
	const diff = Math.floor((Date.now() - new Date(ts).getTime()) / 1000);
	if (diff < 60) return `${diff}s ago`;
	if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
	return `${Math.floor(diff / 3600)}h ago`;
}

export async function AgentList({
	token,
	locale,
}: {
	token: string;
	locale: string;
}) {
	const agents = await fetchAgents(token);
	const t = await getTranslations({ locale, namespace: "app.agents" });

	if (agents.length === 0) {
		return (
			<div className="flex flex-1 items-center justify-center rounded-lg border border-dashed min-h-64">
				<div className="text-center max-w-xs">
					<p className="text-sm font-medium">{t("noAgents")}</p>
					<p className="mt-1 text-xs text-muted-foreground">
						{t("noAgentsDesc")}
					</p>
				</div>
			</div>
		);
	}

	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{agents.map((agent) => (
				<Card key={agent.id}>
					<CardHeader className="pb-2">
						<div className="flex items-center justify-between gap-2">
							<CardTitle className="text-base truncate">
								{agent.name}
							</CardTitle>
							<Badge variant={STATUS_BADGE[agent.status]}>
								{t(`status.${agent.status}`)}
							</Badge>
						</div>
					</CardHeader>
					<CardContent className="space-y-1 text-sm text-muted-foreground">
						<p>
							<span className="font-medium text-foreground">
								{t("wgIp")}
							</span>{" "}
							{agent.wg_ip}
						</p>
						<p>
							<span className="font-medium text-foreground">
								{t("version")}
							</span>{" "}
							{agent.version ?? "—"}
						</p>
						<p>
							<span className="font-medium text-foreground">
								{t("lastHeartbeat")}
							</span>{" "}
							{formatHeartbeat(agent.last_heartbeat)}
						</p>
						<p className="truncate text-xs opacity-60">{agent.id}</p>
					</CardContent>
				</Card>
			))}
		</div>
	);
}
