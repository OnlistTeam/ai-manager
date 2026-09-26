import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse, type JsonBodyType } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PrivacyStatusLine } from "@/features/routing-privacy";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function reply(body: JsonBodyType, status = 200) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_privacy_protection_get`, () =>
      HttpResponse.json(body, { status }),
    ),
  );
}

function mount(onOpenSettings?: () => void) {
  return render(<PrivacyStatusLine onOpenSettings={onOpenSettings} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("PrivacyStatusLine", () => {
  beforeEach(async () => {
    for (const [language, tree] of [
      ["en", en],
      ["zh", zh],
    ] as const) {
      i18n.addResourceBundle(
        language,
        "translation",
        { ds: tree.ds, error: tree.error, routing: tree.routing },
        true,
        true,
      );
    }
    await i18n.changeLanguage("en");
  });

  it("reports the defaults without a word count", async () => {
    reply({ maskSecrets: true, maskPersonal: false, words: [] });
    mount();

    expect(
      await screen.findByText(
        "Privacy: keys and passwords hidden · personal information not hidden",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("counts the custom words with the right plural", async () => {
    reply({ maskSecrets: false, maskPersonal: true, words: ["acme"] });
    const { unmount } = mount();
    expect(
      await screen.findByText(
        "Privacy: keys and passwords not hidden · personal information hidden · 1 custom word",
      ),
    ).toBeInTheDocument();
    unmount();

    await i18n.changeLanguage("zh");
    reply({ maskSecrets: true, maskPersonal: false, words: ["acme", "张三"] });
    mount();
    expect(
      await screen.findByText(
        "隐私：已隐藏密钥和密码 · 个人信息未隐藏 · 2 个自定义词",
      ),
    ).toBeInTheDocument();
  });

  it("links to the privacy settings", async () => {
    reply({ maskSecrets: true, maskPersonal: false, words: [] });
    const onOpenSettings = vi.fn();
    mount(onOpenSettings);

    await userEvent.click(
      await screen.findByRole("button", {
        name: en.routing.privacy.openSettingsLabel,
      }),
    );
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it("says the settings can't be read and still links to them", async () => {
    reply(
      {
        code: "UPSTREAM_ERROR",
        messageKey: "error.settings.loadFailed",
        technicalMessage: null,
        remediation: null,
        contextId: null,
      },
      500,
    );
    mount(vi.fn());

    expect(
      await screen.findByText(
        `Privacy: ${en.routing.privacy.unavailable}`,
        {},
        { timeout: 3000 },
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: en.routing.privacy.openSettingsLabel,
      }),
    ).toBeInTheDocument();
  });
});
