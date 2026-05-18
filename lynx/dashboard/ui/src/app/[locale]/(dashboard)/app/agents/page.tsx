import { Suspense } from "react";
import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { AgentList } from "./AgentList";
import { AgentListSkeleton } from "./AgentListSkeleton";
import { RegisterAgentDialog } from "./RegisterAgentDialog";

export default async function AgentsPage({
	params,
}: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.agents" });
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-6">
			<div className="flex items-center justify-between">
				<h1 className="text-xl font-semibold">{t("title")}</h1>
				<RegisterAgentDialog
					token={token}
					label={t("register")}
					successTitle={t("registerSuccess")}
					successDesc={t("registerSuccessDesc")}
					agentIdLabel={t("agentId")}
					wgIpLabel={t("wgIp")}
					syncTokenLabel={t("syncToken")}
					warnOnce={t("warnOnce")}
					doneLabel={t("done")}
				/>
			</div>

			<Suspense fallback={<AgentListSkeleton />}>
				<AgentList token={token} locale={locale} />
			</Suspense>
		</div>
	);
}
