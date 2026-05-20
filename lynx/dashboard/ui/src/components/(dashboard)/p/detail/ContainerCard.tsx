"use client";

import { useTransition } from "react";
import { toast } from "sonner";
import { containerAction } from "@/actions/(dashboard)/o/[id]/projects/[proj_id]/containers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface Container {
	Image: string;
	Names: string[];
	State: string;
	Status: string;
}

interface Props {
	container: Container;
	labels: {
		start: string;
		stop: string;
		restart: string;
		remove: string;
		success: string;
		error: string;
	};
	orgId: string;
	projId: string;
}

function stateVariant(state: string): "default" | "secondary" | "destructive" {
	if (state === "running") return "default";
	if (state === "exited" || state === "stopped") return "secondary";
	return "destructive";
}

export function ContainerCard({ orgId, projId, container, labels }: Props) {
	const [isPending, startTransition] = useTransition();
	const name = container.Names[0]?.replace(/^\//, "") ?? "unknown";
	const isRunning = container.State === "running";

	function handle(action: "start" | "stop" | "restart" | "remove") {
		startTransition(async () => {
			const result = await containerAction(orgId, projId, name, action);
			if (result.ok) {
				toast.success(`${name}: ${labels.success}`);
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	return (
		<div className="flex items-center justify-between px-4 py-3 gap-3">
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<span className="text-sm font-medium font-mono truncate">{name}</span>
					<Badge className="shrink-0" variant={stateVariant(container.State)}>
						{container.State}
					</Badge>
				</div>
				<p className="text-xs text-muted-foreground truncate mt-0.5">{container.Image}</p>
			</div>
			<div className="flex gap-1 shrink-0">
				{!isRunning && (
					<Button
						className="h-7 text-xs"
						disabled={isPending}
						onClick={() => handle("start")}
						size="sm"
						variant="outline"
					>
						{labels.start}
					</Button>
				)}
				{isRunning && (
					<>
						<Button
							className="h-7 text-xs"
							disabled={isPending}
							onClick={() => handle("restart")}
							size="sm"
							variant="outline"
						>
							{labels.restart}
						</Button>
						<Button
							className="h-7 text-xs"
							disabled={isPending}
							onClick={() => handle("stop")}
							size="sm"
							variant="outline"
						>
							{labels.stop}
						</Button>
					</>
				)}
				<Button
					className="h-7 text-xs text-destructive hover:text-destructive hover:bg-destructive/10"
					disabled={isPending}
					onClick={() => handle("remove")}
					size="sm"
					variant="ghost"
				>
					{labels.remove}
				</Button>
			</div>
		</div>
	);
}
