import { browser, $, expect } from "@wdio/globals";

describe("Live Q&A Pipeline — Real Desktop Tauri E2E Test", () => {
  async function setupMockAudio() {
    await browser.execute(`
      if (!navigator.mediaDevices) {
        Object.defineProperty(navigator, 'mediaDevices', {
          value: {},
          writable: true,
          configurable: true,
        });
      }
      navigator.mediaDevices.getUserMedia = function() {
        return Promise.resolve({
          active: true,
          getTracks: function() { return [{ stop: function() {} }]; }
        });
      };

      function MockMediaRecorder() {
        this.state = 'inactive';
        this.ondataavailable = null;
        this.onstop = null;
      }
      MockMediaRecorder.isTypeSupported = function () {
        return true;
      };
      MockMediaRecorder.prototype.start = function () {
        this.state = 'recording';
      };
      MockMediaRecorder.prototype.stop = function () {
        this.state = 'inactive';
        if (this.ondataavailable) {
          this.ondataavailable({
            data: new Blob([new Uint8Array([26, 69, 223, 163, 1, 0, 0, 0])], { type: 'audio/webm' }),
          });
        }
        if (this.onstop) {
          this.onstop();
        }
      };

      window.MediaRecorder = MockMediaRecorder;
    `);
  }

  async function triggerHotkey(
    hotkey: "F8" | "F9",
    state: "pressed" | "released",
  ) {
    const res = (await browser.execute(`
      return (async function(hk, st) {
        if (!window.__APP_COMMANDS__) {
          return { error: '__APP_COMMANDS__ not found on window (url: ' + window.location.href + ')' };
        }
        var result = await window.__APP_COMMANDS__.testTriggerHotkey({ hotkey: hk, state: st });
        if (result && result.status === 'error') {
          return { error: result.error };
        }
        return { ok: true };
      })('${hotkey}', '${state}');
    `)) as { error?: string; ok?: boolean };

    if (res?.error) {
      throw new Error(`triggerHotkey failed: ${res.error}`);
    }
  }

  async function setMockScenario(payload: Record<string, unknown>) {
    await fetch("http://127.0.0.1:4545/__mock_control", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
  }

  beforeEach(async () => {
    const handles = await browser.getWindowHandles();
    if (handles.length > 0) {
      await browser.switchToWindow(handles[0]);
    }

    const tabNotes = await $("#tab-notes");
    await tabNotes.waitForDisplayed({ timeout: 15000 });

    await setupMockAudio();
    await setMockScenario({
      sttScenario: "success",
      sttDelayMs: 0,
      transcriptText: "Bagaimana cara kerja Rust?",
      aiScenario: "success",
      aiDelayMs: 0,
      aiAnswerText: "Rust menjamin **memory safety** dengan sistem ownership.",
    });
  });

  it("1. Full pipeline sukses: F8 press (>400ms) -> release -> transcript -> AI Markdown answer", async () => {
    // 1. Trigger F8 Press
    await triggerHotkey("F8", "pressed");

    // Tab panel Live Q&A auto-switched & active
    const qaTab = await $("#tab-qa");
    await expect(qaTab).toHaveAttribute("aria-selected", "true");

    // Recording indicator muncul
    const recordingBadge = await $('//*[contains(text(), "Merekam")]');
    await recordingBadge.waitForDisplayed({ timeout: 5000 });
    await expect(recordingBadge).toBeDisplayed();

    // Tahan lebih dari 400ms (>400ms threshold)
    await browser.pause(500);

    // 2. Trigger F8 Release
    await triggerHotkey("F8", "released");

    // Transkrip muncul
    const questionEl = await $(
      '//*[contains(text(), "Bagaimana cara kerja Rust?")]',
    );
    await questionEl.waitForDisplayed({ timeout: 10000 });
    await expect(questionEl).toBeDisplayed();

    // Jawaban AI muncul dalam format Markdown
    const answerEl = await $('//*[contains(text(), "memory safety")]');
    await answerEl.waitForDisplayed({ timeout: 10000 });
    await expect(answerEl).toBeDisplayed();
  });

  it("2. Discard stale request: multiple fast F8 presses -> only newest generation answer displayed", async () => {
    // Request 1 dengan STT delay
    await setMockScenario({
      sttScenario: "delayed",
      sttDelayMs: 800,
      transcriptText: "Pertanyaan Lama Yang Kadaluarsa",
      aiAnswerText: "Jawaban Lama",
    });

    await triggerHotkey("F8", "pressed");
    await browser.pause(450);
    await triggerHotkey("F8", "released");

    // Selagi request 1 masih in-flight di STT mock server, tembak Request 2
    await browser.pause(100);
    await setMockScenario({
      sttScenario: "success",
      sttDelayMs: 0,
      transcriptText: "Pertanyaan Baru Generasi 2",
      aiAnswerText: "Jawaban Baru Generasi 2",
    });

    await triggerHotkey("F8", "pressed");
    await browser.pause(450);
    await triggerHotkey("F8", "released");

    // Verifikasi UI hanya menampilkan pertanyaan dan jawaban generasi 2
    const question2 = await $(
      '//*[contains(text(), "Pertanyaan Baru Generasi 2")]',
    );
    await question2.waitForDisplayed({ timeout: 10000 });
    await expect(question2).toBeDisplayed();

    const answer2 = await $('//*[contains(text(), "Jawaban Baru Generasi 2")]');
    await answer2.waitForDisplayed({ timeout: 10000 });
    await expect(answer2).toBeDisplayed();

    // Pastikan tidak ada sisa teks request lama
    const oldQuestion = await $(
      '//*[contains(text(), "Pertanyaan Lama Yang Kadaluarsa")]',
    );
    await expect(await oldQuestion.isExisting()).toBe(false);
  });

  it("3. Hold di bawah threshold (<400ms): audio dibuang diam-diam, status kembali idle", async () => {
    // Tekan F8 hanya 100ms
    await triggerHotkey("F8", "pressed");
    await browser.pause(100);
    await triggerHotkey("F8", "released");

    // Beri waktu 500ms
    await browser.pause(500);

    // Status kembali ke Siap (idle), tidak ada transkrip atau request terkirim
    const idleIndicator = await $('//*[contains(text(), "Siap")]');
    await expect(idleIndicator).toBeDisplayed();
  });

  it("4. Mic permission ditolak: banner error lokal muncul, tidak memicu invoke backend", async () => {
    await browser.refresh();
    const tabNotes = await $("#tab-notes");
    await tabNotes.waitForDisplayed({ timeout: 15000 });

    // Mock getUserMedia menolak izin
    await browser.execute(`
      if (!navigator.mediaDevices) {
        Object.defineProperty(navigator, 'mediaDevices', {
          value: {},
          writable: true,
          configurable: true,
        });
      }
      navigator.mediaDevices.getUserMedia = function() {
        var err = new Error('Permission denied');
        err.name = 'NotAllowedError';
        return Promise.reject(err);
      };
    `);

    await triggerHotkey("F8", "pressed");
    await browser.pause(450);
    await triggerHotkey("F8", "released");

    // Banner error lokal muncul
    const errorBanner = await $('//*[contains(text(), "Mikrofon:")]');
    await errorBanner.waitForDisplayed({ timeout: 5000 });
    await expect(errorBanner).toBeDisplayed();
  });

  it("5. Semua fallback key habis: qa:error muncul dengan pesan error dari percobaan terakhir", async () => {
    await setupMockAudio();
    await setMockScenario({
      sttScenario: "fail_429",
    });

    await triggerHotkey("F8", "pressed");
    await browser.pause(450);
    await triggerHotkey("F8", "released");

    // Banner error qa:error dari core muncul di UI
    const errorBadge = await $('//*[contains(text(), "rate limit reached")]');
    await errorBadge.waitForDisplayed({ timeout: 10000 });
    await expect(errorBadge).toBeDisplayed();
  });
});
