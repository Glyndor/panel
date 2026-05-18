import { z } from "zod";

const RESERVED = ["admin", "root", "system", "lynx", "support", "api", "null", "undefined"];

export const registerSchema = z.object({
	username: z
		.string()
		.min(3, "Username must be at least 3 characters")
		.max(32, "Username cannot exceed 32 characters")
		.regex(/^[a-z0-9_-]+$/, "Only lowercase letters, numbers, - and _ allowed")
		.refine((v) => !/^[-_]|[-_]$/.test(v), "Cannot start or end with - or _")
		.refine((v) => !RESERVED.includes(v), "This username is reserved"),
	email: z.string().email("Enter a valid email address"),
	password: z
		.string()
		.min(12, "Password must be at least 12 characters")
		.max(30, "Password cannot exceed 30 characters")
		.regex(/[A-Z]/, "Must contain at least one uppercase letter")
		.regex(/[a-z]/, "Must contain at least one lowercase letter")
		.regex(/[0-9]/, "Must contain at least one number")
		.regex(/[^A-Za-z0-9]/, "Must contain at least one special character"),
});

export type RegisterInput = z.infer<typeof registerSchema>;
