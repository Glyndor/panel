"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { acknowledgeAlertAction, type SecurityAlert } from "@/actions/(dashboard)/admin/alerts";
import { Button } from "@/components/ui/button";

type Props = {
	initial: SecurityAlert[];
	labels: {
		title: string;
		noAlerts: string;
		acknowledge: string;
		acknowledged: string;
		error: string;
	};
};

export function AlertsPanel({ initial, labels }: Props) {
	const [alerts, setAlerts] = useState(initial);
	const [pending, startTransition] = useTransition();

	function dismiss(id: string) {
		startTransition(async () => {
			const { success } = await acknowledgeAlertAction(id);
			if (success) {
				setAlerts((prev) => prev.filter((a) => a.id !== id));
				toast.success(labels.acknowledged);
			} else {
				toast.error(labels.error);
			}
		});
	}

	if (alerts.length === 0) {
		return (
			<div className="flex items-center justify-center rounded-lg border border-dashed min-h-20">
				<p className="text-sm text-muted-foreground">{labels.noAlerts}</p>
			</div>
		);
	}

	return (
		<div className="rounded-lg border divide-y">
			{alerts.map((alert) => (
				<div className="flex items-start gap-3 px-4 py-3 text-sm" key={alert.id}>
					<span className="shrink-0 rounded-full bg-destructive/10 text-destructive px-2 py-0.5 text-xs font-mono">
						{alert.kind}
					</span>
					<span className="text-muted-foreground flex-1 min-w-0 break-all">
						{alert.detail ?? alert.agent_id ?? "—"}
					</span>
					<span className="shrink-0 text-xs text-muted-foreground">
						{new Date(alert.created_at).toLocaleTimeString(undefined, {
							hour: "2-digit",
							minute: "2-digit",
						})}
					</span>
					<Button
						className="shrink-0 h-6 px-2 text-xs cursor-pointer select-none"
						disabled={pending}
						onClick={() => dismiss(alert.id)}
						size="sm"
						variant="ghost"
					>
						{labels.acknowledge}
					</Button>
				</div>
			))}
		</div>
	);
}
