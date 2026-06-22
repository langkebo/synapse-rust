import { chromium } from "playwright";

const elementBaseUrl = process.env.ELEMENT_BASE_URL || "https://element.test";
const username = process.env.ELEMENT_TEST_USERNAME;
const password = process.env.ELEMENT_TEST_PASSWORD;
const artifactDir = process.env.ELEMENT_HARNESS_ARTIFACT_DIR || "artifacts/e2ee-interop";
const headless = process.env.PLAYWRIGHT_HEADLESS !== "0";

if (!username || !password) {
    throw new Error("ELEMENT_TEST_USERNAME and ELEMENT_TEST_PASSWORD are required");
}

const browser = await chromium.launch({
    headless,
    args: [
        "--disable-dev-shm-usage",
        "--host-resolver-rules=MAP matrix.test 127.0.0.1, MAP element.test 127.0.0.1",
        "--ignore-certificate-errors",
    ],
});

const context = await browser.newContext({
    ignoreHTTPSErrors: true,
    viewport: { width: 1440, height: 1024 },
});
const page = await context.newPage();
const pageErrors = [];
const consoleErrors = [];

page.on("console", (msg) => {
    if (msg.type() === "error") {
        consoleErrors.push(msg.text());
    }
    console.log(`[element-web:${msg.type()}] ${msg.text()}`);
});

page.on("pageerror", (error) => {
    pageErrors.push(error.stack || error.message);
    console.error(`[element-web:pageerror] ${error.stack || error.message}`);
});

async function firstVisible(locators) {
    for (const locator of locators) {
        if (await locator.count()) {
            const candidate = locator.first();
            if (await candidate.isVisible().catch(() => false)) {
                return candidate;
            }
        }
    }

    return null;
}

async function anyVisible(locators) {
    return Boolean(await firstVisible(locators));
}

async function fillFirstVisible(locators, value, description) {
    const target = await firstVisible(locators);
    if (!target) {
        throw new Error(`Could not find ${description}`);
    }

    await target.fill(value);
}

async function clickFirstVisible(locators, description) {
    const target = await firstVisible(locators);
    if (!target) {
        throw new Error(`Could not find ${description}`);
    }

    await target.click();
}

async function isAuthPageVisible() {
    return anyVisible([
        page.locator(".mx_AuthPage").first(),
        page.getByRole("heading", { name: /^Sign in$/i }).first(),
        page.locator('input[name="username"]').first(),
        page.locator('input[name="password"]').first(),
    ]);
}

async function isVerificationPromptVisible() {
    return anyVisible([
        page.getByText(/Verify this device/i).first(),
        page.getByText(/Unable to verify/i).first(),
        page.getByText(/Confirm your digital identity/i).first(),
        page.getByText(/确认你的数字身份/).first(),
        page.getByText(/无法确认/).first(),
        page.getByText(/移除此设备/).first(),
    ]);
}

async function waitForCondition(conditionFn, timeoutMs = 120_000, intervalMs = 1_000) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        try {
            if (await conditionFn()) {
                return;
            }
        } catch (e) {
            // ignore transient DOM errors while polling
        }
        await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
    throw new Error(`Condition not met within ${timeoutMs}ms`);
}

async function waitForPostLoginState() {
    await waitForCondition(async () => {
        const postLoginMarkers = [
            page.locator('[class*="mx_MatrixChat"]').first(),
            page.locator('[class*="mx_LeftPanel"]').first(),
            page.locator('[class*="mx_RoomList"]').first(),
            page.locator('[class*="mx_HomePage"]').first(),
            page.getByText(/^People$/i).first(),
            page.getByText(/^Rooms$/i).first(),
            page.getByText(/^Home$/i).first(),
            page.getByText(/Setting up keys/i).first(),
            page.getByText(/Confirm your identity by entering your account password below/i).first(),
            page.getByText(/确认你的数字身份/).first(),
            page.getByText(/无法确认/).first(),
            page.getByText(/移除此设备/).first(),
        ];

        if (await anyVisible(postLoginMarkers)) {
            return true;
        }

        if (await isVerificationPromptVisible()) {
            return true;
        }

        if (await isAuthPageVisible()) {
            return false;
        }

        return !page.url().includes("/#/login");
    }, 180_000, 2_000);
}

try {
    console.log(`[element-web] opening ${elementBaseUrl}/#/login`);
    await page.goto(`${elementBaseUrl}/#/login`, {
        timeout: 120_000,
        waitUntil: "domcontentloaded",
    });
    await page.waitForLoadState("networkidle", { timeout: 120_000 });

    let loginSuccess = false;
    const loginPromise = new Promise((resolve) => {
        page.on("console", (msg) => {
            const text = msg.text();
            if (text.includes("setLoggedIn")) {
                console.log(`[element-web] Detected setLoggedIn in console!`);
                loginSuccess = true;
                resolve();
            }
        });
    });

    await fillFirstVisible(
        [
            page.getByLabel(/username|email|phone/i),
            page.locator('input[autocomplete="username"]'),
            page.locator('input[name="username"]'),
            page.locator('input[type="text"]'),
        ],
        username,
        "username input",
    );

    await fillFirstVisible(
        [
            page.getByLabel(/password/i),
            page.locator('input[autocomplete="current-password"]'),
            page.locator('input[name="password"]'),
            page.locator('input[type="password"]'),
        ],
        password,
        "password input",
    );

    await clickFirstVisible(
        [
            page.getByRole("button", { name: /sign in|log in|continue/i }),
            page.locator('button[type="submit"]'),
            page.locator("button").filter({ hasText: /sign in|log in|continue/i }),
        ],
        "login button",
    );

    // 等待登录成功的 Promise 或超时
    const timeoutPromise = new Promise((_, reject) => {
        setTimeout(() => reject(new Error("Login timeout")), 180_000);
    });
    await Promise.race([loginPromise, timeoutPromise]);

    await waitForPostLoginState();
    await page.waitForLoadState("networkidle", { timeout: 120_000 }).catch(() => undefined);

    if (!loginSuccess) {
        throw new Error("Element Web did not complete login flow within the timeout");
    }

    console.log("[element-web] login smoke passed");
    if (pageErrors.length) {
        console.warn(`[element-web] page errors observed after login: ${JSON.stringify(pageErrors, null, 2)}`);
    }
    if (consoleErrors.length) {
        console.warn(`[element-web] console errors observed after login: ${JSON.stringify(consoleErrors, null, 2)}`);
    }
} catch (error) {
    const screenshotPath = `${artifactDir}/element-web-login-failure.png`;
    await page.screenshot({ path: screenshotPath, fullPage: true }).catch(() => undefined);
    console.error(`[element-web] login smoke failed; screenshot: ${screenshotPath}`);
    throw error;
} finally {
    await context.close();
    await browser.close();
}
