import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { AgentList } from "@/components/(dashboard)/app/agents/AgentList";
import { AgentListSkeleton } from "@/components/(dashboard)/app/agents/AgentListSkeleton";
import { RegisterAgentDialog } from "@/components/(dashboard)/app/agents/RegisterAgentDialog";

export default async function AgentsPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.agents" });
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-6">
			<div className="flex items-center justify-between">
				<h1 className="text-xl font-semibold">{t("title")}</h1>
				<RegisterAgentDialog
					agentIdLabel={t("agentId")}
					doneLabel={t("done")}
					label={t("register")}
					successDesc={t("registerSuccessDesc")}
					successTitle={t("registerSuccess")}
					syncTokenLabel={t("syncToken")}
					token={token}
					warnOnce={t("warnOnce")}
					wgIpLabel={t("wgIp")}
				/>
			</div>

			<Suspense fallback={<AgentListSkeleton />}>
				<AgentList locale={locale} token={token} />
			</Suspense>
		</div>
	);
}
