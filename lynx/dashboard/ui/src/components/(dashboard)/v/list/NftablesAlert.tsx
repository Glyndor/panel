"use client";

import { AlertTriangle } from "lucide-react";
import { useTransition } from "react";
import { toast } from "sonner";
import { resolveNftables } from "@/actions/(dashboard)/v";
import { Button } from "@/components/ui/button";

interface Props {
	agentId: string;
	detail: string | null;
	labels: {
		title: string;
		restore: string;
		accept: string;
		restoreSuccess: string;
		acceptSuccess: string;
		error: string;
	};
}

export function NftablesAlert({ agentId, detail, labels }: Props) {
	const [isPending, startTransition] = useTransition();

	function handle(action: "restore" | "accept") {
		startTransition(async () => {
			const result = await resolveNftables(agentId, action);
			if (result.ok) {
				toast.success(action === "restore" ? labels.restoreSuccess : labels.acceptSuccess);
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	return (
		<div className="mt-3 rounded-md border border-orange-300 bg-orange-50 dark:border-orange-800 dark:bg-orange-950/30 p-3 flex flex-col gap-2">
			<div className="flex items-center gap-2 text-orange-700 dark:text-orange-400">
				<AlertTriangle className="size-4 shrink-0" />
				<p className="text-xs font-medium">{labels.title}</p>
			</div>
			{detail && <p className="text-xs text-orange-600 dark:text-orange-500 font-mono">{detail}</p>}
			<div className="flex gap-2">
				<Button
					className="text-xs h-7 border-orange-300 dark:border-orange-700"
					disabled={isPending}
					onClick={() => handle("restore")}
					size="sm"
					variant="outline"
				>
					{labels.restore}
				</Button>
				<Button
					className="text-xs h-7 text-muted-foreground"
					disabled={isPending}
					onClick={() => handle("accept")}
					size="sm"
					variant="ghost"
				>
					{labels.accept}
				</Button>
			</div>
		</div>
	);
}
