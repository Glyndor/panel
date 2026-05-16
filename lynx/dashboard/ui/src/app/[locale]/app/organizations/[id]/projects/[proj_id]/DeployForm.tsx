"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { deployContainer } from "./containerActions";

interface Props {
	orgId: string;
	projId: string;
	labels: {
		name: string;
		image: string;
		ports: string;
		env: string;
		cpus: string;
		memoryMb: string;
		deploy: string;
		success: string;
		error: string;
	};
}

export function DeployForm({ orgId, projId, labels }: Props) {
	const [name, setName] = useState("");
	const [image, setImage] = useState("");
	const [ports, setPorts] = useState("");
	const [env, setEnv] = useState("");
	const [cpus, setCpus] = useState("");
	const [memoryMb, setMemoryMb] = useState("");
	const [isPending, startTransition] = useTransition();

	function handleSubmit(e: React.FormEvent) {
		e.preventDefault();
		if (!name.trim() || !image.trim()) return;

		const parsedPorts = ports
			.split(/[\n,]+/)
			.map((s) => s.trim())
			.filter(Boolean);
		const parsedEnv = env
			.split(/[\n,]+/)
			.map((s) => s.trim())
			.filter(Boolean);

		startTransition(async () => {
			const result = await deployContainer(orgId, projId, {
				name: name.trim(),
				image: image.trim(),
				ports: parsedPorts,
				env: parsedEnv,
				cpus: cpus ? parseFloat(cpus) : null,
				memory_mb: memoryMb ? parseInt(memoryMb, 10) : null,
			});
			if (result.ok) {
				toast.success(labels.success);
				setName("");
				setImage("");
				setPorts("");
				setEnv("");
				setCpus("");
				setMemoryMb("");
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	return (
		<form onSubmit={handleSubmit} className="flex flex-col gap-4">
			<div className="grid grid-cols-2 gap-4">
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="c-name">{labels.name}</Label>
					<Input
						id="c-name"
						value={name}
						onChange={(e) => setName(e.target.value)}
						placeholder="web"
						required
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="c-image">{labels.image}</Label>
					<Input
						id="c-image"
						value={image}
						onChange={(e) => setImage(e.target.value)}
						placeholder="nginx:alpine"
						required
					/>
				</div>
			</div>

			<div className="grid grid-cols-2 gap-4">
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="c-ports">{labels.ports}</Label>
					<Input
						id="c-ports"
						value={ports}
						onChange={(e) => setPorts(e.target.value)}
						placeholder="80:80, 443:443"
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="c-env">{labels.env}</Label>
					<Input
						id="c-env"
						value={env}
						onChange={(e) => setEnv(e.target.value)}
						placeholder="KEY=value"
					/>
				</div>
			</div>

			<div className="flex gap-4">
				<div className="flex flex-col gap-1.5 flex-1">
					<Label htmlFor="c-cpus">{labels.cpus}</Label>
					<Input
						id="c-cpus"
						type="number"
						min="0.1"
						step="0.1"
						value={cpus}
						onChange={(e) => setCpus(e.target.value)}
						placeholder="1.0"
					/>
				</div>
				<div className="flex flex-col gap-1.5 flex-1">
					<Label htmlFor="c-mem">{labels.memoryMb}</Label>
					<Input
						id="c-mem"
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
					{isPending ? "…" : labels.deploy}
				</Button>
			</div>
		</form>
	);
}
