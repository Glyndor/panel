import Image from "next/image";
import { getTranslations } from "next-intl/server";

export default async function AuthLayout({
	children,
	params,
}: {
	children: React.ReactNode;
	params: Promise<{ locale: string }>;
}) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "branding" });

	return (
		<div className="min-h-screen flex">
			<aside className="hidden lg:flex w-[420px] shrink-0 flex-col justify-between overflow-hidden border-r border-white/5 bg-[#030712] px-12 py-12">
				<div />
				<div className="flex flex-col items-center gap-5">
					<div className="relative size-[100px]">
						<Image src="/logo.webp" alt="Lynx" fill className="object-contain" priority />
					</div>
					<div className="text-center">
						<p className="text-2xl font-bold tracking-tight text-white">Lynx</p>
						<p className="mt-2 max-w-[200px] text-sm leading-relaxed text-slate-400">
							Distributed infrastructure orchestration
						</p>
					</div>
				</div>
				<footer className="text-center text-xs text-slate-600">
					{t("madeWith")}{" "}
					<a
						className="text-slate-500 transition-colors hover:text-slate-300"
						href="https://github.com/Jaro-c"
						rel="noopener noreferrer"
						target="_blank"
					>
						{t("author")}
					</a>
					{" · "}
					<a
						className="text-slate-500 transition-colors hover:text-slate-300"
						href="https://github.com/Glyndor/panel"
						rel="noopener noreferrer"
						target="_blank"
					>
						lynx
					</a>
				</footer>
			</aside>

			<main className="flex flex-1 flex-col items-center justify-center bg-background px-6 py-12">
				{children}
			</main>
		</div>
	);
}
