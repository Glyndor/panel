import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";

interface RotationEntry {
	id: string;
	triggered_by: string | null;
	reason: string;
	scope: string;
	created_at: string;
}

async function fetchRotationLog(token: string): Promise<RotationEntry[]> {
	try {
		const res = await fetch(`${BACKEND_URL}/admin/rotation-log`, {
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

function formatTime(ts: string): string {
	return new Date(ts).toLocaleString("en-GB", {
		year: "numeric",
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
		hour12: false,
	});
}

export async function RotationLog({
	token,
	locale,
}: {
	token: string;
	locale: string;
}) {
	const [t, entries] = await Promise.all([
		getTranslations({ locale, namespace: "app.settings" }),
		fetchRotationLog(token),
	]);

	if (entries.length === 0) {
		return (
			<p className="text-sm text-muted-foreground">{t("rotationLogEmpty")}</p>
		);
	}

	return (
		<div className="rounded-lg border divide-y text-sm">
			{entries.map((e) => (
				<div key={e.id} className="flex items-start gap-3 px-4 py-3">
					<div className="flex-1 min-w-0">
						<div className="flex items-center gap-2 flex-wrap">
							<span className="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">
								{e.scope}
							</span>
							<span className="text-muted-foreground text-xs">{e.reason}</span>
						</div>
						{e.triggered_by && (
							<p className="text-xs text-muted-foreground mt-0.5 font-mono">
								by {e.triggered_by.slice(0, 8)}…
							</p>
						)}
					</div>
					<span className="text-xs text-muted-foreground whitespace-nowrap">
						{formatTime(e.created_at)}
					</span>
				</div>
			))}
		</div>
	);
}
