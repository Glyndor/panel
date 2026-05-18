import { ChevronRight } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { notFound } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { Badge } from "@/components/ui/badge";
import { BACKEND_URL } from "@/lib/api";

interface AuditEntry {
	agent_id: string;
	command_type: string;
	created_at: string;
	entry_hash: string;
	error: string | null;
	id: string;
	organization_id: string | null;
	result: "success" | "rejected" | "failed";
	user_id: string | null;
}

interface AuditResponse {
	entries: AuditEntry[];
	limit: number;
	offset: number;
	total: number;
}

async function fetchAuditLog(token: string, agentId: string, limit = 50, offset = 0): Promise<AuditResponse | null> {
	try {
		const res = await fetch(`${BACKEND_URL}/agents/${agentId}/audit-log?limit=${limit}&offset=${offset}`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (res.status === 404) return null;
		if (!res.ok) return { entries: [], limit, offset, total: 0 };
		return res.json();
	} catch {
		return { entries: [], limit, offset, total: 0 };
	}
}

const RESULT_VARIANT: Record<AuditEntry["result"], "default" | "destructive" | "secondary"> = {
	failed: "destructive",
	rejected: "secondary",
	success: "default",
};

function formatTime(ts: string): string {
	const d = new Date(ts);
	return d.toLocaleString("en-GB", {
		day: "numeric",
		hour: "2-digit",
		hour12: false,
		minute: "2-digit",
		month: "short",
		second: "2-digit",
		year: "numeric",
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
				<Link className="hover:text-foreground transition-colors" href={`/${locale}/app/agents`}>
					{tAgents("title")}
				</Link>
				<ChevronRight className="size-3.5" />
				<span className="font-mono text-xs">{agentId.slice(0, 8)}…</span>
				<ChevronRight className="size-3.5" />
				<span className="text-foreground">{t("title")}</span>
			</nav>

			<div>
				<h1 className="text-xl font-semibold">{t("title")}</h1>
				<p className="text-xs text-muted-foreground mt-0.5">{data.total} entries total</p>
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
								<th className="px-3 py-2 text-left hidden md:table-cell">{t("user")}</th>
								<th className="px-3 py-2 text-left hidden lg:table-cell">{t("hash")}</th>
							</tr>
						</thead>
						<tbody className="divide-y">
							{data.entries.map((e) => (
								<tr className="hover:bg-muted/20" key={e.id}>
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
										<Badge className="text-xs" variant={RESULT_VARIANT[e.result]}>
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
						className="text-sm text-muted-foreground underline underline-offset-2 hover:text-foreground"
						href={`/${locale}/app/agents/${agentId}/audit-log?offset=${Math.max(0, offset - limit)}`}
					>
						← Previous
					</Link>
				)}
				{hasMore && (
					<Link
						className="text-sm underline underline-offset-2 hover:text-foreground"
						href={`/${locale}/app/agents/${agentId}/audit-log?offset=${offset + limit}`}
					>
						{t("loadMore")} →
					</Link>
				)}
			</div>
		</div>
	);
}
