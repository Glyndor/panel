import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Building2 } from "lucide-react";

type Org = {
	id: string;
	name: string;
	slug: string;
	owner_id: string;
	created_at: string;
	member_count: number;
};

async function fetchOrgs(token: string): Promise<Org[]> {
	if (!token) return [];
	try {
		const res = await fetch(`${BACKEND_URL}/organizations`, {
			headers: { Authorization: `Bearer ${token}` },
			next: { revalidate: 60 },
		});
		if (!res.ok) return [];
		return res.json();
	} catch {
		return [];
	}
}

export async function OrgList({
	token,
	locale,
}: {
	token: string;
	locale: string;
}) {
	const orgs = await fetchOrgs(token);
	const t = await getTranslations({ locale, namespace: "app.organizations" });

	if (orgs.length === 0) {
		return (
			<div className="flex flex-1 items-center justify-center rounded-lg border border-dashed min-h-64">
				<div className="text-center max-w-xs">
					<Building2 className="mx-auto mb-2 size-8 text-muted-foreground" />
					<p className="text-sm font-medium">{t("noOrgs")}</p>
					<p className="mt-1 text-xs text-muted-foreground">
						{t("noOrgsDesc")}
					</p>
				</div>
			</div>
		);
	}

	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{orgs.map((org) => (
				<Card key={org.id} className="hover:border-foreground/20 transition-colors cursor-pointer">
					<CardHeader className="pb-2">
						<CardTitle className="text-base truncate flex items-center gap-2">
							<Building2 className="size-4 shrink-0 text-muted-foreground" />
							{org.name}
						</CardTitle>
					</CardHeader>
					<CardContent className="space-y-1 text-sm text-muted-foreground">
						<p>
							<span className="font-medium text-foreground">
								{t("slug")}
							</span>{" "}
							{org.slug}
						</p>
						<p>
							<span className="font-medium text-foreground">
								{t("members")}
							</span>{" "}
							{org.member_count}
						</p>
						<p className="truncate text-xs opacity-60">{org.id}</p>
					</CardContent>
				</Card>
			))}
		</div>
	);
}
