"use client";

import { Bell } from "lucide-react";
import { useTranslations } from "next-intl";
import { useCallback, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { type AgentEvent, type AgentEventKind, useAgentEvents } from "@/lib/useAgentEvents";

const ALERT_EVENTS: AgentEventKind[] = [
	"heartbeat_lost",
	"lockdown",
	"nftables_divergence",
	"conflicting_software_detected",
];

interface Notification {
	agent_id: string;
	at: Date;
	detail: string | null;
	event: AgentEventKind;
	id: string;
}

export function NotificationBell() {
	const t = useTranslations("app.notifications");
	const [notifications, setNotifications] = useState<Notification[]>([]);
	const [open, setOpen] = useState(false);

	const handleEvent = useCallback((evt: AgentEvent) => {
		if (!ALERT_EVENTS.includes(evt.event)) return;
		setNotifications((prev) => [
			{
				agent_id: evt.agent_id,
				at: new Date(),
				detail: evt.detail,
				event: evt.event,
				id: crypto.randomUUID(),
			},
			...prev.slice(0, 49), // keep last 50
		]);
	}, []);

	useAgentEvents({ onEvent: handleEvent });

	const unread = notifications.length;

	return (
		<Popover onOpenChange={setOpen} open={open}>
			<PopoverTrigger asChild>
				<button
					aria-label={t("label")}
					className="relative p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer"
					type="button"
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
							className="text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
							onClick={() => setNotifications([])}
							type="button"
						>
							{t("clearAll")}
						</button>
					)}
				</div>
				<div className="max-h-72 overflow-y-auto">
					{notifications.length === 0 ? (
						<p className="px-3 py-4 text-sm text-muted-foreground text-center">{t("empty")}</p>
					) : (
						notifications.map((n) => (
							<div
								className="flex flex-col gap-0.5 px-3 py-2.5 border-b last:border-0 hover:bg-muted/40"
								key={n.id}
							>
								<div className="flex items-start justify-between gap-2">
									<span className="text-xs font-medium truncate">{t(`event.${n.event}`)}</span>
									<span className="text-[10px] text-muted-foreground whitespace-nowrap">
										{n.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
									</span>
								</div>
								<p className="text-[11px] text-muted-foreground font-mono truncate">
									{n.agent_id.slice(0, 8)}…{n.detail && ` · ${n.detail}`}
								</p>
							</div>
						))
					)}
				</div>
			</PopoverContent>
		</Popover>
	);
}
