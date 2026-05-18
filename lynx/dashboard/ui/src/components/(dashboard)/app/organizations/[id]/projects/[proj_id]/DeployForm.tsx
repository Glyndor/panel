"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { deployContainerSchema, type DeployContainerInput } from "@/schemas/(dashboard)/app/organizations/[id]/projects/[proj_id]";
import { deployContainer } from "@/actions/(dashboard)/app/organizations/[id]/projects/[proj_id]/containers";

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
	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<DeployContainerInput>({
		resolver: zodResolver(deployContainerSchema),
	});

	const onSubmit = (data: DeployContainerInput) => {
		const parsedPorts = data.ports
			? data.ports.split(/[\n,]+/).map((s) => s.trim()).filter(Boolean)
			: [];
		const parsedEnv = data.env
			? data.env.split(/[\n,]+/).map((s) => s.trim()).filter(Boolean)
			: [];

		toast.promise(
			deployContainer(orgId, projId, {
				name: data.name,
				image: data.image,
				ports: parsedPorts,
				env: parsedEnv,
				cpus: data.cpus ?? null,
				memory_mb: data.memory_mb ?? null,
			}).then((r) => {
				if (!r.ok) throw new Error(r.error);
				reset();
				return r;
			}),
			{
				loading: labels.deploy,
				success: labels.success,
				error: labels.error,
			},
		);
	};

	return (
		<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4">
			<div className="grid grid-cols-2 gap-4">
				<Field>
					<FieldLabel htmlFor="c-name">{labels.name}</FieldLabel>
					<Input id="c-name" {...register("name")} placeholder="web" disabled={isSubmitting} />
					<FieldError errors={[errors.name]} />
				</Field>
				<Field>
					<FieldLabel htmlFor="c-image">{labels.image}</FieldLabel>
					<Input id="c-image" {...register("image")} placeholder="nginx:alpine" disabled={isSubmitting} />
					<FieldError errors={[errors.image]} />
				</Field>
			</div>

			<div className="grid grid-cols-2 gap-4">
				<Field>
					<FieldLabel htmlFor="c-ports">{labels.ports}</FieldLabel>
					<Input id="c-ports" {...register("ports")} placeholder="80:80, 443:443" disabled={isSubmitting} />
					<FieldError errors={[errors.ports]} />
				</Field>
				<Field>
					<FieldLabel htmlFor="c-env">{labels.env}</FieldLabel>
					<Input id="c-env" {...register("env")} placeholder="KEY=value" disabled={isSubmitting} />
					<FieldError errors={[errors.env]} />
				</Field>
			</div>

			<div className="flex gap-4">
				<Field className="flex-1">
					<FieldLabel htmlFor="c-cpus">{labels.cpus}</FieldLabel>
					<Input
						id="c-cpus"
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
					<FieldLabel htmlFor="c-mem">{labels.memoryMb}</FieldLabel>
					<Input
						id="c-mem"
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
					{isSubmitting ? "…" : labels.deploy}
				</Button>
			</div>
		</form>
	);
}
