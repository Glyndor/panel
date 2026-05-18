import createMiddleware from "next-intl/middleware";
import { type NextRequest, NextResponse } from "next/server";
import { routing } from "./i18n/routing";

const BACKEND_URL = process.env.BACKEND_URL ?? "http://localhost:8080";

const handleI18n = createMiddleware(routing);

export async function proxy(request: NextRequest) {
	const { pathname } = request.nextUrl;

	// Strip locale prefix to check if this is an /app/* path
	const localePattern = new RegExp(
		`^/(${routing.locales.join("|")})(/.*)?$`,
	);
	const match = pathname.match(localePattern);
	const pathWithoutLocale = match ? (match[2] ?? "/") : pathname;
	const isAppRoute = pathWithoutLocale.startsWith("/app");

	if (isAppRoute) {
		const accessToken = request.cookies.get("access_token")?.value;
		const refreshToken = request.cookies.get("refresh_token")?.value;
		const locale = match?.[1] ?? routing.defaultLocale;

		if (accessToken) {
			return handleI18n(request);
		}

		if (refreshToken) {
			const refreshed = await tryRefresh(refreshToken, request);
			if (refreshed) return refreshed;
		}

		const loginUrl = new URL(`/${locale}/login`, request.url);
		return NextResponse.redirect(loginUrl);
	}

	return handleI18n(request);
}

async function tryRefresh(
	refreshToken: string,
	request: NextRequest,
): Promise<NextResponse | null> {
	try {
		const res = await fetch(`${BACKEND_URL}/auth/refresh`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ refresh_token: refreshToken }),
		});

		if (!res.ok) return null;

		const data = (await res.json()) as {
			access_token: string;
			refresh_token: string;
			expires_in: number;
		};

		const response = handleI18n(request);
		const next =
			response instanceof NextResponse
				? response
				: NextResponse.next();

		const secure = request.nextUrl.protocol === "https:";

		next.cookies.set("access_token", data.access_token, {
			httpOnly: true,
			secure,
			sameSite: "strict",
			maxAge: data.expires_in,
			path: "/",
		});
		next.cookies.set("refresh_token", data.refresh_token, {
			httpOnly: true,
			secure,
			sameSite: "strict",
			maxAge: 86400,
			path: "/",
		});

		return next;
	} catch {
		return null;
	}
}

export const config = {
	matcher: [
		"/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
	],
};
