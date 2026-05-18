"use client";

import { useAgentMetrics } from "@/lib/useAgentMetrics";
import { Cpu, HardDrive, MemoryStick, Wifi, WifiOff } from "lucide-react";

interface Props {
	agentId: string;
	labels: {
		metrics: string;
		cpu: string;
		memory: string;
		disk: string;
		connecting: string;
		agentOffline: string;
		offline: string;
	};
}

function pct(used: number, total: number) {
	if (total === 0) return 0;
	return Math.round((used / total) * 100);
}

function Bar({ value }: { value: number }) {
	const color =
		value >= 90
			? "bg-destructive"
			: value >= 70
				? "bg-yellow-500"
				: "bg-primary";
	return (
		<div className="h-1.5 w-full rounded-full bg-muted overflow-hidden">
			<div
				className={`h-full rounded-full transition-all duration-500 ${color}`}
				style={{ width: `${value}%` }}
			/>
		</div>
	);
}

export function MetricsPanel({ agentId, labels }: Props) {
	const { metrics, status } = useAgentMetrics(agentId);

	if (status === "connecting") {
		return (
			<p className="text-xs text-muted-foreground animate-pulse select-none">
				{labels.connecting}
			</p>
		);
	}

	if (status === "agent_offline" || status === "offline") {
		return (
			<div className="flex items-center gap-1.5 text-xs text-muted-foreground select-none">
				<WifiOff className="size-3.5" />
				{status === "agent_offline" ? labels.agentOffline : labels.offline}
			</div>
		);
	}

	if (!metrics) return null;

	const memPct = pct(metrics.mem_used_mb, metrics.mem_total_mb);
	const diskPct = pct(metrics.disk_used_gb, metrics.disk_total_gb);

	return (
		<div className="flex flex-col gap-3">
			<div className="flex items-center gap-1.5 text-xs text-muted-foreground select-none">
				<Wifi className="size-3.5 text-primary" />
				{labels.metrics}
			</div>

			<div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
				{/* CPU */}
				<div className="flex flex-col gap-1.5">
					<div className="flex items-center justify-between text-xs">
						<span className="flex items-center gap-1 text-muted-foreground select-none">
							<Cpu className="size-3" />
							{labels.cpu}
						</span>
						<span className="font-mono font-medium tabular-nums">
							{metrics.cpu_percent.toFixed(1)}%
						</span>
					</div>
					<Bar value={Math.round(metrics.cpu_percent)} />
				</div>

				{/* Memory */}
				<div className="flex flex-col gap-1.5">
					<div className="flex items-center justify-between text-xs">
						<span className="flex items-center gap-1 text-muted-foreground select-none">
							<MemoryStick className="size-3" />
							{labels.memory}
						</span>
						<span className="font-mono font-medium tabular-nums">
							{memPct}%{" "}
							<span className="text-muted-foreground font-normal">
								{metrics.mem_used_mb >= 1024
									? `${(metrics.mem_used_mb / 1024).toFixed(1)}GB`
									: `${metrics.mem_used_mb}MB`}
								/
								{metrics.mem_total_mb >= 1024
									? `${(metrics.mem_total_mb / 1024).toFixed(1)}GB`
									: `${metrics.mem_total_mb}MB`}
							</span>
						</span>
					</div>
					<Bar value={memPct} />
				</div>

				{/* Disk */}
				<div className="flex flex-col gap-1.5">
					<div className="flex items-center justify-between text-xs">
						<span className="flex items-center gap-1 text-muted-foreground select-none">
							<HardDrive className="size-3" />
							{labels.disk}
						</span>
						<span className="font-mono font-medium tabular-nums">
							{diskPct}%{" "}
							<span className="text-muted-foreground font-normal">
								{metrics.disk_used_gb.toFixed(1)}GB/
								{metrics.disk_total_gb.toFixed(1)}GB
							</span>
						</span>
					</div>
					<Bar value={diskPct} />
				</div>
			</div>
		</div>
	);
}
