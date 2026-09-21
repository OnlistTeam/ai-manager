export type GreetingSlot = "morning" | "afternoon" | "evening";

/**
 * Greeting segments from Spec §27. Early morning counts as "evening" —
 * someone still coding at 3am doesn't need to be wished "good morning".
 */
export function greetingSlot(hour: number): GreetingSlot {
  if (hour >= 5 && hour < 12) return "morning";
  if (hour >= 12 && hour < 18) return "afternoon";
  return "evening";
}
