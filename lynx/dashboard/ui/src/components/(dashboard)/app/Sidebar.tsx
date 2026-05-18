"use client";

import { Button } from "@/components/ui/button";
import {
	Building2,
	LayoutDashboard,
	LogOut,
	Monitor,
	Settings,
	ShieldCheck,
} from "lucide-react";
import { useTranslations } from "next-intl";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTransition } from "react";
import { logoutAction } from "@/actions/(dashboard)/app/logout";
import { NotificationBell } from "./NotificationBell";

type Props = { locale: string; companyName: string; logoUrl: string | null; isAdmin: boolean };

export function Sidebar({ locale, companyName, logoUrl, isAdmin }: Props) {
	const t = useTranslations("app.nav");
	const pathname = usePathname();
	const [, startTransition] = useTransition();

	const items = [
		{
			href: `/${locale}/app`,
			label: t("overview"),
			icon: LayoutDashboard,
		},
		{
			href: `/${locale}/app/agents`,
			label: t("agents"),
			icon: Monitor,
		},
		{
			href: `/${locale}/app/organizations`,
			label: t("organizations"),
			icon: Building2,
		},
		{
			href: `/${locale}/app/settings`,
			label: t("settings"),
			icon: Settings,
		},
		...(isAdmin
			? [{ href: `/${locale}/app/admin`, label: t("admin"), icon: ShieldCheck }]
			: []),
	];

	return (
		<aside className="flex h-full w-60 shrink-0 flex-col border-r bg-background">
			<div className="flex h-14 items-center border-b px-5 gap-2">
				<div className="flex items-center gap-2 flex-1 min-w-0">
					{logoUrl ? (
						// eslint-disable-next-line @next/next/no-img-element
						<img src={logoUrl} alt={companyName} className="h-7 w-auto object-contain" />
					) : (
						<span
							className="text-base font-semibold tracking-tight truncate"
							style={{ color: "var(--brand-secondary)" }}
						>
							{companyName}
						</span>
					)}
				</div>
				<NotificationBell />
			</div>

			<nav className="flex flex-col gap-0.5 p-2 flex-1">
				{items.map(({ href, label, icon: Icon }) => {
					const active =
						href === `/${locale}/app`
							? pathname === href
							: pathname.startsWith(href);
					return (
						<Link
							key={href}
							href={href}
							className={`flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors ${
								active
									? "bg-accent text-accent-foreground font-medium"
									: "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
							}`}
						>
							<Icon className="size-4 shrink-0" />
							{label}
						</Link>
					);
				})}
			</nav>

			<div className="border-t p-2">
				<Button
					variant="ghost"
					size="sm"
					className="w-full justify-start gap-2.5 text-muted-foreground"
					onClick={() =>
						startTransition(() => logoutAction(locale))
					}
				>
					<LogOut className="size-4" />
					Sign out
				</Button>
			</div>

			<div className="px-4 pb-3 pt-1 text-center">
				<p className="text-[10px] text-muted-foreground/60 leading-tight">
					Made with love by{" "}
					<a
						href="https://github.com/Jaro-c"
						target="_blank"
						rel="noopener noreferrer"
						className="hover:text-muted-foreground transition-colors"
					>
						Jaroc
					</a>
					{" · "}
					<a
						href="https://github.com/Jaro-c/Lynx"
						target="_blank"
						rel="noopener noreferrer"
						className="hover:text-muted-foreground transition-colors"
					>
						lynx
					</a>
				</p>
			</div>
		</aside>
	);
}
