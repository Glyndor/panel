import { z } from "zod";

export const createOrgSchema = z.object({
	name: z.string().min(1).max(100),
	slug: z.string().min(1).max(100).regex(/^[a-z0-9-]+$/),
});

export type CreateOrgInput = z.infer<typeof createOrgSchema>;
