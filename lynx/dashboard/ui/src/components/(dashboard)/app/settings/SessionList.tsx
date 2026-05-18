import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import { RevokeButton } from "./RevokeButton";

type Session = {
	id: string;
	ip: string | null;
	user_agent: string | null;
	created_at: string;
	last_used_at: string;
	expires_at: string;
};

async function fetchSessions(token: string): Promise<Session[]> {
	if (!token) return [];
	try {
		const res = await fetch(`${BACKEND_URL}/admin/sessions`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

function formatDate(ts: string): string {
	return new Date(ts).toLocaleString(undefined, {
		dateStyle: "medium",
		timeStyle: "short",
	});
}

function shortUA(ua: string | null): string {
	if (!ua) return "—";
	if (ua.length <= 60) return ua;
	return ua.slice(0, 57) + "…";
}

export async function SessionList({
	token,
	locale,
}: {
	token: string;
	locale: string;
}) {
	const sessions = await fetchSessions(token);
	const t = await getTranslations({ locale, namespace: "app.settings" });

	if (sessions.length === 0) {
		return (
			<div className="flex items-center justify-center rounded-lg border border-dashed min-h-32">
				<p className="text-sm text-muted-foreground">{t("noSessions")}</p>
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-2">
			{sessions.map((s) => (
				<div
					key={s.id}
					className="rounded-lg border p-4 flex items-start justify-between gap-4"
				>
					<div className="min-w-0 flex-1 space-y-0.5">
						<p className="text-sm font-medium truncate">
							{s.ip ?? "—"}
						</p>
						<p className="text-xs text-muted-foreground truncate">
							{shortUA(s.user_agent)}
						</p>
						<p className="text-xs text-muted-foreground">
							{t("lastUsed")} {formatDate(s.last_used_at)}
						</p>
					</div>
					<RevokeButton
						sessionId={s.id}
						label={t("revoke")}
						successMsg={t("revokeSuccess")}
						errorMsg={t("revokeError")}
					/>
				</div>
			))}
		</div>
	);
}
