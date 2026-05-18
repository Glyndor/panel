import { z } from "zod";

const RESERVED = ["admin", "root", "system", "lynx", "support", "api", "null", "undefined"];

export const registerSchema = z.object({
	username: z
		.string()
		.min(3)
		.max(32)
		.regex(/^[a-z0-9_-]+$/)
		.refine((v) => !/^[-_]|[-_]$/.test(v))
		.refine((v) => !RESERVED.includes(v)),
	email: z.string().email(),
	password: z
		.string()
		.min(12)
		.max(30)
		.regex(/[A-Z]/)
		.regex(/[a-z]/)
		.regex(/[0-9]/)
		.regex(/[^A-Za-z0-9]/),
});

export type RegisterInput = z.infer<typeof registerSchema>;
