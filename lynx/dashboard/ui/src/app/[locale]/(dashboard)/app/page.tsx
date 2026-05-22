import { Building2, FolderOpen, Monitor } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { redirect } from "next/navigation";
import { getTranslations } from "next-intl/server";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { BACKEND_URL } from "@/lib/api";

interface AccessCheck {
	hasOrg: boolean;
	hasVps: boolean;
}

async function checkAccess(token: string): Promise<AccessCheck> {
	if (!token) return { hasOrg: false, hasVps: false };
	const headers = { Authorization: `Bearer ${token}` };
	try {
		const [agentsRes, orgsRes] = await Promise.all([
			fetch(`${BACKEND_URL}/agents`, { cache: "no-store", headers }),
			fetch(`${BACKEND_URL}/organizations`, { cache: "no-store", headers }),
		]);
		return {
			hasOrg: orgsRes.ok,
			hasVps: agentsRes.ok,
		};
	} catch {
		return { hasOrg: false, hasVps: false };
	}
}

export default async function AppPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	const { hasVps, hasOrg } = await checkAccess(token);

	// Only project access (no VPS, no org) → redirect directly to project list
	if (!hasVps && !hasOrg) {
		redirect(`/${locale}/app/p`);
	}

	const t = await getTranslations({ locale, namespace: "app.nav" });

	const sections = [
		...(hasVps
			? [
					{
						description: t("vpsDesc"),
						href: `/${locale}/app/v`,
						icon: Monitor,
						label: t("vps"),
					},
				]
			: []),
		...(hasOrg
			? [
					{
						description: t("orgsDesc"),
						href: `/${locale}/app/o`,
						icon: Building2,
						label: t("organizations"),
					},
				]
			: []),
		{
			description: t("projectsDesc"),
			href: `/${locale}/app/p`,
			icon: FolderOpen,
			label: t("projects"),
		},
	];

	// Single section → redirect directly
	if (sections.length === 1) {
		redirect(sections[0]!.href);
	}

	return (
		<div className="flex flex-col p-6 gap-8">
			<h1 className="text-xl font-semibold">{t("selectSection")}</h1>
			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{sections.map(({ href, icon: Icon, label, description }) => (
					<Link href={href} key={href}>
						<Card className="hover:border-foreground/20 transition-colors cursor-pointer h-full">
							<CardHeader className="pb-2">
								<CardTitle className="text-base flex items-center gap-2">
									<Icon className="size-4 shrink-0 text-muted-foreground" />
									{label}
								</CardTitle>
							</CardHeader>
							<CardContent>
								<p className="text-sm text-muted-foreground">{description}</p>
							</CardContent>
						</Card>
					</Link>
				))}
			</div>
		</div>
	);
}
