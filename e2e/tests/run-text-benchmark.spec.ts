import { $, expect } from "@wdio/globals";

describe("Benchmark Lab — run text benchmark (mock provider)", () => {
  it("navigates to the lab and renders the run controls", async () => {
    const sidebarLink = await $('[data-testid="sidebar-benchmark-lab"]');
    await sidebarLink.waitForDisplayed({ timeout: 30_000 });
    await sidebarLink.click();

    const page = await $('[data-testid="benchmark-page"]');
    await page.waitForDisplayed({ timeout: 30_000 });

    const runner = await $('[data-testid="benchmark-tab-runner"]');
    await runner.waitForDisplayed();

    const cases = await $('[data-testid="benchmark-cases"]');
    await cases.waitForDisplayed();

    const runButton = await $('[data-testid="benchmark-run"]');
    await expect(runButton).toBeDisplayed();
  });

  it("kicks off a mock-provider run and surfaces a results section", async () => {
    const runButton = await $('[data-testid="benchmark-run"]');
    await runButton.waitForEnabled({ timeout: 15_000 });
    await runButton.click();

    const results = await $('[data-testid="benchmark-results"]');
    await results.waitForDisplayed({ timeout: 90_000 });

    const text = await results.getText();
    expect(text.length).toBeGreaterThan(0);
  });
});
