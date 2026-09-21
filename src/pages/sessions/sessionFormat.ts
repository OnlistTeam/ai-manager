import type { SessionMessageRole, SessionSummary } from "@/entities/session";

export function sessionActivity(summary: SessionSummary): number | null {
  return summary.lastActiveAt ?? summary.createdAt;
}

export function formatSessionTime(
  value: number | null,
  locale: string,
): string | null {
  if (value === null) return null;
  const milliseconds = Math.abs(value) < 10_000_000_000 ? value * 1_000 : value;
  const date = new Date(milliseconds);
  if (!Number.isFinite(date.getTime())) return null;
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

export function roleTone(
  role: SessionMessageRole,
): "brand" | "success" | "warning" | "neutral" {
  if (role === "user") return "brand";
  if (role === "assistant") return "success";
  if (role === "system" || role === "tool") return "warning";
  return "neutral";
}
