"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { updateContainerResources } from "@/actions/(dashboard)/o/[id]/projects/[proj_id]";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
	type ResourceFormInput,
	resourceFormSchema,
} from "@/schemas/(dashboard)/p";

interface Props {
	labels: {
		containerName: string;
		cpus: string;
		memoryMb: string;
		apply: string;
		success: string;
		error: string;
	};
	orgId: string;
	projId: string;
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
				error: labels.error,
				loading: labels.apply,
				success: labels.success,
			},
		);
	};

	return (
		<form className="flex flex-col gap-4" onSubmit={handleSubmit(onSubmit)}>
			<Field>
				<FieldLabel htmlFor="container-name">{labels.containerName}</FieldLabel>
				<Input id="container-name" {...register("container_name")} disabled={isSubmitting} placeholder="web" />
				<FieldError errors={[errors.container_name]} />
			</Field>

			<div className="flex gap-4">
				<Field className="flex-1">
					<FieldLabel htmlFor="cpus">{labels.cpus}</FieldLabel>
					<Input
						id="cpus"
						min="0.1"
						step="0.1"
						type="number"
						{...register("cpus")}
						disabled={isSubmitting}
						placeholder="1.0"
					/>
					<FieldError errors={[errors.cpus]} />
				</Field>
				<Field className="flex-1">
					<FieldLabel htmlFor="memory-mb">{labels.memoryMb}</FieldLabel>
					<Input
						id="memory-mb"
						min="64"
						step="64"
						type="number"
						{...register("memory_mb")}
						disabled={isSubmitting}
						placeholder="512"
					/>
					<FieldError errors={[errors.memory_mb]} />
				</Field>
			</div>

			<div>
				<Button disabled={isSubmitting} size="sm" type="submit">
					{isSubmitting ? "…" : labels.apply}
				</Button>
			</div>
		</form>
	);
}
