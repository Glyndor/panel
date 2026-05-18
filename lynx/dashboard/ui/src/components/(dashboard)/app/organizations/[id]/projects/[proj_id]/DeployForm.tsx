"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { deployContainer } from "@/actions/(dashboard)/app/organizations/[id]/projects/[proj_id]/containers";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
	type DeployContainerInput,
	deployContainerSchema,
} from "@/schemas/(dashboard)/app/organizations/[id]/projects/[proj_id]";

interface Props {
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
	orgId: string;
	projId: string;
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
			? data.ports
					.split(/[\n,]+/)
					.map((s) => s.trim())
					.filter(Boolean)
			: [];
		const parsedEnv = data.env
			? data.env
					.split(/[\n,]+/)
					.map((s) => s.trim())
					.filter(Boolean)
			: [];

		toast.promise(
			deployContainer(orgId, projId, {
				cpus: data.cpus ?? null,
				env: parsedEnv,
				image: data.image,
				memory_mb: data.memory_mb ?? null,
				name: data.name,
				ports: parsedPorts,
			}).then((r) => {
				if (!r.ok) throw new Error(r.error);
				reset();
				return r;
			}),
			{
				error: labels.error,
				loading: labels.deploy,
				success: labels.success,
			},
		);
	};

	return (
		<form className="flex flex-col gap-4" onSubmit={handleSubmit(onSubmit)}>
			<div className="grid grid-cols-2 gap-4">
				<Field>
					<FieldLabel htmlFor="c-name">{labels.name}</FieldLabel>
					<Input id="c-name" {...register("name")} disabled={isSubmitting} placeholder="web" />
					<FieldError errors={[errors.name]} />
				</Field>
				<Field>
					<FieldLabel htmlFor="c-image">{labels.image}</FieldLabel>
					<Input id="c-image" {...register("image")} disabled={isSubmitting} placeholder="nginx:alpine" />
					<FieldError errors={[errors.image]} />
				</Field>
			</div>

			<div className="grid grid-cols-2 gap-4">
				<Field>
					<FieldLabel htmlFor="c-ports">{labels.ports}</FieldLabel>
					<Input id="c-ports" {...register("ports")} disabled={isSubmitting} placeholder="80:80, 443:443" />
					<FieldError errors={[errors.ports]} />
				</Field>
				<Field>
					<FieldLabel htmlFor="c-env">{labels.env}</FieldLabel>
					<Input id="c-env" {...register("env")} disabled={isSubmitting} placeholder="KEY=value" />
					<FieldError errors={[errors.env]} />
				</Field>
			</div>

			<div className="flex gap-4">
				<Field className="flex-1">
					<FieldLabel htmlFor="c-cpus">{labels.cpus}</FieldLabel>
					<Input
						id="c-cpus"
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
					<FieldLabel htmlFor="c-mem">{labels.memoryMb}</FieldLabel>
					<Input
						id="c-mem"
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
					{isSubmitting ? "…" : labels.deploy}
				</Button>
			</div>
		</form>
	);
}
