import { z } from "zod";

export const inviteMemberSchema = z.object({
	role: z.enum(["viewer", "member", "admin"]),
	username: z.string().min(1),
});

export const createProjectSchema = z.object({
	agent_id: z.string().min(1),
	name: z.string().min(1).max(100),
	slug: z
		.string()
		.min(1)
		.max(100)
		.regex(/^[a-z0-9-]+$/),
});

export type InviteMemberInput = z.infer<typeof inviteMemberSchema>;
export type CreateProjectInput = z.infer<typeof createProjectSchema>;
