import { describe, expect, it } from "vitest";
import { registerAgentSchema } from "@/schemas/(dashboard)/v";
import { createOrgSchema, createProjectSchema, inviteMemberSchema } from "@/schemas/(dashboard)/o";
import { addTunnelSchema, deployContainerSchema, resourceFormSchema } from "@/schemas/(dashboard)/p";

// ---------------------------------------------------------------------------
// registerAgentSchema
// ---------------------------------------------------------------------------

describe("registerAgentSchema", () => {
	it("accepts valid agent name", () => {
		expect(registerAgentSchema.safeParse({ name: "my-vps-01" }).success).toBe(true);
	});

	it("rejects empty name", () => {
		expect(registerAgentSchema.safeParse({ name: "" }).success).toBe(false);
	});

	it("rejects name over 100 chars", () => {
		expect(registerAgentSchema.safeParse({ name: "a".repeat(101) }).success).toBe(false);
	});

	it("accepts name at exactly 100 chars", () => {
		expect(registerAgentSchema.safeParse({ name: "a".repeat(100) }).success).toBe(true);
	});

	it("rejects missing name field", () => {
		expect(registerAgentSchema.safeParse({}).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// createOrgSchema
// ---------------------------------------------------------------------------

describe("createOrgSchema", () => {
	const valid = { name: "Acme Corp", slug: "acme-corp" };

	it("accepts valid org data", () => {
		expect(createOrgSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects empty name", () => {
		expect(createOrgSchema.safeParse({ ...valid, name: "" }).success).toBe(false);
	});

	it("rejects name over 100 chars", () => {
		expect(createOrgSchema.safeParse({ ...valid, name: "a".repeat(101) }).success).toBe(false);
	});

	it("rejects empty slug", () => {
		expect(createOrgSchema.safeParse({ ...valid, slug: "" }).success).toBe(false);
	});

	it("rejects slug with uppercase letters", () => {
		expect(createOrgSchema.safeParse({ ...valid, slug: "Acme-Corp" }).success).toBe(false);
	});

	it("rejects slug with spaces", () => {
		expect(createOrgSchema.safeParse({ ...valid, slug: "acme corp" }).success).toBe(false);
	});

	it("rejects slug with underscores", () => {
		expect(createOrgSchema.safeParse({ ...valid, slug: "acme_corp" }).success).toBe(false);
	});

	it("accepts slug with hyphens and digits", () => {
		expect(createOrgSchema.safeParse({ ...valid, slug: "acme-corp-123" }).success).toBe(true);
	});

	it("rejects slug with special chars", () => {
		expect(createOrgSchema.safeParse({ ...valid, slug: "acme@corp" }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// inviteMemberSchema
// ---------------------------------------------------------------------------

describe("inviteMemberSchema", () => {
	it("accepts valid viewer invite", () => {
		expect(inviteMemberSchema.safeParse({ role: "viewer", username: "alice" }).success).toBe(true);
	});

	it("accepts valid member invite", () => {
		expect(inviteMemberSchema.safeParse({ role: "member", username: "bob" }).success).toBe(true);
	});

	it("accepts valid admin invite", () => {
		expect(inviteMemberSchema.safeParse({ role: "admin", username: "carol" }).success).toBe(true);
	});

	it("rejects unknown role", () => {
		expect(inviteMemberSchema.safeParse({ role: "superadmin", username: "dave" }).success).toBe(false);
	});

	it("rejects empty username", () => {
		expect(inviteMemberSchema.safeParse({ role: "viewer", username: "" }).success).toBe(false);
	});

	it("rejects missing role", () => {
		expect(inviteMemberSchema.safeParse({ username: "alice" }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// createProjectSchema
// ---------------------------------------------------------------------------

describe("createProjectSchema", () => {
	const valid = { agent_id: "some-uuid", name: "Web App", slug: "web-app" };

	it("accepts valid project data", () => {
		expect(createProjectSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects empty name", () => {
		expect(createProjectSchema.safeParse({ ...valid, name: "" }).success).toBe(false);
	});

	it("rejects name over 100 chars", () => {
		expect(createProjectSchema.safeParse({ ...valid, name: "a".repeat(101) }).success).toBe(false);
	});

	it("rejects slug with dots", () => {
		expect(createProjectSchema.safeParse({ ...valid, slug: "web.app" }).success).toBe(false);
	});

	it("rejects slug with uppercase", () => {
		expect(createProjectSchema.safeParse({ ...valid, slug: "WebApp" }).success).toBe(false);
	});

	it("accepts slug with hyphens and digits only", () => {
		expect(createProjectSchema.safeParse({ ...valid, slug: "web-app-v2" }).success).toBe(true);
	});

	it("rejects empty agent_id", () => {
		expect(createProjectSchema.safeParse({ ...valid, agent_id: "" }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// deployContainerSchema
// ---------------------------------------------------------------------------

describe("deployContainerSchema", () => {
	const valid = { image: "docker.io/library/nginx:latest", name: "nginx" };

	it("accepts minimal valid container data (no optional fields)", () => {
		expect(deployContainerSchema.safeParse(valid).success).toBe(true);
	});

	it("accepts full container data with optional fields", () => {
		expect(
			deployContainerSchema.safeParse({
				...valid,
				cpus: 0.5,
				env: "FOO=bar",
				memory_mb: 256,
				ports: "80:80",
			}).success,
		).toBe(true);
	});

	it("rejects empty name", () => {
		expect(deployContainerSchema.safeParse({ ...valid, name: "" }).success).toBe(false);
	});

	it("rejects empty image", () => {
		expect(deployContainerSchema.safeParse({ ...valid, image: "" }).success).toBe(false);
	});

	it("rejects cpus below 0.1", () => {
		expect(deployContainerSchema.safeParse({ ...valid, cpus: 0.05 }).success).toBe(false);
	});

	it("accepts cpus at exactly 0.1", () => {
		expect(deployContainerSchema.safeParse({ ...valid, cpus: 0.1 }).success).toBe(true);
	});

	it("rejects memory_mb below 64", () => {
		expect(deployContainerSchema.safeParse({ ...valid, memory_mb: 32 }).success).toBe(false);
	});

	it("accepts memory_mb at exactly 64", () => {
		expect(deployContainerSchema.safeParse({ ...valid, memory_mb: 64 }).success).toBe(true);
	});

	it("rejects non-integer memory_mb", () => {
		expect(deployContainerSchema.safeParse({ ...valid, memory_mb: 128.5 }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// resourceFormSchema
// ---------------------------------------------------------------------------

describe("resourceFormSchema", () => {
	const valid = { container_name: "my-app", cpus: 1, memory_mb: 512 };

	it("accepts valid resource form data", () => {
		expect(resourceFormSchema.safeParse(valid).success).toBe(true);
	});

	it("accepts without optional cpus and memory_mb", () => {
		expect(resourceFormSchema.safeParse({ container_name: "my-app" }).success).toBe(true);
	});

	it("rejects empty container_name", () => {
		expect(resourceFormSchema.safeParse({ ...valid, container_name: "" }).success).toBe(false);
	});

	it("rejects cpus below 0.1", () => {
		expect(resourceFormSchema.safeParse({ ...valid, cpus: 0 }).success).toBe(false);
	});

	it("rejects memory_mb below 64", () => {
		expect(resourceFormSchema.safeParse({ ...valid, memory_mb: 63 }).success).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// addTunnelSchema
// ---------------------------------------------------------------------------

describe("addTunnelSchema", () => {
	const valid = {
		image: "docker.io/library/nginx:latest",
		replica_count: 2,
		target_agent_id: "agent-uuid-xyz",
	};

	it("accepts valid tunnel data", () => {
		expect(addTunnelSchema.safeParse(valid).success).toBe(true);
	});

	it("rejects empty target_agent_id", () => {
		expect(addTunnelSchema.safeParse({ ...valid, target_agent_id: "" }).success).toBe(false);
	});

	it("rejects empty image", () => {
		expect(addTunnelSchema.safeParse({ ...valid, image: "" }).success).toBe(false);
	});

	it("rejects replica_count below 1", () => {
		expect(addTunnelSchema.safeParse({ ...valid, replica_count: 0 }).success).toBe(false);
	});

	it("accepts replica_count at exactly 1", () => {
		expect(addTunnelSchema.safeParse({ ...valid, replica_count: 1 }).success).toBe(true);
	});

	it("rejects replica_count above 20", () => {
		expect(addTunnelSchema.safeParse({ ...valid, replica_count: 21 }).success).toBe(false);
	});

	it("accepts replica_count at exactly 20", () => {
		expect(addTunnelSchema.safeParse({ ...valid, replica_count: 20 }).success).toBe(true);
	});

	it("rejects non-integer replica_count", () => {
		expect(addTunnelSchema.safeParse({ ...valid, replica_count: 1.5 }).success).toBe(false);
	});
});
