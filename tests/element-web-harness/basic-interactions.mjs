import fs from "node:fs";
import { chromium } from "playwright";

const elementBaseUrl = process.env.ELEMENT_BASE_URL || "https://element.test";
const username = process.env.ELEMENT_TEST_USERNAME;
const password = process.env.ELEMENT_TEST_PASSWORD;
const artifactDir = process.env.ELEMENT_HARNESS_ARTIFACT_DIR || "artifacts/e2ee-interop";
const headless = process.env.PLAYWRIGHT_HEADLESS !== "0";
const slowMo = parseInt(process.env.PLAYWRIGHT_SLOWMO || "0", 10);

if (!username || !password) {
    throw new Error("ELEMENT_TEST_USERNAME and ELEMENT_TEST_PASSWORD are required");
}

const browser = await chromium.launch({
    headless,
    slowMo,
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

page.on("console", (msg) => {
    console.log(`[element-web:${msg.type()}] ${msg.text()}`);
});

page.on("pageerror", (error) => {
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

async function waitForCondition(conditionFn, timeoutMs = 120_000, intervalMs = 1_000) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        try {
            if (await conditionFn()) {
                return;
            }
        } catch (e) {
            // ignore errors during polling
        }
        await new Promise(resolve => setTimeout(resolve, intervalMs));
    }
    throw new Error(`Condition not met within ${timeoutMs}ms`);
}

async function takeScreenshot(name) {
    const path = `${artifactDir}/element-web-${name}-${Date.now()}.png`;
    await page.screenshot({ path, fullPage: true }).catch(() => { });
    console.log(`[element-web] Screenshot saved: ${path}`);
}

async function dumpDebugSnapshot(name) {
    const timestamp = Date.now();
    const htmlPath = `${artifactDir}/element-web-${name}-${timestamp}.html`;

    const title = await page.title().catch(() => "");
    const url = page.url();
    const bodyClass = await page.locator("body").getAttribute("class").catch(() => "");
    const buttonTexts = await page.locator("button").evaluateAll((elements) =>
        elements
            .map((element) => (element.textContent || "").trim())
            .filter(Boolean)
            .slice(0, 50),
    ).catch(() => []);
    const roleButtonSummaries = await page.locator('[role="button"]').evaluateAll((elements) =>
        elements
            .map((element) => {
                const text = (element.textContent || "").trim().replace(/\s+/g, " ");
                const label = element.getAttribute("aria-label") || "";
                const className = typeof element.className === "string" ? element.className : "";
                return { text, label, className };
            })
            .filter((entry) => entry.text || entry.label || entry.className)
            .slice(0, 50),
    ).catch(() => []);
    const inputSummaries = await page.locator("input, textarea").evaluateAll((elements) =>
        elements
            .map((element) => ({
                name: element.getAttribute("name") || "",
                type: element.getAttribute("type") || "",
                placeholder: element.getAttribute("placeholder") || "",
                ariaLabel: element.getAttribute("aria-label") || "",
                className: typeof element.className === "string" ? element.className : "",
            }))
            .slice(0, 50),
    ).catch(() => []);
    const headings = await page.locator("h1, h2, h3, [role='heading']").evaluateAll((elements) =>
        elements
            .map((element) => (element.textContent || "").trim().replace(/\s+/g, " "))
            .filter(Boolean)
            .slice(0, 50),
    ).catch(() => []);

    console.log(`[element-web] debug snapshot ${name}`);
    console.log("[element-web] page url:", url);
    console.log("[element-web] page title:", title);
    console.log("[element-web] body class:", bodyClass);
    console.log("[element-web] headings:", headings);
    console.log("[element-web] button texts:", buttonTexts);
    console.log("[element-web] role=button summaries:", JSON.stringify(roleButtonSummaries, null, 2));
    console.log("[element-web] input summaries:", JSON.stringify(inputSummaries, null, 2));

    const html = await page.content().catch(() => "<html><body>Unable to capture page content</body></html>");
    await fs.promises.writeFile(htmlPath, html, "utf8").catch(() => undefined);
    console.log(`[element-web] HTML snapshot saved: ${htmlPath}`);
}

async function maybeCompleteKeySetup() {
    const setupPanel = page.getByText(/Setting up keys/i).first();
    const passwordPrompt = page.getByText(/Confirm your identity by entering your account password below/i).first();

    const setupVisible =
        (await setupPanel.isVisible().catch(() => false)) ||
        (await passwordPrompt.isVisible().catch(() => false));

    if (!setupVisible) {
        return false;
    }

    console.log("[element-web] detected post-login key setup prompt");
    await takeScreenshot("key-setup-prompt");

    const passwordFields = [
        page.getByLabel(/password/i),
        page.locator('input[type="password"]'),
        page.locator('input[autocomplete="current-password"]'),
    ];

    await fillFirstVisible(passwordFields, password, "key setup password input");

    const continueButtons = [
        page.getByRole("button", { name: /continue|confirm|submit/i }),
        page.locator('button').filter({ hasText: /continue|confirm|submit/i }),
    ];

    await clickFirstVisible(continueButtons, "key setup continue button");
    await page.waitForTimeout(5_000);
    await takeScreenshot("key-setup-submitted");

    return true;
}

async function maybeDismissKeySetupFailure() {
    const failureTexts = [
        page.getByText(/Unable to set up keys/i).first(),
        page.getByText(/User-Interactive Authentication required/i).first(),
    ];

    if (!(await anyVisible(failureTexts))) {
        return false;
    }

    console.log("[element-web] detected key setup failure dialog");
    await takeScreenshot("key-setup-failure");

    const dismissButtons = [
        page.getByRole("button", { name: /cancel|skip|close|done|not now/i }),
        page.locator("button").filter({ hasText: /cancel|skip|close|done|not now/i }),
    ];

    await clickFirstVisible(dismissButtons, "key setup failure dismiss button");
    await page.waitForTimeout(3_000);
    await takeScreenshot("key-setup-failure-dismissed");

    return true;
}

async function handlePostLoginBlockers() {
    if (await maybeCompleteKeySetup()) {
        return true;
    }

    if (await maybeDismissKeySetupFailure()) {
        return true;
    }

    return false;
}

async function isAuthPageVisible() {
    return anyVisible([
        page.locator(".mx_AuthPage").first(),
        page.getByRole("heading", { name: /^Sign in$/i }).first(),
        page.locator('input[name="username"]').first(),
        page.locator('input[name="password"]').first(),
    ]);
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
        ];

        if (await anyVisible(postLoginMarkers)) {
            return true;
        }

        if (await isAuthPageVisible()) {
            return false;
        }

        return !page.url().includes("/#/login");
    }, 180_000, 2_000);
}

