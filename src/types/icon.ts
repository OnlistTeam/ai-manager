export interface IconMetadata {
  name: string; // Icon name (lowercase, e.g. "openai")
  displayName: string; // Display name (e.g. "OpenAI")
  category: string; // Category (e.g. "ai-provider", "cloud", "tool")
  keywords: string[]; // Search keywords
  defaultColor?: string; // Default color
}

export interface IconPreset {
  [key: string]: IconMetadata;
}
