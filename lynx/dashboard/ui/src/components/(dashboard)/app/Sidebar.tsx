"use client";

import { Building2, LayoutDashboard, LogOut, Monitor, Settings, ShieldCheck } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { useTransition } from "react";
import { logoutAction } from "@/actions/(dashboard)/app/logout";
import { Button } from "@/components/ui/button";
import { LocaleSwitcher } from "./LocaleSwitcher";
import { NotificationBell } from "./NotificationBell";
import { ThemeToggle } from "./ThemeToggle";

type Props = { locale: string; companyName: string; logoUrl: string | null; isAdmin: boolean };

export function Sidebar({ locale, companyName, logoUrl, isAdmin }: Props) {
	const t = useTranslations("app.nav");
	const pathname = usePathname();
	const [, startTransition] = useTransition();

	const items = [
		{
			href: `/${locale}/app`,
			icon: LayoutDashboard,
			label: t("overview"),
		},
		{
			href: `/${locale}/app/agents`,
			icon: Monitor,
			label: t("agents"),
		},
		{
			href: `/${locale}/app/organizations`,
			icon: Building2,
			label: t("organizations"),
		},
		{
			href: `/${locale}/app/settings`,
			icon: Settings,
			label: t("settings"),
		},
		...(isAdmin ? [{ href: `/${locale}/app/admin`, icon: ShieldCheck, label: t("admin") }] : []),
	];

	return (
		<aside className="flex h-full w-60 shrink-0 flex-col border-r bg-background">
			<div className="flex h-14 items-center border-b px-5 gap-2">
				<div className="flex items-center gap-2 flex-1 min-w-0">
					{logoUrl ? (
						// eslint-disable-next-line @next/next/no-img-element
						<img alt={companyName} className="h-7 w-auto object-contain" src={logoUrl} />
					) : (
						<span
							className="text-base font-semibold tracking-tight truncate"
							style={{ color: "var(--brand-secondary)" }}
						>
							{companyName}
						</span>
					)}
				</div>
				<LocaleSwitcher locale={locale} />
				<ThemeToggle />
				<NotificationBell />
			</div>

			<nav className="flex flex-col gap-0.5 p-2 flex-1">
				{items.map(({ href, label, icon: Icon }) => {
					const active = href === `/${locale}/app` ? pathname === href : pathname.startsWith(href);
					return (
						<Link
							className={`flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors ${
								active
									? "bg-accent text-accent-foreground font-medium"
									: "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
							}`}
							href={href}
							key={href}
						>
							<Icon className="size-4 shrink-0" />
							{label}
						</Link>
					);
				})}
			</nav>

			<div className="border-t p-2">
				<Button
					className="w-full justify-start gap-2.5 text-muted-foreground"
					onClick={() => startTransition(() => logoutAction(locale))}
					size="sm"
					variant="ghost"
				>
					<LogOut className="size-4" />
					{t("signOut")}
				</Button>
			</div>

			<div className="px-4 pb-3 pt-1 text-center">
				<p className="text-[10px] text-muted-foreground/60 leading-tight">
					Made with love by{" "}
					<a
						className="hover:text-muted-foreground transition-colors"
						href="https://github.com/Jaro-c"
						rel="noopener noreferrer"
						target="_blank"
					>
						Jaroc
					</a>
					{" · "}
					<a
						className="hover:text-muted-foreground transition-colors"
						href="https://github.com/Jaro-c/Lynx"
						rel="noopener noreferrer"
						target="_blank"
					>
						lynx
					</a>
				</p>
			</div>
		</aside>
	);
}
