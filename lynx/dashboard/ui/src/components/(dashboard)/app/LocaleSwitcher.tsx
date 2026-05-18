"use client";

import { usePathname, useRouter } from "next/navigation";
import { useTransition } from "react";
import Image from "next/image";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Button } from "@/components/ui/button";
import { updateLocaleAction } from "@/actions/(dashboard)/app/settings/preferences";

const LOCALES: { code: string; label: string; flag: string }[] = [
	{ code: "en", label: "English", flag: "/flags/en.svg" },
	{ code: "es", label: "Español", flag: "/flags/es.svg" },
];

type Props = { locale: string };

export function LocaleSwitcher({ locale }: Props) {
	const pathname = usePathname();
	const router = useRouter();
	const [, startTransition] = useTransition();

	const current = LOCALES.find((l) => l.code === locale) ?? LOCALES[0]!;

	function handleSelect(newLocale: string) {
		if (newLocale === locale) return;
		// Replace locale segment in pathname: /en/app/… → /es/app/…
		const newPath = pathname.replace(`/${locale}/`, `/${newLocale}/`);
		startTransition(async () => {
			await updateLocaleAction(newLocale);
			router.push(newPath);
		});
	}

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button
					className="h-7 w-7 cursor-pointer select-none p-0"
					size="icon"
					variant="ghost"
					aria-label="Switch language"
				>
					<Image
						src={current.flag}
						alt={current.label}
						width={18}
						height={18}
						className="rounded-full object-cover"
					/>
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				{LOCALES.map(({ code, label, flag }) => (
					<DropdownMenuItem
						className="gap-2 cursor-pointer"
						key={code}
						onClick={() => handleSelect(code)}
					>
						<Image src={flag} alt={label} width={16} height={16} className="rounded-full" />
						{label}
						{locale === code && (
							<span className="ml-auto text-xs text-muted-foreground">✓</span>
						)}
					</DropdownMenuItem>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
