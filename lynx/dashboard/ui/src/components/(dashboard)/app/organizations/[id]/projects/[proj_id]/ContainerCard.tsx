"use client";

import { useTransition } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { containerAction } from "@/actions/(dashboard)/app/organizations/[id]/projects/[proj_id]/containers";

interface Container {
	Names: string[];
	Image: string;
	Status: string;
	State: string;
}

interface Props {
	orgId: string;
	projId: string;
	container: Container;
	labels: {
		start: string;
		stop: string;
		restart: string;
		remove: string;
		success: string;
		error: string;
	};
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
					<Badge variant={stateVariant(container.State)} className="shrink-0">
						{container.State}
					</Badge>
				</div>
				<p className="text-xs text-muted-foreground truncate mt-0.5">
					{container.Image}
				</p>
			</div>
			<div className="flex gap-1 shrink-0">
				{!isRunning && (
					<Button
						size="sm"
						variant="outline"
						className="h-7 text-xs"
						disabled={isPending}
						onClick={() => handle("start")}
					>
						{labels.start}
					</Button>
				)}
				{isRunning && (
					<>
						<Button
							size="sm"
							variant="outline"
							className="h-7 text-xs"
							disabled={isPending}
							onClick={() => handle("restart")}
						>
							{labels.restart}
						</Button>
						<Button
							size="sm"
							variant="outline"
							className="h-7 text-xs"
							disabled={isPending}
							onClick={() => handle("stop")}
						>
							{labels.stop}
						</Button>
					</>
				)}
				<Button
					size="sm"
					variant="ghost"
					className="h-7 text-xs text-destructive hover:text-destructive hover:bg-destructive/10"
					disabled={isPending}
					onClick={() => handle("remove")}
				>
					{labels.remove}
				</Button>
			</div>
		</div>
	);
}
