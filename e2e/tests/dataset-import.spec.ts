import { $, expect } from "@wdio/globals";

describe("Benchmark Lab — dataset import affordance", () => {
  it("exposes a dataset import input on the runner tab", async () => {
    const sidebarLink = await $('[data-testid="sidebar-benchmark-lab"]');
    await sidebarLink.waitForDisplayed({ timeout: 30_000 });
    await sidebarLink.click();

    const runnerTab = await $('[data-testid="benchmark-tab-runner"]');
    await runnerTab.waitForDisplayed({ timeout: 30_000 });
    await runnerTab.click();

    const importInput = await $('[data-testid="benchmark-import-input"]');
    await importInput.waitForExist({ timeout: 15_000 });

    const tag = await importInput.getTagName();
    expect(tag.toLowerCase()).toBe("input");

    const inputType = await importInput.getAttribute("type");
    expect(inputType).toBe("file");
  });

  it("seeds at least one case so the runner is usable out of the box", async () => {
    const cases = await $('[data-testid="benchmark-cases"]');
    await cases.waitForDisplayed({ timeout: 15_000 });
    const text = await cases.getText();
    expect(text.length).toBeGreaterThan(0);
  });
});
