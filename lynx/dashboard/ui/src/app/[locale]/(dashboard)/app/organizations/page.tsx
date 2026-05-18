import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { CreateOrgDialog } from "@/components/(dashboard)/app/organizations/CreateOrgDialog";
import { OrgList } from "@/components/(dashboard)/app/organizations/OrgList";
import { OrgListSkeleton } from "@/components/(dashboard)/app/organizations/OrgListSkeleton";

export default async function OrganizationsPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.organizations" });
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-6">
			<div className="flex items-center justify-between">
				<h1 className="text-xl font-semibold">{t("title")}</h1>
				<CreateOrgDialog
					errorMsg={t("createError")}
					label={t("create")}
					slugConflict={t("slugConflict")}
					token={token}
				/>
			</div>

			<Suspense fallback={<OrgListSkeleton />}>
				<OrgList locale={locale} token={token} />
			</Suspense>
		</div>
	);
}
