import { defineRouting } from "next-intl/routing";

export const routing = defineRouting({
	defaultLocale: "en",
	locales: ["en", "es"],
});

export type Locale = (typeof routing.locales)[number];
