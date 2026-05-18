"use client";

import { useCallback, useState } from "react";
import { Bell } from "lucide-react";
import { useAgentEvents, type AgentEvent, type AgentEventKind } from "@/lib/useAgentEvents";
import { useTranslations } from "next-intl";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";

const ALERT_EVENTS: AgentEventKind[] = [
	"heartbeat_lost",
	"lockdown",
	"nftables_divergence",
	"conflicting_software_detected",
];

interface Notification {
	id: string;
	agent_id: string;
	event: AgentEventKind;
	detail: string | null;
	at: Date;
}

export function NotificationBell() {
	const t = useTranslations("app.notifications");
	const [notifications, setNotifications] = useState<Notification[]>([]);
	const [open, setOpen] = useState(false);

	const handleEvent = useCallback((evt: AgentEvent) => {
		if (!ALERT_EVENTS.includes(evt.event)) return;
		setNotifications((prev) => [
			{
				id: crypto.randomUUID(),
				agent_id: evt.agent_id,
				event: evt.event,
				detail: evt.detail,
				at: new Date(),
			},
			...prev.slice(0, 49), // keep last 50
		]);
	}, []);

	useAgentEvents({ onEvent: handleEvent });

	const unread = notifications.length;

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<button
					type="button"
					className="relative p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer"
					aria-label={t("label")}
				>
					<Bell className="size-4" />
					{unread > 0 && (
						<span className="absolute -top-0.5 -right-0.5 flex size-4 items-center justify-center rounded-full bg-destructive text-[9px] font-bold text-destructive-foreground select-none">
							{unread > 9 ? "9+" : unread}
						</span>
					)}
				</button>
			</PopoverTrigger>
			<PopoverContent align="end" className="w-80 p-0">
				<div className="flex items-center justify-between px-3 py-2 border-b">
					<span className="text-sm font-medium">{t("title")}</span>
					{unread > 0 && (
						<button
							type="button"
							className="text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
							onClick={() => setNotifications([])}
						>
							{t("clearAll")}
						</button>
					)}
				</div>
				<div className="max-h-72 overflow-y-auto">
					{notifications.length === 0 ? (
						<p className="px-3 py-4 text-sm text-muted-foreground text-center">
							{t("empty")}
						</p>
					) : (
						notifications.map((n) => (
							<div
								key={n.id}
								className="flex flex-col gap-0.5 px-3 py-2.5 border-b last:border-0 hover:bg-muted/40"
							>
								<div className="flex items-start justify-between gap-2">
									<span className="text-xs font-medium truncate">
										{t(`event.${n.event}`)}
									</span>
									<span className="text-[10px] text-muted-foreground whitespace-nowrap">
										{n.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
									</span>
								</div>
								<p className="text-[11px] text-muted-foreground font-mono truncate">
									{n.agent_id.slice(0, 8)}…
									{n.detail && ` · ${n.detail}`}
								</p>
							</div>
						))
					)}
				</div>
			</PopoverContent>
		</Popover>
	);
}
