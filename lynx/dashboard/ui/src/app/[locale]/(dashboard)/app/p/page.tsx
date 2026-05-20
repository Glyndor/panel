import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { ProjectList } from "@/components/(dashboard)/p/list/ProjectList";
import { ProjectListSkeleton } from "@/components/(dashboard)/p/list/ProjectListSkeleton";

export default async function ProjectsPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, jar] = await Promise.all([getTranslations({ locale, namespace: "app.projects" }), cookies()]);
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-6">
			<h1 className="text-xl font-semibold">{t("title")}</h1>
			<Suspense fallback={<ProjectListSkeleton />}>
				<ProjectList locale={locale} token={token} />
			</Suspense>
		</div>
	);
}
