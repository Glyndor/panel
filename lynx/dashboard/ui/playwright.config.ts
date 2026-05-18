import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
	forbidOnly: !!process.env.CI,
	fullyParallel: true,

	projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
	reporter: process.env.CI ? "github" : "list",
	retries: process.env.CI ? 2 : 0,
	testDir: "./src/e2e",

	use: {
		baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3000",
		trace: "on-first-retry",
	},

	webServer: process.env.PLAYWRIGHT_BASE_URL
		? undefined
		: {
				command: "bun run start",
				reuseExistingServer: !process.env.CI,
				timeout: 120_000,
				url: "http://localhost:3000",
			},
	workers: process.env.CI ? 1 : undefined,
});
