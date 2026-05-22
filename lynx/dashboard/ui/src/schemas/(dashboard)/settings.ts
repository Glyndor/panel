import { z } from "zod";

export const changePasswordSchema = z.object({
	current_password: z.string().min(1),
	new_password: z.string().min(12).max(30).regex(/[A-Z]/).regex(/[a-z]/).regex(/[0-9]/).regex(/[^A-Za-z0-9]/),
});

export const brandingSchema = z.object({
	accent_color: z.string().regex(/^#[0-9a-fA-F]{6}$/).optional().or(z.literal("")),
	company_name: z.string().max(80).optional(),
	logo_url: z.string().optional(),
	primary_color: z.string().regex(/^#[0-9a-fA-F]{6}$/).optional().or(z.literal("")),
	secondary_color: z.string().regex(/^#[0-9a-fA-F]{6}$/).optional().or(z.literal("")),
});

export const domainSetupSchema = z.object({ domain: z.string().min(1), email: z.string().email() });
export const migrationStartSchema = z.object({ migration_token: z.string().min(1), target_url: z.string().url() });

const MAX_CERT_BYTES = 64 * 1024;
export const certUploadSchema = z.object({
	cert_pem: z.string().min(1).refine((s) => s.length <= MAX_CERT_BYTES, "Certificate exceeds 64 KB limit").refine((s) => s.includes("-----BEGIN"), "Must be PEM format"),
	cert_type: z.enum(["cloudflare", "custom"]),
	key_pem: z.string().refine((s) => s.length <= MAX_CERT_BYTES, "Key exceeds 64 KB limit").refine((s) => s === "" || s.includes("-----BEGIN"), "Must be PEM format").optional(),
});

export type CertUploadInput = z.infer<typeof certUploadSchema>;
export type ChangePasswordInput = z.infer<typeof changePasswordSchema>;
export type BrandingInput = z.infer<typeof brandingSchema>;
export type DomainSetupInput = z.infer<typeof domainSetupSchema>;
export type MigrationStartInput = z.infer<typeof migrationStartSchema>;
