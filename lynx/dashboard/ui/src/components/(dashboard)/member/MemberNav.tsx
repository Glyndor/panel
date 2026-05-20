"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

interface NavItem {
	href: string;
	label: string;
}

export function MemberNav({ items }: { items: NavItem[] }) {
	const pathname = usePathname();
	return (
		<nav className="flex flex-col gap-0.5">
			{items.map(({ href, label }) => {
				const active = pathname.startsWith(href);
				return (
					<Link
						className={`select-none cursor-pointer rounded-md px-3 py-2 text-sm transition-colors ${
							active
								? "bg-accent text-accent-foreground font-medium"
								: "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
						}`}
						href={href}
						key={href}
					>
						{label}
					</Link>
				);
			})}
		</nav>
	);
}
