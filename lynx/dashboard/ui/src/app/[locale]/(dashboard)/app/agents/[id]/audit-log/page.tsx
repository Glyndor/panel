import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import Link from "next/link";
import { ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";

interface AuditEntry {
	id: string;
	agent_id: string;
	organization_id: string | null;
	user_id: string | null;
	command_type: string;
	result: "success" | "rejected" | "failed";
	error: string | null;
	entry_hash: string;
	created_at: string;
}

interface AuditResponse {
	entries: AuditEntry[];
	total: number;
	limit: number;
	offset: number;
}

async function fetchAuditLog(
	token: string,
	agentId: string,
	limit = 50,
	offset = 0,
): Promise<AuditResponse | null> {
	try {
		const res = await fetch(
			`${BACKEND_URL}/agents/${agentId}/audit-log?limit=${limit}&offset=${offset}`,
			{ headers: { Authorization: `Bearer ${token}` }, cache: "no-store" },
		);
		if (res.status === 404) return null;
		if (!res.ok) return { entries: [], total: 0, limit, offset };
		return res.json();
	} catch {
		return { entries: [], total: 0, limit, offset };
	}
}

const RESULT_VARIANT: Record<
	AuditEntry["result"],
	"default" | "destructive" | "secondary"
> = {
	success: "default",
	rejected: "secondary",
	failed: "destructive",
};

function formatTime(ts: string): string {
	const d = new Date(ts);
	return d.toLocaleString("en-GB", {
		year: "numeric",
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
		second: "2-digit",
		hour12: false,
	});
}

export default async function AuditLogPage({
	params,
	searchParams,
}: {
	params: Promise<{ locale: string; id: string }>;
	searchParams: Promise<{ offset?: string }>;
}) {
	const { locale, id: agentId } = await params;
	const { offset: offsetParam } = await searchParams;
	const offset = parseInt(offsetParam ?? "0") || 0;
	const limit = 50;

	const [t, tAgents, jar] = await Promise.all([
		getTranslations({ locale, namespace: "app.auditLog" }),
		getTranslations({ locale, namespace: "app.agents" }),
		cookies(),
	]);
	const tok = jar.get("access_token")?.value ?? "";

	const data = await fetchAuditLog(tok, agentId, limit, offset);
	if (!data) notFound();

	const hasMore = offset + limit < data.total;
	const hasPrev = offset > 0;

	return (
		<div className="flex flex-col p-6 gap-6 max-w-5xl">
			<nav className="flex items-center gap-1 text-sm text-muted-foreground">
				<Link
					href={`/${locale}/app/agents`}
					className="hover:text-foreground transition-colors"
				>
					{tAgents("title")}
				</Link>
				<ChevronRight className="size-3.5" />
				<span className="font-mono text-xs">{agentId.slice(0, 8)}…</span>
				<ChevronRight className="size-3.5" />
				<span className="text-foreground">{t("title")}</span>
			</nav>

			<div>
				<h1 className="text-xl font-semibold">{t("title")}</h1>
				<p className="text-xs text-muted-foreground mt-0.5">
					{data.total} entries total
				</p>
			</div>

			{data.entries.length === 0 ? (
				<p className="text-sm text-muted-foreground">{t("noEntries")}</p>
			) : (
				<div className="rounded-lg border overflow-hidden">
					<table className="w-full text-sm">
						<thead>
							<tr className="border-b bg-muted/40 text-xs font-medium text-muted-foreground">
								<th className="px-3 py-2 text-left">{t("time")}</th>
								<th className="px-3 py-2 text-left">{t("command")}</th>
								<th className="px-3 py-2 text-left">{t("result")}</th>
								<th className="px-3 py-2 text-left hidden md:table-cell">
									{t("user")}
								</th>
								<th className="px-3 py-2 text-left hidden lg:table-cell">
									{t("hash")}
								</th>
							</tr>
						</thead>
						<tbody className="divide-y">
							{data.entries.map((e) => (
								<tr key={e.id} className="hover:bg-muted/20">
									<td className="px-3 py-2 whitespace-nowrap text-xs text-muted-foreground">
										{formatTime(e.created_at)}
									</td>
									<td className="px-3 py-2 font-mono text-xs">
										{e.command_type}
										{e.error && (
											<p className="text-destructive text-xs mt-0.5 truncate max-w-xs">
												{e.error}
											</p>
										)}
									</td>
									<td className="px-3 py-2">
										<Badge variant={RESULT_VARIANT[e.result]} className="text-xs">
											{t(`result${e.result.charAt(0).toUpperCase() + e.result.slice(1)}`)}
										</Badge>
									</td>
									<td className="px-3 py-2 hidden md:table-cell font-mono text-xs text-muted-foreground truncate max-w-[8rem]">
										{e.user_id ? e.user_id.slice(0, 8) + "…" : "—"}
									</td>
									<td className="px-3 py-2 hidden lg:table-cell font-mono text-xs text-muted-foreground">
										{e.entry_hash}…
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}

			<div className="flex items-center gap-3">
				{hasPrev && (
					<Link
						href={`/${locale}/app/agents/${agentId}/audit-log?offset=${Math.max(0, offset - limit)}`}
						className="text-sm text-muted-foreground underline underline-offset-2 hover:text-foreground"
					>
						← Previous
					</Link>
				)}
				{hasMore && (
					<Link
						href={`/${locale}/app/agents/${agentId}/audit-log?offset=${offset + limit}`}
						className="text-sm underline underline-offset-2 hover:text-foreground"
					>
						{t("loadMore")} →
					</Link>
				)}
			</div>
		</div>
	);
}