async function waitForRoomShell() {
    await waitForCondition(async () => {
        if (await handlePostLoginBlockers()) {
            return false;
        }

        const blockers = [
            page.getByText(/Setting up keys/i).first(),
            page.getByText(/Confirm your identity by entering your account password below/i).first(),
            page.getByText(/Unable to set up keys/i).first(),
            page.getByText(/User-Interactive Authentication required/i).first(),
        ];

        for (const blocker of blockers) {
            if (await blocker.isVisible().catch(() => false)) {
                return false;
            }
        }

        if (await isAuthPageVisible()) {
            return false;
        }

        if (page.url().includes("/#/login")) {
            return false;
        }

        const shellMarkers = [
            page.locator('[class*="mx_RoomList"]').first(),
            page.locator('[class*="mx_LeftPanel"]').first(),
            page.locator('[class*="mx_HomePage"]').first(),
            page.locator('[class*="mx_MatrixChat"]').first(),
            page.getByText(/^People$/i).first(),
            page.getByText(/^Rooms$/i).first(),
            page.getByText(/^Home$/i).first(),
        ];

        for (const marker of shellMarkers) {
            if (await marker.isVisible().catch(() => false)) {
                return true;
            }
        }

        return false;
    }, 180_000, 2_000);
}

async function sendMessageAndAssertVisible(messageText) {
    const messageInputCandidates = [
        page.getByLabel(/Send a message/i),
        page.locator('textarea[placeholder*="Send a message"]'),
        page.locator('div[role="textbox"]'),
        page.locator('[data-testid*="message-composer"]'),
        page.locator('.mx_SendMessageComposer textarea'),
        page.locator('.mx_BasicMessageComposer textarea'),
        page.locator('[contenteditable="true"]').first(),
    ];

    const messageInput = await firstVisible(messageInputCandidates);
    if (!messageInput) {
        await dumpDebugSnapshot("missing-message-composer");
        throw new Error("Could not find message composer after room creation");
    }

    await messageInput.click().catch(() => undefined);
    await messageInput.fill(messageText).catch(async () => {
        await messageInput.pressSequentially(messageText);
    });
    await page.waitForTimeout(1_000);
    await messageInput.press("Enter");
    console.log(`[element-web] sent message attempt: ${messageText}`);

    const timelineMessageCandidates = [
        page.locator('[data-event-id]').filter({ hasText: messageText }).first(),
        page.locator('[class*="mx_EventTile"]').filter({ hasText: messageText }).first(),
        page.locator('[class*="mx_MTextBody"]').filter({ hasText: messageText }).first(),
        page.getByText(messageText, { exact: true }).first(),
    ];

    await waitForCondition(async () => {
        for (const candidate of timelineMessageCandidates) {
            if (await candidate.isVisible().catch(() => false)) {
                return true;
            }
        }
        return false;
    }, 30_000, 1_000);

    console.log(`[element-web] message appeared in timeline: ${messageText}`);
    await takeScreenshot("message-sent");
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
    const loginTimeoutPromise = new Promise((_, reject) => {
        setTimeout(() => reject(new Error("Login timeout")), 180_000);
    });
    await Promise.race([loginPromise, loginTimeoutPromise]);

    await waitForPostLoginState();
    await page.waitForLoadState("networkidle", { timeout: 120_000 }).catch(() => undefined);

    if (!loginSuccess) {
        throw new Error("Element Web did not complete login flow within the timeout");
    }

    console.log("[element-web] login smoke passed");

    await takeScreenshot("after-login");

    await handlePostLoginBlockers();

    // 现在，我们尝试创建一个新房间
    console.log("[element-web] trying to create a new room...");

    // 等待主界面加载；首次登录时 Element Web 可能先弹出密钥初始化确认框。
    await waitForRoomShell();
    await takeScreenshot("main-ui");
    await dumpDebugSnapshot("main-ui");

    // 查找并点击创建房间按钮
    const createRoomButtonCandidates = [
        page.getByRole("button", { name: /Create|New Room|Add Room|\+ Room|\+ Chat/i }),
        page.locator('[aria-label*="Create room"]'),
        page.locator('[aria-label*="New room"]'),
        page.locator('[data-testid*="create-room"]'),
        page.locator('[data-testid*="add-room"]'),
        page.locator('[aria-label*="Start new chat"]'),
        page.locator('.mx_LeftPanel_buttonBar_createButton'),
        page.locator('.mx_HeaderButton_label').filter({ hasText: /\+/i }),
        page.locator('button').filter({ hasText: /Create.*Room|New.*Room|\+/i }),
    ];

    const createRoomButton = await firstVisible(createRoomButtonCandidates);
    if (createRoomButton) {
        console.log("[element-web] found create room button");
        await createRoomButton.click();
        await page.waitForTimeout(5_000);
        await takeScreenshot("create-room-dialog");

        // 填写房间名称
        const roomName = `Test Room ${Date.now()}`;
        const roomNameInputCandidates = [
            page.getByLabel(/Room name/i),
            page.locator('input[name="name"]'),
            page.locator('input[placeholder*="Room name"]'),
            page.locator('input[placeholder*="Name"]'),
            page.locator('.mx_Dialog_content input[type="text"]'),
        ];

        try {
            await fillFirstVisible(roomNameInputCandidates, roomName, "room name input");
        } catch (e) {
            console.log("[element-web] Could not find room name input, skipping room creation", e);
        }

        // 点击创建按钮
        const finalCreateButtonCandidates = [
            page.getByRole("button", { name: /Create|Start Chat|Continue|Save|Done/i }),
            page.locator('button[type="submit"]'),
            page.locator('.mx_Dialog_primary'),
            page.locator('.mx_Dialog button').filter({ hasText: /Create|Start|Done/i }),
        ];

        try {
            await clickFirstVisible(finalCreateButtonCandidates, "final create room button");
        } catch (e) {
            console.log("[element-web] Could not find final create button, skipping room creation", e);
        }

        console.log(`[element-web] created room: ${roomName}`);
        await page.waitForTimeout(8_000);
        await takeScreenshot("room-created");

        // 现在尝试发送消息
        const messageText = "Hello from synapse-rust harness! 🎉";
        console.log(`[element-web] trying to send message: ${messageText}`);

        await sendMessageAndAssertVisible(messageText);

        console.log("[element-web] basic interactions passed!");
    } else {
        console.log("[element-web] could not find create room button, skipping room creation, but login was successful!");
        await dumpDebugSnapshot("no-create-room-button");
    }

} catch (error) {
    const screenshotPath = `${artifactDir}/element-web-interactions-failure-${Date.now()}.png`;
    await page.screenshot({ path: screenshotPath, fullPage: true }).catch(() => undefined);
    console.error(`[element-web] interactions failed; screenshot: ${screenshotPath}`);
    throw error;
} finally {
    await context.close();
    await browser.close();
}
