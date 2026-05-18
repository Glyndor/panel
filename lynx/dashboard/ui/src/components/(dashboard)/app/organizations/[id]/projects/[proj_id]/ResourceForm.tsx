"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { resourceFormSchema, type ResourceFormInput } from "@/schemas/(dashboard)/app/organizations/[id]/projects/[proj_id]";
import { updateContainerResources } from "@/actions/(dashboard)/app/organizations/[id]/projects/[proj_id]";

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
	const {
		register,
		handleSubmit,
		formState: { errors, isSubmitting },
	} = useForm<ResourceFormInput>({
		resolver: zodResolver(resourceFormSchema),
	});

	const onSubmit = (data: ResourceFormInput) => {
		toast.promise(
			updateContainerResources(
				orgId,
				projId,
				data.container_name,
				data.cpus ?? null,
				data.memory_mb ?? null,
			).then((r) => {
				if (!r.ok) throw new Error(r.error);
				return r;
			}),
			{
				loading: labels.apply,
				success: labels.success,
				error: labels.error,
			},
		);
	};

	return (
		<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4">
			<Field>
				<FieldLabel htmlFor="container-name">{labels.containerName}</FieldLabel>
				<Input
					id="container-name"
					{...register("container_name")}
					placeholder="web"
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.container_name]} />
			</Field>

			<div className="flex gap-4">
				<Field className="flex-1">
					<FieldLabel htmlFor="cpus">{labels.cpus}</FieldLabel>
					<Input
						id="cpus"
						type="number"
						min="0.1"
						step="0.1"
						{...register("cpus")}
						placeholder="1.0"
						disabled={isSubmitting}
					/>
					<FieldError errors={[errors.cpus]} />
				</Field>
				<Field className="flex-1">
					<FieldLabel htmlFor="memory-mb">{labels.memoryMb}</FieldLabel>
					<Input
						id="memory-mb"
						type="number"
						min="64"
						step="64"
						{...register("memory_mb")}
						placeholder="512"
						disabled={isSubmitting}
					/>
					<FieldError errors={[errors.memory_mb]} />
				</Field>
			</div>

			<div>
				<Button type="submit" size="sm" disabled={isSubmitting}>
					{isSubmitting ? "…" : labels.apply}
				</Button>
			</div>
		</form>
	);
}
