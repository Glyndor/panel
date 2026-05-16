import { Suspense } from "react";
import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { OrgList } from "./OrgList";
import { OrgListSkeleton } from "./OrgListSkeleton";

export default async function OrganizationsPage({
	params,
}: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.organizations" });
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-6">
			<div className="flex items-center justify-between">
				<h1 className="text-xl font-semibold">{t("title")}</h1>
			</div>

			<Suspense fallback={<OrgListSkeleton />}>
				<OrgList token={token} locale={locale} />
			</Suspense>
		</div>
	);
}
