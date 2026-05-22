"use client";

import { Monitor, Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";
import { useTransition } from "react";
import { updateThemeAction } from "@/actions/(dashboard)/member/preferences";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

const THEMES = [
	{ icon: Sun, label: "Light", value: "light" },
	{ icon: Moon, label: "Dark", value: "dark" },
	{ icon: Monitor, label: "System", value: "system" },
] as const;

export function ThemeToggle() {
	const { theme, setTheme } = useTheme();
	const [, startTransition] = useTransition();

	function handleSelect(value: string) {
		setTheme(value);
		startTransition(() => updateThemeAction(value));
	}

	const current = THEMES.find((t) => t.value === theme) ?? THEMES[2];
	const Icon = current.icon;

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button
					aria-label="Toggle theme"
					className="h-7 w-7 cursor-pointer select-none text-muted-foreground"
					size="icon"
					variant="ghost"
				>
					<Icon className="size-4" />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				{THEMES.map(({ value, icon: ItemIcon, label }) => (
					<DropdownMenuItem className="gap-2 cursor-pointer" key={value} onClick={() => handleSelect(value)}>
						<ItemIcon className="size-3.5" />
						{label}
						{theme === value && <span className="ml-auto text-xs text-muted-foreground">✓</span>}
					</DropdownMenuItem>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
