import Image from "next/image";
import { getTranslations } from "next-intl/server";
import { LoginForm } from "@/components/(auth)/login/LoginForm";
import { BACKEND_URL } from "@/lib/api";

async function fetchCompanyName(): Promise<string> {
	try {
		const res = await fetch(`${BACKEND_URL}/branding`, { next: { revalidate: 60 } });
		if (!res.ok) return "Lynx";
		const data = (await res.json()) as { company_name?: string };
		return data.company_name ?? "Lynx";
	} catch {
		return "Lynx";
	}
}

export default async function LoginPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, tBrand, companyName] = await Promise.all([
		getTranslations({ locale, namespace: "auth.login" }),
		getTranslations({ locale, namespace: "branding" }),
		fetchCompanyName(),
	]);

	return (
		<div className="w-full max-w-sm">
			<div className="mb-8 flex flex-col items-center gap-3 lg:hidden">
				<Image src="/logo.webp" alt={companyName} width={56} height={56} priority />
				<p className="text-sm font-semibold">{companyName}</p>
			</div>

			<div className="mb-7">
				<h1 className="text-2xl font-bold tracking-tight">{t("title", { company: companyName })}</h1>
				<p className="mt-1.5 text-sm text-muted-foreground">{t("subtitle")}</p>
			</div>

			<LoginForm locale={locale} />

			<footer className="mt-10 text-center text-xs text-muted-foreground lg:hidden">
				{tBrand("madeWith")}{" "}
				<a
					className="font-medium text-foreground hover:underline"
					href="https://github.com/Jaro-c"
					rel="noopener noreferrer"
					target="_blank"
				>
					{tBrand("author")}
				</a>
				{" · "}
				<a
					className="font-medium text-foreground hover:underline"
					href="https://github.com/Glyndor/panel"
					rel="noopener noreferrer"
					target="_blank"
				>
					lynx
				</a>
			</footer>
		</div>
	);
}
