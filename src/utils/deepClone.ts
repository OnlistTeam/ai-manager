export function deepClone<T>(value: T): T {
  if (typeof globalThis.structuredClone === "function") {
    return globalThis.structuredClone(value);
  }

  return deepCloneFallback(value);
}

function deepCloneFallback<T>(value: T): T {
  if (value === null || typeof value !== "object") return value;
  if (value instanceof Date) return new Date(value.getTime()) as T;
  if (Array.isArray(value)) {
    return value.map((item) => deepCloneFallback(item)) as T;
  }

  const cloned = {} as T;
  Object.keys(value).forEach((key) => {
    // `cloned["__proto__"] = ...` goes through the setter, which replaces cloned's
    // own prototype instead of adding an own property. This does not pollute the
    // global `Object.prototype` (verified), but it does leave the clone reading
    // those keys out of thin air from the source object -- a hard-to-track phantom
    // property. `structuredClone` keeps `__proto__` as an ordinary data key as-is,
    // so the two paths would otherwise behave differently -- skip it here so the
    // fallback matches the primary path.
    if (key === "__proto__") return;
    cloned[key as keyof T] = deepCloneFallback(value[key as keyof T]);
  });
  return cloned;
}
