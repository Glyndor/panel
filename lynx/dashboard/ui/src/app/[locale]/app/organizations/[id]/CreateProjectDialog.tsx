"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { createProject } from "./projectActions";

interface Agent {
	id: string;
	name: string;
	status: string;
}

interface Props {
	orgId: string;
	agents: Agent[];
	labels: {
		trigger: string;
		title: string;
		name: string;
		slug: string;
		agent: string;
		noAgents: string;
		create: string;
		success: string;
		slugConflict: string;
		error: string;
	};
}

function deriveSlug(name: string): string {
	return name
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
}

export function CreateProjectDialog({ orgId, agents, labels }: Props) {
	const router = useRouter();
	const [open, setOpen] = useState(false);
	const [name, setName] = useState("");
	const [slug, setSlug] = useState("");
	const [agentId, setAgentId] = useState(agents[0]?.id ?? "");
	const [slugTouched, setSlugTouched] = useState(false);
	const [isPending, startTransition] = useTransition();

	function handleNameChange(v: string) {
		setName(v);
		if (!slugTouched) setSlug(deriveSlug(v));
	}

	function handleSubmit(e: React.FormEvent) {
		e.preventDefault();
		if (!name.trim() || !slug.trim() || !agentId) return;
		startTransition(async () => {
			const result = await createProject(orgId, name.trim(), slug.trim(), agentId);
			if (result.ok) {
				toast.success(labels.success);
				setOpen(false);
				setName("");
				setSlug("");
				setSlugTouched(false);
				setAgentId(agents[0]?.id ?? "");
				router.refresh();
			} else if (result.error === "slug_conflict") {
				toast.error(labels.slugConflict);
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	if (agents.length === 0) {
		return (
			<p className="text-xs text-muted-foreground">{labels.noAgents}</p>
		);
	}

	return (
		<Dialog open={open} onOpenChange={setOpen}>
			<DialogTrigger asChild>
				<Button size="sm">{labels.trigger}</Button>
			</DialogTrigger>
			<DialogContent className="max-w-sm">
				<DialogHeader>
					<DialogTitle>{labels.title}</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit} className="flex flex-col gap-4 mt-2">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="proj-name">{labels.name}</Label>
						<Input
							id="proj-name"
							value={name}
							onChange={(e) => handleNameChange(e.target.value)}
							required
						/>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="proj-slug">{labels.slug}</Label>
						<Input
							id="proj-slug"
							value={slug}
							onChange={(e) => {
								setSlug(e.target.value);
								setSlugTouched(true);
							}}
							pattern="[a-z0-9-]+"
							required
						/>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="proj-agent">{labels.agent}</Label>
						<select
							id="proj-agent"
							value={agentId}
							onChange={(e) => setAgentId(e.target.value)}
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm"
						>
							{agents.map((a) => (
								<option key={a.id} value={a.id}>
									{a.name} ({a.status})
								</option>
							))}
						</select>
					</div>
					<Button type="submit" disabled={isPending}>
						{isPending ? "…" : labels.create}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}
