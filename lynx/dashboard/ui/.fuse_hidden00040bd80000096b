import react from "@vitejs/plugin-react";
import path from "path";
import { defineConfig } from "vitest/config";

export default defineConfig({
	plugins: [react()],
	resolve: {
		conditions: ["import", "module", "browser", "default"],
		alias: {
			"@": path.resolve(__dirname, "./src"),
		},
	},
	test: {
		environment: "jsdom",
		globals: true,
		setupFiles: ["./src/__tests__/setup.ts"],
		include: ["src/__tests__/**/*.test.{ts,tsx}"],
		server: {
			deps: {
				inline: ["zod"],
			},
		},
	},
});
