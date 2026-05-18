"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { useRouter } from "next/navigation";
import { useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { createProject } from "@/actions/(dashboard)/app/organizations/[id]/projects";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type CreateProjectInput, createProjectSchema } from "@/schemas/(dashboard)/app/organizations/[id]";

interface Agent {
	id: string;
	name: string;
	status: string;
}

interface Props {
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
	orgId: string;
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
	const slugTouched = useRef(false);

	const {
		register,
		handleSubmit,
		setValue,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<CreateProjectInput>({
		defaultValues: { agent_id: agents[0]?.id ?? "" },
		resolver: zodResolver(createProjectSchema),
	});

	const onSubmit = (data: CreateProjectInput) => {
		toast.promise(
			createProject(orgId, data.name, data.slug, data.agent_id).then((r) => {
				if (!r.ok) throw new Error(r.error ?? "error");
				setOpen(false);
				slugTouched.current = false;
				reset({ agent_id: agents[0]?.id ?? "" });
				router.refresh();
			}),
			{
				error: (e: Error) => (e.message === "slug_conflict" ? labels.slugConflict : labels.error),
				loading: labels.create,
				success: labels.success,
			},
		);
	};

	if (agents.length === 0) {
		return <p className="text-xs text-muted-foreground">{labels.noAgents}</p>;
	}

	return (
		<Dialog
			onOpenChange={(v) => {
				setOpen(v);
				if (!v) {
					slugTouched.current = false;
					reset({ agent_id: agents[0]?.id ?? "" });
				}
			}}
			open={open}
		>
			<DialogTrigger asChild>
				<Button size="sm">{labels.trigger}</Button>
			</DialogTrigger>
			<DialogContent className="max-w-sm">
				<DialogHeader>
					<DialogTitle>{labels.title}</DialogTitle>
				</DialogHeader>
				<form className="flex flex-col gap-4 mt-2" onSubmit={handleSubmit(onSubmit)}>
					<Field>
						<FieldLabel htmlFor="proj-name">{labels.name}</FieldLabel>
						<Input
							id="proj-name"
							{...register("name", {
								onChange: (e) => {
									if (!slugTouched.current) setValue("slug", deriveSlug(e.target.value));
								},
							})}
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.name]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="proj-slug">{labels.slug}</FieldLabel>
						<Input
							id="proj-slug"
							{...register("slug", {
								onChange: () => {
									slugTouched.current = true;
								},
							})}
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.slug]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="proj-agent">{labels.agent}</FieldLabel>
						<select
							id="proj-agent"
							{...register("agent_id")}
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm"
							disabled={isSubmitting}
						>
							{agents.map((a) => (
								<option key={a.id} value={a.id}>
									{a.name} ({a.status})
								</option>
							))}
						</select>
						<FieldError errors={[errors.agent_id]} />
					</Field>
					<Button disabled={isSubmitting} type="submit">
						{isSubmitting ? "…" : labels.create}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}
