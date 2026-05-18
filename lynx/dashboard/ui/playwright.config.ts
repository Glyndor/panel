import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
	testDir: "./src/e2e",
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	workers: process.env.CI ? 1 : undefined,
	reporter: process.env.CI ? "github" : "list",

	use: {
		baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3000",
		trace: "on-first-retry",
	},

	projects: [
		{ name: "chromium", use: { ...devices["Desktop Chrome"] } },
	],

	webServer: process.env.CI
		? {
				command: "bun run start",
				url: "http://localhost:3000",
				reuseExistingServer: false,
				timeout: 120_000,
		  }
		: process.env.PLAYWRIGHT_NO_SERVER
		  ? undefined
		  : {
				command: "bun run dev",
				url: "http://localhost:3000",
				reuseExistingServer: true,
				timeout: 120_000,
		    },
});
