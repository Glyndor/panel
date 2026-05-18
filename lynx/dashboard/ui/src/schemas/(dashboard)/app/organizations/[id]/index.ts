import { z } from "zod";

export const inviteMemberSchema = z.object({
	username: z.string().min(1),
	role: z.enum(["viewer", "member", "admin"]),
});

export const createProjectSchema = z.object({
	name: z.string().min(1).max(100),
	slug: z.string().min(1).max(100).regex(/^[a-z0-9-]+$/),
	agent_id: z.string().min(1),
});

export type InviteMemberInput = z.infer<typeof inviteMemberSchema>;
export type CreateProjectInput = z.infer<typeof createProjectSchema>;
