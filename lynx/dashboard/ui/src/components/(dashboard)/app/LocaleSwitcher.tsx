"use client";

import Image from "next/image";
import { usePathname, useRouter } from "next/navigation";
import { useTransition } from "react";
import { updateLocaleAction } from "@/actions/(dashboard)/app/member/preferences";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

const LOCALES: { code: string; label: string; flag: string }[] = [
	{ code: "en", flag: "/flags/en.svg", label: "English" },
	{ code: "es", flag: "/flags/es.svg", label: "Español" },
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
					aria-label="Switch language"
					className="h-7 w-7 cursor-pointer select-none p-0"
					size="icon"
					variant="ghost"
				>
					<Image
						alt={current.label}
						className="rounded-full object-cover"
						height={18}
						src={current.flag}
						width={18}
					/>
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				{LOCALES.map(({ code, label, flag }) => (
					<DropdownMenuItem className="gap-2 cursor-pointer" key={code} onClick={() => handleSelect(code)}>
						<Image alt={label} className="rounded-full" height={16} src={flag} width={16} />
						{label}
						{locale === code && <span className="ml-auto text-xs text-muted-foreground">✓</span>}
					</DropdownMenuItem>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
