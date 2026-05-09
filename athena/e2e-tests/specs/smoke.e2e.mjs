import { expect } from "@wdio/globals";

describe("ATHENA smoke", () => {
  it("can enter as guest and show header", async () => {
    const guest = await $('button*=Access as Guest');
    await guest.waitForDisplayed({ timeout: 60_000 });
    await guest.click();

    // After guest login, the universal header should exist.
    const header = await $("header[role='banner']");
    await header.waitForDisplayed({ timeout: 60_000 });
    await expect(header).toBeDisplayed();
  });
});

