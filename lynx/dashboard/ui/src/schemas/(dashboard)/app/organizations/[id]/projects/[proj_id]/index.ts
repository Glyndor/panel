import { z } from "zod";

export const deployContainerSchema = z.object({
	name: z.string().min(1),
	image: z.string().min(1),
	ports: z.string().optional(),
	env: z.string().optional(),
	cpus: z.coerce.number<number>().min(0.1).optional(),
	memory_mb: z.coerce.number<number>().int().min(64).optional(),
});

export const resourceFormSchema = z.object({
	container_name: z.string().min(1),
	cpus: z.coerce.number<number>().min(0.1).optional(),
	memory_mb: z.coerce.number<number>().int().min(64).optional(),
});

export const addTunnelSchema = z.object({
	target_agent_id: z.string().min(1),
	image: z.string().min(1),
	replica_count: z.coerce.number<number>().int().min(1).max(20),
});

export type DeployContainerInput = z.infer<typeof deployContainerSchema>;
export type ResourceFormInput = z.infer<typeof resourceFormSchema>;
export type AddTunnelInput = z.infer<typeof addTunnelSchema>;
