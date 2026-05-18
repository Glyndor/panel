import { describe, expect, it } from "vitest";
import {
	brandingSchema,
	certUploadSchema,
	changePasswordSchema,
	domainSetupSchema,
	migrationStartSchema,
} from "@/schemas/(dashboard)/app/settings";

// ---------------------------------------------------------------------------
// changePasswordSchema
// ---------------------------------------------------------------------------

describe("changePasswordSchema", () => {
	const valid = { current_password: "old", new_password: "ValidP@ss12!" };

	it("accepts valid change password data", () => {
		expect(changePasswordSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects empty current_password", () => {
		expect(changePasswordSchema.safeParse({ ...valid, current_password: "" }).success).toBe(false);
	});

	it("rejects new_password shorter than 12 chars", () => {
		expect(changePasswordSchema.safeParse({ ...valid, new_password: "Short1!" }).success).toBe(false);
	});

	it("rejects new_password without special char", () => {
		expect(changePasswordSchema.safeParse({ ...valid, new_password: "ValidPass12ab" }).success).toBe(false);
	});

	it("rejects new_password without digit", () => {
		expect(changePasswordSchema.safeParse({ ...valid, new_password: "ValidP@ssword!" }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// brandingSchema
// ---------------------------------------------------------------------------

describe("brandingSchema", () => {
	it("accepts empty branding object", () => {
		expect(brandingSchema.safeParse({}).success).toBe(true);
	});

	it("accepts valid hex color", () => {
		expect(brandingSchema.safeParse({ primary_color: "#FF5733" }).success).toBe(true);
	});

	it("rejects invalid hex color", () => {
		expect(brandingSchema.safeParse({ primary_color: "red" }).success).toBe(false);
	});

	it("accepts empty string for color (clears it)", () => {
		expect(brandingSchema.safeParse({ primary_color: "" }).success).toBe(true);
	});

	it("rejects company_name longer than 80 chars", () => {
		expect(brandingSchema.safeParse({ company_name: "a".repeat(81) }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// domainSetupSchema
// ---------------------------------------------------------------------------

describe("domainSetupSchema", () => {
	const valid = { domain: "example.com", email: "admin@example.com" };

	it("accepts valid domain setup", () => {
		expect(domainSetupSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects empty domain", () => {
		expect(domainSetupSchema.safeParse({ ...valid, domain: "" }).success).toBe(false);
	});

	it("rejects invalid email", () => {
		expect(domainSetupSchema.safeParse({ ...valid, email: "notanemail" }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// certUploadSchema
// ---------------------------------------------------------------------------

describe("certUploadSchema", () => {
	const validPem = "-----BEGIN CERTIFICATE-----\nMIIBxxx\n-----END CERTIFICATE-----";
	const validKey = "-----BEGIN PRIVATE KEY-----\nMIIByyy\n-----END PRIVATE KEY-----";

	it("accepts cloudflare cert with valid PEM", () => {
		expect(certUploadSchema.safeParse({ cert_pem: validPem, cert_type: "cloudflare" }).success).toBe(true);
	});

	it("accepts custom cert with cert + key", () => {
		expect(
			certUploadSchema.safeParse({
				cert_pem: validPem,
				cert_type: "custom",
				key_pem: validKey,
			}).success,
		).toBe(true);
	});

	it("rejects cert without PEM header", () => {
		expect(certUploadSchema.safeParse({ cert_pem: "not a cert", cert_type: "cloudflare" }).success).toBe(false);
	});

	it("rejects empty cert_pem", () => {
		expect(certUploadSchema.safeParse({ cert_pem: "", cert_type: "cloudflare" }).success).toBe(false);
	});

	it("rejects cert exceeding 64 KB", () => {
		const big = "-----BEGIN CERTIFICATE-----\n" + "A".repeat(65 * 1024);
		expect(certUploadSchema.safeParse({ cert_pem: big, cert_type: "cloudflare" }).success).toBe(false);
	});

	it("rejects unknown cert_type", () => {
		expect(certUploadSchema.safeParse({ cert_pem: validPem, cert_type: "letsencrypt" }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// migrationStartSchema
// ---------------------------------------------------------------------------

describe("migrationStartSchema", () => {
	const valid = { migration_token: "tok-abc123", target_url: "https://10.0.0.2:19443" };

	it("accepts valid migration start data", () => {
		expect(migrationStartSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects non-URL target_url", () => {
		expect(migrationStartSchema.safeParse({ ...valid, target_url: "not-a-url" }).success).toBe(false);
	});

	it("rejects empty target_url", () => {
		expect(migrationStartSchema.safeParse({ ...valid, target_url: "" }).success).toBe(false);
	});

	it("rejects empty migration_token", () => {
		expect(migrationStartSchema.safeParse({ ...valid, migration_token: "" }).success).toBe(false);
	});

	it("accepts https URL with port", () => {
		expect(
			migrationStartSchema.safeParse({
				...valid,
				target_url: "https://dashboard.example.com:19443",
			}).success,
		).toBe(true);
	});
});
