"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { updateContainerResources } from "./actions";

interface Props {
	orgId: string;
	projId: string;
	labels: {
		containerName: string;
		cpus: string;
		memoryMb: string;
		apply: string;
		success: string;
		error: string;
	};
}

export function ResourceForm({ orgId, projId, labels }: Props) {
	const [containerName, setContainerName] = useState("");
	const [cpus, setCpus] = useState("");
	const [memoryMb, setMemoryMb] = useState("");
	const [isPending, startTransition] = useTransition();

	function handleSubmit(e: React.FormEvent) {
		e.preventDefault();
		if (!containerName.trim()) return;
		const cpuVal = cpus ? parseFloat(cpus) : null;
		const memVal = memoryMb ? parseInt(memoryMb, 10) : null;
		if (cpuVal !== null && (isNaN(cpuVal) || cpuVal <= 0)) return;
		if (memVal !== null && (isNaN(memVal) || memVal <= 0)) return;

		startTransition(async () => {
			const result = await updateContainerResources(
				orgId,
				projId,
				containerName.trim(),
				cpuVal,
				memVal,
			);
			if (result.ok) {
				toast.success(labels.success);
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	return (
		<form onSubmit={handleSubmit} className="flex flex-col gap-4">
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="container-name">{labels.containerName}</Label>
				<Input
					id="container-name"
					value={containerName}
					onChange={(e) => setContainerName(e.target.value)}
					placeholder="web"
					required
				/>
			</div>

			<div className="flex gap-4">
				<div className="flex flex-col gap-1.5 flex-1">
					<Label htmlFor="cpus">{labels.cpus}</Label>
					<Input
						id="cpus"
						type="number"
						min="0.1"
						step="0.1"
						value={cpus}
						onChange={(e) => setCpus(e.target.value)}
						placeholder="1.0"
					/>
				</div>
				<div className="flex flex-col gap-1.5 flex-1">
					<Label htmlFor="memory-mb">{labels.memoryMb}</Label>
					<Input
						id="memory-mb"
						type="number"
						min="64"
						step="64"
						value={memoryMb}
						onChange={(e) => setMemoryMb(e.target.value)}
						placeholder="512"
					/>
				</div>
			</div>

			<div>
				<Button type="submit" size="sm" disabled={isPending}>
					{isPending ? "…" : labels.apply}
				</Button>
			</div>
		</form>
	);
}
