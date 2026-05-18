import { describe, expect, it } from "vitest";
import { loginSchema } from "@/schemas/(auth)/login";
import { registerSchema } from "@/schemas/(auth)/register";

// ---------------------------------------------------------------------------
// Login schema
// ---------------------------------------------------------------------------

describe("loginSchema", () => {
	it("accepts valid credentials", () => {
		expect(loginSchema.safeParse({ username: "alice", password: "secret" }).success).toBe(true);
	});

	it("rejects empty username", () => {
		expect(loginSchema.safeParse({ username: "", password: "secret" }).success).toBe(false);
	});

	it("rejects empty password", () => {
		expect(loginSchema.safeParse({ username: "alice", password: "" }).success).toBe(false);
	});

	it("rejects missing fields", () => {
		expect(loginSchema.safeParse({}).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// Register schema
// ---------------------------------------------------------------------------

describe("registerSchema", () => {
	const valid = {
		username: "alice42",
		email: "alice@example.com",
		password: "ValidP@ss12!",
	};

	it("accepts valid registration data", () => {
		expect(registerSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects username shorter than 3 chars", () => {
		expect(registerSchema.safeParse({ ...valid, username: "ab" }).success).toBe(false);
	});

	it("rejects username longer than 32 chars", () => {
		expect(registerSchema.safeParse({ ...valid, username: "a".repeat(33) }).success).toBe(false);
	});

	it("rejects username with uppercase", () => {
		expect(registerSchema.safeParse({ ...valid, username: "Alice" }).success).toBe(false);
	});

	it("rejects username with spaces", () => {
		expect(registerSchema.safeParse({ ...valid, username: "al ice" }).success).toBe(false);
	});

	it("rejects username starting with dash", () => {
		expect(registerSchema.safeParse({ ...valid, username: "-alice" }).success).toBe(false);
	});

	it("rejects username ending with underscore", () => {
		expect(registerSchema.safeParse({ ...valid, username: "alice_" }).success).toBe(false);
	});

	it("rejects reserved username 'admin'", () => {
		expect(registerSchema.safeParse({ ...valid, username: "admin" }).success).toBe(false);
	});

	it("rejects reserved username 'root'", () => {
		expect(registerSchema.safeParse({ ...valid, username: "root" }).success).toBe(false);
	});

	it("rejects reserved username 'null'", () => {
		expect(registerSchema.safeParse({ ...valid, username: "null" }).success).toBe(false);
	});

	it("rejects invalid email", () => {
		expect(registerSchema.safeParse({ ...valid, email: "not-an-email" }).success).toBe(false);
	});

	it("rejects password shorter than 12 chars", () => {
		expect(registerSchema.safeParse({ ...valid, password: "Short1!" }).success).toBe(false);
	});

	it("rejects password longer than 30 chars", () => {
		expect(registerSchema.safeParse({ ...valid, password: "ValidP@ss12!" + "x".repeat(20) }).success).toBe(false);
	});

	it("rejects password without uppercase", () => {
		expect(registerSchema.safeParse({ ...valid, password: "validp@ss12!" }).success).toBe(false);
	});

	it("rejects password without lowercase", () => {
		expect(registerSchema.safeParse({ ...valid, password: "VALIDP@SS12!" }).success).toBe(false);
	});

	it("rejects password without digit", () => {
		expect(registerSchema.safeParse({ ...valid, password: "ValidP@ssword!" }).success).toBe(false);
	});

	it("rejects password without special char", () => {
		expect(registerSchema.safeParse({ ...valid, password: "ValidPass12ab" }).success).toBe(false);
	});

	it("accepts password exactly 12 chars with all requirements", () => {
		expect(registerSchema.safeParse({ ...valid, password: "ValidP@ss12!" }).success).toBe(true);
	});

	it("accepts password exactly 30 chars", () => {
		const p = "ValidP@ss12!" + "a".repeat(18); // 12 + 18 = 30
		expect(registerSchema.safeParse({ ...valid, password: p }).success).toBe(true);
	});
});
