import { describe, expect, it } from "vitest";
import { providerPresets } from "@/config/claudeProviderPresets";

describe("Kimi For Coding Provider Preset", () => {
  const kimiForCoding = providerPresets.find(
    (p) => p.name === "Kimi For Coding",
  );

  it("should include Kimi For Coding preset", () => {
    expect(kimiForCoding).toBeDefined();
  });

  // CLAUDE_CODE_MAX_CONTEXT_TOKENS is ignored for claude-* model ids, so the
  // preset must route the endpoint's own alias for the context envs to bite
  it("should route the kimi-for-coding model id on every tier", () => {
    const env = (
      kimiForCoding!.settingsConfig as { env: Record<string, string> }
    ).env;
    expect(env).toMatchObject({
      ANTHROPIC_MODEL: "kimi-for-coding",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "kimi-for-coding",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "kimi-for-coding",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "kimi-for-coding",
    });
  });

  // The preset pins the values directly and no longer exposes form input fields; users who want to adjust them edit the JSON directly
  it("should pin the 256K context envs without exposing form fields", () => {
    const env = (
      kimiForCoding!.settingsConfig as { env: Record<string, string> }
    ).env;
    expect(env.CLAUDE_CODE_MAX_CONTEXT_TOKENS).toBe("262144");
    expect(env.CLAUDE_CODE_AUTO_COMPACT_WINDOW).toBe("262144");
    expect(kimiForCoding!.templateValues).toBeUndefined();
  });
});

describe("Codex Provider Preset", () => {
  const codex = providerPresets.find((p) => p.name === "Codex");

  it("should include the Codex preset", () => {
    expect(codex).toBeDefined();
  });

  // The preset pins the Codex catalog's 372K window directly (openai/codex#31860), without exposing form input fields
  it("should pin the Codex-catalog 372K window without exposing form fields", () => {
    const env = (codex!.settingsConfig as { env: Record<string, string> }).env;
    expect(env.CLAUDE_CODE_MAX_CONTEXT_TOKENS).toBe("372000");
    expect(env.CLAUDE_CODE_AUTO_COMPACT_WINDOW).toBe("372000");
    expect(codex!.templateValues).toBeUndefined();
  });
});

describe("OpenCode Go Provider Preset", () => {
  const openCodeGo = providerPresets.find((p) => p.name === "OpenCode Go");

  it("should use the Go Anthropic compatibility endpoint with x-api-key auth", () => {
    expect(openCodeGo).toBeDefined();

    const env = (openCodeGo!.settingsConfig as { env: Record<string, string> })
      .env;
    expect(env).toMatchObject({
      ANTHROPIC_BASE_URL: "https://opencode.ai/zen/go",
      ANTHROPIC_API_KEY: "",
      ANTHROPIC_MODEL: "deepseek-v4-flash",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "deepseek-v4-flash",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "deepseek-v4-flash",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "deepseek-v4-flash",
    });
    // /messages only recognizes x-api-key; Bearer is silently ignored by the gateway -- don't switch back to AUTH_TOKEN
    expect(env).not.toHaveProperty("ANTHROPIC_AUTH_TOKEN");
    // Presets for native anthropic direct connections don't set apiFormat (the default is already routing-free)
    expect(openCodeGo!.apiFormat).toBeUndefined();
    expect(openCodeGo!.apiKeyField).toBe("ANTHROPIC_API_KEY");
  });
});

describe("AWS Bedrock Provider Presets", () => {
  const bedrockAksk = providerPresets.find(
    (p) => p.name === "AWS Bedrock (AKSK)",
  );

  it("should include AWS Bedrock (AKSK) preset", () => {
    expect(bedrockAksk).toBeDefined();
  });

  it("AKSK preset should have required AWS env variables", () => {
    const env = (bedrockAksk!.settingsConfig as { env: Record<string, string> })
      .env;
    expect(env).toHaveProperty("AWS_ACCESS_KEY_ID");
    expect(env).toHaveProperty("AWS_SECRET_ACCESS_KEY");
    expect(env).toHaveProperty("AWS_REGION");
    expect(env).toHaveProperty("CLAUDE_CODE_USE_BEDROCK", "1");
  });

  it("AKSK preset should have template values for AWS credentials", () => {
    expect(bedrockAksk!.templateValues).toBeDefined();
    expect(bedrockAksk!.templateValues!.AWS_ACCESS_KEY_ID).toBeDefined();
    expect(bedrockAksk!.templateValues!.AWS_SECRET_ACCESS_KEY).toBeDefined();
    expect(bedrockAksk!.templateValues!.AWS_REGION).toBeDefined();
    expect(bedrockAksk!.templateValues!.AWS_REGION.editorValue).toBe(
      "us-west-2",
    );
  });

  it("AKSK preset should have correct base URL template", () => {
    const env = (bedrockAksk!.settingsConfig as { env: Record<string, string> })
      .env;
    expect(env.ANTHROPIC_BASE_URL).toContain("bedrock-runtime");
    expect(env.ANTHROPIC_BASE_URL).toContain("${AWS_REGION}");
  });

  it("AKSK preset should have cloud_provider category", () => {
    expect(bedrockAksk!.category).toBe("cloud_provider");
  });

  it("AKSK preset should have Bedrock model as default", () => {
    const env = (bedrockAksk!.settingsConfig as { env: Record<string, string> })
      .env;
    expect(env.ANTHROPIC_MODEL).toContain("anthropic.claude");
  });

  const bedrockApiKey = providerPresets.find(
    (p) => p.name === "AWS Bedrock (API Key)",
  );

  it("should include AWS Bedrock (API Key) preset", () => {
    expect(bedrockApiKey).toBeDefined();
  });

  it("API Key preset should have apiKey field and AWS env variables", () => {
    const config = bedrockApiKey!.settingsConfig as {
      env: Record<string, string>;
    };
    expect(config).toHaveProperty("apiKey", "");
    expect(config.env).toHaveProperty("AWS_REGION");
    expect(config.env).toHaveProperty("CLAUDE_CODE_USE_BEDROCK", "1");
  });

  it("API Key preset should NOT have AKSK env variables", () => {
    const env = (
      bedrockApiKey!.settingsConfig as { env: Record<string, string> }
    ).env;
    expect(env).not.toHaveProperty("AWS_ACCESS_KEY_ID");
    expect(env).not.toHaveProperty("AWS_SECRET_ACCESS_KEY");
  });

  it("API Key preset should have template values for region only", () => {
    expect(bedrockApiKey!.templateValues).toBeDefined();
    expect(bedrockApiKey!.templateValues!.AWS_REGION).toBeDefined();
    expect(bedrockApiKey!.templateValues!.AWS_REGION.editorValue).toBe(
      "us-west-2",
    );
  });

  it("API Key preset should have cloud_provider category", () => {
    expect(bedrockApiKey!.category).toBe("cloud_provider");
  });
});
