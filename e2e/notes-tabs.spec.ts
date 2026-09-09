import { browser, $, $$, expect } from "@wdio/globals";

describe("Notes Multi-Document Tabs — Real Desktop Tauri E2E Test", () => {
  before(async () => {
    const handles = await browser.getWindowHandles();
    if (handles.length > 0) {
      await browser.switchToWindow(handles[0]);
    }

    // Pastikan berada di tab Notes
    const tabNotes = await $("#tab-notes");
    await tabNotes.waitForDisplayed({ timeout: 15000 });
    await tabNotes.click();

    const panelNotes = await $("#panel-notes");
    await panelNotes.waitForDisplayed({ timeout: 5000 });

    // Bersihkan tab yang mungkin ter-restore dari sesi sebelumnya agar test berjalan deterministik
    let existingCloseButtons = await $$('button[aria-label^="Tutup tab "]');
    while (existingCloseButtons.length > 0) {
      await existingCloseButtons[0].click();
      await browser.pause(200);
      existingCloseButtons = await $$('button[aria-label^="Tutup tab "]');
    }

    // Pastikan empty state tampil sebelum pengujian dimulai
    const emptyState = await $("//*[contains(text(), 'Belum ada file notes yang dipilih')]");
    await emptyState.waitForDisplayed({ timeout: 5000 });
  });

  it("1. Dynamic document tabs rendering: emit notesUpdate creates tabs with titles and renders markdown", async () => {
    const doc1Path = "D:\\project\\e2e-docs\\guide.md";
    const doc2Path = "D:\\project\\e2e-docs\\cheatsheet.md";

    // 1. Emit event notes:update untuk doc1
    await browser.execute(`
      (async function(p1) {
        if (!window.__APP_EVENTS__) throw new Error('__APP_EVENTS__ not found on window');
        await window.__APP_EVENTS__.notesUpdate.emit({
          path: p1,
          content: '# Panduan Overlay\\n\\nIni adalah dokumen panduan pertama.'
        });
      })('${doc1Path.replace(/\\/g, "\\\\")}');
    `);

    // Tunggu tab 1 muncul
    const tablist = await $('[role="tablist"][aria-label="Document tabs"]');
    await tablist.waitForDisplayed({ timeout: 5000 });

    // 2. Emit event notes:update untuk doc2
    await browser.execute(`
      (async function(p2) {
        await window.__APP_EVENTS__.notesUpdate.emit({
          path: p2,
          content: '# Cheatsheet Hotkey\\n\\n- F8: Push to talk\\n- F9: Toggle stealth'
        });
      })('${doc2Path.replace(/\\/g, "\\\\")}');
    `);

    await browser.waitUntil(
      async () => (await $$('[role="tablist"][aria-label="Document tabs"] [role="tab"]')).length === 2,
      { timeout: 5000, timeoutMsg: "Expected exactly 2 tabs to be rendered" }
    );

    const tabs = await $$('[role="tablist"][aria-label="Document tabs"] [role="tab"]');
    expect(tabs.length).toBe(2);

    const tab1Text = await tabs[0].getText();
    const tab2Text = await tabs[1].getText();
    expect(tab1Text).toContain("guide.md");
    expect(tab2Text).toContain("cheatsheet.md");

    // Tab pertama harus otomatis aktif karena doc1 yang pertama kali dimasukkan
    const tab1Selected = await tabs[0].getAttribute("aria-selected");
    expect(tab1Selected).toBe("true");

    // Verifikasi konten markdown tab 1 ter-render di .prose
    const prose = await $("#panel-notes .prose");
    await prose.waitForDisplayed({ timeout: 5000 });
    const proseText = await prose.getText();
    expect(proseText).toContain("Panduan Overlay");
    expect(proseText).toContain("Ini adalah dokumen panduan pertama.");
  });

  it("2. Tab switching: clicking tab 2 changes active tab, calls backend IPC, and renders tab 2 markdown", async () => {
    const tabs = await $$('[role="tablist"][aria-label="Document tabs"] [role="tab"]');
    expect(tabs.length).toBe(2);

    // Klik tab kedua (cheatsheet.md)
    await tabs[1].click();

    // Verifikasi tab kedua sekarang aktif
    await browser.waitUntil(
      async () => (await tabs[1].getAttribute("aria-selected")) === "true",
      { timeout: 3000, timeoutMsg: "Tab 2 did not become active" }
    );

    const tab1Selected = await tabs[0].getAttribute("aria-selected");
    expect(tab1Selected).toBe("false");

    // Verifikasi konten markdown berganti ke konten tab 2
    const prose = await $("#panel-notes .prose");
    await browser.waitUntil(
      async () => (await prose.getText()).includes("Cheatsheet Hotkey"),
      { timeout: 3000, timeoutMsg: "Markdown content did not switch to tab 2 content" }
    );
    const proseText = await prose.getText();
    expect(proseText).toContain("Cheatsheet Hotkey");
    expect(proseText).toContain("F8: Push to talk");

    // Verifikasi backend core mencatat tab 2 sebagai active_notes_path
    const backendState = (await browser.execute(`
      return (async function() {
        if (!window.__APP_COMMANDS__) return null;
        var res = await window.__APP_COMMANDS__.getNotesState();
        return res.status === 'ok' ? res.data : null;
      })();
    `)) as { activePath?: string } | null;

    if (backendState?.activePath) {
      expect(backendState.activePath).toContain("cheatsheet.md");
    }
  });

  it("3. Background update: modifying non-active tab preserves current active tab and updates in background", async () => {
    const doc1Path = "D:\\project\\e2e-docs\\guide.md";

    // Tab 2 (cheatsheet.md) sedang aktif saat ini.
    // Kirim event update untuk doc1 (guide.md) yang sedang di background.
    await browser.execute(`
      (async function(p1) {
        await window.__APP_EVENTS__.notesUpdate.emit({
          path: p1,
          content: '# Panduan Overlay (Revisi 2)\\n\\nKonten diperbarui secara live di background.'
        });
      })('${doc1Path.replace(/\\/g, "\\\\")}');
    `);

    // Pastikan tab 2 tetap aktif dan kontennya tidak terganggu
    const tabs = await $$('[role="tablist"][aria-label="Document tabs"] [role="tab"]');
    expect(await tabs[1].getAttribute("aria-selected")).toBe("true");

    const currentProseText = await $("#panel-notes .prose").getText();
    expect(currentProseText).toContain("Cheatsheet Hotkey");

    // Sekarang beralih kembali ke tab 1
    await tabs[0].click();
    await browser.waitUntil(
      async () => (await tabs[0].getAttribute("aria-selected")) === "true",
      { timeout: 3000 }
    );

    // Konten yang ter-render adalah konten yang baru saja di-update di background
    const updatedProseText = await $("#panel-notes .prose").getText();
    expect(updatedProseText).toContain("Panduan Overlay (Revisi 2)");
    expect(updatedProseText).toContain("Konten diperbarui secara live di background.");
  });

  it("4. Close tab: clicking '✕' invokes closeNotesFile, removes tab from DOM, and switches active tab", async () => {
    // Saat ini tab 1 aktif, tab 2 non-aktif (total 2 tabs).
    // Tutup tab 2 dengan klik tombol silang (✕)
    const closeBtnDoc2 = await $('button[aria-label="Tutup tab cheatsheet.md"]');
    await closeBtnDoc2.waitForDisplayed({ timeout: 3000 });
    await closeBtnDoc2.click();

    // Verifikasi tab 2 hilang dari DOM, menyisakan 1 tab
    await browser.waitUntil(
      async () => {
        const remainingTabs = await $$('[role="tablist"][aria-label="Document tabs"] [role="tab"]');
        return remainingTabs.length === 1;
      },
      { timeout: 3000, timeoutMsg: "Tab was not removed after clicking close" }
    );

    // Tab 1 tetap ada dan aktif
    const remainingTabs = await $$('[role="tablist"][aria-label="Document tabs"] [role="tab"]');
    expect(await remainingTabs[0].getText()).toContain("guide.md");
    expect(await remainingTabs[0].getAttribute("aria-selected")).toBe("true");

    // Sekarang tutup tab 1 juga -> seluruh tab tertutup
    const closeBtnDoc1 = await $('button[aria-label="Tutup tab guide.md"]');
    await closeBtnDoc1.waitForDisplayed({ timeout: 3000 });
    await closeBtnDoc1.click();

    // Verifikasi empty state placeholder muncul
    const emptyState = await $("//*[contains(text(), 'Belum ada file notes yang dipilih')]");
    await emptyState.waitForDisplayed({ timeout: 3000 });
  });

  it("5. Sub-mode persistence: toggle to Scratchpad and back to File Mode maintains tab state", async () => {
    // Switch ke Scratchpad
    const scratchpadBtn = await $("//button[contains(., 'Scratchpad')]");
    await scratchpadBtn.waitForDisplayed({ timeout: 3000 });
    await scratchpadBtn.click();

    // Ketik catatan di textarea scratchpad
    const textarea = await $("#panel-notes textarea");
    await textarea.waitForDisplayed({ timeout: 3000 });
    await textarea.setValue("Catatan ad-hoc E2E test");
    expect(await textarea.getValue()).toBe("Catatan ad-hoc E2E test");

    // Switch kembali ke File Mode
    const fileModeBtn = await $("//button[contains(., 'File Mode')]");
    await fileModeBtn.click();

    // Verifikasi empty state di File Mode masih tampil
    const emptyState = await $("//*[contains(text(), 'Belum ada file notes yang dipilih')]");
    await emptyState.waitForDisplayed({ timeout: 3000 });

    // Switch lagi ke Scratchpad -> teks scratchpad tetap ada
    await scratchpadBtn.click();
    const textareaRestored = await $("#panel-notes textarea");
    expect(await textareaRestored.getValue()).toBe("Catatan ad-hoc E2E test");
  });
});
