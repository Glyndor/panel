import type { NextConfig } from "next";
import createNextIntlPlugin from "next-intl/plugin";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

const backendUrl = process.env.BACKEND_URL ?? "http://localhost:8080";

const nextConfig: NextConfig = {
	reactCompiler: true,
	output: "standalone",
	async rewrites() {
		return [
			{
				source: "/api/:path*",
				destination: `${backendUrl}/:path*`,
			},
		];
	},
};

export default withNextIntl(nextConfig);
