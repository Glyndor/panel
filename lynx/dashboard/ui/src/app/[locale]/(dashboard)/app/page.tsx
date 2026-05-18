import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { listAlertsAction, type SecurityAlert } from "@/actions/(dashboard)/app/admin/alerts";
import { AlertsPanel } from "@/components/(dashboard)/app/admin/AlertsPanel";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { BACKEND_URL } from "@/lib/api";

// ---------------------------------------------------------------------------
// Data fetching
// ---------------------------------------------------------------------------

type AgentSummary = { id: string; status: string };
type OrgSummary = { id: string };
type AgentEvent = {
	id: string;
	agent_id: string;
	event: string;
	detail: string | null;
	created_at: string;
};

async function fetchStats(token: string) {
	const headers = { Authorization: `Bearer ${token}` };
	try {
		const [agentsRes, orgsRes] = await Promise.all([
			fetch(`${BACKEND_URL}/agents`, { headers, next: { revalidate: 30 } }),
			fetch(`${BACKEND_URL}/organizations`, { headers, next: { revalidate: 30 } }),
		]);
		const agents: AgentSummary[] = agentsRes.ok ? await agentsRes.json() : [];
		const orgs: OrgSummary[] = orgsRes.ok ? await orgsRes.json() : [];
		return { agents, orgs };
	} catch {
		return { agents: [], orgs: [] };
	}
}

async function fetchRecentEvents(token: string): Promise<AgentEvent[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/agents/events?limit=10`, {
			headers: { Authorization: `Bearer ${token}` },
			next: { revalidate: 15 },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

async function OverviewStats({ token, locale }: { token: string; locale: string }) {
	const t = await getTranslations({ locale, namespace: "app.overview" });
	const { agents, orgs } = await fetchStats(token);
	const online = agents.filter((a) => a.status === "online").length;

	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			<StatCard sub={`/ ${agents.length}`} title={t("agentsOnline")} value={String(online)} />
			<StatCard title={t("organizations")} value={String(orgs.length)} />
		</div>
	);
}

function StatCard({ title, value, sub }: { title: string; value: string; sub?: string }) {
	return (
		<Card>
			<CardHeader className="pb-2">
				<CardTitle className="text-sm font-medium text-muted-foreground">{title}</CardTitle>
			</CardHeader>
			<CardContent>
				<p className="text-3xl font-bold">
					{value}
					{sub && <span className="ml-1 text-base font-normal text-muted-foreground">{sub}</span>}
				</p>
			</CardContent>
		</Card>
	);
}

async function RecentEvents({ token, locale }: { token: string; locale: string }) {
	const t = await getTranslations({ locale, namespace: "app.overview" });
	const events = await fetchRecentEvents(token);

	if (events.length === 0) {
		return (
			<div className="flex items-center justify-center rounded-lg border border-dashed min-h-32">
				<p className="text-sm text-muted-foreground">{t("noEvents")}</p>
			</div>
		);
	}

	return (
		<div className="rounded-lg border divide-y">
			{events.map((ev) => (
				<div className="flex items-start gap-3 px-4 py-3 text-sm" key={ev.id}>
					<span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs font-mono">{ev.event}</span>
					<span className="text-muted-foreground truncate flex-1">{ev.detail ?? ev.agent_id}</span>
					<span className="shrink-0 text-xs text-muted-foreground">
						{new Date(ev.created_at).toLocaleTimeString(undefined, {
							hour: "2-digit",
							minute: "2-digit",
						})}
					</span>
				</div>
			))}
		</div>
	);
}

function StatsSkeleton() {
	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{[0, 1].map((i) => (
				<Card key={i}>
					<CardHeader className="pb-2">
						<Skeleton className="h-4 w-24" />
					</CardHeader>
					<CardContent>
						<Skeleton className="h-8 w-16" />
					</CardContent>
				</Card>
			))}
		</div>
	);
}

function EventsSkeleton() {
	return (
		<div className="rounded-lg border divide-y">
			{[0, 1, 2].map((i) => (
				<div className="flex items-center gap-3 px-4 py-3" key={i}>
					<Skeleton className="h-5 w-20" />
					<Skeleton className="h-4 flex-1" />
					<Skeleton className="h-4 w-10" />
				</div>
			))}
		</div>
	);
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default async function OverviewPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.overview" });
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";
	const alerts: SecurityAlert[] = await listAlertsAction();

	return (
		<div className="flex flex-col p-6 gap-8">
			<h1 className="text-xl font-semibold">{t("title")}</h1>

			{alerts.length > 0 && (
				<section className="flex flex-col gap-3">
					<h2 className="text-xs font-medium text-destructive uppercase tracking-wider">
						{t("securityAlerts")}
					</h2>
					<AlertsPanel
						initial={alerts}
						labels={{
							acknowledge: t("acknowledge"),
							acknowledged: t("acknowledged"),
							error: t("acknowledgeError"),
							noAlerts: t("noAlerts"),
							title: t("securityAlerts"),
						}}
					/>
				</section>
			)}

			<Suspense fallback={<StatsSkeleton />}>
				<OverviewStats locale={locale} token={token} />
			</Suspense>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("recentEvents")}
				</h2>
				<Suspense fallback={<EventsSkeleton />}>
					<RecentEvents locale={locale} token={token} />
				</Suspense>
			</section>
		</div>
	);
}
