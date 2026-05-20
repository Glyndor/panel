import { expect, test } from "@playwright/test";

// ---------------------------------------------------------------------------
// Login page
// ---------------------------------------------------------------------------

test.describe("Login page", () => {
	test("renders form with username and password fields", async ({ page }) => {
		await page.goto("/en/login");

		await expect(page.getByLabel("Username")).toBeVisible();
		await expect(page.locator("#password")).toBeVisible();
		await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
	});

	test("shows link to register page", async ({ page }) => {
		await page.goto("/en/login");

		const link = page.getByRole("link", { name: "Create account" });
		await expect(link).toBeVisible();
		await expect(link).toHaveAttribute("href", "/en/register");
	});

	test("root path redirects to login", async ({ page }) => {
		await page.goto("/");
		await page.waitForURL(/\/en\/login/, { timeout: 10_000 });
	});

	test("submit button is disabled while submitting", async ({ page }) => {
		await page.goto("/en/login");

		// Delay POST so we can observe the submitting state before the response
		await page.route("**/en/login", async (route) => {
			if (route.request().method() === "POST") {
				await new Promise<void>((resolve) => setTimeout(resolve, 1000));
				await route.continue();
			} else {
				await route.continue();
			}
		});

		await page.getByLabel("Username").fill("testuser");
		await page.locator("#password").fill("password123");
		await page.getByRole("button", { name: "Sign in" }).click();

		// During the network delay, button shows submitting state
		await expect(page.getByRole("button", { name: "Signing in..." })).toBeVisible();
	});

	test("password field masks input", async ({ page }) => {
		await page.goto("/en/login");

		const passwordInput = page.locator("#password");
		await expect(passwordInput).toHaveAttribute("type", "password");
	});
});

// ---------------------------------------------------------------------------
// Register page
// ---------------------------------------------------------------------------

test.describe("Register page", () => {
	test("renders form with username, email, and password fields", async ({ page }) => {
		await page.goto("/en/register");

		await expect(page.getByLabel("Username")).toBeVisible();
		await expect(page.getByLabel("Email")).toBeVisible();
		await expect(page.locator("#password")).toBeVisible();
		await expect(page.getByRole("button", { name: "Create account" })).toBeVisible();
	});

	test("shows link back to login page", async ({ page }) => {
		await page.goto("/en/register");

		const link = page.getByRole("link", { name: "Sign in" });
		await expect(link).toBeVisible();
		await expect(link).toHaveAttribute("href", "/en/login");
	});

	test("password field masks input", async ({ page }) => {
		await page.goto("/en/register");

		const passwordInput = page.locator("#password");
		await expect(passwordInput).toHaveAttribute("type", "password");
	});

	test("shows client-side validation for empty username", async ({ page }) => {
		await page.goto("/en/register");

		// Submit without filling username
		await page.getByLabel("Email").fill("user@example.com");
		await page.locator("#password").fill("ValidP@ss12!");
		await page.getByRole("button", { name: "Create account" }).click();

		// Zod validation fires before any network call
		await expect(page.locator("text=at least 3 characters")).toBeVisible();
	});

	test("shows validation for short password", async ({ page }) => {
		await page.goto("/en/register");

		await page.getByLabel("Username").fill("validuser");
		await page.getByLabel("Email").fill("user@example.com");
		await page.locator("#password").fill("Short1!");
		await page.getByRole("button", { name: "Create account" }).click();

		await expect(page.locator("text=at least 12 characters")).toBeVisible();
	});

	test("shows validation for missing uppercase", async ({ page }) => {
		await page.goto("/en/register");

		await page.getByLabel("Username").fill("validuser");
		await page.getByLabel("Email").fill("user@example.com");
		await page.locator("#password").fill("nouppercase12!");
		await page.getByRole("button", { name: "Create account" }).click();

		await expect(page.locator("text=uppercase")).toBeVisible();
	});

	test("shows validation for missing number in password", async ({ page }) => {
		await page.goto("/en/register");

		await page.getByLabel("Username").fill("validuser");
		await page.getByLabel("Email").fill("user@example.com");
		await page.locator("#password").fill("NoNumbersHere!");
		await page.getByRole("button", { name: "Create account" }).click();

		await expect(page.locator("text=number")).toBeVisible();
	});

	test("shows validation for missing special character", async ({ page }) => {
		await page.goto("/en/register");

		await page.getByLabel("Username").fill("validuser");
		await page.getByLabel("Email").fill("user@example.com");
		await page.locator("#password").fill("NoSpecialChar12");
		await page.getByRole("button", { name: "Create account" }).click();

		await expect(page.locator("text=special")).toBeVisible();
	});

	test("shows validation for invalid email", async ({ page }) => {
		await page.goto("/en/register");

		await page.getByLabel("Username").fill("validuser");
		await page.getByLabel("Email").fill("notanemail");
		await page.locator("#password").fill("ValidP@ss12!");
		await page.getByRole("button", { name: "Create account" }).click();

		await expect(page.locator("text=valid email")).toBeVisible();
	});

	test("shows validation for reserved username", async ({ page }) => {
		await page.goto("/en/register");

		await page.getByLabel("Username").fill("admin");
		await page.getByLabel("Email").fill("user@example.com");
		await page.locator("#password").fill("ValidP@ss12!");
		await page.getByRole("button", { name: "Create account" }).click();

		await expect(page.locator("text=reserved")).toBeVisible();
	});
});

// ---------------------------------------------------------------------------
// Navigation between auth pages
// ---------------------------------------------------------------------------

test.describe("Auth navigation", () => {
	test("clicking 'Create account' on login navigates to register", async ({ page }) => {
		await page.goto("/en/login");
		await page.getByRole("link", { name: "Create account" }).click();
		await expect(page).toHaveURL(/\/en\/register/);
	});

	test("clicking 'Sign in' on register navigates to login", async ({ page }) => {
		await page.goto("/en/register");
		await page.getByRole("link", { name: "Sign in" }).click();
		await expect(page).toHaveURL(/\/en\/login/);
	});
});
