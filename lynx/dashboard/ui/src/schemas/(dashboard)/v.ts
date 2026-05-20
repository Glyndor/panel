import { z } from "zod";

export const registerAgentSchema = z.object({ name: z.string().min(1).max(100) });
export type RegisterAgentInput = z.infer<typeof registerAgentSchema>;
